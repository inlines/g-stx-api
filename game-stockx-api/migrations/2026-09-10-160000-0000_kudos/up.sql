-- No FK to the request: clearing the moderation archive must preserve awards.
CREATE TABLE IF NOT EXISTS kudos_awards (
    request_id INTEGER PRIMARY KEY,
    user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    points INTEGER NOT NULL DEFAULT 10 CHECK (points = 10),
    awarded_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX IF NOT EXISTS kudos_awards_user ON kudos_awards(user_id);
INSERT INTO kudos_awards(request_id,user_id,points,awarded_at)
    SELECT id,submitter_id,10,COALESCE(reviewed_at,created_at)
    FROM release_serial_requests WHERE status='accepted'
    ON CONFLICT(request_id) DO NOTHING;
