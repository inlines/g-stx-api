-- Refuse to discard requests or earned Kudos during rollback.
DO $$ BEGIN
 IF EXISTS(SELECT 1 FROM release_serial_requests WHERE kind='alternative_name') OR EXISTS(SELECT 1 FROM kudos_awards WHERE points=5) OR EXISTS(SELECT 1 FROM alternative_names WHERE id<0) THEN
  RAISE EXCEPTION 'Cannot roll back name requests while contributions or rewards exist';
 END IF;
END $$;
DROP INDEX IF EXISTS name_requests_pending_unique;
DROP INDEX IF EXISTS name_requests_product;
ALTER TABLE release_serial_requests DROP CONSTRAINT contribution_target;
ALTER TABLE release_serial_requests DROP CONSTRAINT contribution_value;
ALTER TABLE release_serial_requests DROP CONSTRAINT contribution_accepted_value;
ALTER TABLE release_serial_requests DROP COLUMN product_id;
ALTER TABLE release_serial_requests DROP COLUMN kind;
ALTER TABLE release_serial_requests ALTER COLUMN release_id SET NOT NULL;
ALTER TABLE release_serial_requests ADD CONSTRAINT release_serial_requests_serial_check CHECK(length(serial) BETWEEN 3 AND 64);
ALTER TABLE release_serial_requests ADD CONSTRAINT release_serial_requests_accepted_serial_check CHECK(accepted_serial IS NULL OR (status='accepted' AND length(accepted_serial) BETWEEN 3 AND 64));
ALTER TABLE kudos_awards DROP CONSTRAINT kudos_awards_points_check;
ALTER TABLE kudos_awards ADD CONSTRAINT kudos_awards_points_check CHECK(points=10);
DROP SEQUENCE local_alternative_name_id;
DROP TABLE catalog_name_revision;
