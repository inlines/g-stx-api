-- Compare only the same game and platform; preserve Worldwide-only serials.
-- Diesel executes this migration in a transaction. Block concurrent writers while
-- taking the relationship snapshot, so a newly added collection cannot be cascaded away.
LOCK TABLE releases IN SHARE ROW EXCLUSIVE MODE;
-- Both regional serial display and cleanup need all releases, including digital rows.
CREATE INDEX idx_releases_product_platform_lookup_20260912 ON releases(product_id,platform);
CREATE TEMP TABLE worldwide_protected_releases(id integer PRIMARY KEY) ON COMMIT DROP;
DO $$ DECLARE link record; BEGIN
 IF EXISTS(SELECT 1 FROM pg_constraint c WHERE c.contype='f' AND c.confrelid='releases'::regclass
   AND (cardinality(c.conkey)<>1 OR c.confkey[1]<>(SELECT attnum FROM pg_attribute WHERE attrelid='releases'::regclass AND attname='id'))) THEN
   RAISE EXCEPTION 'Unsupported release relationship: aborting cleanup before changing data';
 END IF;
 FOR link IN
   SELECT c.conrelid::regclass AS tbl, a.attname AS col
   FROM pg_constraint c JOIN pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=c.conkey[1]
   WHERE c.contype='f' AND c.confrelid='releases'::regclass
 LOOP
   EXECUTE format('LOCK TABLE %s IN SHARE ROW EXCLUSIVE MODE',link.tbl);
   EXECUTE format('INSERT INTO worldwide_protected_releases SELECT %I FROM %s WHERE %I IS NOT NULL ON CONFLICT DO NOTHING',link.col,link.tbl,link.col);
 END LOOP;
END $$;
CREATE TABLE worldwide_serial_backup_20260912 AS
 SELECT w.*, cleaned.serial AS cleaned_serial, false AS deleted
 FROM releases w
 CROSS JOIN LATERAL (
   SELECT ARRAY(
     SELECT s.value FROM unnest(w.serial) WITH ORDINALITY s(value,position)
     WHERE NOT EXISTS (
       SELECT 1 FROM releases regional CROSS JOIN LATERAL unnest(regional.serial) known(value)
       WHERE regional.product_id=w.product_id AND regional.platform=w.platform
         AND regional.release_region IS DISTINCT FROM 8
         AND btrim(known.value)<>''
         AND normalize_release_serial(known.value)=normalize_release_serial(s.value)
     ) ORDER BY s.position
   ) AS serial
 ) cleaned
 WHERE w.release_region=8 AND w.serial IS NOT NULL AND w.serial IS DISTINCT FROM cleaned.serial;
UPDATE releases r SET serial=b.cleaned_serial FROM worldwide_serial_backup_20260912 b WHERE r.id=b.id;
UPDATE worldwide_serial_backup_20260912 b SET deleted=true
 WHERE NOT EXISTS(SELECT 1 FROM unnest(b.cleaned_serial) s WHERE btrim(s)<>'')
 AND NOT EXISTS(SELECT 1 FROM worldwide_protected_releases p WHERE p.id=b.id);
DELETE FROM releases r USING worldwide_serial_backup_20260912 b WHERE r.id=b.id AND b.deleted;
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
UPDATE products SET cache_revision=cache_revision+1 WHERE id IN(SELECT product_id FROM worldwide_serial_backup_20260912);
