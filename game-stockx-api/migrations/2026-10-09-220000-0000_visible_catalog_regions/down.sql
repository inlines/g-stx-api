CREATE OR REPLACE FUNCTION public.refresh_platform_catalog_counts()
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
 -- Same serialization point as catalogue writers, including manual imports and IGDB.
 PERFORM 1 FROM catalog_cache_revision WHERE id=1 FOR UPDATE;
 WITH calculated AS (
WITH candidates AS MATERIALIZED (
 SELECT pp.platform_id, p.id FROM product_platforms pp JOIN products p ON p.id=pp.product_id
 WHERE pp.digital_only=false
 AND EXISTS(SELECT 1 FROM releases r WHERE r.product_id=p.id AND r.platform=pp.platform_id AND r.release_status IS DISTINCT FROM 5 AND (r.release_date IS NULL OR r.release_date<=EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)))
 AND (effective_game_type(p.id,pp.platform_id,p.game_type) NOT IN (1,2,4,13,6,5,14)
 OR effective_game_type(p.id,pp.platform_id,p.game_type) IS NULL
 OR (pp.platform_id=7 AND effective_game_type(p.id,pp.platform_id,p.game_type) IN (2,4) AND EXISTS(SELECT 1 FROM releases r WHERE r.product_id=p.id AND r.platform=7 AND NOT r.digital_only AND EXISTS(SELECT 1 FROM unnest(r.serial) s WHERE btrim(s)<>''))))
), release_flags AS (
 SELECT c.platform_id,c.id,
 bool_or(r.release_region=1) has_eu,
 bool_or(r.release_region=2) has_us,
 bool_or(r.release_region=5) has_jp,
 bool_or(r.release_region=1 AND v.physical) eu,
 bool_or(r.release_region=2 AND v.physical) us,
 bool_or(r.release_region=5 AND v.physical) jp,
 bool_or(r.release_region=8 AND v.physical) ww,
 bool_or((r.release_region IS NULL OR r.release_region NOT IN(1,2,5)) AND v.physical) other
 FROM candidates c JOIN releases r ON r.product_id=c.id AND r.platform=c.platform_id
 CROSS JOIN LATERAL (SELECT NOT r.digital_only AND r.release_status IS DISTINCT FROM 5 AND (r.release_date IS NULL OR r.release_date<=EXTRACT(EPOCH FROM CURRENT_TIMESTAMP)) AS physical) v
 GROUP BY c.platform_id,c.id
), scoped AS (
 SELECT platform_id,id,eu OR (ww AND NOT coalesce(has_eu,false)) eu,
 us OR (ww AND NOT coalesce(has_us,false)) us,
 jp OR (ww AND NOT coalesce(has_jp,false)) jp,other FROM release_flags
), regions AS (
 SELECT * FROM scoped WHERE eu OR us OR jp OR other
)

SELECT p.id,p.name,count(r.id)::integer total_games,
 count(r.id) FILTER(WHERE eu)::integer europe_games,
 count(r.id) FILTER(WHERE us)::integer america_games,
 count(r.id) FILTER(WHERE jp)::integer japan_games,
 count(r.id) FILTER(WHERE other)::integer other_games
 FROM platforms p LEFT JOIN regions r ON r.platform_id=p.id GROUP BY p.id ORDER BY p.id
 )
 UPDATE platforms p SET total_games=c.total_games,europe_games=c.europe_games,
 america_games=c.america_games,japan_games=c.japan_games,other_games=c.other_games,
 counts_updated_at=CURRENT_TIMESTAMP FROM calculated c WHERE c.id=p.id;
END;
$function$;


SELECT refresh_platform_catalog_counts();
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;

DROP VIEW public.catalog_visible_releases;
