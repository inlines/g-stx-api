ALTER TABLE release_serial_requests ADD COLUMN IF NOT EXISTS accepted_serial TEXT
    CHECK (accepted_serial IS NULL OR (status = 'accepted' AND length(accepted_serial) BETWEEN 3 AND 64));
UPDATE release_serial_requests SET accepted_serial = serial WHERE status = 'accepted' AND accepted_serial IS NULL;
