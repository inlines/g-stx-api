CREATE TABLE IF NOT EXISTS products (
    id          UUID PRIMARY KEY        NOT NULL,
    name        TEXT                    NOT NULL,
    summary     TEXT                    NOT NULL,
    first_release_date TIMESTAMP        NOT NULL,
    price       NUMERIC(10, 2)          NOT NULL,
    stock       INTEGER                 NOT NULL
);

CREATE TABLE IF NOT EXISTS sales (
    id          UUID PRIMARY KEY        NOT NULL,
    created_at  TIMESTAMP DEFAULT now() NOT NULL,
    product_id  UUID REFERENCES products(id) ON DELETE CASCADE NOT NULL,
    total_price NUMERIC(10, 2)          NOT NULL
);

CREATE TABLE IF NOT EXISTS covers (
    id          UUID PRIMARY KEY        NOT NULL,
    product_id  UUID REFERENCES products(id) ON DELETE CASCADE NOT NULL,
    image_url   TEXT                    NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_products_name ON products(name);
CREATE INDEX IF NOT EXISTS idx_sales_product_id ON sales(product_id);
CREATE INDEX IF NOT EXISTS idx_covers_product_id ON covers(product_id);
CREATE INDEX IF NOT EXISTS idx_sales_total_price ON sales(total_price);