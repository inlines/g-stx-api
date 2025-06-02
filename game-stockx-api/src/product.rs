use diesel::prelude::*;
use actix_web::web::{self, Data, Json, Path};
use actix_web::HttpResponse;
use diesel::sql_types::{Integer, Numeric, Text, Nullable};
use diesel::{sql_query, RunQueryDsl};
use serde::{Deserialize, Serialize};
use serde::ser::{Serializer};
use bigdecimal::BigDecimal;

use crate::constants::{APPLICATION_JSON, CONNECTION_POOL_ERROR};
use crate::response::Response;
use crate::{DBPool, DBPooledConnection};
use crate::pagination::Pagination;

pub type Products = Response<Product>;

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

    #[diesel(sql_type = Text)]
    pub summary: String,

    #[diesel(sql_type = Nullable<Integer>)]
    pub first_release_date: Option<i32>,

    #[diesel(sql_type = Nullable<Text>)]
    pub image_url: Option<String>,
}

/// list 50 last products `/products`
#[get("/products")]
pub async fn list(pool: Data<DBPool>, query: web::Query<Pagination>) -> HttpResponse {
    let conn = &mut pool.get().expect(CONNECTION_POOL_ERROR);

    let limit = query.limit.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let query = r#"
        SELECT 
            prod.id AS id,
            prod.name AS name,
            prod.summary AS summary,
            prod.first_release_date AS first_release_date,
            cov.image_url AS image_url
        FROM public.products AS prod
        LEFT JOIN covers AS cov ON prod.cover_id = cov.id
        ORDER BY prod.first_release_date DESC
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

/// find a product by its id `/products/{id}`
#[get("/products/{id}")]
pub async fn get(path: Path<(String,)>) -> HttpResponse {
    let found_product: Option<Product> = None;

    match found_product {
        Some(product) => HttpResponse::Ok()
            .content_type(APPLICATION_JSON)
            .json(product),
        None => HttpResponse::NoContent()
            .content_type(APPLICATION_JSON)
            .await
            .unwrap(),
    }
}
