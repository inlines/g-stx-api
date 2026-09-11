use crate::auth::verify_jwt;
use crate::pagination::Pagination;
use crate::{
    DBPool,
    redis::{self, Cache, RedisPool},
};
use actix_web::web::{self, Data};
use actix_web::{HttpRequest, HttpResponse};
use diesel::RunQueryDsl;
use diesel::prelude::*;
use diesel::sql_types::{Array, BigInt, Bool, Double, Integer, Nullable, Text};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, QueryableByName)]
pub struct ProductListItem {
    #[diesel(sql_type = Integer)]
    pub id: i32,

    #[diesel(sql_type = Bool)]
    pub has_serials: bool,

    #[diesel(sql_type = Text)]
    pub name: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub first_release_date: Option<i32>,

    #[diesel(sql_type = Nullable<Text>)]
    pub image_url: Option<String>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub parent_game: Option<i32>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub game_type: Option<i32>,

    #[diesel(sql_type = Nullable<Double>)]
    pub total_rating: Option<f64>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub total_rating_count: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub local_players: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub online_players: Option<i32>,
    #[diesel(sql_type = Bool)]
    pub local_multiplayer: bool,
    #[diesel(sql_type = Bool)]
    pub online_multiplayer: bool,
}

#[derive(QueryableByName)]
pub struct CountResult {
    #[diesel(sql_type = BigInt)]
    pub total: i64,
}

#[derive(Serialize, Deserialize)]
pub struct ProductListResponse {
    items: Vec<ProductListItem>,
    total_count: i64,
}

fn build_cache_key(
    cat: i64,
    limit: i64,
    offset: i64,
    query: &str,
    ignore_digital: bool,
    sort: &str,
) -> String {
    // JSON encoding keeps delimiters in user-supplied search strings unambiguous.
    format!(
        "cache:v5:catalog:regions:{}",
        serde_json::json!([cat, limit, offset, query, ignore_digital, sort])
    )
}

// Shared by the list and count queries. Digital filtering stays on
// product_platforms.digital_only; serial availability does not affect visibility.
fn visibility_filter(unreleased: &str) -> String {
    format!(
        r#"
        AND ({unreleased} OR p.first_release_date IS NOT NULL)
        AND (p.game_type NOT IN (1, 2, 4, 13, 6, 5) OR p.game_type IS NULL)
    "#
    )
}

// EXISTS keeps a game unique even when several releases match selected regions.
fn build_region_filter(platform: &str, regions: &str) -> String {
    format!(" AND (cardinality({regions}::text[])=0 OR EXISTS (
        SELECT 1 FROM releases region_release
        WHERE region_release.product_id=p.id AND region_release.platform={platform}
        AND (CASE region_release.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' ELSE 'other' END)=ANY({regions})
    )) ")
}

#[get("/products")]
pub async fn list(
    pool: Data<DBPool>,
    redis_pool: Data<RedisPool>,
    query: web::Query<Pagination>,
    req: HttpRequest,
) -> HttpResponse {
    let regions = match query.region_groups() {
        Ok(regions) => regions,
        Err(()) => return HttpResponse::BadRequest().body("Invalid region group"),
    };
    let company_role = query.company_role.as_deref().unwrap_or("developer");
    if !matches!(company_role, "developer" | "publisher") {
        return HttpResponse::BadRequest().body("Invalid company role");
    }
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let cat = query.cat;
    let text_query = query.query.clone().unwrap_or_default();
    let ignore_digital = query.ignore_digital.unwrap_or(false);
    let sort = query.sort.clone().unwrap_or_default();
    let include_unreleased = query.include_unreleased.unwrap_or(false);

    if limit > 20 || offset > 20 {
        // Извлекаем токен из заголовка
        let token = crate::auth::bearer_token(&req);

        // Проверяем JWT токен
        let _claims = match token.and_then(verify_jwt) {
            Some(c) => c,
            None => {
                return HttpResponse::Unauthorized()
                    .body("Invalid or missing token. Authorization required for large queries.");
            }
        };
    }

    let mut cache_key = build_cache_key(cat, limit, offset, &text_query, ignore_digital, &sort);
    cache_key.push_str(&format!(":unreleased_{include_unreleased}"));
    cache_key.push_str(&format!(":regions_{}", regions.join(",")));
    if let Some(id) = query.franchise_id {
        cache_key.push_str(&format!(":franchise_{id}"));
    }

    if let Some(id) = query.company_id {
        cache_key.push_str(&format!(":company_{id}:role_{company_role}"));
    }

    let versions = match redis::versions(pool.clone(), 0).await {
        Ok(value) => value,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    cache_key.push_str(&format!(":catalog_v{}", versions.catalog));
    cache_key.push_str(&format!(
        ":local_{}:online_{}",
        query.local_multiplayer.unwrap_or(false),
        query.online_multiplayer.unwrap_or(false)
    ));
    if !text_query.is_empty() {
        cache_key.push_str(&format!(":names_v{}", versions.names));
    }
    if let Some(cached) =
        redis::read::<ProductListResponse>(&redis_pool, Cache::Catalog, &cache_key).await
    {
        return HttpResponse::Ok().json(cached);
    }

    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Database connection error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let db_text_query = format!("%{}%", text_query);

    let (order_column, order_direction, nulls_order) = match sort.as_str() {
        "rating" => ("p.total_rating", "DESC", "NULLS LAST"),
        "date" => ("p.first_release_date", "ASC", "NULLS LAST"),
        _ => ("p.name", "ASC", "NULLS LAST"),
    };

    let local = crate::game_features::LOCAL;
    let online = crate::game_features::ONLINE;
    let features_filter = format!(
        " AND (NOT $9 OR EXISTS(SELECT 1 FROM product_multiplayer_modes m WHERE m.game=p.id AND m.platform=$4 AND {local})) AND (NOT $10 OR EXISTS(SELECT 1 FROM product_multiplayer_modes m WHERE m.game=p.id AND m.platform=$4 AND {online})) "
    );
    let visibility = visibility_filter("$11");
    let region_filter = build_region_filter("$4", "$12");
    let sql = format!(
        r#"
        SELECT 
            p.id AS id,
            p.name AS name,
            EXISTS (
                SELECT 1 FROM releases r
                CROSS JOIN LATERAL unnest(r.serial) AS serial_number(value)
                WHERE r.product_id = p.id AND r.platform = $4
                  AND btrim(serial_number.value) <> ''
            ) AS has_serials,
            p.first_release_date AS first_release_date,
            p.total_rating,
            p.total_rating_count,
            (SELECT NULLIF(MAX(GREATEST(m.offlinemax,m.offlinecoopmax)),0) FROM product_multiplayer_modes m WHERE m.game=p.id AND m.platform=$4) AS local_players,
            (SELECT NULLIF(MAX(GREATEST(m.onlinemax,m.onlinecoopmax)),0) FROM product_multiplayer_modes m WHERE m.game=p.id AND m.platform=$4) AS online_players,
            EXISTS(SELECT 1 FROM product_multiplayer_modes m WHERE m.game=p.id AND m.platform=$4 AND {local}) AS local_multiplayer,
            EXISTS(SELECT 1 FROM product_multiplayer_modes m WHERE m.game=p.id AND m.platform=$4 AND {online}) AS online_multiplayer,
            p.game_type,
            p.parent_game,
            '//89.104.66.193/static/covers-full/' || c.id || '.jpg' AS image_url
        FROM products p
        LEFT JOIN covers c ON p.cover_id = c.id
        WHERE EXISTS (
            SELECT 1 
            FROM product_platforms pp 
            WHERE pp.product_id = p.id
                AND pp.platform_id = $4
                AND ($5 = false OR pp.digital_only = false)
        )
        AND (
            p.name ILIKE $3 
            OR EXISTS (
                SELECT 1 FROM alternative_names an
                WHERE an.product_id = p.id AND an.name ILIKE $3
            )
        )
        AND ($6::integer IS NULL OR EXISTS (
            SELECT 1 FROM game_franschises gf
            WHERE gf.product_id = p.id AND gf.franschise_id = $6
        ))
        AND ($7::integer IS NULL OR EXISTS (
            SELECT 1 FROM involved_companies ic
            WHERE ic.game = p.id AND ic.company = $7
              AND (($8 = 'developer' AND ic.developer = true) OR ($8 = 'publisher' AND ic.publisher = true))
        ))
        {visibility}
        {region_filter}
        {features_filter}
        ORDER BY {} {} {}, p.id ASC
        LIMIT $1 OFFSET $2
        "#,
        order_column, order_direction, nulls_order
    );

    let results = diesel::sql_query(sql)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .bind::<diesel::sql_types::Text, _>(db_text_query.clone())
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::Bool, _>(ignore_digital)
        .bind::<Nullable<Integer>, _>(query.franchise_id)
        .bind::<Nullable<Integer>, _>(query.company_id)
        .bind::<Text, _>(company_role)
        .bind::<Bool, _>(query.local_multiplayer.unwrap_or(false))
        .bind::<Bool, _>(query.online_multiplayer.unwrap_or(false))
        .bind::<Bool, _>(include_unreleased)
        .bind::<Array<Text>, _>(&regions)
        .load::<ProductListItem>(conn);

    let count_filter = features_filter
        .replace("$9", "$7")
        .replace("$10", "$8")
        .replace("$4", "$1");
    let visibility = visibility_filter("$9");
    let region_filter = build_region_filter("$1", "$10");
    let count_sql = format!(
        r#"
        SELECT COUNT(DISTINCT p.id) as total
        FROM products p
        WHERE EXISTS (
            SELECT 1 
            FROM product_platforms pp 
            WHERE pp.product_id = p.id
                AND pp.platform_id = $1
                AND ($3 = false OR pp.digital_only = false)
        )
        AND (
            p.name ILIKE $2
            OR EXISTS (
                SELECT 1 FROM alternative_names an
                WHERE an.product_id = p.id AND an.name ILIKE $2
            )
        )
        AND ($4::integer IS NULL OR EXISTS (
            SELECT 1 FROM game_franschises gf
            WHERE gf.product_id = p.id AND gf.franschise_id = $4
        ))
        AND ($5::integer IS NULL OR EXISTS (
            SELECT 1 FROM involved_companies ic
            WHERE ic.game = p.id AND ic.company = $5
              AND (($6 = 'developer' AND ic.developer = true) OR ($6 = 'publisher' AND ic.publisher = true))
        ))
        {visibility}
        {region_filter}
        {count_filter}
    "#
    );

    let count_result = diesel::sql_query(count_sql)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::Text, _>(db_text_query)
        .bind::<diesel::sql_types::Bool, _>(ignore_digital)
        .bind::<Nullable<Integer>, _>(query.franchise_id)
        .bind::<Nullable<Integer>, _>(query.company_id)
        .bind::<Text, _>(company_role)
        .bind::<Bool, _>(query.local_multiplayer.unwrap_or(false))
        .bind::<Bool, _>(query.online_multiplayer.unwrap_or(false))
        .bind::<Bool, _>(include_unreleased)
        .bind::<Array<Text>, _>(&regions)
        .load::<CountResult>(conn);

    match (results, count_result) {
        (Ok(items), Ok(count)) => {
            let response = ProductListResponse {
                items,
                total_count: count.first().map(|c| c.total).unwrap_or(0),
            };

            let ttl = if offset == 0 { 300 } else { 60 };
            redis::write(&redis_pool, Cache::Catalog, &cache_key, &response, ttl).await;

            HttpResponse::Ok().json(response)
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}
