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
WITH releases AS MATERIALIZED (
 SELECT r.*, (r.release_status IS DISTINCT FROM 5 AND r.release_date <= EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)) AS released, EXISTS(SELECT 1 FROM unnest(r.serial) s WHERE btrim(s)<>'') AS known
 FROM public.releases r JOIN public.platforms pl ON pl.id=r.platform AND pl.active=true AND pl.id<>6
), flags AS (
 SELECT product_id,platform,
   bool_or(released) AS visible,
   bool_or(NOT digital_only AND released AND known) AS physical,
   bool_or(release_region=1) AS pal, bool_or(release_region=2) AS usa,
   bool_or(release_region=5) AS jap, bool_or(release_region=8) AS ww,
   bool_or(release_region IS NULL OR release_region NOT IN (1,2,5)) AS other,
   bool_or(release_region=1 AND NOT digital_only AND released) AS physical_pal,
   bool_or(release_region=2 AND NOT digital_only AND released) AS physical_usa,
   bool_or(release_region=5 AND NOT digital_only AND released) AS physical_jap,
   bool_or(release_region=8 AND NOT digital_only AND released) AS physical_ww,
   bool_or((release_region IS NULL OR release_region NOT IN (1,2,5)) AND NOT digital_only AND released) AS physical_other,
   bool_or(release_region=1 AND NOT digital_only AND released AND known) AS known_pal,
   bool_or(release_region=2 AND NOT digital_only AND released AND known) AS known_usa,
   bool_or(release_region=5 AND NOT digital_only AND released AND known) AS known_jap,
   bool_or(release_region=8 AND NOT digital_only AND released AND known) AS known_ww,
   bool_or((release_region IS NULL OR release_region NOT IN (1,2,5)) AND NOT digital_only AND released AND known) AS known_other
 FROM releases GROUP BY product_id,platform
), eligible AS (
 SELECT f.* FROM flags f JOIN public.products p ON p.id=f.product_id
 WHERE f.visible
 AND (public.effective_game_type(p.id,f.platform,p.game_type) NOT IN (1,2,4,13,6,5,14) OR public.effective_game_type(p.id,f.platform,p.game_type) IS NULL
      OR (f.platform=7 AND public.effective_game_type(p.id,f.platform,p.game_type) IN (2,4) AND f.physical))
 AND EXISTS(SELECT 1 FROM public.product_platforms pp WHERE pp.product_id=p.id
            AND pp.platform_id=f.platform AND pp.digital_only=false)
), regional AS (
 SELECT e.product_id,e.platform,r.region,r.present,r.known FROM eligible e
 CROSS JOIN LATERAL (VALUES
 ('pal',CASE WHEN COALESCE(e.pal,false) THEN COALESCE(e.physical_pal,false) ELSE COALESCE(e.physical_ww,false) END,
    CASE WHEN COALESCE(e.pal,false) THEN COALESCE(e.known_pal,false) ELSE COALESCE(e.known_ww,false) END),
 ('usa',CASE WHEN COALESCE(e.usa,false) THEN COALESCE(e.physical_usa,false) ELSE COALESCE(e.physical_ww,false) END,
    CASE WHEN COALESCE(e.usa,false) THEN COALESCE(e.known_usa,false) ELSE COALESCE(e.known_ww,false) END),
 ('jap',CASE WHEN COALESCE(e.jap,false) THEN COALESCE(e.physical_jap,false) ELSE COALESCE(e.physical_ww,false) END,
    CASE WHEN COALESCE(e.jap,false) THEN COALESCE(e.known_jap,false) ELSE COALESCE(e.known_ww,false) END),
 ('other',COALESCE(e.physical_other,false),COALESCE(e.known_other,false))
 ) r(region,present,known)
), counts AS (
 SELECT platform, count(DISTINCT product_id) FILTER (WHERE present)::integer AS total_games,
 count(*) FILTER (WHERE region='pal' AND present)::integer AS europe_games,
 count(*) FILTER (WHERE region='usa' AND present)::integer AS america_games,
 count(*) FILTER (WHERE region='jap' AND present)::integer AS japan_games,
 count(*) FILTER (WHERE region='other' AND present)::integer AS other_games
 FROM regional GROUP BY platform
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
    let cache_key = format!("cache:v6:platforms:released-regions:catalog_v{}", versions.catalog);
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
