ALTER TABLE group_v2s
    ADD COLUMN access_required_for_member_labels INTEGER NOT NULL DEFAULT 0;

-- members
ALTER TABLE group_v2_members
    ADD COLUMN label TEXT;
ALTER TABLE group_v2_members
    ADD COLUMN label_emoji TEXT;
