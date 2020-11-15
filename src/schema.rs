table! {
    message (id) {
        id -> Integer,
        session_id -> BigInt,
        source -> Text,
        #[sql_name="message"]
        text -> Text,
        timestamp -> BigInt,
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
        timestamp -> BigInt,
    }
}

table! {
    session (id) {
        id -> BigInt,
        source -> Text,
        message -> Text,
        timestamp -> BigInt,
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

allow_tables_to_appear_in_same_query!(message, sentq, session,);

joinable!(sentq -> message (message_id));
