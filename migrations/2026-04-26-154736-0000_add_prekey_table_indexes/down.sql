-- Drop indexes for faster prekey querying
DROP INDEX IF EXISTS kyber_prekeys_identity_last_resort;
DROP INDEX IF EXISTS prekeys_identity;
DROP INDEX IF EXISTS signed_prekeys_identity;
