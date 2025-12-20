-- Your SQL goes here
CREATE TABLE IF NOT EXISTS alternative_names (
    id          INTEGER PRIMARY KEY      NOT NULL,
    product_id  INTEGER REFERENCES products(id) ON DELETE CASCADE NOT NULL,
    name TEXT,
    comment TEXT
);