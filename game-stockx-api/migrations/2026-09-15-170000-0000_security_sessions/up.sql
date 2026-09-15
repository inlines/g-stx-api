ALTER TABLE users ADD COLUMN IF NOT EXISTS auth_version BIGINT NOT NULL DEFAULT 0;

-- Derive duplicated product IDs from their authoritative release. Keep prices intact.
UPDATE users_have_releases owned SET product_id = r.product_id
FROM releases r WHERE r.id = owned.release_id AND owned.product_id IS DISTINCT FROM r.product_id;

CREATE OR REPLACE FUNCTION validate_owned_release() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    SELECT r.product_id INTO NEW.product_id FROM releases r WHERE r.id=NEW.release_id;
    -- Existing negative prices are preserved until explicitly edited by their owner.
    IF NEW.price < 0 AND (TG_OP='INSERT' OR NEW.price IS DISTINCT FROM OLD.price) THEN
        RAISE EXCEPTION 'Purchase price must be non-negative' USING ERRCODE='23514';
    END IF;
    RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS validate_owned_release ON users_have_releases;
CREATE TRIGGER validate_owned_release BEFORE INSERT OR UPDATE ON users_have_releases
FOR EACH ROW EXECUTE FUNCTION validate_owned_release();
