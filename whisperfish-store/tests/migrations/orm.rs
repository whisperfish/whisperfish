//! Helper structs that map directly on `old_schema::*`

use chrono::prelude::*;
use whisperfish_store::schema::migrations as schemas;

pub mod original {
    use super::*;
    use schemas::original::*;

    /// Session as it relates to the schema
    #[derive(Queryable, Debug, Clone)]
    pub struct Session {
        pub id: i64,
        pub source: String,
        pub message: String,
        pub timestamp: i64,
        pub sent: bool,
        pub received: bool,
        pub unread: bool,
        pub is_group: bool,
        pub group_members: Option<String>,
        #[allow(dead_code)]
        pub group_id: Option<String>,
        pub group_name: Option<String>,
        pub has_attachment: bool,
    }

    /// ID-free Session model for insertions
    #[derive(Insertable, Debug)]
    #[diesel(table_name = session)]
    pub struct NewSession {
        pub source: String,
        pub message: String,
        pub timestamp: i64,
        pub sent: bool,
        pub received: bool,
        pub unread: bool,
        pub is_group: bool,
        pub group_members: Option<String>,
        #[allow(dead_code)]
        pub group_id: Option<String>,
        pub group_name: Option<String>,
        pub has_attachment: bool,
    }

    /// Message as it relates to the schema
    #[derive(Queryable, Debug)]
    pub struct Message {
        pub id: i64,
        pub sid: i64,
        pub source: String,
        pub message: String, // NOTE: "text" in schema, doesn't apparently matter
        pub timestamp: i64,
        pub sent: bool,
        pub received: bool,
        pub flags: i32,
        pub attachment: Option<String>,
        pub mimetype: Option<String>,
        pub hasattachment: bool,
        pub outgoing: bool,
        pub queued: bool,
        pub message_type: Option<String>,
    }

    /// ID-free Message model for insertions
    #[derive(Insertable)]
    #[diesel(table_name = message)]
    pub struct NewMessage {
        pub session_id: Option<i64>,
        pub source: String,
        pub text: String,
        pub timestamp: i64,
        pub sent: bool,
        pub received: bool,
        pub flags: i32,
        pub attachment: Option<String>,
        pub mime_type: Option<String>,
        pub has_attachment: bool,
        pub outgoing: bool,
        pub message_type: Option<String>,
    }
}

mod corrected_datatypes {
    use super::*;
    use schemas::corrected_datatypes::*;

    /// Session as it relates to the schema
    #[derive(Queryable, Debug, Clone)]
    pub struct Session {
        pub id: i32,
        pub source: String,
        pub message: String,
        pub timestamp: NaiveDateTime,
        pub sent: bool,
        pub received: bool,
        pub unread: bool,
        pub is_group: bool,
        pub group_members: Option<String>,
        #[allow(dead_code)]
        pub group_id: Option<String>,
        pub group_name: Option<String>,
        pub has_attachment: bool,
    }

    /// ID-free Session model for insertions
    #[derive(Insertable, Debug)]
    #[diesel(table_name = session)]
    pub struct NewSession {
        pub source: String,
        pub message: String,
        pub timestamp: NaiveDateTime,
        pub sent: bool,
        pub received: bool,
        pub unread: bool,
        pub is_group: bool,
        pub group_members: Option<String>,
        #[allow(dead_code)]
        pub group_id: Option<String>,
        pub group_name: Option<String>,
        pub has_attachment: bool,
    }

    /// Message as it relates to the schema
    #[derive(Queryable, Debug)]
    pub struct Message {
        pub id: i32,
        pub sid: i32,
        pub source: String,
        pub message: String, // NOTE: "text" in schema, doesn't apparently matter
        pub timestamp: NaiveDateTime,
        pub sent: bool,
        pub received: bool,
        pub flags: i32,
        pub attachment: Option<String>,
        pub mimetype: Option<String>,
        pub hasattachment: bool,
        pub outgoing: bool,
        pub queued: bool,
        pub message_type: Option<String>,
    }

    /// ID-free Message model for insertions
    #[derive(Insertable)]
    #[diesel(table_name = message)]
    pub struct NewMessage {
        pub session_id: Option<i32>,
        pub source: String,
        pub text: String,
        pub timestamp: NaiveDateTime,
        pub sent: bool,
        pub received: bool,
        pub flags: i32,
        pub attachment: Option<String>,
        pub mime_type: Option<String>,
        pub has_attachment: bool,
        pub outgoing: bool,
    }
}

pub mod current {
    pub use whisperfish_store::orm::*;
}
