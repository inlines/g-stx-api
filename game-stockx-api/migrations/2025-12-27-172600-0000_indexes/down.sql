-- 5. Удаление частичного индекса
DROP INDEX IF EXISTS idx_pp_no_digital;

-- 4. Удаление индексов для JOIN операций
DROP INDEX IF EXISTS idx_alt_names_product;
DROP INDEX IF EXISTS idx_products_cover;
DROP INDEX IF EXISTS idx_covers_product;

-- 3. Удаление индекса для сортировки
DROP INDEX IF EXISTS idx_products_sort;

-- 2. Удаление GIN индексов и расширения
DROP INDEX IF EXISTS idx_alt_names_name_trgm;
DROP INDEX IF EXISTS idx_products_name_trgm;
-- ВНИМАНИЕ: расширение pg_trgm удаляем только если оно больше нигде не используется
-- DROP EXTENSION IF EXISTS pg_trgm;

-- 1. Удаление составного индекса
DROP INDEX IF EXISTS idx_product_platforms_filter;-- This file should undo anything in `up.sql`
