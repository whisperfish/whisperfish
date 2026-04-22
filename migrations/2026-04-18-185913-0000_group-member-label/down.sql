ALTER TABLE group_v2s
    DROP COLUMN access_required_for_member_labels;

-- members
ALTER TABLE group_v2_members
    DROP COLUMN label;
ALTER TABLE group_v2_members
    DROP COLUMN label_emoji;
