ALTER TABLE messages
    ADD COLUMN expire_timer_version INTEGER DEFAULT 1 NOT NULL;

ALTER TABLE sessions
    ADD COLUMN expire_timer_version INTEGER DEFAULT 1 NOT NULL;
