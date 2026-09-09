-- Match the index names created by up.sql. Keep the shared pg_trgm extension.
DROP INDEX IF EXISTS idx_pp_platform_product;
DROP INDEX IF EXISTS idx_pp_no_digital;
DROP INDEX IF EXISTS idx_alt_names_product_id;
DROP INDEX IF EXISTS idx_products_cover_id;
DROP INDEX IF EXISTS idx_covers_id;
DROP INDEX IF EXISTS idx_products_name_id;
DROP INDEX IF EXISTS idx_alt_names_name_trgm;
DROP INDEX IF EXISTS idx_products_name_trgm;
DROP INDEX IF EXISTS idx_product_platforms_filter;
