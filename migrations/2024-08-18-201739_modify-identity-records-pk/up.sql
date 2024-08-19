CREATE TABLE new_identity_records (
    address TEXT NOT NULL,
    record BLOB NOT NULL,
    identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci',

    -- TODO: Signal adds a lot more fields here that I don't yet care about.

    PRIMARY KEY(address, identity)
);

INSERT INTO new_identity_records(address, record, identity)
    SELECT address, record, identity
    FROM identity_records;

DROP TABLE identity_records;

ALTER TABLE new_identity_records RENAME TO identity_records;