-- Stored catalogue totals: refresh on committed catalogue revision changes, never on GET /platforms.
ALTER TABLE platforms
 ADD COLUMN IF NOT EXISTS europe_games integer NOT NULL DEFAULT 0,
 ADD COLUMN IF NOT EXISTS america_games integer NOT NULL DEFAULT 0,
 ADD COLUMN IF NOT EXISTS japan_games integer NOT NULL DEFAULT 0,
 ADD COLUMN IF NOT EXISTS other_games integer NOT NULL DEFAULT 0,
 ADD COLUMN IF NOT EXISTS counts_updated_at timestamptz;

CREATE OR REPLACE FUNCTION refresh_platform_catalog_counts() RETURNS void
LANGUAGE plpgsql AS $$
BEGIN
 -- Same serialization point as catalogue writers, including manual imports and IGDB.
 PERFORM 1 FROM catalog_cache_revision WHERE id=1 FOR UPDATE;
 WITH calculated AS (
WITH candidates AS MATERIALIZED (
 SELECT pp.platform_id, p.id FROM product_platforms pp JOIN products p ON p.id=pp.product_id
 WHERE pp.digital_only=false
 AND EXISTS(SELECT 1 FROM releases r WHERE r.product_id=p.id AND r.platform=pp.platform_id AND r.release_status IS DISTINCT FROM 5 AND r.release_date<=EXTRACT(EPOCH FROM CURRENT_TIMESTAMP))
 AND (effective_game_type(p.id,pp.platform_id,p.game_type) NOT IN (1,2,4,13,6,5,14)
 OR effective_game_type(p.id,pp.platform_id,p.game_type) IS NULL
 OR (pp.platform_id=7 AND effective_game_type(p.id,pp.platform_id,p.game_type) IN (2,4) AND EXISTS(SELECT 1 FROM releases r WHERE r.product_id=p.id AND r.platform=7 AND NOT r.digital_only AND EXISTS(SELECT 1 FROM unnest(r.serial) s WHERE btrim(s)<>''))))
), regions AS (
 SELECT c.platform_id,c.id,
 bool_or(r.release_region IN(1,8)) eu,
 bool_or(r.release_region IN(2,8)) us,
 bool_or(r.release_region IN(5,8)) jp,
 bool_or(r.release_region IS NULL OR r.release_region NOT IN(1,2,5)) other
 FROM candidates c JOIN releases r ON r.product_id=c.id AND r.platform=c.platform_id GROUP BY c.platform_id,c.id
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
$$;

CREATE OR REPLACE FUNCTION refresh_platform_counts_after_revision() RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
 -- Deferred to see ALL changes in an import; run once even with multiple revision bumps.
 IF current_setting('gstx.platform_counts_refreshed',true) IS DISTINCT FROM (SELECT revision::text FROM catalog_cache_revision WHERE id=1) THEN
   PERFORM refresh_platform_catalog_counts();
   PERFORM set_config('gstx.platform_counts_refreshed',(SELECT revision::text FROM catalog_cache_revision WHERE id=1),true);
 END IF;
 RETURN NULL;
END;
$$;
DROP TRIGGER IF EXISTS refresh_platform_counts_on_revision ON catalog_cache_revision;
CREATE CONSTRAINT TRIGGER refresh_platform_counts_on_revision
AFTER UPDATE ON catalog_cache_revision DEFERRABLE INITIALLY DEFERRED
FOR EACH ROW WHEN (OLD.revision IS DISTINCT FROM NEW.revision)
EXECUTE FUNCTION refresh_platform_counts_after_revision();
SELECT refresh_platform_catalog_counts();
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
