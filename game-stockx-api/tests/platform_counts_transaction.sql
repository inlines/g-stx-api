\set ON_ERROR_STOP on
BEGIN;
CREATE TEMP TABLE counter_before AS SELECT * FROM platforms;
CREATE TEMP TABLE test_pairs AS SELECT pp.product_id FROM product_platforms pp JOIN products p ON p.id=pp.product_id WHERE pp.platform_id=38 AND NOT pp.digital_only AND (effective_game_type(p.id,38,p.game_type) IS NULL OR effective_game_type(p.id,38,p.game_type) NOT IN(1,2,4,13,6,5,14)) AND EXISTS(SELECT 1 FROM releases r WHERE r.product_id=p.id AND r.platform=38 AND r.release_status IS DISTINCT FROM 5 AND r.release_date<=extract(epoch from current_timestamp)) ORDER BY pp.product_id LIMIT 2;
UPDATE product_platforms SET digital_only=true WHERE platform_id=38 AND product_id=(SELECT min(product_id) FROM test_pairs);
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
-- Deliberately mutate after the revision changes: the deferred refresh must see this too.
UPDATE product_platforms SET digital_only=true WHERE platform_id=38 AND product_id=(SELECT max(product_id) FROM test_pairs);
DO $$ BEGIN IF (SELECT total_games FROM platforms WHERE id=38)<>(SELECT total_games FROM counter_before WHERE id=38) THEN RAISE EXCEPTION 'Counter changed before end of transaction'; END IF; END $$;
SET CONSTRAINTS refresh_platform_counts_on_revision IMMEDIATE;
DO $$ BEGIN IF (SELECT total_games FROM platforms WHERE id=38)<>(SELECT total_games-2 FROM counter_before WHERE id=38) THEN RAISE EXCEPTION 'Counter did not include final transaction state'; END IF; END $$;
ROLLBACK;
SELECT 'PASS: deferred refresh sees final changes; rollback preserves snapshot' AS result;
