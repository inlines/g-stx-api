/// Prefer releases of each selected region; use Worldwide only when that region
/// has no release on this platform. An existing release with no serial is not a fallback.
pub fn selected_sql(platform: &str, regions: &str) -> String {
    format!(
        "ARRAY(SELECT DISTINCT n.value FROM releases r
        CROSS JOIN LATERAL unnest(format_release_serials(r.serial)) n(value)
        WHERE r.product_id=p.id AND r.platform={platform} AND btrim(n.value)<>''
        AND (cardinality({regions}::text[])=0 OR EXISTS(SELECT 1 FROM unnest({regions}::text[]) selected(region)
            WHERE (CASE r.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=selected.region
            OR (r.release_region=8 AND NOT EXISTS(
                SELECT 1 FROM releases exact WHERE exact.product_id=p.id AND exact.platform={platform}
                AND (CASE exact.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=selected.region
            )))) ORDER BY n.value)"
    )
}
