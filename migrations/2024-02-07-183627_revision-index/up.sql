CREATE INDEX latest_message_revision ON messages(session_id, server_timestamp)
  WHERE latest_revision_id IS NULL
     OR latest_revision_id = id;
