ALTER TABLE messages
    ADD COLUMN message_type TEXT;
UPDATE messages SET message_type = "Endsession" WHERE flags = 1;
UPDATE messages SET message_type = "ExpirationTimerUpdate" WHERE flags = 2;
UPDATE messages SET message_type = "ProfileKeyUpdate" WHERE flags = 4;
UPDATE messages SET message_type = "GroupChange" WHERE (text LIKE "Group changed by %" AND LENGTH(text) = 53) OR text = "Group changed by nobody" OR text LIKE "Group changed by +%";
CREATE INDEX messages_message_type ON messages (message_type);
