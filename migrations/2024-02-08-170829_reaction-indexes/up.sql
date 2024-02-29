-- These never got recreated in 2021-07-23-211500_remove-session-cascade/up.sql because of a typo
CREATE INDEX reaction_message ON reactions(message_id);
CREATE INDEX reaction_author ON reactions(author);
