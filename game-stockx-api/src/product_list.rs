use diesel::prelude::*;
use actix_web::web::{self, Data};
use actix_web::HttpResponse;
use diesel::sql_types::{Integer, Text, Nullable, BigInt};
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
    redis_pool: Data<RedisPool>,
    query: web::Query<Pagination>
) -> HttpResponse {
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let cat = query.cat;
    let text_query = query.query.clone().unwrap_or_default();
    let ignore_digital = query.ignore_digital.unwrap_or(false);

    let cache_key = build_cache_key(cat, limit, offset, &text_query, ignore_digital);

    if let Ok(mut redis_conn) = redis_pool.get().await {
        if let Ok(Some(cached)) = redis_conn.get_json::<ProductListResponse>(&cache_key).await {
            return HttpResponse::Ok().json(cached);
        }
    }

    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Database connection error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let db_text_query = format!("%{}%", text_query);

    let mut base_query = String::from(r#"
        SELECT 
        prod.id AS id,
        prod.name AS name,
        prod.first_release_date AS first_release_date,
        '//89.104.66.193/static/covers-thumb/' || cov.id || '.jpg' AS image_url
        FROM products AS prod
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE prod.id IN (
            SELECT DISTINCT prod.id
            FROM product_platforms AS pp
            INNER JOIN products AS prod ON pp.product_id = prod.id
            LEFT JOIN alternative_names as an on an.product_id = prod.id
            WHERE pp.platform_id = $4 
                AND (prod.name ILIKE $3 OR an.name ILIKE $3)
        )
    "#);

    if ignore_digital {
        base_query.push_str(" AND pp.digital_only = false");
    }

    base_query.push_str(" ORDER BY prod.name ASC LIMIT $1 OFFSET $2");

    let results = diesel::sql_query(base_query)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .bind::<diesel::sql_types::Text, _>(db_text_query.clone())
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<ProductListItem>(conn);

    let mut count_query = String::from(r#"
        SELECT COUNT(*) as total
        FROM product_platforms AS pp
        INNER JOIN products AS prod ON pp.product_id = prod.id
        WHERE pp.platform_id = $1 AND prod.name ILIKE $2
    "#);

    if ignore_digital {
        count_query.push_str(" AND pp.digital_only = false");
    }

    let count_result = diesel::sql_query(count_query)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::Text, _>(db_text_query)
        .load::<CountResult>(conn);

    match (results, count_result) {
        (Ok(items), Ok(count)) => {
            let response = ProductListResponse {
                items,
                total_count: count.get(0).map(|c| c.total).unwrap_or(0),
            };

            if let Ok(mut redis_conn) = redis_pool.get().await {
                let ttl = if offset == 0 { 300 } else { 60 };
                if let Err(e) = redis_conn.set_json(&cache_key, &response, ttl).await {
                    eprintln!("Failed to cache response: {}", e);
                }
            }

            HttpResponse::Ok().json(response)
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}