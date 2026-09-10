-- Preserve the actual accepted value for readers of the older schema.
UPDATE release_serial_requests SET serial = accepted_serial WHERE accepted_serial IS NOT NULL;
ALTER TABLE release_serial_requests DROP COLUMN IF EXISTS accepted_serial;
