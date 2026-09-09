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
#[derive(serde::Deserialize)]
struct CollectorWtsQuery {
    limit: Option<i64>,
    offset: Option<i64>,
}

#[get("/collectors/{login}/wts")]
async fn get_collector_wts(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    login: web::Path<String>,
    pagination: web::Query<CollectorWtsQuery>,
) -> HttpResponse {
    let claims = req.headers().get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .and_then(verify_jwt);
    if claims.is_none() {
        return HttpResponse::Unauthorized().finish();
    }
    let limit = pagination.limit.unwrap_or(100);
    let offset = pagination.offset.unwrap_or(0);
    if !(1..=1000).contains(&limit) || offset < 0 {
        return HttpResponse::BadRequest().body("Invalid pagination");
    }
    let conn = &mut match pool.get() {
        Ok(conn) => conn,
        Err(err) => {
            log::error!("Collector WTS connection error: {}", err);
            return HttpResponse::InternalServerError().finish();
        }
    };
    let query = r#"
        SELECT sale.release_id, r.release_date, p.name AS platform_name,
            prod.id AS product_id, prod.name AS product_name,
            '//89.104.66.193/static/covers-thumb/' || cover.id || '.jpg' AS image_url,
            reg.name AS region_name, ARRAY[]::text[] AS serial,
            sale.price, COALESCE(sale.cib, false) AS cib
        FROM users_have_wts sale
        INNER JOIN users_have_releases owned
            ON owned.release_id = sale.release_id AND owned.user_login = sale.user_login
        INNER JOIN releases r ON r.id = sale.release_id
        INNER JOIN products prod ON prod.id = r.product_id
        INNER JOIN platforms p ON p.id = r.platform
        LEFT JOIN covers cover ON cover.id = prod.cover_id
        LEFT JOIN regions reg ON reg.id = r.release_region
        WHERE sale.user_login = $1
        ORDER BY prod.name, sale.release_id
        LIMIT $2 OFFSET $3
    "#;
    match diesel::sql_query(query)
        .bind::<Text, _>(login.into_inner())
        .bind::<BigInt, _>(limit)
        .bind::<BigInt, _>(offset)
        .load::<crate::collection::WtsItem>(conn)
    {
        Ok(items) => HttpResponse::Ok().json(items),
        Err(err) => {
            log::error!("Collector WTS query error: {}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}
