-- Creates the identity column for the prekeys tables.
ALTER TABLE prekeys
    ADD COLUMN identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci';

ALTER TABLE signed_prekeys
    ADD COLUMN identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci';

ALTER TABLE kyber_prekeys
    ADD COLUMN identity TEXT CHECK(identity IN ('aci', 'pni')) NOT NULL DEFAULT 'aci';
