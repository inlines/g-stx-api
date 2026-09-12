use crate::auth::verify_jwt;
use crate::pagination::Pagination;
use crate::{
    DBPool,
    redis::{self, Cache, RedisPool},
};
use actix_web::web::{self, Data};
use actix_web::{HttpRequest, HttpResponse, ResponseError};
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

    #[diesel(sql_type = Array<Text>)]
    pub serial: Vec<String>,

    #[diesel(sql_type = Bool)]
    pub digital_only: bool,

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
    #[diesel(sql_type = BigInt)]
    pub europe: i64,
    #[diesel(sql_type = BigInt)]
    pub america: i64,
    #[diesel(sql_type = BigInt)]
    pub japan: i64,
    #[diesel(sql_type = BigInt)]
    pub other: i64,
}

#[derive(Serialize, Deserialize)]
pub struct ProductListResponse {
    items: Vec<ProductListItem>,
    total_count: i64,
    region_counts: Option<std::collections::BTreeMap<String, i64>>,
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
        "cache:v9:catalog:regions:{}",
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
        AND (region_release.release_region=8 OR (CASE region_release.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=ANY({regions}))
    )) ")
}

fn search_filter(serial: bool, platform: &str, text: &str, regions: &str) -> String {
    if serial {
        format!(
            "EXISTS(SELECT 1 FROM releases sr WHERE sr.product_id=p.id AND sr.platform={platform} AND release_serial_search_keys(sr.serial) @> ARRAY[{text}]::text[] AND (cardinality({regions}::text[])=0 OR sr.release_region=8 OR (CASE sr.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=ANY({regions})))"
        )
    } else {
        format!(
            "(p.name ILIKE {text} OR EXISTS(SELECT 1 FROM alternative_names an WHERE an.product_id=p.id AND an.name ILIKE {text}))"
        )
    }
}

fn serials_exist(platform: &str) -> String {
    format!(
        "EXISTS (SELECT 1 FROM releases r CROSS JOIN LATERAL unnest(r.serial) AS serial_number(value) WHERE r.product_id=p.id AND r.platform={platform} AND btrim(serial_number.value) <> '')"
    )
}

#[get("/products")]
pub async fn list(
    pool: Data<DBPool>,
    redis_pool: Data<RedisPool>,
    query: web::Query<Pagination>,
    req: HttpRequest,
) -> HttpResponse {
    let unknown = query.unknown.unwrap_or(false);
    if unknown {
        let Some(claims) = crate::auth::authenticated_claims(&req) else {
            return HttpResponse::Unauthorized().finish();
        };
        if let Err(error) = crate::admin::db(pool.clone(), move |conn| {
            crate::admin::require_admin(conn, &claims)
        })
        .await
        {
            return error.error_response();
        }
    }
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
    let search_mode = query.search_mode.as_deref().unwrap_or("name");
    if !matches!(search_mode, "name" | "serial") {
        return HttpResponse::BadRequest().body("Invalid search mode");
    }
    let mut text_query = query.query.clone().unwrap_or_default();
    if search_mode == "serial" && !text_query.trim().is_empty() {
        text_query = match crate::serial_number::parse(&text_query) {
            Ok(value) => value,
            Err(()) => return HttpResponse::BadRequest().body(crate::serial_number::FORMAT_ERROR),
        };
    }
    if search_mode == "serial" {
        text_query = text_query.trim().to_owned();
    }
    let serial_search = search_mode == "serial" && !text_query.is_empty();
    // Unknown always excludes digital-only games, regardless of the catalogue toggle.
    let ignore_digital = unknown || query.ignore_digital.unwrap_or(false);
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
    cache_key.push_str(&format!(":search_{search_mode}:unknown_{unknown}"));
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

    let db_text_query = if serial_search {
        text_query.clone()
    } else {
        format!("%{}%", text_query)
    };

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
    let serials = serials_exist("$4");
    let unknown_filter = if unknown {
        format!("AND NOT {serials}")
    } else {
        String::new()
    };
    let search_predicate = search_filter(serial_search, "$4", "$3", "$12");
    let sql = format!(
        r#"
        SELECT 
            p.id AS id,
            p.name AS name,
            {serials} AS has_serials,
            ARRAY(SELECT value FROM (
                SELECT n.value AS value,
                    MIN(CASE WHEN cardinality($12::text[])=0 OR r.release_region=8 OR
                      (CASE r.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=ANY($12)
                      THEN 0 ELSE 1 END) AS priority
                FROM releases r CROSS JOIN LATERAL unnest(format_release_serials(r.serial)) n(value)
                WHERE r.product_id=p.id AND r.platform=$4 AND btrim(n.value)<>''
                GROUP BY n.value
            ) serial_values ORDER BY priority,value) AS serial,
            EXISTS(SELECT 1 FROM product_platforms pp WHERE pp.product_id=p.id AND pp.platform_id=$4 AND pp.digital_only) AS digital_only,
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
        AND {search_predicate}
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
        {unknown_filter}
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
    let unknown_filter = if unknown {
        format!("AND NOT {}", serials_exist("$1"))
    } else {
        String::new()
    };
    let regional_columns = ["europe", "america", "japan", "other"]
        .map(|region| {
            if unknown {
                let predicate = build_region_filter("$1", &format!("ARRAY['{region}']::text[]"));
                format!("COUNT(DISTINCT p.id) FILTER (WHERE true {predicate}) AS {region}")
            } else {
                format!("0::bigint AS {region}")
            }
        })
        .join(", ");
    let search_predicate = search_filter(serial_search, "$1", "$2", "$10");
    let count_sql = format!(
        r#"
        SELECT COUNT(DISTINCT p.id) FILTER (WHERE true {region_filter}) as total, {regional_columns}
        FROM products p
        WHERE EXISTS (
            SELECT 1 
            FROM product_platforms pp 
            WHERE pp.product_id = p.id
                AND pp.platform_id = $1
                AND ($3 = false OR pp.digital_only = false)
        )
        AND {search_predicate}
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
        {unknown_filter}
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
                region_counts: if unknown {
                    count.first().map(|c| {
                        [
                            ("europe", c.europe),
                            ("america", c.america),
                            ("japan", c.japan),
                            ("other", c.other),
                        ]
                        .into_iter()
                        .map(|(k, v)| (k.to_owned(), v))
                        .collect()
                    })
                } else {
                    None
                },
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
