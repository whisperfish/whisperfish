CREATE INDEX failed_messages ON messages(sending_has_failed) WHERE sent_timestamp IS NULL and is_outbound = TRUE;
