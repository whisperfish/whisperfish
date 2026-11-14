use super::observer::Observable;
use crate::orm;
use chrono::NaiveDateTime;
use diesel::prelude::*;

impl<O: Observable + Default> super::Storage<O> {
    /// Return (session, call ID)
    #[allow(clippy::too_many_arguments)]
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
    ) -> (orm::Session, i32) {
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

        (session, new_call_id)
    }

    /// Fetch the one-to-one call row belonging to a call ID and peer.
    ///
    /// Returns `None` if no call was recorded for this peer and call ID.
    #[tracing::instrument(skip(self))]
    pub fn fetch_one_to_one_call(&self, call_id: u64, recipient_id: i32) -> Option<orm::Call> {
        let session = self.fetch_session_by_recipient_id(recipient_id)?;
        use crate::schema::calls;
        calls::table
            .filter(calls::call_id.eq(call_id as i32))
            .filter(calls::session_id.eq(session.id))
            .first(&mut *self.db())
            .optional()
            .expect("fetching a call")
    }

    /// Update the lifecycle state of a one-to-one call,
    /// and keep the call log message's type in sync.
    ///
    /// Returns the updated call log message ID,
    /// or `None` if the call or its message could not be found.
    #[tracing::instrument(skip(self))]
    pub fn update_one_to_one_call_event(
        &self,
        call_id: u64,
        recipient_id: i32,
        event: orm::EventType,
    ) -> Option<i32> {
        let session = self.fetch_session_by_recipient_id(recipient_id)?;
        use crate::schema::{calls, messages};

        let call: orm::Call = calls::table
            .filter(calls::call_id.eq(call_id as i32))
            .filter(calls::session_id.eq(session.id))
            .first(&mut *self.db())
            .optional()
            .expect("fetching a call")?;

        diesel::update(calls::table)
            .filter(calls::id.eq(call.id))
            .set(calls::event.eq(event))
            .execute(&mut *self.db())
            .expect("updating a call");

        let Some(message_id) = call.message_id else {
            tracing::warn!(
                "Call log message for call {} was deleted; not updating.",
                call.id
            );
            return None;
        };

        let message_type = orm::MessageType::from_call_type(call.r#type, call.is_outbound, event);
        diesel::update(messages::table)
            .filter(messages::id.eq(message_id))
            .set(messages::message_type.eq(Some(message_type)))
            .execute(&mut *self.db())
            .expect("updating a call log message");

        self.observe_update(messages::table, message_id)
            .with_relation(crate::schema::sessions::table, session.id);

        Some(message_id)
    }

    /// Conclude all calls that are ringing.
    ///
    /// To be called on startup, to end calls that were ringing when the application stopped.
    ///
    /// Returns the number of concluded calls.
    #[tracing::instrument(skip(self))]
    pub fn conclude_stale_calls(&self) -> usize {
        use crate::schema::messages;

        let mut swept =
            self.conclude_calls_with_event(orm::EventType::Ringing, orm::EventType::Missed);
        swept.extend(
            self.conclude_calls_with_event(
                orm::EventType::OutgoingRing,
                orm::EventType::NotAccepted,
            ),
        );

        // All swept calls are unanswered, so their message type
        // only depends on the call's media type.
        for call_type in [orm::CallType::Audio, orm::CallType::Video] {
            let missed_message_type =
                orm::MessageType::from_call_type(call_type, false, orm::EventType::Missed);
            let missed_message_ids: Vec<i32> = swept
                .iter()
                .filter(|(_, _, ty)| *ty == call_type)
                .filter_map(|(message_id, _, _)| *message_id)
                .collect();
            if missed_message_ids.is_empty() {
                continue;
            }
            diesel::update(messages::table)
                .filter(messages::id.eq_any(missed_message_ids))
                .set(messages::message_type.eq(Some(missed_message_type)))
                .execute(&mut *self.db())
                .expect("updating call log messages");
        }

        for (message_id, session_id, _) in &swept {
            if let Some(message_id) = message_id {
                self.observe_update(messages::table, *message_id)
                    .with_relation(crate::schema::sessions::table, *session_id);
            }
        }

        swept.len()
    }

    /// Conclude all calls in a given stale lifecycle state.
    ///
    /// Returns concluded call log message, session, and media type.
    fn conclude_calls_with_event(
        &self,
        stale: orm::EventType,
        concluded: orm::EventType,
    ) -> Vec<(Option<i32>, i32, orm::CallType)> {
        use crate::schema::calls;
        diesel::update(calls::table)
            .filter(calls::event.eq(stale))
            .set(calls::event.eq(concluded))
            .returning((calls::message_id, calls::session_id, calls::type_))
            .get_results(&mut *self.db())
            .expect("concluding stale calls")
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

        self.observe_insert(messages, new_message_id)
            .with_relation(crate::schema::sessions::table, session);

        new_message_id
    }
}
