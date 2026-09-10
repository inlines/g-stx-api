ALTER TABLE messages DROP CONSTRAINT IF EXISTS messages_sender_login_fkey;
ALTER TABLE messages ADD CONSTRAINT messages_sender_login_fkey
    FOREIGN KEY (sender_login) REFERENCES users(user_login);
ALTER TABLE messages DROP CONSTRAINT IF EXISTS messages_recipient_login_fkey;
ALTER TABLE messages ADD CONSTRAINT messages_recipient_login_fkey
    FOREIGN KEY (recipient_login) REFERENCES users(user_login);
DROP TABLE IF EXISTS auth_signing_keys;
ALTER TABLE users DROP COLUMN IF EXISTS is_admin;
