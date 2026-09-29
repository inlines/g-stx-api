use serde::Deserialize;

#[derive(Deserialize)]
pub struct Pagination {
    pub limit: Option<i64>,
    pub offset: Option<i64>,
    pub unknown: Option<bool>,
    pub genre_id: Option<i32>,
    pub regions: Option<String>,
    pub query: Option<String>,
    pub search_mode: Option<String>,
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

pub fn page_bounds(
    limit: Option<i64>,
    offset: Option<i64>,
    maximum: i64,
) -> Result<(i64, i64), actix_web::HttpResponse> {
    let limit = limit.unwrap_or(100);
    let offset = offset.unwrap_or(0);
    if !(1..=maximum).contains(&limit) || !(0..=1_000_000).contains(&offset) {
        return Err(actix_web::HttpResponse::BadRequest().body("Invalid pagination"));
    }
    Ok((limit, offset))
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

/// Two pages at the requested page size, capped at twenty cards each.
pub fn guest_catalog_page(limit: i64, offset: i64) -> bool {
    (1..=20).contains(&limit) && (offset == 0 || offset == limit)
}

#[cfg(test)]
mod guest_tests {
    #[actix_web::test]
    async fn only_first_two_pages() {
        for size in [1, 10, 20] {
            assert!(super::guest_catalog_page(size, 0));
            assert!(super::guest_catalog_page(size, size));
            assert!(!super::guest_catalog_page(size, size * 2));
        }
        for (size, offset) in [(21, 0), (1000, 0), (20, 21), (10, 20), (0, 0), (20, -1)] {
            assert!(!super::guest_catalog_page(size, offset));
        }
    }
}
