/// Dates are scoped to a game/platform pair. Existing undated releases stay undated.
/// Keep the group definitions aligned with the catalogue region filters.
pub(crate) fn map_sql(product: &str, platform: &str) -> String {
    format!(
        r#"(SELECT jsonb_build_object(
        'present', jsonb_build_object('all',COUNT(*)>0,'europe',COUNT(*) FILTER(WHERE d.release_region=1)>0,'america',COUNT(*) FILTER(WHERE d.release_region=2)>0,'japan',COUNT(*) FILTER(WHERE d.release_region=5)>0,'other',COUNT(*) FILTER(WHERE d.release_region=8)>0,'worldwide',COUNT(*) FILTER(WHERE d.release_region=8)>0),
        'all', MIN(d.release_date),
        'europe', MIN(d.release_date) FILTER (WHERE d.release_region=1),
        'america', MIN(d.release_date) FILTER (WHERE d.release_region=2),
        'japan', MIN(d.release_date) FILTER (WHERE d.release_region=5),
        'other', MIN(d.release_date) FILTER (WHERE d.release_region=8),
        'worldwide', MIN(d.release_date) FILTER (WHERE d.release_region=8),
        'first', {product}.first_release_date
    ) FROM catalog_visible_releases d WHERE d.product_id={product}.id AND d.platform={platform})"#
    )
}

pub(crate) fn selected_sql(map: &str, regions: &str) -> String {
    format!(
        r#"CASE WHEN CASE WHEN cardinality({regions}::text[])=0 THEN ({map}->'present'->>'all')::boolean ELSE EXISTS(SELECT 1 FROM unnest({regions}::text[]) g WHERE ({map}->'present'->>g)::boolean) END THEN
        CASE WHEN cardinality({regions}::text[])=0 THEN ({map}->>'all')::bigint
        ELSE (SELECT MIN(({map}->>g)::bigint) FROM unnest({regions}::text[]) g) END
        WHEN ({map}->'present'->>'worldwide')::boolean THEN ({map}->>'worldwide')::bigint
        ELSE COALESCE(
        CASE WHEN cardinality({regions}::text[])=0 THEN ({map}->>'all')::bigint
        ELSE (SELECT MIN(({map}->>g)::bigint) FROM unnest({regions}::text[]) g) END,
        ({map}->>'worldwide')::bigint,
        ({map}->>'first')::bigint
    ) END"#
    )
}
