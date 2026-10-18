CREATE TABLE IF NOT EXISTS release_serial_editions (
 release_id integer NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
 serial text NOT NULL CHECK (btrim(serial) <> ''),
 edition text NOT NULL CHECK (edition IN ('standard','platinum','essentials')),
 evidence jsonb NOT NULL CHECK (jsonb_typeof(evidence) = 'array' AND jsonb_array_length(evidence) > 0),
 verified_at timestamptz NOT NULL DEFAULT now(),
 PRIMARY KEY (release_id, serial, edition)
);
ALTER TABLE users_have_releases ADD COLUMN IF NOT EXISTS selected_edition text
 CHECK (selected_edition IN ('standard','platinum','essentials'));
COMMENT ON TABLE release_serial_editions IS 'Verified edition associations; a serial may be shared by several editions. Absence is not evidence of a standard edition.';
-- The data import may have run while the previous API was still populating caches.
UPDATE products SET cache_revision = cache_revision + 1
WHERE EXISTS (SELECT 1 FROM release_serial_editions e WHERE e.release_id IN
 (SELECT r.id FROM releases r WHERE r.product_id = products.id));
UPDATE catalog_cache_revision SET revision = revision + 1 WHERE id = 1;
