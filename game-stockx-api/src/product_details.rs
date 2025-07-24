use diesel::prelude::*;
use actix_web::web::{Data, Path};
use actix_web::{HttpRequest, HttpResponse};
use diesel::sql_types::{Integer, Text, Nullable, Bool, Array};
use serde::{Deserialize, Serialize};
use crate::constants::CONNECTION_POOL_ERROR;
use actix_web::http::header;
use crate::auth::verify_jwt;
use crate::DBPool;

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