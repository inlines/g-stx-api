use actix_web::{HttpRequest, HttpResponse, web, get};
use crate::constants::CONNECTION_POOL_ERROR;
use crate::DBPool;
use crate::auth::verify_jwt;
use actix_web::http::header;
use diesel::prelude::*;
use diesel::sql_types::{Text, BigInt};
use serde::Serialize;

#[derive(QueryableByName, Serialize)]
struct Collector {
    #[diesel(sql_type = Text)]
    user_login: String,

    #[diesel(sql_type = BigInt)]
    release_count: i64,
}

#[get("/collectors")]
async fn get_collectors(pool: web::Data<DBPool>, req: HttpRequest) -> HttpResponse {
    let token = match req.headers().get(header::AUTHORIZATION) {
        Some(header_value) => {
            let header_str = header_value.to_str().unwrap_or("");
            if header_str.starts_with("Bearer ") {
                Some(&header_str[7..])
            } else {
                None
            }
        }
        None => None,
    };

    let claims = match token.and_then(|t| verify_jwt(t)) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let query = r#"
        SELECT 
            u.user_login,
            COUNT(uhr.release_id) AS release_count
        FROM 
            users u
        INNER JOIN 
            users_have_releases uhr ON u.user_login = uhr.user_login
        WHERE u.user_login <> $1 
        GROUP BY 
            u.user_login
        HAVING 
            COUNT(uhr.release_id) > 0
        ORDER BY 
            release_count DESC;
    "#;

    let result = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .load::<Collector>(conn);

    match result {
        Ok(collectors) => {
            // Сериализуем результат в JSON
            HttpResponse::Ok().json(collectors)
        }
        Err(err) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().body("Database error")
        }
    }
}