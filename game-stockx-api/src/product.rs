use diesel::prelude::*;
use actix_web::web::{self, Data, Path};
use actix_web::HttpResponse;
use diesel::sql_types::{Integer, Text, Nullable};
use diesel::{RunQueryDsl};
use serde::{Deserialize, Serialize};
use actix_web::http::header;
use crate::constants::{APPLICATION_JSON, CONNECTION_POOL_ERROR};
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
#[get("/products")]
pub async fn list(pool: Data<DBPool>, query: web::Query<Pagination>) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let query = r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.first_release_date AS first_release_date,
            cov.image_url AS image_url
        FROM public.products AS prod
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        ORDER BY prod.first_release_date ASC
        LIMIT $1 OFFSET $2
    "#;

    let results = diesel::sql_query(query)
        .bind::<diesel::sql_types::BigInt, _>(limit) // Привязываем параметр LIMIT
        .bind::<diesel::sql_types::BigInt, _>(offset) // Привязываем параметр OFFSET
        .load::<ProductListItem>(conn);

    match results {
        Ok(items) => {
            // Возвращаем полученные данные как JSON
            HttpResponse::Ok()
                .insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
                .content_type(APPLICATION_JSON)
                .json(items) // отправляем массив ProductListItem
        }
        Err(err) => {
            // Логирование и возврат ошибки
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
    pub release_date: i32,

    #[diesel(sql_type = Integer)]
    pub release_region: i32,

    #[diesel(sql_type = Text)]
    pub platform_name: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub platform_generation: Option<i32>
}

#[derive(Debug, Serialize)]
pub struct ProductResponse {
    pub product: ProductProperties,
    pub releases: Vec<ProductReleaseInfo>,
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
                 let release_query = r#"
                    SELECT
                        r.release_date AS release_date,
                        r.release_region AS release_region,
                        p.name AS platform_name,
                        p.generation AS platform_generation
                    FROM releases as r
                    LEFT JOIN platforms as p ON r.platform = p.id
                    where r.product_id = $1
                "#;

                let release_result = diesel::sql_query(release_query)
                    .bind::<diesel::sql_types::BigInt, _>(product_id)
                    .load::<ProductReleaseInfo>(conn);

                match release_result {
                    Ok(releases) => {
                        let response = ProductResponse {
                            product,
                            releases,
                        };
                        HttpResponse::Ok()
                            .insert_header((header::ACCESS_CONTROL_ALLOW_ORIGIN, "*"))
                            .content_type(APPLICATION_JSON)
                            .json(response)
                    }
                    Err(err) => {
                        eprintln!("Release query error: {:?}", err);
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
