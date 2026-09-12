-- Canonical keys preserve edition suffixes. Unknown legacy values are not repaired by guessing.
CREATE FUNCTION normalize_release_serial(value text) RETURNS text
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$
 SELECT regexp_replace(upper(regexp_replace(translate(value, '‐‑‒–—−', '------'), '[[:space:]]', '', 'g')), '^([A-Z]{4})-?([0-9]{5})(.*)$', '\1-\2\3');
$$;
CREATE FUNCTION release_serial_search_keys(values_array text[]) RETURNS text[]
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$
 SELECT COALESCE(array_agg(DISTINCT normalize_release_serial(s)), ARRAY[]::text[])
 FROM unnest(values_array) s WHERE s IS NOT NULL AND btrim(s) <> '';
$$;
CREATE FUNCTION format_release_serials(values_array text[]) RETURNS text[]
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$
 SELECT CASE WHEN cardinality(values_array)=0 THEN values_array ELSE
 array_agg(CASE WHEN normalize_release_serial(s) ~ '^[A-Z]{4}-[0-9]{5}([A-Z]{1,4}|([-/][A-Z0-9]{1,8}){1,3})?$'
 THEN normalize_release_serial(s) ELSE s END ORDER BY position) END
 FROM unnest(values_array) WITH ORDINALITY entry(s,position);
$$;
-- Keep a reversible record; don't delete duplicate elements or invalid historic data.
CREATE TABLE serial_format_backup_20260912 AS
 SELECT id, serial AS previous_serial, format_release_serials(serial) AS formatted_serial
 FROM releases WHERE serial IS DISTINCT FROM format_release_serials(serial);
UPDATE releases r SET serial=b.formatted_serial FROM serial_format_backup_20260912 b WHERE r.id=b.id;
CREATE FUNCTION canonicalize_release_serials() RETURNS trigger LANGUAGE plpgsql AS $$
 BEGIN NEW.serial := format_release_serials(NEW.serial); RETURN NEW; END;
$$;
CREATE TRIGGER releases_format_serials BEFORE INSERT OR UPDATE OF serial ON releases
FOR EACH ROW EXECUTE FUNCTION canonicalize_release_serials();
CREATE INDEX releases_serial_search_idx ON releases USING gin (release_serial_search_keys(serial));
UPDATE catalog_cache_revision SET revision=revision+1 WHERE id=1;
UPDATE products SET cache_revision=cache_revision+1
WHERE id IN (SELECT r.product_id FROM releases r JOIN serial_format_backup_20260912 b ON b.id=r.id);
