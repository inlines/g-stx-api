use diesel::prelude::*;
use diesel::sql_types::{Array, BigInt, Bool, Integer, Nullable, Text};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
pub(crate) struct TrackReleaseRequest {
    pub(crate) release_id: i32,
    pub(crate) product_id: Option<i32>,
    pub(crate) price: Option<i32>,
    pub(crate) cib: Option<bool>,
}

#[derive(Serialize, QueryableByName)]
pub(crate) struct CollectionItem {
    #[diesel(sql_type = Integer)]
    pub(crate) release_id: i32,

    #[diesel(sql_type = Nullable<Array<Text>>)]
    pub(crate) serial: Option<Vec<String>>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub(crate) release_date: Option<i32>,

    #[diesel(sql_type = Text)]
    pub(crate) platform_name: String,

    #[diesel(sql_type = Text)]
    pub(crate) product_name: String,

    #[diesel(sql_type = Integer)]
    pub(crate) product_id: i32,

    #[diesel(sql_type = Nullable<Text>)]
    pub(crate) image_url: Option<String>,

    #[diesel(sql_type = Nullable<Text>)]
    pub(crate) region_name: Option<String>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub(crate) price: Option<i32>,
}

#[derive(Serialize, QueryableByName)]
pub(crate) struct WtsItem {
    #[diesel(sql_type = Integer)]
    pub(crate) release_id: i32,

    #[diesel(sql_type = Nullable<Array<Text>>)]
    pub(crate) serial: Option<Vec<String>>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub(crate) release_date: Option<i32>,

    #[diesel(sql_type = Text)]
    pub(crate) platform_name: String,

    #[diesel(sql_type = Text)]
    pub(crate) product_name: String,

    #[diesel(sql_type = Integer)]
    pub(crate) product_id: i32,

    #[diesel(sql_type = Nullable<Text>)]
    pub(crate) image_url: Option<String>,

    #[diesel(sql_type = Nullable<Text>)]
    pub(crate) region_name: Option<String>,

    #[diesel(sql_type = Nullable<Integer>)]
    pub(crate) price: Option<i32>,

    #[diesel(sql_type = Bool)]
    pub(crate) cib: bool,
}

#[derive(Serialize)]
pub(crate) struct WtsResponse {
    pub(crate) items: Vec<WtsItem>,
    pub(crate) total_count: i64,
}

#[derive(Serialize)]
pub(crate) struct CollectionResponse {
    pub(crate) items: Vec<CollectionItem>,
    pub(crate) total_count: i64,
}

#[derive(QueryableByName)]
pub(crate) struct CountResult {
    #[diesel(sql_type = BigInt)]
    pub total: i64,
}

#[derive(Serialize, QueryableByName)]
pub(crate) struct CollectionStats {
    #[diesel(sql_type = diesel::sql_types::Integer)]
    pub(crate) platform: i32,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) have_count: i64,

    #[diesel(sql_type = diesel::sql_types::Array<diesel::sql_types::Integer>)]
    pub(crate) have_prod_ids: Vec<i32>,

    #[diesel(sql_type = diesel::sql_types::Array<diesel::sql_types::Integer>)]
    pub(crate) have_ids: Vec<i32>,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) wish_count: i64,

    #[diesel(sql_type = diesel::sql_types::Array<diesel::sql_types::Integer>)]
    pub(crate) wish_ids: Vec<i32>,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) wts_count: i64,

    #[diesel(sql_type = diesel::sql_types::Array<diesel::sql_types::Integer>)]
    pub(crate) wts_ids: Vec<i32>,

    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub(crate) total_spent: i64,
}
