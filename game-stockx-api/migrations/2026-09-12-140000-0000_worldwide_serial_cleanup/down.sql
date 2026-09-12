LOCK TABLE releases IN SHARE ROW EXCLUSIVE MODE;
-- Restore deleted rows only when their identities have not been reused and their
-- referenced records still exist. New user contributions on surviving rows win.
INSERT INTO releases(id,release_date,product_id,platform,release_status,release_region,digital_only,serial)
 SELECT b.id,b.release_date,b.product_id,b.platform,b.release_status,b.release_region,b.digital_only,b.serial
 FROM worldwide_serial_backup_20260912 b
 WHERE b.deleted AND EXISTS(SELECT 1 FROM products p WHERE p.id=b.product_id)

 ON CONFLICT(id) DO NOTHING;
UPDATE releases r SET serial=b.serial FROM worldwide_serial_backup_20260912 b
 WHERE NOT b.deleted AND r.id=b.id AND r.product_id=b.product_id AND r.platform=b.platform
 AND r.release_region=8 AND r.serial IS NOT DISTINCT FROM b.cleaned_serial;
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
UPDATE products SET cache_revision=cache_revision+1 WHERE id IN(SELECT product_id FROM worldwide_serial_backup_20260912);
DROP TABLE worldwide_serial_backup_20260912;

DROP INDEX IF EXISTS idx_releases_product_platform_lookup_20260912;
