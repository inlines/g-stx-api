use actix_web::web::{Json, Path};
use actix_web::HttpResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::APPLICATION_JSON;
use crate::response::Response;

pub type Products = Response<Product>;

#[derive(Debug, Deserialize, Serialize)]
pub struct Product {
    pub id: u32,
    pub cover: String,
    pub first_release_date: String,
    pub name: String,
}

/// list 50 last products `/products`
#[get("/products")]
pub async fn list() -> HttpResponse {
    // TODO find the last 50 products and return them

    let products = Products { results: vec![] };

    HttpResponse::Ok()
        .content_type(APPLICATION_JSON)
        .json(products)
}

/// create a product `/products`
// #[post("/products")]
// pub async fn create(product_req: Json<ProductRequest>) -> HttpResponse {
//     HttpResponse::Created()
//         .content_type(APPLICATION_JSON)
//         .json(product_req.to_product())
// }

/// find a product by its id `/products/{id}`
#[get("/products/{id}")]
pub async fn get(path: Path<(String,)>) -> HttpResponse {
    // TODO find product a product by ID and return it
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
