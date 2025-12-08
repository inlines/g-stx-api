use diesel::prelude::*;
use actix_web::web::{self, Data, Path};
use actix_web::{HttpRequest, HttpResponse};
use diesel::sql_types::{Integer, Text, Nullable, Bool, Array};
use serde::{Deserialize, Serialize};
use crate::constants::CONNECTION_POOL_ERROR;
use actix_web::http::header;
use crate::auth::verify_jwt;
use crate::{DBPool, redis::{RedisPool, RedisCacheExt}};

#[derive(Debug, Clone, Deserialize, Serialize, QueryableByName)]
pub struct ProductProperties {
    #[diesel(sql_type = Integer)]
    pub id: i32,

    #[diesel(sql_type = Text)]
    pub name: String,

    #[diesel(sql_type = Text)]
    pub summary: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub first_release_date: Option<i32>,

    #[diesel(sql_type = Nullable<Text>)]
    pub image_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, QueryableByName)]
pub struct ProductReleaseInfo {
    #[diesel(sql_type = Integer)]
    pub release_id: i32,

    #[diesel(sql_type = Nullable<Integer>)]
    pub release_date: Option<i32>,

    #[diesel(sql_type = Text)]
    pub release_region: String,

    #[diesel(sql_type = Text)]
    pub platform_name: String,

    #[diesel(sql_type = Integer)]
    pub platform_id: i32,

    #[diesel(sql_type = Nullable<Integer>)]
    pub release_status: Option<i32>,

    #[diesel(sql_type = Array<Text>)]
    pub bid_user_logins: Vec<String>,

    #[diesel(sql_type = Bool)]
    pub digital_only: bool,

    #[diesel(sql_type = Nullable<Array<Text>>)]
    pub serial: Option<Vec<String>>,
}

#[derive(QueryableByName, Clone, Serialize, Deserialize)]
struct ScreenshotUrl {
    #[diesel(sql_type = Text)]
    pub image_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProductResponse {
    pub product: ProductProperties,
    pub releases: Vec<ProductReleaseInfo>,
    pub screenshots: Vec<String>,
}

fn build_product_cache_key(product_id: i32) -> String {
    format!("product_details:basic:{}", product_id)
}

fn build_bids_cache_key(product_id: i32) -> String {
    format!("product_details:bids:{}", product_id)
}

#[get("/products/{id}")]
pub async fn get(
    pool: Data<DBPool>,
    redis_pool: Data<RedisPool>,
    path: Path<i32>,
    req: HttpRequest,
) -> HttpResponse {
    let product_id = path.into_inner();

    let user_login_opt = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|token| {
            if token.starts_with("Bearer ") {
                verify_jwt(&token[7..]).map(|claims| claims.sub)
            } else {
                None
            }
        });

    let basic_info = match get_product_basic_info(&pool, &redis_pool, product_id).await {
        Ok(info) => info,
        Err(e) => {
            eprintln!("Error getting product info: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let (mut releases, screenshots) = match get_product_releases(&pool, &redis_pool, product_id).await {
        Ok(data) => data,
        Err(e) => {
            eprintln!("Error getting releases: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    if let Some(login) = user_login_opt {
        for release in &mut releases {
            release.bid_user_logins.retain(|l| l != &login);
        }
    }

    HttpResponse::Ok().json(ProductResponse {
        product: basic_info,
        releases,
        screenshots,
    })
}

async fn get_product_basic_info(
    pool: &Data<DBPool>,
    redis_pool: &Data<RedisPool>,
    product_id: i32,
) -> Result<ProductProperties, String> {
    let cache_key = build_product_cache_key(product_id);

    if let Ok(mut redis_conn) = redis_pool.get().await {
        if let Ok(Some(cached)) = redis_conn.get_json::<ProductProperties>(&cache_key).await {
            return Ok(cached);
        }
    }

    let conn = &mut pool.get().map_err(|e| e.to_string())?;

    let query = r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.summary AS summary,
            prod.first_release_date AS first_release_date,
            '//89.104.66.193/static/covers-full/' || cov.id || '.jpg' AS image_url
        FROM products AS prod
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE prod.id = $1
    "#;

    let product_info = diesel::sql_query(query)
        .bind::<Integer, _>(product_id)
        .get_result::<ProductProperties>(conn)
        .map_err(|e| e.to_string())?;

    if let Ok(mut redis_conn) = redis_pool.get().await {
        let _ = redis_conn.set_json(&cache_key, &product_info, 86400).await;
    }

    Ok(product_info)
}

async fn get_product_releases(
    pool: &Data<DBPool>,
   redis_pool: &Data<RedisPool>,
    product_id: i32,
) -> Result<(Vec<ProductReleaseInfo>, Vec<String>), String> {
    let cache_key = build_bids_cache_key(product_id);
    
    if let Ok(mut redis_conn) = redis_pool.get().await {
        if let Ok(Some(cached)) = redis_conn.get_json::<(Vec<ProductReleaseInfo>, Vec<String>)>(&cache_key).await {
            return Ok(cached);
        }
    }

    let conn = &mut pool.get().map_err(|e| e.to_string())?;

    let releases_query = r#"
        SELECT
            r.id AS release_id,
            r.release_date AS release_date,
            reg.name AS release_region,
            p.name AS platform_name,
            p.id AS platform_id,
            r.release_status AS release_status,
            COALESCE(
                ARRAY_AGG(uhb.user_login) FILTER (WHERE uhb.user_login IS NOT NULL), 
                ARRAY[]::text[]
            ) AS bid_user_logins,
            r.digital_only AS digital_only,
            r.serial AS serial
        FROM releases AS r
        LEFT JOIN platforms AS p ON r.platform = p.id
        INNER JOIN regions AS reg ON reg.id = r.release_region
        LEFT JOIN users_have_bids AS uhb ON uhb.release_id = r.id
        WHERE r.product_id = $1 AND p.active = true
        GROUP BY r.id, reg.name, p.name, p.id
        ORDER BY p.name
    "#;

    let releases: Vec<ProductReleaseInfo> = diesel::sql_query(releases_query)
        .bind::<Integer, _>(product_id)
        .load(conn)
        .map_err(|e| e.to_string())?;

    let screenshots_query = r#"
        SELECT image_url
        FROM screenshots
        WHERE game = $1
    "#;

    let screenshots: Vec<String> = diesel::sql_query(screenshots_query)
        .bind::<Integer, _>(product_id)
        .load::<ScreenshotUrl>(conn)
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|s| s.image_url)
        .collect();

    if let Ok(mut redis_conn) = redis_pool.get().await {
        let cache_data = (releases.clone(), screenshots.clone());
        let _ = redis_conn.set_json(&cache_key, &cache_data, 60).await;
    }

    Ok((releases, screenshots))
}