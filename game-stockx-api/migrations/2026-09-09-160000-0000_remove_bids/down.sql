-- Rollback restores the old schema, not the discarded exchange flags.
CREATE TABLE IF NOT EXISTS users_have_bids (
    release_id INTEGER REFERENCES releases(id) ON DELETE CASCADE NOT NULL,
    user_login TEXT REFERENCES users(user_login) ON DELETE CASCADE NOT NULL,
    PRIMARY KEY (release_id, user_login)
);
