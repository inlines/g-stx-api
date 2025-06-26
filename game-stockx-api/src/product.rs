use diesel::prelude::*;
use actix_web::web::{self, Data, Path};
use actix_web::HttpResponse;
use diesel::sql_types::{Integer, Text, Nullable, BigInt};
use diesel::{RunQueryDsl};
use serde::{Deserialize, Serialize};
use crate::constants::{ CONNECTION_POOL_ERROR};
use crate::{DBPool};
use crate::pagination::Pagination;


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

    let query = r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.first_release_date AS first_release_date,
            cov.image_url AS image_url
            FROM product_platforms AS pp
            INNER JOIN products as prod ON pp.product_id = prod.id
            LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        WHERE pp.platform_id = $4 AND prod.name ILIKE $3
        ORDER BY prod.first_release_date ASC
        LIMIT $1 OFFSET $2
    "#;

    let results = diesel::sql_query(query)
        .bind::<diesel::sql_types::BigInt, _>(limit) // Привязываем параметр LIMIT
        .bind::<diesel::sql_types::BigInt, _>(offset) // Привязываем параметр OFFSET
        .bind::<diesel::sql_types::Text, _>(text_query.clone())
        .bind::<diesel::sql_types::BigInt, _>(cat)
        .load::<ProductListItem>(conn);

    let count_query = r#"
        SELECT COUNT(*) as total
        FROM product_platforms AS pp
        INNER JOIN products as prod ON pp.product_id = prod.id
        WHERE pp.platform_id = $1 AND prod.name ILIKE $2
    "#;

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
            // Возвращаем полученные данные как JSON
            HttpResponse::Ok()
                .json(response) // отправляем массив ProductListItem
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

    #[diesel(sql_type = Nullable<Integer>)]
    pub platform_generation: Option<i32>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub release_status: Option<i32>,
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
pub async fn get(pool: Data<DBPool>, path: Path<(i64,)>) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let (product_id,) = path.into_inner();

    let prod_query = r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.summary AS summary,
            prod.first_release_date AS first_release_date,
            cov.image_url AS image_url
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
                    .load::<ScreenshotUrl>(&mut *conn);


                 let release_query = r#"
                    SELECT
                        r.release_date AS release_date,
                        r.id AS release_id,
                        r.release_status AS release_status,
                        reg.name AS release_region,
                        p.name AS platform_name,
                        p.generation AS platform_generation
                    FROM releases as r
                    LEFT JOIN platforms as p ON r.platform = p.id
                    INNER JOIN regions as reg ON reg.id = r.release_region
                    where r.product_id = $1
                    ORDER BY r.release_date
                "#;

                let release_result = diesel::sql_query(release_query)
                    .bind::<diesel::sql_types::BigInt, _>(product_id)
                    .load::<ProductReleaseInfo>(conn);

                match (release_result, screenshots_result) {
                    (Ok(releases), Ok(screenshot_urls)) => {
                        let screenshots = screenshot_urls.into_iter().map(|s| s.image_url).collect();
                        let response = ProductResponse {
                            product,
                            releases,
                            screenshots,
                        };
                        HttpResponse::Ok()
                            .json(response)
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
