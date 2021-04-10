-- This is a no-op on SQLite <3.26
PRAGMA legacy_alter_table = ON;

ALTER TABLE group_v1s
    -- HEX encoded 32 byte.
    ADD COLUMN expected_v2_id VARCHAR(64);
CREATE INDEX v1_expected_v2_id ON group_v1s(expected_v2_id);

CREATE TABLE group_v2s (
    id VARCHAR(64) PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,

    master_key VARCHAR(64) NOT NULL,
    revision INTEGER NOT NULL DEFAULT 0,

    -- Access control.
    -- enum AccessRequired {
    --  UNKNOWN       = 0;
    --  ANY           = 1;
    --  MEMBER        = 2;
    --  ADMINISTRATOR = 3;
    --  UNSATISFIABLE = 4;
    --}
    access_required_for_attributes INTEGER NOT NULL DEFAULT 0,
    access_required_for_members INTEGER NOT NULL DEFAULT 0,
    access_required_for_add_from_invite_link INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE group_v2_members (
    group_v2_id VARCHAR(64) NOT NULL,
    recipient_id INTEGER NOT NULL,
    member_since TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
    joined_at_revision INTEGER NOT NULL,
    role INTEGER NOT NULL,

    FOREIGN KEY(group_v2_id) REFERENCES group_v2s(id),
    FOREIGN KEY(recipient_id) REFERENCES recipients(id), -- on delete RESTRICT because we shouldn't delete a group member because we don't like the receiver.
    -- artificial primary key
    PRIMARY KEY(group_v2_id, recipient_id)
);

CREATE TRIGGER assert_uuid_for_group_v2_members
  BEFORE INSERT ON group_v2_members
BEGIN
  SELECT
    RAISE (ABORT, 'UUID or profile key of GroupV2 member is not set')
  WHERE EXISTS (
    SELECT
      recipient.id
    FROM recipients
    WHERE recipient.id = NEW.recipient_id
      AND (recipient.uuid IS NULL
          OR recipient.profile_key IS NULL)
  );
END;

-- Now we need to add a group_v2_id to sessions.
-- Sadly, our CHECK constraint over there needs an alteration too,
-- which means we have to completely redo the table.
-- In order not to trigger weird effects (renames) on foreign keys references,
-- we turn OFF foreign keys and ON legacy behaviour.
-- ref: https://sqlite.org/lang_altertable.html#altertabrename

ALTER TABLE sessions RENAME TO sessions_old;

CREATE TABLE sessions (
    id INTEGER PRIMARY KEY NOT NULL,

    -- Exactly one of these three should be filed
    direct_message_recipient_id INTEGER,
    group_v1_id VARCHAR(32),
    group_v2_id VARCHAR(64),

    is_archived BOOLEAN DEFAULT FALSE NOT NULL,
    is_pinned BOOLEAN DEFAULT FALSE NOT NULL,

    -- silent: notification without sound or vibration
    is_silent BOOLEAN DEFAULT FALSE NOT NULL,
    -- muted: no notification at all
    is_muted BOOLEAN DEFAULT FALSE NOT NULL,

    draft TEXT,

    expiring_message_timeout INTEGER,

    -- Deleting recipients should be separate from deleting sessions. ON DELETE RESTRICT
    FOREIGN KEY(direct_message_recipient_id) REFERENCES recipients(id),
    FOREIGN KEY(group_v1_id) REFERENCES group_v1s(id),
    FOREIGN KEY(group_v2_id) REFERENCES group_v2s(id),

    -- Either a session is dm, gv1 or gv2
    CHECK (NOT(direct_message_recipient_id IS NULL AND group_v1_id IS NULL AND group_v2_id IS NULL))
);

-- These indices seem to stay intact. Weird.
-- CREATE INDEX session_dm_recipient_id ON sessions(direct_message_recipient_id DESC);
-- CREATE INDEX session_group_v1_id ON sessions(group_v1_id DESC);

INSERT INTO sessions (
    id,
    direct_message_recipient_id,
    group_v1_id,
    is_archived,
    is_pinned,
    is_silent,
    is_muted,
    draft,
    expiring_message_timeout
)
SELECT
    id,
    direct_message_recipient_id,
    group_v1_id,
    is_archived,
    is_pinned,
    is_silent,
    is_muted,
    draft,
    expiring_message_timeout
FROM sessions_old;
DROP TABLE sessions_old;
