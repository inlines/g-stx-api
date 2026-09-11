-- Catalogue visibility and has_serials look up non-digital releases on one platform.
-- Keep ordinary CREATE INDEX: Diesel migrations run inside a transaction.
CREATE INDEX IF NOT EXISTS idx_releases_physical_product_platform
ON releases(product_id, platform) WHERE NOT digital_only;
