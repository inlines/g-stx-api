use super::models::*;
use crate::metrics::{SUCCESSFUL_ADD_TO_COLLECTION, SUCCESSFUL_ADD_TO_WISHLIST, WTS_SAVES};
use crate::{DBPool, constants::CONNECTION_POOL_ERROR};
use actix_web::{HttpRequest, HttpResponse, web};
use diesel::prelude::*;
use diesel::sql_types::{Bool, Integer, Nullable, Text};

fn editable_release(conn: &mut diesel::PgConnection, id: i32) -> Result<(), HttpResponse> {
    #[derive(QueryableByName)]
    struct Platform {
        #[diesel(sql_type = Integer)]
        platform: i32,
    }
    match diesel::sql_query("SELECT platform FROM releases WHERE id=$1")
        .bind::<Integer, _>(id)
        .get_result::<Platform>(conn)
        .optional()
    {
        Ok(Some(row)) if [8, 9, 48, 167, 38].contains(&row.platform) => Ok(()),
        Ok(Some(_)) => Err(HttpResponse::BadRequest().body("Unsupported platform")),
        Ok(None) => Err(HttpResponse::NotFound().body("Release not found")),
        Err(e) => {
            log::error!("Release validation failed: {e}");
            Err(HttpResponse::InternalServerError().finish())
        }
    }
}

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
    if let Err(response) = editable_release(conn, data.release_id) {
        return response;
    }

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
            WTS_SAVES.inc();
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

    if data.price.is_some_and(|price| price < 0) {
        return HttpResponse::BadRequest().body("Purchase price must be non-negative");
    }

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);
    if let Err(response) = editable_release(conn, data.release_id) {
        return response;
    }

    let insert_query = r#"
        INSERT INTO users_have_releases (release_id, user_login, price, product_id)
        SELECT id, $2, $3, product_id FROM releases WHERE id=$1
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .bind::<Nullable<Integer>, _>(data.price)
        .execute(conn);

    match result {
        Ok(inserted) => {
            SUCCESSFUL_ADD_TO_COLLECTION.inc_by(inserted as f64);
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

    if data.price.is_some_and(|price| price < 0) {
        return HttpResponse::BadRequest().body("Purchase price must be non-negative");
    }

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
    if let Err(response) = editable_release(conn, data.release_id) {
        return response;
    }

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
        Ok(inserted) => {
            SUCCESSFUL_ADD_TO_WISHLIST.inc_by(inserted as f64);
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
