ALTER TABLE users ADD COLUMN IF NOT EXISTS avatar BYTEA;
DO $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'users_avatar_size' AND conrelid = 'users'::regclass) THEN
        ALTER TABLE users ADD CONSTRAINT users_avatar_size CHECK (avatar IS NULL OR octet_length(avatar) <= 32768);
    END IF;
END $$;
