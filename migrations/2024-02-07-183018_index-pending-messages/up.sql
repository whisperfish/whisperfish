CREATE INDEX failed_messages ON messages(sending_has_failed, is_outbound) WHERE sent_timestamp IS NULL;
