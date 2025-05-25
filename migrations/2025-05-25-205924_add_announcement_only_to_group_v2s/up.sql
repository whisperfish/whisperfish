-- Your SQL goes here
ALTER TABLE
    group_v2s
ADD
    COLUMN announcement_only BOOLEAN NOT NULL DEFAULT FALSE;