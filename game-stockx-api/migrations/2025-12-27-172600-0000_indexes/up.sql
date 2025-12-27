-- Включаем расширение для триграмм (если еще не включено)
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- 1. Основной индекс для product_platforms (самый важный!)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_product_platforms_filter 
ON product_platforms(platform_id, digital_only, product_id);

-- 2. GIN индексы для поиска по имени (чуть медленнее на вставку, но быстрее на поиск)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_products_name_trgm 
ON products USING gin(name gin_trgm_ops);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_alt_names_name_trgm 
ON alternative_names USING gin(name gin_trgm_ops);

-- 3. Индекс для сортировки (критически важен для OFFSET пагинации)
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_products_name_id 
ON products(name, id);

-- 4. Индексы для JOIN операций
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_covers_id 
ON covers(id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_products_cover_id 
ON products(cover_id);

CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_alt_names_product_id 
ON alternative_names(product_id);

-- 5. Частичный индекс для ignore_digital = true (если часто используется)
-- Сначала проверьте статистику использования
SELECT 
    digital_only,
    COUNT(*) as count,
    ROUND(COUNT(*) * 100.0 / SUM(COUNT(*)) OVER(), 2) as percentage
FROM product_platforms
GROUP BY digital_only;

-- Если digital_only = false встречается часто (> 30%), создаем индекс
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_pp_no_digital 
ON product_platforms(platform_id, product_id) 
WHERE digital_only = false;

-- ДОПОЛНИТЕЛЬНО: Индекс для поиска по platform_id + product_id
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_pp_platform_product 
ON product_platforms(platform_id, product_id);


