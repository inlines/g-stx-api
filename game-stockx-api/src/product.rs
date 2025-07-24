use diesel::{prelude::*, sql_query, sql_types};
use actix_web::web::{self, Data, Path};
use actix_web::{get, HttpRequest, HttpResponse};
use serde::{Deserialize, Serialize};
use crate::constants::CONNECTION_POOL_ERROR;
use crate::pagination::Pagination;
use actix_web::http::header;
use crate::auth::verify_jwt;
use crate::{DBPool, redis::{RedisPool, RedisCacheExt}};
use serde_json::json;

#[derive(QueryableByName, Serialize)]
pub struct Product {
    #[diesel(sql_type = sql_types::Integer)] 
    pub id: i32,
    #[diesel(sql_type = sql_types::Text)]
    pub cover: String,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Integer>)]
    pub first_release_date: Option<i32>,
    #[diesel(sql_type = sql_types::Text)]
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, QueryableByName)]
pub struct ProductListItem {
    #[diesel(sql_type = sql_types::Integer)]
    pub id: i32,
    #[diesel(sql_type = sql_types::Text)]
    pub name: String,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Integer>)]
    pub first_release_date: Option<i32>,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Text>)]
    pub image_url: Option<String>,
}

#[derive(QueryableByName)]
pub struct CountResult {
    #[diesel(sql_type = sql_types::BigInt)]
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

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);
    let db_text_query = format!("%{}%", text_query);

    let mut base_query = String::from(r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.first_release_date AS first_release_date,
            '//89.104.66.193/static/covers-thumb/' || cov.id || '.jpg' AS image_url
        FROM product_platforms AS pp
        INNER JOIN products AS prod ON pp.product_id = prod.id
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE pp.platform_id = $4 AND prod.name ILIKE $3
    "#);

    if ignore_digital {
        base_query.push_str(" AND pp.digital_only = false");
    }

    base_query.push_str(" ORDER BY prod.name ASC LIMIT $1 OFFSET $2");

    let results = diesel::sql_query(base_query)
        .bind::<sql_types::BigInt, _>(limit)
        .bind::<sql_types::BigInt, _>(offset)
        .bind::<sql_types::Text, _>(db_text_query.clone())
        .bind::<sql_types::BigInt, _>(cat)
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
        .bind::<sql_types::BigInt, _>(cat)
        .bind::<sql_types::Text, _>(db_text_query)
        .load::<CountResult>(conn);

    match (results, count_result) {
        (Ok(items), Ok(count)) => {
            let response = ProductListResponse {
                items,
                total_count: count.get(0).map(|c| c.total).unwrap_or(0),
            };

            if let Ok(mut redis_conn) = redis_pool.get().await {
                let ttl = if offset == 0 { 300 } else { 60 };
                let _ = redis_conn.set_json(&cache_key, &response, ttl).await;
            }

            HttpResponse::Ok().json(response)
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[derive(Debug, Serialize, QueryableByName)]
pub struct ProductProperties {
    #[diesel(sql_type = sql_types::Integer)]
    pub id: i32,
    #[diesel(sql_type = sql_types::Text)]
    pub name: String,
    #[diesel(sql_type = sql_types::Text)]
    pub summary: String,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Integer>)]
    pub first_release_date: Option<i32>,
    #[diesel(sql_type = sql_types::Text)]
    pub image_url: String,
}

#[derive(Debug, Serialize, QueryableByName)]
pub struct ReleaseInfo {
    #[diesel(sql_type = sql_types::Integer)]
    pub id: i32,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Integer>)]
    pub release_date: Option<i32>,
    #[diesel(sql_type = sql_types::Integer)]
    pub product_id: i32,
    #[diesel(sql_type = sql_types::Integer)]
    pub platform: i32,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Integer>)]
    pub release_status: Option<i32>,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Integer>)]
    pub release_region: Option<i32>,
    #[diesel(sql_type = sql_types::Bool)]
    pub digital_only: bool,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Array<sql_types::Text>>)]
    pub serial: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct ProductResponse {
    product: ProductProperties,
    releases: Vec<ReleaseInfo>,
}

#[get("/products/{id}")]
pub async fn get(
    pool: Data<DBPool>,
    path: Path<(i32,)>,
) -> HttpResponse {
    let (product_id,) = path.into_inner();
    let conn = &mut pool.get().expect("Failed to get DB connection");

    // Запрос продукта
    let product = match sql_query(
        r#"SELECT 
            p.id,
            p.name,
            p.summary,
            p.first_release_date,
            p.image_url
        FROM products p
        WHERE p.id = $1"#
    )
    .bind::<sql_types::Integer, _>(product_id)
    .get_result::<ProductProperties>(conn) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Product query failed: {}", e);
            return HttpResponse::NotFound().finish();
        }
    };

    // Запрос релизов
    let releases = match sql_query(
        r#"SELECT
            r.id,
            r.release_date,
            r.product_id,
            r.platform,
            r.release_status,
            r.release_region,
            r.digital_only,
            r.serial
        FROM releases r
        WHERE r.product_id = $1"#
    )
    .bind::<sql_types::Integer, _>(product_id)
    .load::<ReleaseInfo>(conn) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Releases query failed: {}", e);
            Vec::new()
        }
    };

    HttpResponse::Ok().json(ProductResponse {
        product,
        releases
    })
}