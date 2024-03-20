ALTER TABLE messages
    ADD COLUMN message_type TEXT;

-- Session reset messages
UPDATE messages SET message_type = 'end_session'
  WHERE flags = 1;
UPDATE messages SET message_type = 'end_session', flags = 1
  WHERE text = '[Whisperfish] Reset secure session' AND flags IN (0, 1);

-- Expiration timer updates
-- Note: If expires_in is not set, QML tries to parse it from the text.
UPDATE messages SET message_type = 'expiration_timer_update'
  WHERE flags = 2;
UPDATE messages SET message_type = 'expiration_timer_update', flags = 2
  WHERE text LIKE '[Whisperfish] Message expiry set %' AND flags IN (0, 2);

-- Profile key updates
UPDATE messages SET message_type = 'profile_key_update'
  WHERE flags = 4;

-- Changes in groups (v2)
UPDATE messages SET message_type = 'group_change'
  WHERE (text LIKE 'Group changed by %' AND LENGTH(text) = 53)
  OR text = 'Group changed by nobody'
  OR text LIKE 'Group changed by +%';

-- Identity teset messages
UPDATE messages SET message_type = 'identity_reset'
  WHERE text LIKE '[Whisperfish] The identity key for this contact has changed.%Please verify your safety number.';

CREATE INDEX messages_message_type ON messages (message_type);
