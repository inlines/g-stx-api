use super::models::*;
use crate::pagination::Pagination;
use crate::{DBPool, constants::CONNECTION_POOL_ERROR};
use actix_web::web::Path;
use actix_web::{HttpRequest, HttpResponse, web};
use diesel::prelude::*;
use diesel::sql_types::Text;

#[get("/collection-stats")]
async fn get_collection_stats(pool: web::Data<DBPool>, req: HttpRequest) -> HttpResponse {
    // Извлечение токена из заголовка
    let claims = match crate::auth::authenticated_claims(&req) {
        Some(c) => c,
        None => return HttpResponse::Unauthorized().body("Invalid or missing token"),
    };

    let user_login = claims.sub;

    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let query = r#"
        SELECT
        COALESCE(h.platform, w.platform, s.platform) AS platform,

        COALESCE(h.release_count, 0) AS have_count,
        COALESCE(h.release_ids, ARRAY[]::int[]) AS have_ids,
        COALESCE(h.product_ids, ARRAY[]::int[]) AS have_prod_ids,
		

        COALESCE(w.release_count, 0) AS wish_count,
        COALESCE(w.release_ids, ARRAY[]::int[]) AS wish_ids,

        COALESCE(s.release_count, 0) AS wts_count,
        COALESCE(s.release_ids, ARRAY[]::int[]) AS wts_ids,
        COALESCE((
            SELECT SUM(uhr2.price)
            FROM users_have_releases uhr2
            WHERE uhr2.user_login = $1 
            AND uhr2.release_id = ANY(COALESCE(h.release_ids, ARRAY[]::int[]))
        ), 0) AS total_spent


        FROM
        (
            SELECT 
            r.platform,
            COUNT(uhr.release_id) AS release_count,
            ARRAY_AGG(uhr.release_id) AS release_ids,
            ARRAY_AGG(uhr.product_id) AS product_ids
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
            SELECT r.platform, COUNT(sale.release_id) AS release_count,
                   ARRAY_AGG(sale.release_id) AS release_ids
            FROM users_have_wts sale
            JOIN users_have_releases owned ON owned.release_id = sale.release_id AND owned.user_login = sale.user_login
            JOIN releases r ON r.id = sale.release_id
            WHERE sale.user_login = $1
            GROUP BY r.platform
        ) s ON COALESCE(h.platform, w.platform) = s.platform;
    "#;

    let result: Result<Vec<CollectionStats>, diesel::result::Error> = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .load::<CollectionStats>(conn);

    match result {
        Ok(items) => HttpResponse::Ok().json(items),
        Err(err) => HttpResponse::InternalServerError().body(format!("DB error: {:?}", err)),
    }
}

#[get("/collection")]
async fn get_collection(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    query: web::Query<Pagination>,
) -> HttpResponse {
    // Извлечение токена из заголовка
    let claims = match crate::auth::authenticated_claims(&req) {
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
            uhr.price,
            r.release_date,
            r.platform AS platform_id,
            r.release_region AS region_id,
            (r.digital_only OR EXISTS(SELECT 1 FROM product_platforms pp WHERE pp.product_id=r.product_id AND pp.platform_id=r.platform AND pp.digital_only)) AS digital_only,
            r.serial,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            ARRAY(SELECT a.name FROM alternative_names a WHERE a.product_id=prod.id AND a.name IS NOT NULL ORDER BY a.name) AS alternative_names,
            '//89.104.66.193/static/covers-thumb/' || cover.id ||'.jpg' AS image_url,
            reg.name AS region_name
        FROM public.users_have_releases AS uhr
        INNER JOIN releases AS r ON uhr.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        LEFT JOIN covers AS cover ON cover.id = prod.cover_id
        LEFT JOIN regions as reg on reg.id = r.release_region
        WHERE uhr.user_login = $1 AND p.id = $2
        ORDER BY prod.name, r.id
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
                total_count: count.first().map(|c| c.total).unwrap_or(0),
            };
            HttpResponse::Ok().json(response) // отправляем массив ProductListItem
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Query error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/collection-by-login/{login}")]
async fn get_collection_by_login(
    pool: web::Data<DBPool>,
    path: Path<String>,
    query: web::Query<Pagination>,
) -> HttpResponse {
    let login = path.into_inner();

    if login.is_empty() {
        return HttpResponse::BadRequest().json("Login cannot be empty");
    }

    // Изменяем на mut conn
    let mut conn = match pool.get() {
        Ok(conn) => conn,
        Err(_) => return HttpResponse::InternalServerError().json("Database connection error"),
    };

    //let cat = query.cat;
    let limit = query.limit.unwrap_or(100).min(1000);
    let offset = query.offset.unwrap_or(0);

    let query_text = r#"
        SELECT 
            uhr.release_id,
            r.release_date,
            r.platform AS platform_id,
            r.release_region AS region_id,
            (r.digital_only OR EXISTS(SELECT 1 FROM product_platforms pp WHERE pp.product_id=r.product_id AND pp.platform_id=r.platform AND pp.digital_only)) AS digital_only,
            r.serial,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            ARRAY(SELECT a.name FROM alternative_names a WHERE a.product_id=prod.id AND a.name IS NOT NULL ORDER BY a.name) AS alternative_names,
            '//89.104.66.193/static/covers-thumb/' || cover.id ||'.jpg' AS image_url,
            reg.name AS region_name,
            null AS price
        FROM public.users_have_releases AS uhr
        INNER JOIN releases AS r ON uhr.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        LEFT JOIN covers AS cover ON cover.id = prod.cover_id
        LEFT JOIN regions as reg on reg.id = r.release_region
        WHERE uhr.user_login = $1
        ORDER BY prod.name, r.id
        LIMIT $2 OFFSET $3
    "#;

    let result = diesel::sql_query(query_text)
        .bind::<Text, _>(&login)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .load::<CollectionItem>(&mut conn); // Используем &mut conn

    match result {
        Ok(items) => HttpResponse::Ok().json(items),
        Err(err) => {
            eprintln!("Query error: {:?}", err);
            HttpResponse::InternalServerError().body(format!("DB error: {:?}", err))
        }
    }
}

#[get("/wishlist")]
async fn get_wishlist(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    query: web::Query<Pagination>,
) -> HttpResponse {
    // Извлечение токена из заголовка
    let claims = match crate::auth::authenticated_claims(&req) {
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
            r.platform AS platform_id,
            r.release_region AS region_id,
            (r.digital_only OR EXISTS(SELECT 1 FROM product_platforms pp WHERE pp.product_id=r.product_id AND pp.platform_id=r.platform AND pp.digital_only)) AS digital_only,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            ARRAY(SELECT a.name FROM alternative_names a WHERE a.product_id=prod.id AND a.name IS NOT NULL ORDER BY a.name) AS alternative_names,
            '//89.104.66.193/static/covers-thumb/' || cover.id ||'.jpg' AS image_url,
            reg.name AS region_name,
            ARRAY[]::text[] AS serial,
            null as price
        FROM public.users_have_wishes AS uhw
        INNER JOIN releases AS r ON uhw.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        LEFT JOIN covers AS cover ON cover.id = prod.cover_id
        LEFT JOIN regions as reg on reg.id = r.release_region
        WHERE uhw.user_login = $1 AND p.id = $2
        ORDER BY prod.name, r.id
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
                total_count: count.first().map(|c| c.total).unwrap_or(0),
            };
            HttpResponse::Ok().json(response) // отправляем массив ProductListItem
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Query error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}

#[get("/wts")]
async fn get_wts(
    pool: web::Data<DBPool>,
    req: HttpRequest,
    query: web::Query<Pagination>,
) -> HttpResponse {
    // Извлечение токена из заголовка
    let claims = match crate::auth::authenticated_claims(&req) {
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
            uhwts.release_id,
            r.release_date,
            p.name as platform_name,
            prod.id as product_id,
            prod.name AS product_name,
            '//89.104.66.193/static/covers-thumb/' || cover.id ||'.jpg' AS image_url,
            reg.name AS region_name,
            ARRAY[]::text[] AS serial,
            uhwts.price,
            COALESCE(uhwts.cib, false) AS cib
        FROM public.users_have_wts AS uhwts
        INNER JOIN users_have_releases owned ON owned.release_id = uhwts.release_id AND owned.user_login = uhwts.user_login
        INNER JOIN releases AS r ON uhwts.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        INNER JOIN products AS prod ON r.product_id = prod.id
        INNER JOIN covers AS cover ON cover.id = prod.cover_id
        INNER JOIN regions as reg on reg.id = r.release_region 
        WHERE uhwts.user_login = $1 AND p.id = $2
        ORDER BY prod.name
        LIMIT $3 OFFSET $4
    "#;

    let result = diesel::sql_query(query)
        .bind::<Text, _>(&user_login)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .load::<WtsItem>(conn);

    let count_query = r#"
        SELECT COUNT(*) as total FROM public.users_have_wts AS uhwts
        INNER JOIN users_have_releases owned ON owned.release_id = uhwts.release_id AND owned.user_login = uhwts.user_login
        INNER JOIN releases AS r ON uhwts.release_id = r.id
        INNER JOIN platforms AS p ON r.platform = p.id
        WHERE uhwts.user_login = $1 AND p.id = $2
    "#;

    let count_result = diesel::sql_query(count_query)
        .bind::<Text, _>(&user_login)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<CountResult>(conn);

    match (result, count_result) {
        (Ok(items), Ok(count)) => {
            let response = WtsResponse {
                items,
                total_count: count.first().map(|c| c.total).unwrap_or(0),
            };
            HttpResponse::Ok().json(response) // отправляем массив ProductListItem
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Query error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}
