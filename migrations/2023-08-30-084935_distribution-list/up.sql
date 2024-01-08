--- Differences with Signal-Android:
--- - Signal Android calls our "session" a "recipient", whereas our "recipient" is a single user.
--- - Primary key is the distribution_id
--- - Use BOOLEAN where appropriate
--- - deletion_timestamp is NULL when unset instead of 0, and an actual TIMESTAMP
--- - bunch of NOT NULL constraints
CREATE TABLE distribution_lists (
    name TEXT UNIQUE NOT NULL,
    distribution_id VARCHAR(36) PRIMARY KEY NOT NULL,
    session_id INTEGER UNIQUE REFERENCES sessions(id),
    allows_replies BOOLEAN DEFAULT TRUE NOT NULL,
    deletion_timestamp TIMESTAMP,
    is_unknown BOOLEAN DEFAULT FALSE NOT NULL,
    --- A list can explicit ([ONLY_WITH]) where only members of the list can send or exclusionary ([ALL_EXCEPT]) where
    --- all connections are sent the story except for those members of the list. [ALL] is all of your Signal Connections.
    privacy_mode INTEGER DEFAULT 0 NOT NULL -- 0 means "ONLY WITH"
);

--- Differences with Signal-Android:
--- - Signal Android calls our "session" a "recipient", whereas our "recipient" is a single user.
--- - Primary key is the (distribution_id, session_id) tuple
CREATE TABLE distribution_list_members (
    distribution_id VARCHAR(36) NOT NULL REFERENCES distribution_lists(distribution_id) ON DELETE CASCADE,
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    privacy_mode INTEGER DEFAULT 0 NOT NULL,

    PRIMARY KEY(distribution_id, session_id)
);

CREATE INDEX distribution_list_members_session_id ON distribution_list_members(session_id);
