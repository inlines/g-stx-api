/// Dates are scoped to a game/platform pair. Null dates never block a fallback.
/// Keep the group definitions aligned with the catalogue region filters.
pub(crate) fn map_sql(product: &str, platform: &str) -> String {
    format!(
        r#"(SELECT jsonb_build_object(
        'all', MIN(d.release_date),
        'europe', MIN(d.release_date) FILTER (WHERE d.release_region=1),
        'america', MIN(d.release_date) FILTER (WHERE d.release_region=2),
        'japan', MIN(d.release_date) FILTER (WHERE d.release_region=5),
        'other', MIN(d.release_date) FILTER (WHERE d.release_region NOT IN (1,2,5) OR d.release_region IS NULL),
        'worldwide', MIN(d.release_date) FILTER (WHERE d.release_region=8),
        'first', {product}.first_release_date
    ) FROM releases d WHERE d.product_id={product}.id AND d.platform={platform})"#
    )
}

pub(crate) fn selected_sql(map: &str, regions: &str) -> String {
    format!(
        r#"COALESCE(
        CASE WHEN cardinality({regions}::text[])=0 THEN ({map}->>'all')::bigint
        ELSE (SELECT MIN(({map}->>g)::bigint) FROM unnest({regions}::text[]) g) END,
        ({map}->>'worldwide')::bigint,
        ({map}->>'first')::bigint
    )"#
    )
}
