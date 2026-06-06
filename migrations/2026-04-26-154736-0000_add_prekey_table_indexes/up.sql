-- Create indexes for faster prekey querying
CREATE INDEX kyber_prekeys_identity_last_resort ON kyber_prekeys(identity, is_last_resort);
CREATE INDEX prekeys_identity ON prekeys(identity);
CREATE INDEX signed_prekeys_identity ON signed_prekeys(identity);
