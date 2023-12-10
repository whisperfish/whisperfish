DROP INDEX message_destroy_after;
CREATE INDEX message_expiry ON messages(expiry_started);