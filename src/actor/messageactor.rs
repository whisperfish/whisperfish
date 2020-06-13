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
pub struct FetchAllMessages(pub i64);

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
