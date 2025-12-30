use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub query: Option<String>,
    pub ignore_digital: Option<bool>,
    pub sort: Option<String>,
    pub cat: i64,
}