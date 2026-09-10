ALTER TABLE users ADD COLUMN IF NOT EXISTS is_admin BOOLEAN NOT NULL DEFAULT FALSE;
-- Only an account already present at migration time gets the initial role.
UPDATE users SET is_admin = TRUE WHERE user_login = 'segasanshiro';

-- A shared, persistent signing key is generated with OS randomness by the API.
-- Keeping it in PostgreSQL makes restarts and multiple API instances consistent.
CREATE TABLE IF NOT EXISTS auth_signing_keys (
    id INTEGER PRIMARY KEY CHECK (id = 1),
    secret BYTEA NOT NULL CHECK (octet_length(secret) = 32)
);

ALTER TABLE messages DROP CONSTRAINT IF EXISTS messages_sender_login_fkey;
ALTER TABLE messages ADD CONSTRAINT messages_sender_login_fkey
    FOREIGN KEY (sender_login) REFERENCES users(user_login) ON DELETE CASCADE;
ALTER TABLE messages DROP CONSTRAINT IF EXISTS messages_recipient_login_fkey;
ALTER TABLE messages ADD CONSTRAINT messages_recipient_login_fkey
    FOREIGN KEY (recipient_login) REFERENCES users(user_login) ON DELETE CASCADE;
