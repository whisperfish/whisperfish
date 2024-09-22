-- Replace /home/nemo and /home/defaultuser in attachment paths with ~
UPDATE attachments SET attachment_path = REPLACE(attachment_path, '/home/nemo/', '~/');
UPDATE attachments SET attachment_path = REPLACE(attachment_path, '/home/defaultuser/', '~/');
