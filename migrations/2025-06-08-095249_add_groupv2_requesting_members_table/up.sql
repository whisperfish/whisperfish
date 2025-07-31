CREATE TABLE group_v2_requesting_members (
    group_v2_id TEXT NOT NULL,
    aci TEXT NOT NULL,
    profile_key BLOB NOT NULL,
    timestamp DATETIME NOT NULL,
    PRIMARY KEY (group_v2_id, aci),
    FOREIGN KEY(group_v2_id) REFERENCES group_v2s(id) ON DELETE CASCADE
);