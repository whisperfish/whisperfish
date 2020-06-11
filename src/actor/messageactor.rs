use crate::model::message::MessageModel;
use crate::sfos::SailfishApp;
use crate::store::{Storage, StorageReady};

use actix::prelude::*;
use qmetaobject::*;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchSession(pub i64);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchMessage(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchAllMessages(pub i64);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteMessage(pub i32, pub usize);

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
        StorageReady(storage): StorageReady,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage);
        log::trace!("MessageActor has a registered storage");
    }
}

impl Handler<FetchSession> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        FetchSession(sid): FetchSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let sess = self.storage.as_ref().unwrap().fetch_session(sid);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_fetch_session(sess.expect("FIXME No session returned!"));
    }
}

impl Handler<FetchMessage> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        FetchMessage(id): FetchMessage,
        _ctx: &mut Self::Context
    ) -> Self::Result {
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
        _ctx: &mut Self::Context
    ) -> Self::Result {
        let del_rows = self.storage.as_ref().unwrap().delete_message(id);
        self.inner
            .pinned()
            .borrow_mut()
            .handle_delete_message(id, idx, del_rows.expect("FIXME no rows deleted"));
    }
}
