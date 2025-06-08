CREATE TABLE group_v2_pending_members (
    group_v2_id TEXT NOT NULL,
    service_id TEXT NOT NULL,
    role INTEGER NOT NULL,
    added_by_aci TEXT NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    PRIMARY KEY (group_v2_id, service_id),
    FOREIGN KEY(group_v2_id) REFERENCES group_v2s(id) ON DELETE CASCADE
);