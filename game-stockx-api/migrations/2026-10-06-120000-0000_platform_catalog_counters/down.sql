DROP TRIGGER IF EXISTS refresh_platform_counts_on_revision ON catalog_cache_revision;
DROP FUNCTION IF EXISTS refresh_platform_counts_after_revision();
DROP FUNCTION IF EXISTS refresh_platform_catalog_counts();
ALTER TABLE platforms DROP COLUMN IF EXISTS europe_games, DROP COLUMN IF EXISTS america_games,
 DROP COLUMN IF EXISTS japan_games, DROP COLUMN IF EXISTS other_games, DROP COLUMN IF EXISTS counts_updated_at;
