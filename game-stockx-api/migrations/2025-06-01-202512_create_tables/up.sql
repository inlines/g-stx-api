CREATE TABLE IF NOT EXISTS covers (
    id          INTEGER PRIMARY KEY        NOT NULL,
    image_url   TEXT                    NOT NULL
);

CREATE TABLE IF NOT EXISTS products (
    id          INTEGER PRIMARY KEY        NOT NULL,
    name        TEXT                    NOT NULL,
    summary     TEXT                    NOT NULL,
    first_release_date INTEGER,
    cover_id    INTEGER 
);

CREATE TABLE IF NOT EXISTS sales (
    id          INTEGER PRIMARY KEY        NOT NULL,
    created_at  TIMESTAMP DEFAULT now() NOT NULL,
    product_id  INTEGER REFERENCES products(id) ON DELETE CASCADE NOT NULL,
    total_price INTEGER                 NOT NULL
);

CREATE TABLE IF NOT EXISTS releases (
    id          INTEGER PRIMARY KEY      NOT NULL,
    release_date INTEGER,
    product_id  INTEGER REFERENCES products(id) ON DELETE CASCADE NOT NULL,
    platform INTEGER NOT NULL,
    release_status INTEGER,
    release_region INTEGER
);

CREATE TABLE IF NOT EXISTS platforms (
    id INTEGER PRIMARY KEY NOT NULL,
    abbreviation TEXT,
    name TEXT NOT NULL,
    generation INTEGER
);

CREATE TABLE IF NOT EXISTS regions (
    id INTEGER PRIMARY KEY NOT NULL,
    name TEXT NOT NULL
);

CREATE IF NOT EXISTS TABLE users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_login TEXT UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT now()
);
