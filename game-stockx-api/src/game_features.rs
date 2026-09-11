use crate::{
    DBPool,
    admin::{self, AdminError},
};
use actix_web::web::Data;
use diesel::{
    prelude::*,
    sql_types::{Bool, Integer, Nullable, Text},
};
use serde::{Deserialize, Serialize};

// Unknown player counts stay null; boolean support alone does not invent a maximum.
pub const LOCAL: &str =
    "(GREATEST(m.offlinemax,m.offlinecoopmax)>1 OR m.offlinecoop=true OR m.splitscreen=true)";
pub const ONLINE: &str =
    "(GREATEST(m.onlinemax,m.onlinecoopmax)>1 OR m.onlinecoop=true OR m.splitscreenonline=true)";

#[derive(Debug, Clone, QueryableByName, Serialize, Deserialize)]
pub struct MultiplayerMode {
    #[diesel(sql_type = Nullable<Integer>)]
    pub platform_id: Option<i32>,
    #[diesel(sql_type = Nullable<Text>)]
    pub platform_name: Option<String>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub local_players: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub online_players: Option<i32>,
    #[diesel(sql_type = Nullable<Bool>)]
    pub local_multiplayer: Option<bool>,
    #[diesel(sql_type = Nullable<Bool>)]
    pub online_multiplayer: Option<bool>,
    #[diesel(sql_type = Nullable<Bool>)]
    pub offline_coop: Option<bool>,
    #[diesel(sql_type = Nullable<Bool>)]
    pub online_coop: Option<bool>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub offline_coop_players: Option<i32>,
    #[diesel(sql_type = Nullable<Integer>)]
    pub online_coop_players: Option<i32>,
}
#[derive(Debug, Clone, QueryableByName, Serialize, Deserialize)]
pub struct SimilarGame {
    #[diesel(sql_type = Integer)]
    pub id: i32,
    #[diesel(sql_type = Text)]
    pub name: String,
    #[diesel(sql_type = Nullable<Text>)]
    pub image_url: Option<String>,
    #[diesel(sql_type = diesel::sql_types::Array<Integer>)]
    pub platform_ids: Vec<i32>,
}
pub async fn load(
    pool: Data<DBPool>,
    id: i32,
) -> Result<(Vec<MultiplayerMode>, Vec<SimilarGame>), AdminError> {
    admin::db(pool, move |conn| {
  let modes = diesel::sql_query(format!("SELECT m.platform AS platform_id, p.name AS platform_name, NULLIF(MAX(GREATEST(m.offlinemax,m.offlinecoopmax)),0) AS local_players, NULLIF(MAX(GREATEST(m.onlinemax,m.onlinecoopmax)),0) AS online_players, bool_or({LOCAL}) AS local_multiplayer, bool_or({ONLINE}) AS online_multiplayer, bool_or(m.offlinecoop) AS offline_coop, bool_or(m.onlinecoop) AS online_coop, NULLIF(MAX(m.offlinecoopmax),0) AS offline_coop_players, NULLIF(MAX(m.onlinecoopmax),0) AS online_coop_players FROM product_multiplayer_modes m LEFT JOIN platforms p ON p.id=m.platform WHERE m.game=$1 GROUP BY m.platform,p.name ORDER BY m.platform NULLS LAST"))
   .bind::<Integer,_>(id).load::<MultiplayerMode>(conn)?;
  let similar = diesel::sql_query("SELECT p.id,p.name, CASE WHEN p.cover_id IS NOT NULL THEN '//89.104.66.193/static/covers-full/'||p.cover_id||'.jpg' ELSE NULL END AS image_url, ARRAY(SELECT pp.platform_id FROM product_platforms pp WHERE pp.product_id=p.id ORDER BY pp.platform_id) AS platform_ids FROM products source CROSS JOIN LATERAL unnest(source.similar_game_ids) WITH ORDINALITY s(id,position) JOIN products p ON p.id=s.id WHERE source.id=$1 AND p.id<>source.id ORDER BY s.position LIMIT 30")
   .bind::<Integer,_>(id).load::<SimilarGame>(conn)?;
  Ok((modes,similar))
 }).await
}
