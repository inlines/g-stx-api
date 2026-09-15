CREATE TABLE genres (id INTEGER PRIMARY KEY, name TEXT NOT NULL CHECK (btrim(name) <> ''));
CREATE TABLE product_genres (
 product_id INTEGER NOT NULL REFERENCES products(id) ON DELETE CASCADE,
 genre_id INTEGER NOT NULL REFERENCES genres(id) ON DELETE CASCADE,
 PRIMARY KEY(product_id,genre_id)
);
CREATE INDEX product_genres_genre_product_idx ON product_genres(genre_id,product_id);
