CREATE INDEX latest_revision_id_backref ON messages(latest_revision_id);
CREATE INDEX original_message_id_backref ON messages(original_message_id);
