/// Prefer exact selected regions, falling back to Worldwide only when absent.
fn region_scope(platform: &str, regions: &str) -> String {
    format!("(cardinality({regions}::text[])=0 OR EXISTS(SELECT 1 FROM unnest({regions}::text[]) selected(region)
      WHERE (CASE r.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=selected.region
      OR (r.release_region=8 AND NOT EXISTS(SELECT 1 FROM catalog_visible_releases exact
        WHERE exact.product_id=p.id AND exact.platform={platform}
        AND (CASE exact.release_region WHEN 1 THEN 'europe' WHEN 2 THEN 'america' WHEN 5 THEN 'japan' ELSE 'other' END)=selected.region))))")
}

pub fn selected_sql(platform: &str, regions: &str) -> String {
    let scope = region_scope(platform, regions);
    format!(
        "ARRAY(SELECT DISTINCT n.value FROM catalog_visible_releases r
      CROSS JOIN LATERAL unnest(format_release_serials(r.serial)) n(value)
      WHERE r.product_id=p.id AND r.platform={platform} AND btrim(n.value)<>''
      AND {scope} ORDER BY n.value)"
    )
}

pub fn editions_sql(platform: &str, regions: &str) -> String {
    let scope = region_scope(platform, regions);
    format!("COALESCE((SELECT jsonb_agg(DISTINCT jsonb_build_object('serial', n.value, 'edition', e.edition))
      FROM catalog_visible_releases r JOIN release_serial_editions e ON e.release_id=r.id AND e.serial=ANY(r.serial)
      CROSS JOIN LATERAL unnest(format_release_serials(ARRAY[e.serial])) n(value)
      WHERE r.product_id=p.id AND r.platform={platform} AND btrim(n.value)<>'' AND {scope}), '[]'::jsonb)")
}
