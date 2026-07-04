#![allow(non_snake_case)]

use crate::worker::ClientActor;
use crate::worker::{
    DeleteMessage, DeleteMessageForAll, ExportAttachment, NewAttachment, QueueExpiryUpdate,
    QueueMessage, SendReaction,
};
use actix::prelude::*;
use futures::prelude::*;
use qmeta_async::with_executor;
use qmetaobject::QMetaType;
use qmetaobject::prelude::*;
use qttypes::{QVariantList, QVariantMap};

pub fn pad_fingerprint(fp: &mut String) {
    if fp.len() == 60 {
        // twelve groups, eleven spaces.
        for i in 1..12 {
            fp.insert(6 * i - 1, ' ');
        }
    }
}

#[derive(QObject, Default)]
pub struct MessageMethods {
    base: qt_base_class!(trait QObject),
    pub client_actor: Option<Addr<ClientActor>>,

    createMessage: qt_method!(
        fn(
            &self,
            session_id: i32,
            message: QString,
            attachment: QVariantList,
            quote: i32,
            add: bool,
            is_voice_note: bool,
        )
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
    #[tracing::instrument(skip(self))]
    fn createMessage(
        &mut self,
        session_id: i32,
        message: QString,
        mut attachments_qml: QVariantList,
        quote: i32,
        _add: bool,
        is_voice_note: bool,
    ) {
        let message = message.to_string();
        let mut attachments: Vec<NewAttachment> = vec![];

        while !attachments_qml.is_empty() {
            let attachment_map = attachments_qml.remove(0);
            // QMetaType::QVariantMap = 8
            // https://doc.qt.io/archives/qt-5.6/qmetatype.html#Type-enum
            if attachment_map.user_type() == 8 {
                let attachment = QVariantMap::from_qvariant(attachment_map).unwrap();
                attachments.push(NewAttachment {
                    path: attachment
                        .value("data".into(), QVariant::default())
                        .to_qstring()
                        .to_string(),
                    mime_type: attachment
                        .value("type".into(), QVariant::default())
                        .to_qstring()
                        .to_string(),
                });
            }
        }

        actix::spawn(
            self.client_actor
                .as_ref()
                .unwrap()
                .send(QueueMessage {
                    session_id,
                    message,
                    attachments,
                    quote,
                    is_voice_note,
                })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self))]
    fn endSession(&mut self, id: i32) {
        actix::spawn(
            self.client_actor
                .as_mut()
                .unwrap()
                .send(crate::worker::ResetSession::Recipient(id))
                .map(Result::unwrap),
        );
    }

    /// Remove a message from the database.
    #[with_executor]
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self))]
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pad_fingerprint_smoke() {
        let mut s = "892064087450853131489552767731995657884565179277972848560834".to_string();
        pad_fingerprint(&mut s);
        assert_eq!(
            s,
            "89206 40874 50853 13148 95527 67731 99565 78845 65179 27797 28485 60834"
        );
    }
}
