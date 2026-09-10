-- Reuse the moderation IDs and photo lifecycle; legacy serial fields store the proposed/accepted value.
ALTER TABLE release_serial_requests ADD COLUMN IF NOT EXISTS kind TEXT NOT NULL DEFAULT 'serial';
ALTER TABLE release_serial_requests ADD COLUMN IF NOT EXISTS product_id INTEGER REFERENCES products(id) ON DELETE CASCADE;
ALTER TABLE release_serial_requests ALTER COLUMN release_id DROP NOT NULL;
ALTER TABLE release_serial_requests DROP CONSTRAINT IF EXISTS release_serial_requests_serial_check;
ALTER TABLE release_serial_requests DROP CONSTRAINT IF EXISTS release_serial_requests_accepted_serial_check;
-- PostgreSQL names the old multi-column CHECK from ADD COLUMN this way.
ALTER TABLE release_serial_requests DROP CONSTRAINT IF EXISTS release_serial_requests_check1;
ALTER TABLE release_serial_requests DROP CONSTRAINT IF EXISTS contribution_target;
ALTER TABLE release_serial_requests ADD CONSTRAINT contribution_target CHECK (
 (kind='serial' AND release_id IS NOT NULL AND product_id IS NULL) OR
 (kind='alternative_name' AND release_id IS NULL AND product_id IS NOT NULL));
ALTER TABLE release_serial_requests DROP CONSTRAINT IF EXISTS contribution_value;
ALTER TABLE release_serial_requests ADD CONSTRAINT contribution_value CHECK (
 (kind='serial' AND length(serial) BETWEEN 3 AND 64) OR
 (kind='alternative_name' AND length(serial) BETWEEN 1 AND 200));
ALTER TABLE release_serial_requests DROP CONSTRAINT IF EXISTS contribution_accepted_value;
ALTER TABLE release_serial_requests ADD CONSTRAINT contribution_accepted_value CHECK (accepted_serial IS NULL OR
 (status='accepted' AND ((kind='serial' AND length(accepted_serial) BETWEEN 3 AND 64) OR
 (kind='alternative_name' AND length(accepted_serial) BETWEEN 1 AND 200))));
CREATE UNIQUE INDEX IF NOT EXISTS name_requests_pending_unique ON release_serial_requests(product_id,lower(btrim(serial))) WHERE status='pending' AND kind='alternative_name';
CREATE INDEX IF NOT EXISTS name_requests_product ON release_serial_requests(product_id);
ALTER TABLE kudos_awards DROP CONSTRAINT IF EXISTS kudos_awards_points_check;
ALTER TABLE kudos_awards ADD CONSTRAINT kudos_awards_points_check CHECK(points IN (5,10));
-- IGDB uses positive IDs. Local contributions occupy a separate, negative range.
CREATE SEQUENCE IF NOT EXISTS local_alternative_name_id AS INTEGER INCREMENT BY -1 MAXVALUE -1 MINVALUE -2147483648 START WITH -1;
SELECT setval('local_alternative_name_id',LEAST((SELECT last_value FROM local_alternative_name_id),COALESCE((SELECT min(id)::bigint FROM alternative_names WHERE id<0),0)-1),false);
-- A version in cache keys also protects against a stale in-flight cache fill.
CREATE TABLE IF NOT EXISTS catalog_name_revision (id INTEGER PRIMARY KEY CHECK(id=1), revision BIGINT NOT NULL DEFAULT 0);
INSERT INTO catalog_name_revision(id) VALUES(1) ON CONFLICT DO NOTHING;
