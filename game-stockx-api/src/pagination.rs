use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub query: Option<String>,
    pub ignore_digital: Option<bool>,
    pub cat: i64,
}