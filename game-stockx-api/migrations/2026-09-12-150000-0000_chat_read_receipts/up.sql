ALTER TABLE messages ADD COLUMN read_at timestamptz;
ALTER TABLE messages ADD COLUMN client_id text;
CREATE UNIQUE INDEX messages_sender_client_id ON messages(sender_login,client_id) WHERE client_id IS NOT NULL;
CREATE INDEX messages_unread_recipient_sender ON messages(recipient_login,sender_login,id) WHERE NOT read;
CREATE INDEX messages_dialog_order ON messages(sender_login,recipient_login,id);
CREATE TABLE chat_receipt_state(user_login text PRIMARY KEY REFERENCES users(user_login) ON DELETE CASCADE, revision bigint NOT NULL DEFAULT 0);

-- Keep unread snapshots monotonic when an administrator deletes a correspondent.
CREATE FUNCTION chat_deleted_messages_revision() RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
 UPDATE chat_receipt_state s SET revision=revision+1
 WHERE EXISTS(SELECT 1 FROM deleted_messages m WHERE m.recipient_login=s.user_login);
 RETURN NULL;
END $$;
CREATE TRIGGER messages_deleted_receipt_revision AFTER DELETE ON messages
REFERENCING OLD TABLE AS deleted_messages FOR EACH STATEMENT EXECUTE FUNCTION chat_deleted_messages_revision();
