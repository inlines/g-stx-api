CREATE TABLE IF NOT EXISTS release_serial_requests (
    id SERIAL PRIMARY KEY,
    release_id INTEGER NOT NULL REFERENCES releases(id) ON DELETE CASCADE,
    submitter_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    serial TEXT NOT NULL CHECK (length(serial) BETWEEN 3 AND 64),
    photo BYTEA NOT NULL CHECK (octet_length(photo) BETWEEN 1 AND 786432),
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'accepted')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    reviewed_at TIMESTAMPTZ,
    reviewed_by INTEGER REFERENCES users(id) ON DELETE SET NULL,
    CHECK ((status = 'pending' AND reviewed_at IS NULL) OR (status = 'accepted' AND reviewed_at IS NOT NULL))
);
CREATE UNIQUE INDEX IF NOT EXISTS release_serial_requests_pending_unique
    ON release_serial_requests (release_id, upper(btrim(serial))) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS release_serial_requests_status_date
    ON release_serial_requests(status, created_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS release_serial_requests_submitter ON release_serial_requests(submitter_id);
CREATE INDEX IF NOT EXISTS release_serial_requests_reviewer ON release_serial_requests(reviewed_by);
CREATE INDEX IF NOT EXISTS release_serial_requests_archive_date
    ON release_serial_requests(reviewed_at DESC,id DESC) WHERE status='accepted';
