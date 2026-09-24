use actix_web::{get, web::Data, HttpResponse};
use diesel::{QueryableByName, RunQueryDsl, sql_types::Jsonb};
use serde_json::Value;
use crate::DBPool;

#[derive(QueryableByName)]
struct Calendar { #[diesel(sql_type = Jsonb)] payload: Value }

#[get("/release-calendar")]
pub async fn get_calendar(pool: Data<DBPool>) -> HttpResponse {
    let result = crate::admin::db(pool, |conn| {
        let row = diesel::sql_query(r#"
WITH bounds AS (
 SELECT date_trunc('month',CURRENT_TIMESTAMP AT TIME ZONE 'UTC') AS start
), events AS (
 SELECT DISTINCT p.id, p.name, r.platform,
 to_char(to_timestamp(r.release_date) AT TIME ZONE 'UTC','YYYY-MM-DD') AS day,
 CASE WHEN p.cover_id IS NOT NULL THEN '//89.104.66.193/static/covers-full/'||p.cover_id||'.jpg' END AS image_url
 FROM releases r JOIN products p ON p.id=r.product_id CROSS JOIN bounds b
 WHERE r.platform IN (48,167) AND r.release_status IS DISTINCT FROM 5
 AND r.release_date >= EXTRACT(EPOCH FROM b.start)
 AND r.release_date < EXTRACT(EPOCH FROM b.start + INTERVAL '3 months')
 AND (effective_game_type(p.id,r.platform,p.game_type) NOT IN (1,2,4,13,6,5,14)
      OR effective_game_type(p.id,r.platform,p.game_type) IS NULL)
)
SELECT jsonb_build_object('start',to_char(b.start,'YYYY-MM-DD'),
 'items',COALESCE((SELECT jsonb_agg(to_jsonb(e) ORDER BY e.day,e.name,e.platform,e.id) FROM events e),'[]'::jsonb)) AS payload
FROM bounds b
"#).get_result::<Calendar>(conn)?;
        Ok(row.payload)
    }).await;
    match result { Ok(value) => HttpResponse::Ok().json(value), Err(error) => { log::error!("Calendar query failed: {}",error); HttpResponse::InternalServerError().finish() } }
}
