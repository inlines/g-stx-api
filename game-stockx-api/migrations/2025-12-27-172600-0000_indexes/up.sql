-- Сначала выполняем операции, которые можно делать в транзакции
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Обычные индексы (без CONCURRENTLY)
CREATE INDEX IF NOT EXISTS idx_product_platforms_filter 
ON product_platforms(platform_id, digital_only, product_id);

CREATE INDEX IF NOT EXISTS idx_products_name_trgm 
ON products USING gin(name gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_alt_names_name_trgm 
ON alternative_names USING gin(name gin_trgm_ops);

CREATE INDEX IF NOT EXISTS idx_products_name_id 
ON products(name, id);

CREATE INDEX IF NOT EXISTS idx_covers_id 
ON covers(id);

CREATE INDEX IF NOT EXISTS idx_products_cover_id 
ON products(cover_id);

CREATE INDEX IF NOT EXISTS idx_alt_names_product_id 
ON alternative_names(product_id);

CREATE INDEX IF NOT EXISTS idx_pp_no_digital 
ON product_platforms(platform_id, product_id) 
WHERE digital_only = false;

CREATE INDEX IF NOT EXISTS idx_pp_platform_product 
ON product_platforms(platform_id, product_id);

-- Затем создаем отдельный файл для concurrent индексов
-- Или выполняем их вне транзакции