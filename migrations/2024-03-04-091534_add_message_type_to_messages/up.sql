ALTER TABLE messages
    ADD COLUMN message_type TEXT;
UPDATE messages SET message_type = "end_session" WHERE flags = 1;
UPDATE messages SET message_type = "expiration_timer_update" WHERE flags = 2;
UPDATE messages SET message_type = "profile_key_update" WHERE flags = 4;
UPDATE messages SET message_type = "group_change" WHERE (text LIKE "Group changed by %" AND LENGTH(text) = 53) OR text = "Group changed by nobody" OR text LIKE "Group changed by +%";
CREATE INDEX messages_message_type ON messages (message_type);
