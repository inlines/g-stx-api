use actix_web::{post, HttpRequest, HttpResponse, web};
use crate::constants::{CONNECTION_POOL_ERROR};
use crate::{DBPool};
use crate::auth::{verify_jwt};
use diesel::prelude::*;
use diesel::sql_types::{Text, Integer, Nullable, BigInt};
use actix_web::http::header;
use serde::{Deserialize, Serialize};
use crate::pagination::Pagination;

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

    #[diesel(sql_type = Text)]
    platform_name: String,

    #[diesel(sql_type = Text)]
    product_name: String,

    #[diesel(sql_type = Integer)]
    product_id: i32,

    #[diesel(sql_type = Nullable<Text>)]
    image_url: Option<String>,

    #[diesel(sql_type = Nullable<Text>)]
    region_name: Option<String>,
}

#[derive(Serialize)]
pub struct CollectionResponse {
    items: Vec<CollectionItem>,
    total_count: i64,
}

#[derive(QueryableByName)]
pub struct CountResult {
    #[diesel(sql_type = BigInt)]
    pub total: i64,
}


#[derive(Serialize, QueryableByName)]
struct CollectionStats {
    #[sql_type = "diesel::sql_types::Integer"]
    platform: i32,

    #[sql_type = "diesel::sql_types::BigInt"]
    have_count: i64,
    
    #[sql_type = "diesel::sql_types::BigInt"]
    have_games: i64,

    #[sql_type = "diesel::sql_types::Array<diesel::sql_types::Integer>"]
    have_ids: Vec<i32>,

    #[sql_type = "diesel::sql_types::BigInt"]
    wish_count: i64,

    #[sql_type = "diesel::sql_types::Array<diesel::sql_types::Integer>"]
    wish_ids: Vec<i32>,

    #[sql_type = "diesel::sql_types::BigInt"]
    bid_count: i64,

    #[sql_type = "diesel::sql_types::Array<diesel::sql_types::Integer>"]
    bid_ids: Vec<i32>,
}


#[get("/collection-stats")]
async fn get_collection_stats(pool: web::Data<DBPool>, req: HttpRequest) -> HttpResponse {
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
        COALESCE(h.platform, w.platform, b.platform) AS platform,

        COALESCE(h.release_count, 0) AS have_count,
        COALESCE(h.release_ids, ARRAY[]::int[]) AS have_ids,

		(
	        SELECT COUNT(DISTINCT r.product_id)
	        FROM unnest(COALESCE(h.release_ids, ARRAY[]::int[])) AS rel_id
	        JOIN releases r ON r.id = rel_id
	    ) AS have_games,

        COALESCE(w.release_count, 0) AS wish_count,
        COALESCE(w.release_ids, ARRAY[]::int[]) AS wish_ids,

        COALESCE(b.release_count, 0) AS bid_count,
        COALESCE(b.release_ids, ARRAY[]::int[]) AS bid_ids

        FROM
        (
            SELECT 
            r.platform,
            COUNT(uhr.release_id) AS release_count,
            ARRAY_AGG(uhr.release_id) AS release_ids
            FROM users_have_releases AS uhr
            JOIN releases AS r ON uhr.release_id = r.id
            WHERE uhr.user_login = $1
            GROUP BY r.platform
        ) h

        FULL OUTER JOIN (
            SELECT 
            r.platform,
            COUNT(uhw.release_id) AS release_count,
            ARRAY_AGG(uhw.release_id) AS release_ids
            FROM users_have_wishes AS uhw
            JOIN releases AS r ON uhw.release_id = r.id
            WHERE uhw.user_login = $1
            GROUP BY r.platform
        ) w ON h.platform = w.platform

        FULL OUTER JOIN (
            SELECT 
            r.platform,
            COUNT(uhb.release_id) AS release_count,
            ARRAY_AGG(uhb.release_id) AS release_ids
            FROM users_have_bids AS uhb
            JOIN releases AS r ON uhb.release_id = r.id
            WHERE uhb.user_login = $1
            GROUP BY r.platform
        ) b ON COALESCE(h.platform, w.platform) = b.platform;
    "#;

    let result = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .load::<CollectionStats>(conn);

    match result {
        Ok(items) => HttpResponse::Ok().json(items),
        Err(err) => {
          HttpResponse::InternalServerError().body(format!("DB error: {:?}", err))
        }
    }
}

#[get("/collection")]
async fn get_collection(pool: web::Data<DBPool>, req: HttpRequest, query: web::Query<Pagination>) -> HttpResponse {
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
    let cat = query.cat;
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let query = r#"
        SELECT 
            uhr.release_id,
            r.release_date,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            '//89.104.66.193/static/covers-thumb/' || cover.id ||'.jpg' AS image_url,
            reg.name AS region_name
        FROM public.users_have_releases AS uhr
        INNER JOIN releases AS r ON uhr.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        INNER JOIN covers AS cover ON cover.id = prod.cover_id
        INNER JOIN regions as reg on reg.id = r.release_region 
        WHERE uhr.user_login = $1 AND p.id = $2
        ORDER BY prod.name
        LIMIT $3 OFFSET $4
    "#;

    let result = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .load::<CollectionItem>(conn);

    let count_query = r#"
        SELECT COUNT(*) as total FROM public.users_have_releases AS uhr
        INNER JOIN releases AS r ON uhr.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        WHERE uhr.user_login = $1 AND p.id = $2
    "#;

    let count_result = diesel::sql_query(count_query)
        .bind::<Text, _>(&user_login)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<CountResult>(conn);

    match (result, count_result) {
        (Ok(items), Ok(count)) => {
            let response = CollectionResponse {
                items,
                total_count: count.get(0).map(|c| c.total).unwrap_or(0),
            };
            HttpResponse::Ok()
                .json(response) // отправляем массив ProductListItem
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Query error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/wishlist")]
async fn get_wishlist(pool: web::Data<DBPool>, req: HttpRequest, query: web::Query<Pagination>) -> HttpResponse {
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
    let cat = query.cat;
    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let query = r#"
        SELECT 
            uhw.release_id,
            r.release_date,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            '//89.104.66.193/static/covers-thumb/' || cover.id ||'.jpg' AS image_url,
            reg.name AS region_name
        FROM public.users_have_wishes AS uhw
        INNER JOIN releases AS r ON uhw.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        INNER JOIN covers AS cover ON cover.id = prod.cover_id
        INNER JOIN regions as reg on reg.id = r.release_region 
        WHERE uhw.user_login = $1 AND p.id = $2
        ORDER BY prod.name
        LIMIT $3 OFFSET $4
    "#;

    let result = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .load::<CollectionItem>(conn);

    let count_query = r#"
        SELECT COUNT(*) as total FROM public.users_have_wishes AS uhw
        INNER JOIN releases AS r ON uhw.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        WHERE uhw.user_login = $1 AND p.id = $2
    "#;

    let count_result = diesel::sql_query(count_query)
        .bind::<Text, _>(&user_login)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<CountResult>(conn);

    match (result, count_result) {
        (Ok(items), Ok(count)) => {
            let response = CollectionResponse {
                items,
                total_count: count.get(0).map(|c| c.total).unwrap_or(0),
            };
            HttpResponse::Ok()
                .json(response) // отправляем массив ProductListItem
        }
        (Err(err), _) | (_, Err(err)) => {
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
        .execute(conn);

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
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
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
        INSERT INTO users_have_wishes (release_id, user_login)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
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
        DELETE FROM users_have_wishes
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
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
        INSERT INTO users_have_bids (release_id, user_login)
        VALUES ($1, $2)
        ON CONFLICT DO NOTHING
    "#;

    let result = diesel::sql_query(insert_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
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
        DELETE FROM users_have_bids
        WHERE release_id = $1 AND user_login = $2
    "#;

    let result = diesel::sql_query(delete_query)
        .bind::<Integer, _>(data.release_id)
        .bind::<Text, _>(&user_login)
        .execute(conn);

    match result {
        Ok(_) => HttpResponse::Ok().body({}),
        Err(err) => {
            eprintln!("Delete error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}