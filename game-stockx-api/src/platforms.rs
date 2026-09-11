use crate::{
    DBPool,
    redis::{self, Cache, RedisPool},
};
use actix_web::{HttpResponse, get, web::Data};
use diesel::{
    QueryableByName, RunQueryDsl,
    sql_types::{Integer, Nullable, Text},
};
use serde::{Deserialize, Serialize};

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
pub async fn get_platforms(pool: Data<DBPool>, redis_pool: Data<RedisPool>) -> HttpResponse {
    let versions = match redis::versions(pool.clone(), 0).await {
        Ok(value) => value,
        Err(_) => return HttpResponse::InternalServerError().finish(),
    };
    let cache_key = format!("cache:v2:platforms:catalog_v{}", versions.catalog);
    if let Some(cached) =
        redis::read::<Vec<PlatformItem>>(&redis_pool, Cache::Platforms, &cache_key).await
    {
        return HttpResponse::Ok().json(cached);
    }

    // 2. Load from DB
    let items = match load_from_db(&pool).await {
        Ok(items) => items,
        Err(resp) => return resp,
    };

    redis::write(&redis_pool, Cache::Platforms, &cache_key, &items, 86400).await;

    HttpResponse::Ok().json(items)
}
