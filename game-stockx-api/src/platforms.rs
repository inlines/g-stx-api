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
    #[diesel(sql_type = Integer)]
    pub europe_games: i32,
    #[diesel(sql_type = Integer)]
    pub america_games: i32,
    #[diesel(sql_type = Integer)]
    pub japan_games: i32,
    #[diesel(sql_type = Integer)]
    pub other_games: i32,
}

async fn load_from_db(pool: &Data<DBPool>) -> Result<Vec<PlatformItem>, HttpResponse> {
    // Recomputed transactionally after research imports / IGDB catalogue revisions.
    // This endpoint only reads persisted counters, including on a Redis miss.
    let query = r#"
        SELECT id, abbreviation, name, generation, COALESCE(total_games,0) AS total_games,
               europe_games, america_games, japan_games, other_games
        FROM public.platforms WHERE active=true
        ORDER BY (id=32) DESC, name ASC
    "#;

    crate::admin::db(pool.clone(), move |conn| {
        Ok(diesel::sql_query(query).load::<PlatformItem>(conn)?)
    })
    .await
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
    let cache_key = format!(
        "cache:v8:platforms:stored-catalog-totals:catalog_v{}:day{}",
        versions.catalog,
        chrono::Utc::now().format("%Y%m%d")
    );
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
