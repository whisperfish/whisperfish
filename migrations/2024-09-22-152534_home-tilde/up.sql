-- Replace /home/nemo and /home/defaultuser in attachment paths with ~
-- Note that the SailfishOS RPM validator really doesn't like us hardcoding
-- paths, but it notably doesn't fail if we don't have the trailing slash explicitely.
UPDATE attachments SET attachment_path = REPLACE(attachment_path, CONCAT('/home/nemo', '/'), '~/');
UPDATE attachments SET attachment_path = REPLACE(attachment_path, CONCAT('/home/defaultuser', '/'), '~/');
