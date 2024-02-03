DROP INDEX message_expiry;
CREATE INDEX message_destroy_after
    ON messages(DATETIME(expiry_started, '+' || expires_in || ' seconds'))
    WHERE expiry_started NOT NULL AND expires_in NOT NULL;