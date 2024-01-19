CREATE INDEX message_latest_messages_only
    ON messages(session_id, latest_revision_id)
    WHERE latest_revision_id IS NULL OR latest_revision_id = id;
