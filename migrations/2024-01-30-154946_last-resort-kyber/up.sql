ALTER TABLE kyber_prekeys
    ADD COLUMN is_last_resort BOOLEAN DEFAULT FALSE NOT NULL;
