use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub query: Option<String>,
    pub include_unreleased: Option<bool>,
    pub ignore_digital: Option<bool>,
    pub local_multiplayer: Option<bool>,
    pub online_multiplayer: Option<bool>,
    pub sort: Option<String>,
    pub cat: i64,
    pub franchise_id: Option<i32>,
    pub company_id: Option<i32>,
    pub company_role: Option<String>,
}
