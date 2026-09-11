use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub regions: Option<String>,
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

impl Pagination {
    pub fn region_groups(&self) -> Result<Vec<String>, ()> {
        let mut groups: Vec<String> = self
            .regions
            .as_deref()
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect();
        if groups
            .iter()
            .any(|s| !matches!(s.as_str(), "europe" | "america" | "japan" | "other"))
        {
            return Err(());
        }
        groups.sort();
        groups.dedup();
        Ok(groups)
    }
}
