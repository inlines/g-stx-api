-- Unknown until a user establishes an authenticated connection after this release.
ALTER TABLE users ADD COLUMN IF NOT EXISTS last_seen_at TIMESTAMPTZ;
