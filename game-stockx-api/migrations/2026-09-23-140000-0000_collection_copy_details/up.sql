ALTER TABLE users_have_releases ADD COLUMN selected_serial TEXT;
ALTER TABLE users_have_releases ADD COLUMN cib BOOLEAN;
ALTER TABLE users_have_releases ADD CONSTRAINT owned_serial_length CHECK (selected_serial IS NULL OR length(selected_serial) BETWEEN 1 AND 128);
-- Only an affirmative legacy flag is evidence of a complete copy; false was the default.
UPDATE users_have_releases owned SET cib = TRUE
FROM users_have_wts sale
WHERE sale.release_id=owned.release_id AND sale.user_login=owned.user_login AND sale.cib=TRUE;
