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
    pub other_games: i32,
}

async fn load_from_db(pool: &Data<DBPool>) -> Result<Vec<PlatformItem>, HttpResponse> {
    let conn = &mut pool.get().map_err(|e| {
        log::error!("Failed to get DB connection: {}", e);
        HttpResponse::InternalServerError().finish()
    })?;

    let query = r#"
        WITH counts AS (
            SELECT r.platform,
                COUNT(DISTINCT r.product_id)::integer AS total_games,
                COUNT(DISTINCT r.product_id) FILTER (WHERE r.release_region=1)::integer AS europe_games,
                COUNT(DISTINCT r.product_id) FILTER (WHERE r.release_region=2)::integer AS america_games,
                COUNT(DISTINCT r.product_id) FILTER (WHERE r.release_region IS NULL OR r.release_region NOT IN (1,2))::integer AS other_games
            FROM releases r
            WHERE NOT r.digital_only AND NOT EXISTS (
                SELECT 1 FROM product_platforms pp WHERE pp.product_id=r.product_id
                AND pp.platform_id=r.platform AND pp.digital_only
            )
            GROUP BY r.platform
        )
        SELECT p.id, p.abbreviation, p.name, p.generation,
            COALESCE(c.total_games,0) AS total_games,
            COALESCE(c.europe_games,0) AS europe_games,
            COALESCE(c.america_games,0) AS america_games,
            COALESCE(c.other_games,0) AS other_games
        FROM public.platforms p LEFT JOIN counts c ON c.platform=p.id
        WHERE p.active = true
        ORDER BY p.name ASC
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
    let cache_key = format!("cache:v3:platforms:regions:catalog_v{}", versions.catalog);
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
