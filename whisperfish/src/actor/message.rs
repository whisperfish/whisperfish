use crate::gui::StorageReady;
use crate::platform::QmlApp;
use crate::store::Storage;
use crate::worker::ClientActor;
use actix::prelude::*;
use qmetaobject::prelude::*;

mod methods;
use methods::*;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchSession {
    pub id: i32,
    pub mark_read: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct UpdateSession {
    pub id: i32,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchMessage(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct FetchAllMessages(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteMessage(pub i32);

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct SendReaction {
    pub message_id: i32,
    pub sender_id: i32,
    pub emoji: String,
    pub remove: bool,
}

pub struct MessageActor {
    inner: QObjectBox<MessageMethods>,
    storage: Option<Storage>,
}

pub fn pad_fingerprint(fp: &mut String) {
    if fp.len() == 60 {
        // twelve groups, eleven spaces.
        for i in 1..12 {
            fp.insert(6 * i - 1, ' ');
        }
    }
}

impl MessageActor {
    pub fn new(app: &mut QmlApp, client: Addr<ClientActor>) -> Self {
        let inner = QObjectBox::new(MessageMethods::default());
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

    fn handle(&mut self, storageready: StorageReady, _ctx: &mut Self::Context) -> Self::Result {
        self.storage = Some(storageready.storage);
        tracing::trace!("MessageActor has a registered storage");
    }
}

impl Handler<DeleteMessage> for MessageActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteMessage(id): DeleteMessage,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_mut().unwrap().delete_message(id);
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
