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
    id          UUID PRIMARY KEY        NOT NULL,
    created_at  TIMESTAMP DEFAULT now() NOT NULL,
    product_id  INTEGER REFERENCES products(id) ON DELETE CASCADE NOT NULL,
    total_price INTEGER                 NOT NULL
);
