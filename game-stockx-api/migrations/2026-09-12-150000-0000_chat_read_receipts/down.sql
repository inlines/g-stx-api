DROP TRIGGER messages_deleted_receipt_revision ON messages;
DROP FUNCTION chat_deleted_messages_revision();
DROP TABLE chat_receipt_state;
DROP INDEX messages_dialog_order;
DROP INDEX messages_unread_recipient_sender;
DROP INDEX messages_sender_client_id;
ALTER TABLE messages DROP COLUMN client_id;
ALTER TABLE messages DROP COLUMN read_at;
