use crate::DBPool;
use actix_web::{HttpResponse, web};
use diesel::prelude::*;
use diesel::sql_types::{Integer, Nullable, Text};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, QueryableByName, Serialize, Deserialize)]
pub struct Genre {
    #[diesel(sql_type = Integer)]
    pub id: i32,
    #[diesel(sql_type = Text)]
    pub name: String,
}

pub async fn load(pool: web::Data<DBPool>, product_id: Option<i32>) -> Result<Vec<Genre>, ()> {
    web::block(move || {
        let mut conn = pool.get().map_err(|e| { log::error!("Genres connection: {e}"); })?;
        diesel::sql_query("SELECT g.id,g.name FROM genres g WHERE $1::integer IS NULL OR EXISTS (SELECT 1 FROM product_genres pg WHERE pg.genre_id=g.id AND pg.product_id=$1) ORDER BY g.name,g.id")
            .bind::<Nullable<Integer>, _>(product_id)
            .load::<Genre>(&mut conn).map_err(|e| { log::error!("Genres query: {e}"); })
    }).await.map_err(|e| { log::error!("Genres task: {e}"); })?
}

#[get("/genres")]
pub async fn list(pool: web::Data<DBPool>) -> HttpResponse {
    match load(pool, None).await {
        Ok(genres) => HttpResponse::Ok().json(genres),
        Err(_) => HttpResponse::InternalServerError().finish(),
    }
}
