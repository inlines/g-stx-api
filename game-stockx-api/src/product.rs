use diesel::prelude::*;
use actix_web::web::{self, Data, Path};
use actix_web::{HttpRequest, HttpResponse};
use diesel::sql_types::{Integer, Text, Nullable, BigInt, Bool, Array};
use diesel::{RunQueryDsl};
use serde::{Deserialize, Serialize};
use crate::constants::{ CONNECTION_POOL_ERROR};
use crate::{DBPool};
use crate::pagination::Pagination;
use actix_web::http::header;
use crate::auth::{verify_jwt};

#[derive(Debug, Deserialize, Serialize)]
pub struct Product {
    pub id: u32,
    pub cover: String,
    pub first_release_date: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Serialize, QueryableByName)]
pub struct ProductListItem {
    #[diesel(sql_type = Integer)]
    pub id: i32,

    #[diesel(sql_type = Text)]
    pub name: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub first_release_date: Option<i32>,

    #[diesel(sql_type = Nullable<Text>)]
    pub image_url: Option<String>,
}

#[derive(QueryableByName)]
pub struct CountResult {
    #[diesel(sql_type = BigInt)]
    pub total: i64,
}

#[derive(Serialize)]
pub struct ProductListResponse {
    items: Vec<ProductListItem>,
    total_count: i64,
}

#[get("/products")]
pub async fn list(pool: Data<DBPool>, query: web::Query<Pagination>) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);
    let cat = query.cat;
    let text_query = format!("%{}%", query.query.clone().unwrap_or_default());
    let ignore_digital = query.ignore_digital.unwrap_or(false);

    // === Основной SELECT ===
    let mut base_query = String::from(r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.first_release_date AS first_release_date,
            '//89.104.66.193/static/covers-thumb/' || cov.id || '.jpg' AS image_url
        FROM product_platforms AS pp
        INNER JOIN products AS prod ON pp.product_id = prod.id
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE pp.platform_id = $4 AND prod.name ILIKE $3
    "#);

    if ignore_digital {
        base_query.push_str(" AND pp.digital_only = false");
    }

    base_query.push_str(" ORDER BY prod.first_release_date ASC, prod.name ASC LIMIT $1 OFFSET $2");

    let results = diesel::sql_query(base_query)
        .bind::<diesel::sql_types::BigInt, _>(limit)
        .bind::<diesel::sql_types::BigInt, _>(offset)
        .bind::<diesel::sql_types::Text, _>(text_query.clone())
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<ProductListItem>(conn);

    // === COUNT SELECT ===
    let mut count_query = String::from(r#"
        SELECT COUNT(*) as total
        FROM product_platforms AS pp
        INNER JOIN products AS prod ON pp.product_id = prod.id
        WHERE pp.platform_id = $1 AND prod.name ILIKE $2
    "#);

    if ignore_digital {
        count_query.push_str(" AND pp.digital_only = false");
    }

    let count_result = diesel::sql_query(count_query)
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .bind::<diesel::sql_types::Text, _>(text_query.clone())
        .load::<CountResult>(conn);

    match (results, count_result) {
        (Ok(items), Ok(count)) => {
            let response = ProductListResponse {
                items,
                total_count: count.get(0).map(|c| c.total).unwrap_or(0),
            };
            HttpResponse::Ok().json(response)
        }
        (Err(err), _) | (_, Err(err)) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}


#[derive(Debug, Deserialize, Serialize, QueryableByName)]
pub struct ProductProperties {
    #[diesel(sql_type = Integer)]
    pub id: i32,

    #[diesel(sql_type = Text)]
    pub name: String,

    #[diesel(sql_type = Text)]
    pub summary: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub first_release_date: Option<i32>,

    #[diesel(sql_type = Nullable<Text>)]
    pub image_url: Option<String>,
}

#[derive(Debug, Serialize, QueryableByName)]
pub struct ProductReleaseInfo {
    #[diesel(sql_type = Integer)]
    pub release_id: i32,

    #[diesel(sql_type = Nullable<Integer>)]
    pub release_date: Option<i32>,

    #[diesel(sql_type = Text)]
    pub release_region: String,

    #[diesel(sql_type = Text)]
    pub platform_name: String,

    #[diesel(sql_type = Integer)]
    pub platform_id: i32,

    #[diesel(sql_type = Nullable<Integer>)]
    pub release_status: Option<i32>,

    #[diesel(sql_type = Array<Text>)]
    bid_user_logins: Vec<String>,

    #[diesel(sql_type = Bool)]
    pub digital_only: bool,

    #[diesel(sql_type = Nullable<Array<Text>>)]
    pub serial: Option<Vec<String>>

}

#[derive(QueryableByName)]
struct ScreenshotUrl {
    #[sql_type = "diesel::sql_types::Text"]
    image_url: String,
}


#[derive(Debug, Serialize)]
pub struct ProductResponse {
    pub product: ProductProperties,
    pub releases: Vec<ProductReleaseInfo>,
    pub screenshots: Vec<String>,
}

#[get("/products/{id}")]
pub async fn get(pool: Data<DBPool>, path: Path<(i64,)>, req: HttpRequest) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);
    let (product_id,) = path.into_inner();

    // Извлекаем токен и логин пользователя, если он есть
    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header_value| header_value.to_str().ok())
        .and_then(|header_str| {
            if header_str.starts_with("Bearer ") {
                Some(&header_str[7..])
            } else {
                None
            }
        });

    let user_login_opt = token.and_then(|t| verify_jwt(t)).map(|claims| claims.sub);

    let prod_query = r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.summary AS summary,
            prod.first_release_date AS first_release_date,
            '//89.104.66.193/static/covers-full/' || cov.id ||'.jpg' AS image_url
        FROM public.products AS prod
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE prod.id = $1
    "#;

    let prod_result = diesel::sql_query(prod_query)
        .bind::<diesel::sql_types::BigInt, _>(product_id)
        .load::<ProductProperties>(conn);

    match prod_result {
        Ok(mut items) => {
            if let Some(product) = items.pop() {
                let screenshot_query = r#"
                    SELECT image_url
                    FROM screenshots
                    WHERE game = $1
                "#;

                let screenshots_result = diesel::sql_query(screenshot_query)
                    .bind::<diesel::sql_types::BigInt, _>(product_id)
                    .load::<ScreenshotUrl>(conn);

                let release_query = r#"
                    SELECT
                        r.release_date AS release_date,
                        r.id AS release_id,
                        r.release_status AS release_status,
                        r.digital_only AS digital_only,
                        r.serial AS serial,
                        reg.name AS release_region,
                        p.name AS platform_name,
                        p.id AS platform_id,
                        COALESCE(ARRAY_AGG(uhb.user_login) FILTER (WHERE uhb.user_login IS NOT NULL), ARRAY[]::text[]) AS bid_user_logins
                    FROM releases AS r
                    LEFT JOIN platforms AS p ON r.platform = p.id
                    INNER JOIN regions AS reg ON reg.id = r.release_region
                    LEFT JOIN users_have_bids AS uhb ON uhb.release_id = r.id
                    WHERE r.product_id = $1 AND p.active = true
                    GROUP BY
                        r.id, r.release_date, r.release_status,
                        reg.name, p.name, p.id
                    ORDER BY p.name;
                "#;

                let release_result = diesel::sql_query(release_query)
                    .bind::<diesel::sql_types::BigInt, _>(product_id)
                    .load::<ProductReleaseInfo>(conn);

                match (release_result, screenshots_result) {
                    (Ok(mut releases), Ok(screenshot_urls)) => {
                        // Фильтрация bid_user_logins
                        match &user_login_opt {
                            Some(user_login) => {
                                for release in &mut releases {
                                    release
                                        .bid_user_logins
                                        .retain(|login| login != user_login);
                                }
                            }
                            None => {
                                for release in &mut releases {
                                    release.bid_user_logins.clear();
                                }
                            }
                        }

                        let screenshots = screenshot_urls.into_iter().map(|s| s.image_url).collect();

                        let response = ProductResponse {
                            product,
                            releases,
                            screenshots,
                        };
                        HttpResponse::Ok().json(response)
                    }
                    (Err(err), _) => {
                        eprintln!("Release query error: {:?}", err);
                        HttpResponse::InternalServerError().finish()
                    }
                    (_, Err(err)) => {
                        eprintln!("Screenshot query error: {:?}", err);
                        HttpResponse::InternalServerError().finish()
                    }
                }
            } else {
                HttpResponse::NotFound().body("Product not found")
            }
        }
        Err(err) => {
            eprintln!("Database error: {:?}", err);
            HttpResponse::InternalServerError().finish()
        }
    }
}
