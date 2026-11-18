CREATE TABLE calls (
    id INTEGER PRIMARY KEY NOT NULL,
    -- XXX: call_id should be a 64-bit integer;
    --      calling code on the Rust side truncates and hence it "works",
    --      but ideally this table is re-materialized with a BIGINT for call_id.
    call_id INTEGER NOT NULL,
    message_id INTEGER DEFAULT NULL REFERENCES messages(id) ON DELETE SET NULL,
    session_id INTEGER NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    type TEXT CHECK(type IN (
            'audio',
            'video',
            'group',
            'ad_hoc'
    )) NOT NULL,
    is_outbound BOOLEAN NOT NULL,
    event TEXT CHECK(event IN (
            'ongoing',
            'accepted',
            'not_accepted',
            'missed',
            'generic_group_call',
            'joined',
            'ringing',
            'declined',
            'outgoing_ring'
    )) NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    ringer INTEGER NOT NULL REFERENCES recipients(id) ON DELETE CASCADE,
    -- DELETEed calls exist in the database for some hours as to prevent out-of-order reappearance of calls
    deletion_timestamp TIMESTAMP DEFAULT NULL,
    is_read BOOLEAN NOT NULL,
    local_joined BOOLEAN NOT NULL DEFAULT FALSE,
    group_call_active BOOLEAN NOT NULL DEFAULT FALSE,

    UNIQUE (call_id, session_id) ON CONFLICT FAIL
)
