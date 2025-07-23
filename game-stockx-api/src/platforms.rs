use actix_web::{get, web::Data, HttpResponse};
use diesel::{sql_types::{Integer, Text, Nullable}, QueryableByName, RunQueryDsl};
use serde::Serialize;
use crate::{DBPool};

#[derive(Debug, Serialize, QueryableByName)]
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

#[get("/platforms")]
pub async fn get_platforms(pool: Data<DBPool>) -> HttpResponse {
    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(e) => {
            log::error!("Failed to get DB connection: {}", e);
            return HttpResponse::InternalServerError()
                .body("Database connection error");
        }
    };

    let query = r#"
        SELECT 
            id, 
            abbreviation, 
            name, 
            generation, 
            total_games
        FROM 
            public.platforms 
        WHERE 
            active = true
        ORDER BY
            name ASC
    "#;

    match diesel::sql_query(query).load::<PlatformItem>(conn) {
        Ok(items) => HttpResponse::Ok().json(items),
        Err(e) => {
            log::error!("Database query error: {}", e);
            HttpResponse::InternalServerError().body("Failed to load platforms")
        }
    }
}