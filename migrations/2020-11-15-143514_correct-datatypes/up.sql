-- This migration changes the datatypes used in the tables.
-- SQLite does not like altering columns, so we recreate every table,
-- and then move all data over.
--
-- https://www.sqlite.org/datatype3.html
--
-- In principe, datatypes do not matter.  They do matter to us though, since Diesel uses them
-- to infer the schema.rs file.
--
-- Additionally, we introduce FOREIGN KEY constrains on the relevant fields;
-- SQLite does *not* by default enforce these, but Diesel uses them to infer automatic joins.
-- At a later time, we can make Whisperfish manually do constraint checking, cleaning,
-- and as of then enforcement.

-- BEGIN table session --

-- diff: message -> TEXT, many NOT NULLs, introduction of BOOLEAN
CREATE TABLE new_session (
    id INTEGER PRIMARY KEY NOT NULL,
    source TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    sent BOOLEAN DEFAULT 0 NOT NULL,
    received BOOLEAN DEFAULT 0 NOT NULL,
    unread BOOLEAN DEFAULT 0 NOT NULL,
    is_group BOOLEAN DEFAULT 0 NOT NULL,
    group_members TEXT,
    group_id TEXT,
    group_name TEXT,
    has_attachment BOOLEAN DEFAULT 0 NOT NULL
);

INSERT INTO new_session(id, source, message, timestamp, sent, received, unread, is_group, group_members, group_id, group_name, has_attachment)
SELECT id, source, message, timestamp, sent, received, unread, is_group, group_members, group_id, group_name, has_attachment
FROM session;

DROP TABLE session;
ALTER TABLE new_session RENAME TO session;

-- END table session

-- BEGIN table message --

-- diff: most NOT NULL, source and mime_type become TEXT
CREATE TABLE new_message (
    id INTEGER PRIMARY KEY NOT NULL,
    session_id INTEGER NOT NULL,
    source TEXT NOT NULL,
    message TEXT NOT NULL,
    timestamp TIMESTAMP NOT NULL,
    sent BOOLEAN DEFAULT 0 NOT NULL,
    received BOOLEAN DEFAULT 0 NOT NULL,
    flags INTEGER DEFAULT 0 NOT NULL,
    attachment TEXT,
    mime_type TEXT,
    has_attachment BOOLEAN DEFAULT 0 NOT NULL,
    outgoing BOOLEAN DEFAULT 0 NOT NULL,

    FOREIGN KEY(session_id) REFERENCES session(id)
);

INSERT INTO new_message(
    id, session_id, source, message, timestamp, sent, received, flags, attachment, mime_type, has_attachment, outgoing
) SELECT id, session_id, source, message, timestamp, sent, received, flags, attachment, mime_type, has_attachment, outgoing
FROM message;

DROP TABLE message;
ALTER TABLE new_message RENAME TO message;

-- END table message --

-- BEGIN table sentq

-- diff: NOT NULL
CREATE TABLE new_sentq (
    message_id INTEGER PRIMARY KEY NOT NULL,
    timestamp TIMESTAMP NOT NULL,

    FOREIGN KEY(message_id) REFERENCES message(id)
);

INSERT INTO new_sentq(message_id, timestamp)
SELECT message_id, timestamp FROM sentq;

DROP TABLE sentq;
ALTER TABLE new_sentq RENAME TO sentq;

-- END table sentq
