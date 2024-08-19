CREATE TABLE new_session_records (
    address TEXT NOT NULL,
    device_id INTEGER NOT NULL,
    record BLOB NOT NULL,
    identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci',

    PRIMARY KEY(address, device_id, identity)
);

INSERT INTO new_session_records(address, device_id, record, identity)
    SELECT address, device_id, record, identity
    FROM session_records;

DROP TABLE session_records;

ALTER TABLE new_session_records RENAME TO session_records;