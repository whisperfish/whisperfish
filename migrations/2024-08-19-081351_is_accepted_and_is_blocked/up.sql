-- Recipient
ALTER TABLE recipients ADD COLUMN is_accepted BOOLEAN NOT NULL DEFAULT FALSE;

-- Mark every recipient that we have an ACI identity for as accepted
UPDATE recipients SET is_accepted = TRUE WHERE uuid IN (SELECT DISTINCT address FROM identity_records WHERE identity = "aci");