-- We had some typos in the groupv2/up.sql

DROP TRIGGER assert_uuid_for_group_v2_members;

CREATE TRIGGER assert_uuid_for_group_v2_members
  BEFORE INSERT ON group_v2_members
BEGIN
  SELECT
    RAISE (ABORT, 'UUID or profile key of GroupV2 member is not set')
  WHERE EXISTS (
    SELECT
      recipients.id
    FROM recipients
    WHERE recipients.id = NEW.recipient_id
      AND (recipients.uuid IS NULL
          OR recipients.profile_key IS NULL)
  );
END;
