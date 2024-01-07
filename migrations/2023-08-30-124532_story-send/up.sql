CREATE TABLE story_sends (
    message_id INTEGER NOT NULL REFERENCES messages(id),
    session_id INTEGER NOT NULL REFERENCES sessions(id),
    sent_timestamp TIMESTAMP NOT NULL,
    allows_replies BOOLEAN NOT NULL,
    distribution_id VARCHAR(36) NOT NULL REFERENCES distribution_lists(distribution_id) ON DELETE CASCADE,

    PRIMARY KEY (message_id, session_id)
);

CREATE INDEX story_sends_message_id_distribution_id_index ON story_sends(message_id, distribution_id);
