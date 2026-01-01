use diesel::prelude::*;
use actix_web::web::{self, Data};
use actix_web::HttpResponse;
use diesel::sql_types::{BigInt, Double, Integer, Nullable, Text};
use diesel::{RunQueryDsl};
use serde::{Deserialize, Serialize};
use crate::pagination::Pagination;
use crate::{DBPool, redis::{RedisPool, RedisCacheExt}};

#[derive(Debug, Deserialize, Serialize)]
pub struct Product {
    pub id: u32,
    pub cover: String,
    pub first_release_date: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, QueryableByName)]
pub struct ProductListItem {
    #[diesel(sql_type = Integer)]
    pub id: i32,

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

fn build_cache_key(cat: i64, limit: i64, offset: i64, query: &str, ignore_digital: bool) -> String {
    format!(
        "products:cat_{}:limit_{}:offset_{}:q_{}:dig_{}",
        cat, limit, offset, query, ignore_digital
    )
}

#[get("/products")]
pub async fn list(
    pool: Data<DBPool>,
    // redis_pool: Data<RedisPool>,
    query: web::Query<Pagination>
) -> HttpResponse {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let cat = query.cat;
    let text_query = query.query.clone().unwrap_or_default();
    let ignore_digital = query.ignore_digital.unwrap_or(false);
    let sort = query.sort.clone().unwrap_or_default();

    let cache_key = build_cache_key(cat, limit, offset, &text_query, ignore_digital);

    // if let Ok(mut redis_conn) = redis_pool.get().await {
    //     if let Ok(Some(cached)) = redis_conn.get_json::<ProductListResponse>(&cache_key).await {
    //         return HttpResponse::Ok().json(cached);
    //     }
    // }

    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Database connection error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let db_text_query = format!("%{}%", text_query);


    let (order_column, order_direction, nulls_order) = match sort.as_str() {
        "date" => ("p.first_release_date", "ASC", "NULLS LAST"),
        "name" | _ => ("p.name", "ASC", "NULLS LAST"),
    };

    let sql = format!(
        r#"
        SELECT 
            p.id AS id,
            p.name AS name,
            p.first_release_date AS first_release_date,
            p.total_rating,
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
        AND (p.game_type NOT IN (1, 2, 4) OR p.game_type IS NULL)
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
        .load::<ProductListItem>(conn);

    let count_sql = r#"
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
        AND (p.game_type NOT IN (1, 2, 4) OR p.game_type IS NULL)
    "#;

    let count_result = diesel::sql_query(count_sql)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::Text, _>(db_text_query)
        .bind::<diesel::sql_types::Bool, _>(ignore_digital)
        .load::<CountResult>(conn);

    match (results, count_result) {
        (Ok(items), Ok(count)) => {
            let response = ProductListResponse {
                items,
                total_count: count.get(0).map(|c| c.total).unwrap_or(0),
            };

            // if let Ok(mut redis_conn) = redis_pool.get().await {
            //     let ttl = if offset == 0 { 300 } else { 60 };
            //     if let Err(e) = redis_conn.set_json(&cache_key, &response, ttl).await {
            //         eprintln!("Failed to cache response: {}", e);
            //     }
            // }

            HttpResponse::Ok().json(response)
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}