use actix_web::web::{Json, Path};
use actix_web::HttpResponse;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::constants::APPLICATION_JSON;
use crate::response::Response;

pub type Sales = Response<Sale>;

#[derive(Debug, Deserialize, Serialize)]
pub struct Sale {
    pub id: String,
    pub created_at: DateTime<Utc>,
    sum: i32,
}

impl Sale {
    pub fn new(sum: i32) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            created_at: Utc::now(),
            sum: sum,
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct SaleRequest {
    pub sum: Option<i32>,
}

impl SaleRequest {
    pub fn to_sale(&self) -> Option<Sale> {
        match &self.sum {
            Some(sum) => Some(Sale::new(*sum)),
            None => None,
        }
    }
}



/// list last 50 likes from a tweet `/tweets/{id}/likes`
#[get("/products/{id}/sales")]
pub async fn list(path: Path<(String,)>) -> HttpResponse {
    let sales = Sales { results: vec![] };

    HttpResponse::Ok()
        .content_type(APPLICATION_JSON)
        .json(sales)
}

/// add one like to a tweet `/tweets/{id}/likes`
#[post("/products/{id}/sales")]
pub async fn add_sale(sale_req: Json<SaleRequest>) -> HttpResponse {

    HttpResponse::Created()
        .content_type(APPLICATION_JSON)
        .json(sale_req.to_sale())
}

