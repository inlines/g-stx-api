use super::models::*;
use crate::metrics::{SUCCESSFUL_ADD_TO_WISHLIST, SUCCESSFUL_ADD_TO_WTS};
use crate::{DBPool, constants::CONNECTION_POOL_ERROR};
use actix_web::{HttpRequest, HttpResponse, web};
use diesel::prelude::*;
use diesel::sql_types::{Bool, Integer, Nullable, Text};

#[post("/add_wts")]
async fn add_wts(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    if data.price.is_some_and(|price| price < 0) {
        return HttpResponse::BadRequest().body("Sale price must be non-negative");
    }
    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    // Lock the owned release so adding a sale and removing ownership cannot race.
    let insert_query = r#"
        WITH owned AS (
            SELECT release_id, user_login FROM users_have_releases
            WHERE release_id = $1 AND user_login = $2
            FOR UPDATE
        ), inserted AS (
            INSERT INTO users_have_wts (release_id, user_login, price, cib)
            SELECT release_id, user_login, $3, $4 FROM owned
            ON CONFLICT (release_id, user_login) DO UPDATE
            SET price = EXCLUDED.price, cib = EXCLUDED.cib
            RETURNING release_id
        )
        SELECT COUNT(*) AS total FROM owned
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .bind::<Nullable<Integer>, _>(data.price)
        .bind::<Bool, _>(data.cib.unwrap_or(false))
        .get_result::<CountResult>(conn);

    match result {
        Ok(count) if count.total > 0 => {
            SUCCESSFUL_ADD_TO_WTS.inc();
            HttpResponse::Ok().finish()
        }
        Ok(_) => HttpResponse::Forbidden().body("Only owned releases can be offered for sale"),
        Err(err) => {
            eprintln!("Insert WTS error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/remove_wts")]
async fn remove_wts(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let delete_query = r#"
        DELETE FROM users_have_wts
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(err) => {
            eprintln!("Delete error: {:?}", err);
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
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let insert_query = r#"
        INSERT INTO users_have_releases (release_id, user_login, price, product_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .bind::<Nullable<Integer>, _>(data.price)
        .bind::<Nullable<Integer>, _>(data.product_id)
        .execute(conn);

    match result {
        Ok(_) => {
            SUCCESSFUL_ADD_TO_WTS.inc();
            HttpResponse::Ok().body(())
        }
        Err(err) => {
            eprintln!("Insert error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/set_release_price")]
async fn set_release_price(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let insert_query = r#"
        UPDATE users_have_releases
        SET price = $3
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .bind::<Nullable<Integer>, _>(data.price)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body(()),
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
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let delete_query = r#"
        WITH removed AS (
            DELETE FROM users_have_releases
            WHERE release_id = $1 AND user_login = $2
            RETURNING release_id, user_login
        )
        DELETE FROM users_have_wts sale USING removed
        WHERE sale.release_id = removed.release_id AND sale.user_login = removed.user_login
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(err) => {
            eprintln!("Delete error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/add_wish")]
async fn add_wish(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let insert_query = r#"
        INSERT INTO users_have_wishes (release_id, user_login)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => {
            SUCCESSFUL_ADD_TO_WISHLIST.inc();
            HttpResponse::Ok().body(())
        }
        Err(err) => {
            eprintln!("Insert error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/remove_wish")]
async fn remove_wish(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let delete_query = r#"
        DELETE FROM users_have_wishes
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(err) => {
            eprintln!("Delete error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/add_bid")]
async fn add_bid(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let insert_query = r#"
        INSERT INTO users_have_bids (release_id, user_login)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(err) => {
            eprintln!("Insert error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[post("/remove_bid")]
async fn remove_bid(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    data: web::Json<TrackReleaseRequest>,
) -> HttpResponse {
    // Проверка токена
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let delete_query = r#"
        DELETE FROM users_have_bids
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body(()),
        Err(err) => {
            eprintln!("Delete error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}
