use diesel::{prelude::*, sql_query, sql_types};
use actix_web::web::{self, Data, Path};
use actix_web::{HttpRequest, HttpResponse};
use diesel::{RunQueryDsl};
use serde::{Deserialize, Serialize};
use crate::constants::{ CONNECTION_POOL_ERROR};
use crate::pagination::Pagination;
use actix_web::http::header;
use crate::auth::{verify_jwt};
use crate::{DBPool, redis::{RedisPool, RedisCacheExt}};
use chrono::NaiveDate;

#[derive(Debug, Deserialize, Serialize)]
pub struct Product {
    pub id: u32,
    pub cover: String,
    pub first_release_date: String,
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
    // Пробуем получить данные из кеша
    // Пытаемся получить данные из кеша
    if let Ok(mut redis_conn) = redis_pool.get().await {
        if let Ok(Some(cached)) = redis_conn.get_json::<ProductListResponse>(&cache_key).await {
            return HttpResponse::Ok().json(cached);
        }
    }

     // Если в кеше нет, выполняем запрос к БД
    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Database connection error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    let db_text_query = format!("%{}%", text_query);

    // === Основной SELECT ===
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
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .bind::<diesel::sql_types::Text, _>(db_text_query.clone())
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<ProductListItem>(conn);

    // === COUNT SELECT ===
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

            // Кешируем результат
            if let Ok(mut redis_conn) = redis_pool.get().await {
                let ttl = if offset == 0 { 300 } else { 60 }; // 5 мин для первой страницы, 1 мин для остальных
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


#[derive(Debug, Serialize, Deserialize, QueryableByName)]
struct ProductProperties {
    #[diesel(sql_type = sql_types::BigInt)]
    id: i64,
    #[diesel(sql_type = sql_types::Text)]
    name: String,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Text>)]
    summary: Option<String>,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Date>)]
    first_release_date: Option<NaiveDate>,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Text>)]
    image_url: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, QueryableByName)]
struct ReleaseInfo {
    #[diesel(sql_type = sql_types::Nullable<sql_types::Date>)]
    release_date: Option<NaiveDate>,
    #[diesel(sql_type = sql_types::BigInt)]
    release_id: i64,
    #[diesel(sql_type = sql_types::Text)]
    release_status: String,
    #[diesel(sql_type = sql_types::Bool)]
    digital_only: bool,
    #[diesel(sql_type = sql_types::Nullable<sql_types::Text>)]
    serial: Option<String>,
    #[diesel(sql_type = sql_types::Text)]
    release_region: String,
    #[diesel(sql_type = sql_types::Text)]
    platform_name: String,
    #[diesel(sql_type = sql_types::BigInt)]
    platform_id: i64,
}

#[derive(Debug, Serialize, QueryableByName)]
struct ScreenshotUrl {
    #[diesel(sql_type = sql_types::Text)]
    image_url: String,
}

#[derive(Debug, Serialize)]
struct ReleaseInfoWithBids {
    #[serde(flatten)]
    release: ReleaseInfo,
    bid_user_logins: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProductResponse {
    product: ProductProperties,
    releases: Vec<ReleaseInfoWithBids>,
    screenshots: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct CachedProduct {
    product: ProductProperties,
    releases: Vec<ReleaseInfo>,
    screenshots: Vec<String>,
}

#[derive(QueryableByName)]
struct UserBid {
    #[diesel(sql_type = sql_types::BigInt)]
    release_id: i64,
    #[diesel(sql_type = sql_types::Text)]
    user_login: String,
}

#[get("/products/{id}")]
pub async fn get(
    pool: Data<DBPool>,
    redis_pool: Data<RedisPool>,
    path: Path<(i64,)>,
    req: HttpRequest,
) -> HttpResponse {
    let (product_id,) = path.into_inner();
    
    // Получаем соединение с БД
    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            eprintln!("Database connection error: {}", e);
            return HttpResponse::InternalServerError().finish();
        }
    };

    // Извлекаем логин пользователя из токена
    let user_login = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|h| h.to_str().ok())
        .and_then(|h| h.strip_prefix("Bearer "))
        .and_then(|token| verify_jwt(token)) // verify_jwt возвращает Option<Claims>
        .map(|claims| claims.sub); // Извлекаем sub из Claims

     // Пытаемся получить данные из кеша
    let cache_key = format!("product:{}", product_id);
    if let Ok(mut redis_conn) = redis_pool.get().await {
        if let Ok(Some(cached)) = redis_conn.get_json::<CachedProduct>(&cache_key).await {
            // Получаем свежие биды
            match get_fresh_bids(conn, product_id, user_login.as_deref()) {
                Ok(bids) => {
                    let response = combine_response(cached, bids);
                    return HttpResponse::Ok().json(response);
                }
                Err(e) => eprintln!("Failed to get fresh bids: {}", e),
            }
        }
    }

    // Полный запрос к БД если нет в кеше
    match get_full_product_data(conn, product_id, user_login.as_deref()) {
        Ok((cached_product, bids)) => {
            // Сохраняем в кеш (без бидов)
            if let Ok(mut redis_conn) = redis_pool.get().await {
                let _ = redis_conn.set_json(&cache_key, &cached_product, 3600).await;
            }

            let response = combine_response(cached_product, bids);
            HttpResponse::Ok().json(response)
        }
        Err(e) => {
            eprintln!("Database error: {}", e);
            HttpResponse::InternalServerError().finish()
        }
    }
}

// Получение полных данных о продукте
fn get_full_product_data(
    conn: &mut PgConnection,
    product_id: i64,
    user_login: Option<&str>,
) -> Result<(CachedProduct, Vec<(i64, Vec<String>)>), diesel::result::Error> {
    // Основная информация о продукте
    let product = sql_query(
        r#"SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.summary AS summary,
            prod.first_release_date AS first_release_date,
            '//89.104.66.193/static/covers-full/' || cov.id ||'.jpg' AS image_url
        FROM products AS prod
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE prod.id = $1"#,
    )
    .bind::<sql_types::BigInt, _>(product_id)
    .get_result::<ProductProperties>(conn)?;

    // Скриншоты
    let screenshots = sql_query(
        "SELECT image_url FROM screenshots WHERE game = $1",
    )
    .bind::<sql_types::BigInt, _>(product_id)
    .load::<ScreenshotUrl>(conn)?
    .into_iter()
    .map(|s| s.image_url)
    .collect();

    // Релизы (без бидов)
    let releases = sql_query(
        r#"SELECT
            r.release_date AS release_date,
            r.id AS release_id,
            r.release_status AS release_status,
            r.digital_only AS digital_only,
            r.serial AS serial,
            reg.name AS release_region,
            p.name AS platform_name,
            p.id AS platform_id
        FROM releases AS r
        LEFT JOIN platforms AS p ON r.platform = p.id
        INNER JOIN regions AS reg ON reg.id = r.release_region
        WHERE r.product_id = $1 AND p.active = true
        ORDER BY p.name"#,
    )
    .bind::<sql_types::BigInt, _>(product_id)
    .load::<ReleaseInfo>(conn)?;

    // Свежие биды
    let bids = get_fresh_bids(conn, product_id, user_login)?;

    Ok((CachedProduct { product, releases, screenshots }, bids))
}

// Получение свежих бидов из БД
fn get_fresh_bids(
    conn: &mut PgConnection,
    product_id: i64,
    user_login: Option<&str>,
) -> Result<Vec<(i64, Vec<String>)>, diesel::result::Error> {
    // Загружаем данные в структуру UserBid
    let bids = sql_query(
        "SELECT release_id, user_login FROM users_have_bids
         WHERE release_id IN (
             SELECT id FROM releases WHERE product_id = $1
         )",
    )
    .bind::<sql_types::BigInt, _>(product_id)
    .load::<UserBid>(conn)?;

    // Фильтрация бидов текущего пользователя
    let mut filtered_bids = bids;
    if let Some(login) = user_login {
        filtered_bids.retain(|bid| bid.user_login != login);
    }

    // Группировка по release_id
    let mut grouped = std::collections::HashMap::new();
    for bid in filtered_bids {
        grouped.entry(bid.release_id)
            .or_insert_with(Vec::new)
            .push(bid.user_login);
    }

    Ok(grouped.into_iter().collect())
}

// Комбинирование кешированных данных и свежих бидов
fn combine_response(
    cached: CachedProduct,
    bids: Vec<(i64, Vec<String>)>,
) -> ProductResponse {
    let bids_map: std::collections::HashMap<_, _> = bids.into_iter().collect();
    
    let releases = cached.releases.into_iter().map(|release| {
        let bid_user_logins = bids_map.get(&release.release_id).cloned().unwrap_or_default();
        ReleaseInfoWithBids {
            release,
            bid_user_logins,
        }
    }).collect();

    ProductResponse {
        product: cached.product,
        releases,
        screenshots: cached.screenshots,
    }
}