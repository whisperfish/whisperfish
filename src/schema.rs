table! {
    message (id) {
        id -> Integer,
        session_id -> Integer,
        source -> Text,
        #[sql_name = "message"]
        text -> Text,
        timestamp -> Timestamp,
        sent -> Bool,
        received -> Bool,
        flags -> Integer,
        attachment -> Nullable<Text>,
        mime_type -> Nullable<Text>,
        has_attachment -> Bool,
        outgoing -> Bool,
    }
}

table! {
    sentq (message_id) {
        message_id -> Integer,
        timestamp -> Timestamp,
    }
}

table! {
    session (id) {
        id -> Integer,
        source -> Text,
        message -> Text,
        timestamp -> Timestamp,
        sent -> Bool,
        received -> Bool,
        unread -> Bool,
        is_group -> Bool,
        group_members -> Nullable<Text>,
        group_id -> Nullable<Text>,
        group_name -> Nullable<Text>,
        has_attachment -> Bool,
    }
}

joinable!(message -> session (session_id));
joinable!(sentq -> message (message_id));

allow_tables_to_appear_in_same_query!(
    message,
    sentq,
    session,
);
