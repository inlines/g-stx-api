use diesel::prelude::*;
use actix_web::web::{self, Data};
use actix_web::{HttpResponse};
use diesel::sql_types::{Integer, Text, Nullable};
use diesel::{RunQueryDsl};
use serde::{Serialize};
use crate::constants::{ CONNECTION_POOL_ERROR};
use crate::{DBPool};


#[derive(Serialize, QueryableByName)]
struct PlatformItem {
  #[diesel(sql_type = Integer)]
  id: i32,

  #[diesel(sql_type = Text)]
  abbreviation: String,

  #[diesel(sql_type = Text)]
  name: String,

  #[diesel(sql_type = Nullable<Integer>)]
  generation: Option<i32>,

  #[diesel(sql_type = Integer)]
  total_games: i32,
}

#[get("/platforms")]
pub async fn get_platforms(pool: Data<DBPool>) -> HttpResponse {
  let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);
  let query = r#"
    SELECT id, abbreviation, name, generation, total_games
	  FROM public.platforms where active = true
  "#;

  let result = diesel::sql_query(query)
    .load::<PlatformItem>(conn);
  
  match result {
    Ok(items) => HttpResponse::Ok().json(items),
    Err(err) => {
      HttpResponse::InternalServerError().body(format!("DB error: {:?}", err))
    }
  }
}