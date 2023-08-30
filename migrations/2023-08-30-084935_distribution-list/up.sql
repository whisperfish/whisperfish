--- Differences with Signal-Android:
--- - Primary key is the distribution_id
--- - Use BOOLEAN where appropriate
--- - deletion_timestamp is NULL when unset instead of 0, and an actual TIMESTAMP
--- - bunch of NOT NULL constraints
CREATE TABLE distribution_lists (
    name TEXT UNIQUE NOT NULL,
    distribution_id VARCHAR(36) PRIMARY KEY NOT NULL,
    recipient_id INTEGER UNIQUE REFERENCES recipients(id),
    allows_replies BOOLEAN DEFAULT TRUE NOT NULL,
    deletion_timestamp TIMESTAMP,
    is_unknown BOOLEAN DEFAULT FALSE NOT NULL,
    --- A list can explicit ([ONLY_WITH]) where only members of the list can send or exclusionary ([ALL_EXCEPT]) where
    --- all connections are sent the story except for those members of the list. [ALL] is all of your Signal Connections.
    privacy_mode INTEGER DEFAULT 0 NOT NULL -- 0 means "ONLY WITH"
);

--- Differences with Signal-Android:
--- - Primary key is the (distribution_id, recipient_id) tuple
CREATE TABLE distribution_list_members (
    distribution_id VARCHAR(36) NOT NULL REFERENCES distribution_lists(distribution_id) ON DELETE CASCADE,
    recipient_id INTEGER NOT NULL REFERENCES recipients(id),
    privacy_mode INTEGER DEFAULT 0 NOT NULL,

    PRIMARY KEY(distribution_id, recipient_id)
);

CREATE INDEX distribution_list_members_recipient_id ON distribution_list_members(recipient_id);
