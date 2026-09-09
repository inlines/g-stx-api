CREATE TABLE IF NOT EXISTS users_have_wts (
    release_id  INTEGER REFERENCES releases(id) ON DELETE CASCADE NOT NULL,
    user_login  TEXT REFERENCES users(user_login) ON DELETE CASCADE NOT NULL,
    price INTEGER NULL DEFAULT NULL,
  	cib boolean DEFAULT FALSE,
    PRIMARY KEY (release_id, user_login)
);
