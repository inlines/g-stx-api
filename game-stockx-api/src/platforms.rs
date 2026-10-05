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
    let query = r#"
WITH eligible AS MATERIALIZED (
 SELECT r.* FROM public.releases r
 JOIN public.platforms pl ON pl.id=r.platform AND pl.active=true AND pl.id<>6
 JOIN public.products p ON p.id=r.product_id
 WHERE r.digital_only IS NOT TRUE
 AND r.release_status IS DISTINCT FROM 5
 AND r.release_date <= EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)
 AND (public.effective_game_type(p.id,r.platform,p.game_type) NOT IN (1,2,4,13,6,5,14)
      OR public.effective_game_type(p.id,r.platform,p.game_type) IS NULL
      OR (r.platform=7 AND public.effective_game_type(p.id,r.platform,p.game_type) IN (2,4)
          AND EXISTS(SELECT 1 FROM unnest(r.serial) s WHERE btrim(s)<>'')))
), counts AS (
 SELECT platform, count(*)::integer AS total_games,
 count(*) FILTER (WHERE release_region=1)::integer AS europe_games,
 count(*) FILTER (WHERE release_region=2)::integer AS america_games,
 count(*) FILTER (WHERE release_region=5)::integer AS japan_games,
 count(*) FILTER (WHERE release_region IS NULL OR release_region NOT IN (1,2,5))::integer AS other_games
 FROM eligible GROUP BY platform
)
SELECT p.id,p.abbreviation,p.name,p.generation,
COALESCE(c.total_games,0) AS total_games,COALESCE(c.europe_games,0) AS europe_games,
COALESCE(c.america_games,0) AS america_games,COALESCE(c.japan_games,0) AS japan_games,
COALESCE(c.other_games,0) AS other_games
FROM public.platforms p LEFT JOIN counts c ON c.platform=p.id
WHERE p.active=true ORDER BY (p.id=32) DESC,p.name ASC
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
    let cache_key = format!("cache:v7:platforms:released-release-regions:catalog_v{}:day{}", versions.catalog, chrono::Utc::now().format("%Y%m%d"));
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
