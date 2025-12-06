use crate::store::orm::shorten;

use super::*;
use chrono::prelude::*;
use libsignal_service::{
    proto::{typing_message, TypingMessage},
    protocol::ServiceId,
};
use std::collections::HashMap;

const TYPING_EXPIRY_DELAY: chrono::Duration = chrono::Duration::seconds(5);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct TypingNotification {
    pub typing: TypingMessage,
    pub sender: ServiceId,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct UpdateTypingNotifications;

#[derive(Clone)]
pub(super) struct TypingQueueItem {
    inner: TypingMessage,
    sender: ServiceId,
    expire: DateTime<Utc>,
}

impl Handler<TypingNotification> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        TypingNotification { typing, sender }: TypingNotification,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let started = if let Some(timestamp) = typing.timestamp {
            DateTime::from_naive_utc_and_offset(
                whisperfish_store::millis_to_naive_chrono(timestamp),
                Utc,
            )
        } else {
            Utc::now()
        };
        if typing.action() == typing_message::Action::Started {
            let expire = started + TYPING_EXPIRY_DELAY;
            if expire < Utc::now() {
                tracing::debug!(
                    "Received a typing notification too late (sent {}, now is {}, expired {}).",
                    started,
                    Utc::now(),
                    expire,
                );
                return;
            }

            self.typing_queue.push_back(TypingQueueItem {
                inner: typing,
                sender,
                expire,
            });
        } else {
            self.typing_queue
                .retain(|item| !(item.inner.group_id == typing.group_id && item.sender == sender));
        }

        ctx.notify(UpdateTypingNotifications);
    }
}

impl Handler<UpdateTypingNotifications> for SessionActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: UpdateTypingNotifications, ctx: &mut Self::Context) -> Self::Result {
        let now = Utc::now();

        // Remove all expired typing notifications.
        while let Some(item) = self.typing_queue.front() {
            if item.expire <= now {
                self.typing_queue.pop_front();
            } else {
                break;
            }
        }

        if let Some(item) = self.typing_queue.front() {
            let next_delay = item.expire - now;
            ctx.notify_later(
                UpdateTypingNotifications,
                next_delay
                    .to_std()
                    .expect("positive duration to next expiry"),
            );
        }

        let typings = self.typing_queue.clone();

        let storage = self.storage.clone().unwrap();
        let fetch_sessions = async move {
            let mut map: HashMap<i32, Vec<orm::Recipient>> = HashMap::new();
            for typing in &typings {
                let sender_recipient = storage.fetch_recipient(&typing.sender);
                if sender_recipient.is_none() {
                    continue;
                }
                let sender_recipient = sender_recipient.unwrap();

                let group_id = typing.inner.group_id.as_ref().map(hex::encode);
                let session = match &group_id {
                    // Group V1
                    Some(group_id) if group_id.len() == 32 => {
                        storage.fetch_session_by_group_v1_id(group_id)
                    }
                    // Group V2
                    Some(group_id) if group_id.len() == 64 => {
                        storage.fetch_session_by_group_v2_id(group_id)
                    }
                    // Group version ?!?
                    Some(group_id) => {
                        anyhow::bail!("Impossible group id {} for typing message", group_id)
                    }
                    // 1:1
                    None => storage.fetch_session_by_recipient_id(sender_recipient.id),
                };
                if session.is_none() {
                    tracing::trace!(
                        "No session found for sender {:?} in {:?}",
                        sender_recipient.uuid,
                        shorten(group_id.unwrap_or("1:1".to_string()).as_ref(), 12),
                    );
                    continue;
                }
                let session = session.unwrap();

                let session: &mut Vec<orm::Recipient> = map.entry(session.id).or_default();
                if !session.iter().any(|x| x.id == sender_recipient.id) {
                    session.push(sender_recipient);
                }
            }

            Ok(map)
        };

        Box::pin(
            fetch_sessions
                .into_actor(self)
                .map(|result, act, _ctx| match result {
                    Ok(typings) => {
                        if !typings.is_empty() {
                            tracing::info!("Sending typings {:?} to model", typings);
                            act.handle_update_typing(&typings);
                        }
                    }
                    Err(e) => tracing::error!("Could not process typings: {}", e),
                }),
        )
    }
}
