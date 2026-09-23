use crate::{DBPool, admin};
use actix_web::{HttpRequest, HttpResponse, get, web};
use diesel::{
    prelude::*,
    sql_types::{Array, BigInt, Bool, Integer, Jsonb, Nullable, Text},
};
use serde::Deserialize;

#[derive(Deserialize)]
pub(crate) struct LibraryQuery {
    login: Option<String>,
    cat: Option<i32>,
    regions: Option<String>,
    query: Option<String>,
    search_mode: Option<String>,
    sort: Option<String>,
    limit: Option<i64>,
    offset: Option<i64>,
}
#[derive(QueryableByName)]
struct Document {
    #[diesel(sql_type=Jsonb)]
    document: serde_json::Value,
}

#[get("/library/{kind}")]
pub(crate) async fn get_library(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    kind: web::Path<String>,
    params: web::Query<LibraryQuery>,
) -> HttpResponse {
    let Some(claims) = crate::auth::authenticated_claims_async(&req).await else {
        return HttpResponse::Unauthorized().finish();
    };
    let params = params.into_inner();
    let login = params.login.unwrap_or_else(|| claims.sub.clone());
    let own = login == claims.sub;
    let kind = kind.into_inner();
    let source = match kind.as_str() {
        "collection" => "users_have_releases l",
        "wishlist" if own => "users_have_wishes l",
        "wts" => {
            "users_have_wts l JOIN users_have_releases valid_owner ON valid_owner.user_login=l.user_login AND valid_owner.release_id=l.release_id"
        }
        _ => return HttpResponse::BadRequest().body("Invalid library"),
    };
    let (limit, offset) = match crate::pagination::page_bounds(params.limit, params.offset, 1000) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let regions: Vec<String> = params
        .regions
        .unwrap_or_default()
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    if regions
        .iter()
        .any(|s| !matches!(s.as_str(), "europe" | "america" | "japan" | "other"))
        || params.query.as_ref().is_some_and(|s| s.len() > 256)
    {
        return HttpResponse::BadRequest().body("Invalid filters");
    }
    let sort = params.sort.unwrap_or_else(|| "name".into());
    if !matches!(sort.as_str(), "name" | "date" | "price" | "rating") {
        return HttpResponse::BadRequest().body("Invalid sort");
    }
    let mode = params.search_mode.unwrap_or_else(|| "name".into());
    if !matches!(mode.as_str(), "name" | "serial") {
        return HttpResponse::BadRequest().body("Invalid search mode");
    }
    let price = if kind == "wts" {
        "l.price"
    } else if kind == "collection" {
        "CASE WHEN $9 THEN o.price END"
    } else {
        "NULL::integer"
    };
    let sql = format!(
        r#"
WITH base AS MATERIALIZED (
 SELECT r.id release_id, r.product_id, r.platform platform_id, r.release_region region_id, r.release_date,
 COALESCE(r.serial,ARRAY[]::text[]) serial,
 (r.digital_only OR EXISTS(SELECT 1 FROM product_platforms pp WHERE pp.product_id=r.product_id AND pp.platform_id=r.platform AND pp.digital_only)) digital_only,
 p.name product_name, p.cover_id, p.total_rating,
 plat.name platform_name, reg.name region_name,
 o.selected_serial, o.cib, CASE WHEN $9 THEN o.price END purchase_price, {price} price,
 CASE WHEN r.release_region=1 THEN 'europe' WHEN r.release_region=2 THEN 'america' WHEN r.release_region=5 THEN 'japan' ELSE 'other' END region_group
 FROM {source} JOIN releases r ON r.id=l.release_id JOIN products p ON p.id=r.product_id
 JOIN platforms plat ON plat.id=r.platform LEFT JOIN regions reg ON reg.id=r.release_region
 LEFT JOIN users_have_releases o ON o.release_id=l.release_id AND o.user_login=l.user_login
 WHERE l.user_login=$1
), platform_items AS MATERIALIZED (SELECT * FROM base WHERE $2::integer IS NULL OR platform_id=$2),
filtered AS MATERIALIZED (
 SELECT * FROM platform_items b
 WHERE (cardinality($3::text[])=0 OR b.region_id=8 OR b.region_group=ANY($3))
 AND ($4='' OR CASE WHEN $5='serial' THEN EXISTS(SELECT 1 FROM unnest(b.serial) s WHERE
 regexp_replace(upper(s),'[^A-Z0-9]','','g')=regexp_replace(upper($4),'[^A-Z0-9]','','g'))
 ELSE strpos(lower(b.product_name),lower($4))>0 OR EXISTS(SELECT 1 FROM alternative_names a WHERE a.product_id=b.product_id AND strpos(lower(a.name),lower($4))>0) END)
), page AS (
 SELECT * FROM filtered ORDER BY
 CASE WHEN $6='date' THEN release_date END ASC NULLS LAST,
 CASE WHEN $6='price' THEN price END ASC NULLS LAST,
 CASE WHEN $6='rating' THEN total_rating END DESC NULLS LAST,
 lower(product_name), release_id LIMIT $7 OFFSET $8
)
SELECT jsonb_build_object(
 'total_count',(SELECT count(*) FROM filtered), 'unfiltered_total',(SELECT count(*) FROM platform_items),
 'platform_ids',COALESCE((SELECT jsonb_agg(id ORDER BY id) FROM (SELECT DISTINCT platform_id id FROM base) ids),'[]'),
 'owned_regions',(SELECT jsonb_object_agg(g,(SELECT count(DISTINCT product_id) FROM platform_items b WHERE NOT b.digital_only AND (b.region_group=g OR b.region_id=8))) FROM unnest(ARRAY['europe','america','japan','other']) g),
 'items',COALESCE((SELECT jsonb_agg(to_jsonb(b) || jsonb_build_object(
 'image_url',CASE WHEN cover_id IS NOT NULL THEN '//89.104.66.193/static/covers-full/'||cover_id||'.jpg' END,
 'local_players',mp.local_players,'online_players',mp.online_players,'local_multiplayer',mp.local_multiplayer,'online_multiplayer',mp.online_multiplayer) ORDER BY CASE WHEN $6='date' THEN release_date END ASC NULLS LAST, CASE WHEN $6='price' THEN price END ASC NULLS LAST, CASE WHEN $6='rating' THEN total_rating END DESC NULLS LAST, lower(product_name),release_id)
 FROM page b LEFT JOIN LATERAL (SELECT NULLIF(MAX(GREATEST(m.offlinemax,m.offlinecoopmax)),0) local_players,
 NULLIF(MAX(GREATEST(m.onlinemax,m.onlinecoopmax)),0) online_players,
 bool_or({local}) local_multiplayer, bool_or({online}) online_multiplayer
 FROM product_multiplayer_modes m WHERE m.game=b.product_id AND m.platform=b.platform_id) mp ON TRUE),'[]')
) document
"#,
        local = crate::game_features::LOCAL,
        online = crate::game_features::ONLINE
    );
    match admin::db(pool, move |conn| {
        Ok(diesel::sql_query(sql)
            .bind::<Text, _>(login)
            .bind::<Nullable<Integer>, _>(params.cat.filter(|v| *v > 0))
            .bind::<Array<Text>, _>(regions)
            .bind::<Text, _>(params.query.unwrap_or_default().trim())
            .bind::<Text, _>(mode)
            .bind::<Text, _>(sort)
            .bind::<BigInt, _>(limit)
            .bind::<BigInt, _>(offset)
            .bind::<Bool, _>(own)
            .get_result::<Document>(conn)?)
    })
    .await
    {
        Ok(row) => HttpResponse::Ok().json(row.document),
        Err(e) => {
            log::error!("Library page: {e}");
            HttpResponse::InternalServerError().finish()
        }
    }
}
