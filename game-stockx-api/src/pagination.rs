use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,   // Параметр лимита, по умолчанию можно установить 100
    pub offset: Option<i64>,  // Параметр смещения, по умолчанию можно установить 0
    pub query: Option<String>,
    pub cat: i64,
}