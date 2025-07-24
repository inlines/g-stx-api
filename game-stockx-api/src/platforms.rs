use actix_web::{get, web::Data, HttpResponse};
use diesel::{sql_types::{Integer, Text, Nullable}, QueryableByName, RunQueryDsl};
use serde::{Serialize, Deserialize};
use crate::{DBPool, redis::{RedisPool, RedisCacheExt}};

#[derive(Debug, Serialize, Deserialize, QueryableByName)]
pub struct PlatformItem {
    #[diesel(sql_type = Integer)]
    pub id: i32,

    #[diesel(sql_type = Text)]
    pub abbreviation: String,

    #[diesel(sql_type = Text)]
    pub name: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub generation: Option<i32>,

    #[diesel(sql_type = Integer)]
    pub total_games: i32,
}

async fn load_from_db(pool: &Data<DBPool>) -> Result<Vec<PlatformItem>, HttpResponse> {
    let conn = &mut pool.get().map_err(|e| {
        log::error!("Failed to get DB connection: {}", e);
        HttpResponse::InternalServerError().finish()
    })?;
    
    let query = r#"
        SELECT id, abbreviation, name, generation, total_games
        FROM public.platforms 
        WHERE active = true
        ORDER BY name ASC
    "#;

    diesel::sql_query(query)
        .load::<PlatformItem>(conn)
        .map_err(|e| {
            log::error!("DB error: {}", e);
            HttpResponse::InternalServerError().finish()
        })
}

#[get("/platforms")]
pub async fn get_platforms(
    pool: Data<DBPool>,
    redis_pool: Data<RedisPool>,
) -> HttpResponse {
    const CACHE_KEY: &str = "platforms:active_list";
    const CACHE_TTL_SEC: usize = 86400;

    // 1. Try to get from cache
    if let Ok(mut conn) = redis_pool.get().await {
        if let Ok(Some(cached)) = conn.get_json::<Vec<PlatformItem>>(CACHE_KEY).await {
            return HttpResponse::Ok().json(cached);
        }
    }

    // 2. Load from DB
    let items = match load_from_db(&pool).await {
        Ok(items) => items,
        Err(resp) => return resp,
    };

    // 3. Save to cache (ignore errors)
    if let Ok(mut conn) = redis_pool.get().await {
        let _ = conn.set_json(CACHE_KEY, &items, CACHE_TTL_SEC).await;
    }

    HttpResponse::Ok().json(items)
}