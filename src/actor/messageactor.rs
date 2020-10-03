use crate::gui::StorageReady;
use crate::model::message::MessageModel;
use crate::sfos::SailfishApp;
use crate::store::Storage;

use actix::prelude::*;
use qmetaobject::*;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchSession {
    pub id: i64,
    pub mark_read: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchMessage(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchAllMessages(pub i64);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteMessage(pub i32, pub usize);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct QueueMessage {
    pub source: String,
    pub message: String,
    pub attachment: String,
    pub group: String,
}

pub struct MessageActor {
    inner: QObjectBox<MessageModel>,
    storage: Option<Storage>,
}

impl MessageActor {
    pub fn new(app: &mut SailfishApp) -> Self {
        let inner = QObjectBox::new(MessageModel::default());
        app.set_object_property("MessageModel".into(), inner.pinned());

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
        let mut sess = storage
            .fetch_session(sid)
            .expect("FIXME No session returned!");
        if mark_read {
            storage.mark_session_read(&sess);
            sess.unread = false;
        }
        self.inner.pinned().borrow_mut().handle_fetch_session(sess);
    }
}

impl Handler<FetchMessage> for MessageActor {
    type Result = ();

    fn handle(&mut self, FetchMessage(id): FetchMessage, _ctx: &mut Self::Context) -> Self::Result {
        let message = self.storage.as_ref().unwrap().fetch_message(id);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_fetch_message(message.expect("FIXME No message returned!"));
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
            .handle_fetch_all_messages(messages.expect("FIXME No messages returned!"));
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
        assert!(
            &msg.attachment == "",
            "Sending attachments is unimplemented"
        );
        assert!(&msg.group == "", "Sending group messages is unimplemented");

        let storage = self.storage.as_mut().unwrap();
        let (msg, _session) = storage.process_message(
            crate::store::NewMessage {
                session_id: None,
                source: msg.source,
                text: msg.message,
                timestamp: chrono::Utc::now().timestamp_millis(),
                has_attachment: msg.attachment.len() != 0,
                attachment: if msg.attachment.len() != 0 {
                    Some(msg.attachment)
                } else {
                    None
                },
                flags: 0,
                outgoing: true,
                mime_type: None,
                received: false,
                sent: false,
            },
            None,
            false,
        );

        storage.queue_message(&msg);

        self.inner.pinned().borrow_mut().handle_queue_message(msg);
    }
}
