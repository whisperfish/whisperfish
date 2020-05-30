use crate::store::sql_types::*;

table! {
    message (id) {
        id -> Nullable<Integer>,
        session_id -> Nullable<Integer>,
        source -> Nullable<Text>,
        #[sql_name="message"]
        text -> Nullable<SqliteString>,
        timestamp -> Nullable<Integer>,
        sent -> Nullable<Integer>,
        received -> Nullable<Integer>,
        flags -> Nullable<Integer>,
        attachment -> Nullable<Text>,
        mime_type -> Nullable<Text>,
        has_attachment -> Nullable<Integer>,
        outgoing -> Nullable<Integer>,
    }
}

table! {
    sentq (message_id) {
        message_id -> Nullable<Integer>,
        timestamp -> Nullable<Timestamp>,
    }
}

table! {
    session (id) {
        id -> Nullable<Integer>,
        source -> Nullable<Text>,
        message -> Nullable<Text>,
        timestamp -> Nullable<Integer>,
        sent -> Nullable<Integer>,
        received -> Nullable<Integer>,
        unread -> Nullable<Integer>,
        is_group -> Nullable<Integer>,
        group_members -> Nullable<Text>,
        group_id -> Nullable<Text>,
        group_name -> Nullable<Text>,
        has_attachment -> Nullable<Integer>,
    }
}

allow_tables_to_appear_in_same_query!(
    message,
    sentq,
    session,
);
