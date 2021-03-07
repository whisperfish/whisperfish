use crate::gui::StorageReady;
use crate::model::message::MessageModel;
use crate::sfos::SailfishApp;
use crate::store::{orm, Storage};
use crate::worker::ClientActor;

use actix::prelude::*;
use qmetaobject::*;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchSession {
    pub id: i32,
    pub mark_read: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchMessage(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchAllMessages(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteMessage(pub i32, pub usize);

#[derive(actix::Message, Debug)]
#[rtype(result = "()")]
pub struct QueueMessage {
    pub source: String,
    pub message: String,
    pub attachment: String,
    pub group: String,
}

#[derive(Message)]
#[rtype(result = "()")]
/// Send a ne
pub struct EndSession(pub String);

pub struct MessageActor {
    inner: QObjectBox<MessageModel>,
    storage: Option<Storage>,
}

impl MessageActor {
    pub fn new(app: &mut SailfishApp, client: Addr<ClientActor>) -> Self {
        let inner = QObjectBox::new(MessageModel::default());
        app.set_object_property("MessageModel".into(), inner.pinned());
        inner.pinned().borrow_mut().client_actor = Some(client);

        Self {
            inner,
            storage: None,
        }
    }
}

impl Actor for MessageActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
    }
}

impl Handler<StorageReady> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        StorageReady(storage, _config): StorageReady,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage);
        log::trace!("MessageActor has a registered storage");
    }
}

impl Handler<FetchSession> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        FetchSession { id: sid, mark_read }: FetchSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let storage = self.storage.as_ref().unwrap();
        let sess = storage
            .fetch_session_by_id(sid)
            .expect("FIXME No session returned!");
        if mark_read {
            storage.mark_session_read(sess.id);
        }

        let peer_identity = if let orm::SessionType::DirectMessage(recipient) = &sess.r#type {
            // FIXME UUID
            match storage.peer_identity(recipient.e164.as_deref().expect("fixme")) {
                Ok(ident) => ident,
                Err(e) => {
                    log::warn!(
                        "FetchSession: returning empty string for peer_ident because {:?}",
                        e
                    );
                    String::new()
                }
            }
        } else {
            String::new()
        };

        self.inner
            .pinned()
            .borrow_mut()
            .handle_fetch_session(sess, peer_identity);
    }
}

impl Handler<FetchMessage> for MessageActor {
    type Result = ();

    fn handle(&mut self, FetchMessage(id): FetchMessage, _ctx: &mut Self::Context) -> Self::Result {
        let message = self
            .storage
            .as_ref()
            .unwrap()
            .fetch_message_by_id(id)
            .expect(&format!("No message with id {}", id));
        let recipient = if let Some(id) = message.sender_recipient_id {
            self.storage.as_ref().unwrap().fetch_recipient_by_id(id)
        } else {
            None
        };
        self.inner
            .pinned()
            .borrow_mut()
            .handle_fetch_message(message, recipient);
    }
}

impl Handler<FetchAllMessages> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        FetchAllMessages(sid): FetchAllMessages,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let messages = self.storage.as_ref().unwrap().fetch_all_messages(sid);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_fetch_all_messages(messages);
    }
}

impl Handler<DeleteMessage> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteMessage(id, idx): DeleteMessage,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let del_rows = self.storage.as_ref().unwrap().delete_message(id);
        self.inner.pinned().borrow_mut().handle_delete_message(
            id,
            idx,
            del_rows.expect("FIXME no rows deleted"),
        );
    }
}

impl Handler<QueueMessage> for MessageActor {
    type Result = ();

    fn handle(&mut self, msg: QueueMessage, _ctx: &mut Self::Context) -> Self::Result {
        log::trace!("MessageActor::handle({:?})", msg);
        assert!(&msg.group == "", "Creating a new group is unimplemented");

        let storage = self.storage.as_mut().unwrap();

        let has_attachment = !msg.attachment.is_empty();

        let (msg, _session) = storage.process_message(
            crate::store::NewMessage {
                session_id: None,
                source: msg.source,
                text: msg.message,
                timestamp: chrono::Utc::now().naive_utc(),
                has_attachment,
                mime_type: if has_attachment {
                    Some(
                        mime_guess::from_path(&msg.attachment)
                            .first_or_octet_stream()
                            .essence_str()
                            .into(),
                    )
                } else {
                    None
                },
                attachment: if has_attachment {
                    Some(msg.attachment)
                } else {
                    None
                },
                flags: 0,
                outgoing: true,
                received: false,
                sent: false,
                is_read: true,
            },
            None,
        );

        self.inner.pinned().borrow_mut().handle_queue_message(msg);
    }
}

impl Handler<EndSession> for MessageActor {
    type Result = ();

    fn handle(&mut self, EndSession(e164): EndSession, _ctx: &mut Self::Context) -> Self::Result {
        use libsignal_service::content::DataMessageFlags;
        log::trace!("MessageActor::EndSession({})", e164);

        let storage = self.storage.as_mut().unwrap();

        let (msg, _session) = storage.process_message(
            crate::store::NewMessage {
                session_id: None,
                source: e164,
                text: "[Whisperfish] Reset secure session".into(),
                timestamp: chrono::Utc::now().naive_utc(),
                has_attachment: false,
                mime_type: None,
                attachment: None,
                flags: DataMessageFlags::EndSession.into(),
                outgoing: true,
                received: false,
                sent: false,
                is_read: true,
            },
            None,
        );

        self.inner.pinned().borrow_mut().handle_queue_message(msg);
    }
}
