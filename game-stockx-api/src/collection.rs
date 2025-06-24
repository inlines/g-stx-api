use actix_web::{post, HttpRequest, HttpResponse, web};
use crate::constants::{CONNECTION_POOL_ERROR};
use crate::{DBPool};
use crate::auth::{verify_jwt};
use diesel::prelude::*;
use diesel::sql_types::{Text, Integer, Nullable};
use actix_web::http::header;
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct TrackReleaseRequest {
    release_id: i32,
}

#[derive(Serialize, QueryableByName)]
struct CollectionItem {
    #[diesel(sql_type = Integer)]
    release_id: i32,

    #[diesel(sql_type = Nullable<Integer>)]
    release_date: Option<i32>,

    #[sql_type = "diesel::sql_types::Text"]
    platform_name: String,

    #[sql_type = "diesel::sql_types::Text"]
    product_name: String,

    #[diesel(sql_type = Integer)]
    product_id: i32,

    #[sql_type = "diesel::sql_types::Nullable<diesel::sql_types::Text>"]
    image_url: Option<String>,

    #[sql_type = "diesel::sql_types::Nullable<diesel::sql_types::Text>"]
    region_name: Option<String>,
}

#[get("/collection")]
async fn get_collection(pool: web::Data<DBPool>, req: HttpRequest) -> HttpResponse {
    // Извлечение токена из заголовка
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
            uhr.release_id,
            r.release_date,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            cover.image_url,
            reg.name AS region_name
        FROM public.users_have_releases AS uhr
        INNER JOIN releases AS r ON uhr.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        INNER JOIN covers AS cover ON cover.id = prod.cover_id
        INNER JOIN regions as reg on reg.id = r.release_region 
        WHERE uhr.user_login = $1
    "#;

    let result = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .load::<CollectionItem>(&mut *conn);

    match result {
        Ok(items) => HttpResponse::Ok().json(items),
        Err(err) => {
            eprintln!("Query error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}


#[post("/add_release")]
async fn add_release(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
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

    let insert_query = r#"
        INSERT INTO users_have_releases (release_id, user_login)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(&mut *conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
        Err(err) => {
            eprintln!("Insert error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/remove_release")]
async fn remove_release(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
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

    let delete_query = r#"
        DELETE FROM users_have_releases
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(&mut *conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
        Err(err) => {
            eprintln!("Delete error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}