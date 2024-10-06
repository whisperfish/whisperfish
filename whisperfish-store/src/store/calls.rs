use super::observer::Observable;
use crate::orm;
use chrono::NaiveDateTime;
use diesel::prelude::*;

impl<O: Observable + Default> super::Storage<O> {
    #[tracing::instrument(skip(self))]
    pub fn insert_one_to_one_call(
        &self,
        call_id: u64,
        timestamp: NaiveDateTime,
        // Recipient is set if the call is incoming
        recipient_id: i32,
        r#type: orm::CallType,
        is_outgoing: bool,
        event: orm::EventType,
        unidentified: bool,
    ) -> i32 {
        let session = self
            .fetch_session_by_recipient_id(recipient_id)
            .expect("fetching session by recipient id");

        let _message = self.insert_call_log(
            Some(recipient_id),
            session.id,
            orm::MessageType::from_call_type(r#type, is_outgoing, event),
            timestamp,
            is_outgoing,
            unidentified,
        );

        use crate::schema::calls;

        let ringer = if is_outgoing {
            self.fetch_self_recipient_id()
        } else {
            recipient_id
        };

        let new_call_id: i32 = diesel::insert_into(calls::table)
            .values((
                calls::call_id.eq(call_id as i32),
                calls::message_id.eq(Some(_message)),
                calls::session_id.eq(session.id),
                calls::type_.eq(r#type),
                calls::is_outbound.eq(is_outgoing),
                calls::event.eq(event),
                calls::timestamp.eq(timestamp),
                calls::ringer.eq(ringer),
                calls::is_read.eq(true),
                calls::local_joined.eq(false),
                calls::group_call_active.eq(false),
            ))
            .returning(calls::id)
            .get_result(&mut *self.db())
            .expect("inserting a call");

        new_call_id
    }

    #[tracing::instrument(skip(self))]
    fn insert_call_log(
        &self,
        recipient: Option<i32>,
        session: i32,
        r#type: orm::MessageType,
        timestamp: NaiveDateTime,
        is_outgoing: bool,
        unidentified: bool,
    ) -> i32 {
        use crate::schema::messages::dsl::*;

        let new_message_id = diesel::insert_into(messages)
            .values((
                session_id.eq(session),
                sender_recipient_id.eq(recipient),
                received_timestamp.eq(Some(chrono::Utc::now().naive_utc())),
                sent_timestamp.eq(timestamp),
                server_timestamp.eq(timestamp),
                is_read.eq(true),
                is_outbound.eq(is_outgoing),
                use_unidentified.eq(unidentified),
                flags.eq(0),
                message_type.eq(r#type),
            ))
            .returning(id)
            .get_result(&mut *self.db())
            .expect("inserting a call log message");

        self.observe_insert(messages, new_message_id);

        new_message_id
    }
}
