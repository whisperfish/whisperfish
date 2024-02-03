#![allow(non_snake_case)]

use crate::worker::{
    DeleteMessage, DeleteMessageForAll, ExportAttachment, QueueExpiryUpdate, QueueMessage,
};

use super::*;
use futures::prelude::*;
use qmeta_async::with_executor;

#[derive(QObject, Default)]
pub struct MessageMethods {
    base: qt_base_class!(trait QObject),
    pub actor: Option<Addr<MessageActor>>,
    pub client_actor: Option<Addr<ClientActor>>,

    createMessage: qt_method!(
        fn(&self, session_id: i32, message: QString, attachment: QString, quote: i32, add: bool)
    ),
    createExpiryUpdate: qt_method!(fn(&self, session_id: i32, expires_in: i32)),

    sendMessage: qt_method!(fn(&self, mid: i32)),
    sendReaction:
        qt_method!(fn(&self, message_id: i32, sender_id: i32, emoji: QString, remove: bool)),
    endSession: qt_method!(fn(&self, recipient_id: i32)),

    remove: qt_method!(fn(&self, id: i32)),
    removeForAll: qt_method!(fn(&self, id: i32)),

    exportAttachment: qt_method!(fn(&self, attachment_id: i32)),
}

impl MessageMethods {
    #[with_executor]
    fn createMessage(
        &mut self,
        session_id: i32,
        message: QString,
        attachment: QString,
        quote: i32,
        _add: bool,
    ) {
        let message = message.to_string();
        let attachment = attachment.to_string();

        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(QueueMessage {
                    session_id,
                    message,
                    attachment,
                    quote,
                })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    fn createExpiryUpdate(&mut self, session_id: i32, expires_in: i32) {
        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(QueueExpiryUpdate {
                    session_id,
                    expires_in: match expires_in {
                        x if x > 0 => Some(std::time::Duration::from_secs(x as u64)),
                        _ => None,
                    },
                })
                .map(Result::unwrap),
        );
    }

    /// Called when a message should be queued to be sent to OWS
    #[with_executor]
    fn sendMessage(&mut self, mid: i32) {
        actix::spawn(
            self.client_actor
                .as_mut()
                .unwrap()
                .send(crate::worker::SendMessage(mid))
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    fn sendReaction(&self, message_id: i32, sender_id: i32, emoji: QString, remove: bool) {
        let emoji = emoji.to_string();

        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(SendReaction {
                    message_id,
                    sender_id,
                    emoji,
                    remove,
                })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    fn endSession(&mut self, id: i32) {
        actix::spawn(
            self.client_actor
                .as_mut()
                .unwrap()
                .send(crate::worker::EndSession(id))
                .map(Result::unwrap),
        );
    }

    /// Remove a message from the database.
    #[with_executor]
    pub fn remove(&self, id: i32) {
        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(DeleteMessage(id))
                .map(Result::unwrap),
        );

        tracing::trace!("Dispatched DeleteMessage({})", id);
    }

    /// Remove a message from everyone and from the database.
    #[with_executor]
    pub fn removeForAll(&self, id: i32) {
        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(DeleteMessageForAll(id))
                .map(Result::unwrap),
        );

        tracing::trace!("Dispatched DeleteMessageRemotely({})", id);
    }

    #[with_executor]
    pub fn exportAttachment(&self, attachment_id: i32) {
        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(ExportAttachment { attachment_id })
                .map(Result::unwrap),
        );

        tracing::trace!("Dispatched ExportAttachment({})", attachment_id);
    }
}
