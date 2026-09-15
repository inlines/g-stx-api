DROP TRIGGER IF EXISTS validate_owned_release ON users_have_releases;
DROP FUNCTION IF EXISTS validate_owned_release();
-- Retain the column and its values: rolling back code must not reset session versions.
