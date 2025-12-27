-- Your SQL goes here
-- Самые критичные индексы (сделайте прямо сейчас)
CREATE INDEX CONCURRENTLY idx_product_platforms_platform_digital 
ON product_platforms(platform_id, digital_only);

CREATE INDEX CONCURRENTLY idx_products_name_sort 
ON products(name ASC, id ASC);

-- Временный индекс для OFFSET (удалить после перехода на keyset)
CREATE INDEX CONCURRENTLY idx_products_for_offset 
ON products(name, id) 
INCLUDE (first_release_date, cover_id);