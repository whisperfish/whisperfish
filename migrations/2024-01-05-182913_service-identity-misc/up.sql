-- Creates the identity column for the session and identity tables.
ALTER TABLE identity_records
    ADD COLUMN identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci';

ALTER TABLE session_records
    ADD COLUMN identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci';

ALTER TABLE sender_key_records
    ADD COLUMN identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci';
