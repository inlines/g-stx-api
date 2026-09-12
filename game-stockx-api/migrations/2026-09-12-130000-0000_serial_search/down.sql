DROP TRIGGER releases_format_serials ON releases;
DROP FUNCTION canonicalize_release_serials();
DROP INDEX releases_serial_search_idx;
-- Never overwrite contributions added after this migration.
UPDATE releases r SET serial=b.previous_serial FROM serial_format_backup_20260912 b
WHERE r.id=b.id AND r.serial IS NOT DISTINCT FROM b.formatted_serial;
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
UPDATE products SET cache_revision=cache_revision+1
WHERE id IN (SELECT r.product_id FROM releases r JOIN serial_format_backup_20260912 b ON b.id=r.id);
DROP TABLE serial_format_backup_20260912;
DROP FUNCTION format_release_serials(text[]);
DROP FUNCTION release_serial_search_keys(text[]);
DROP FUNCTION normalize_release_serial(text);
