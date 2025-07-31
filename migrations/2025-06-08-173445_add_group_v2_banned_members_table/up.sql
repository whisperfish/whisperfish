-- Your SQL goes here
CREATE TABLE group_v2_banned_members (
    group_v2_id TEXT NOT NULL,
    service_id TEXT NOT NULL,
    banned_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (group_v2_id, service_id),
    FOREIGN KEY(group_v2_id) REFERENCES group_v2s(id) ON DELETE CASCADE
);