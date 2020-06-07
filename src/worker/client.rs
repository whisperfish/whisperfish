use actix::prelude::*;
use qmetaobject::*;

use crate::sfos::SailfishApp;
use crate::store::{Storage, StorageReady};

mod socket;
use socket::*;

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct ClientWorker {
    base: qt_base_class!(trait QObject),
    messageReceived: qt_signal!(),
    messageReceipt: qt_signal!(),
    notifyMessage: qt_signal!(),
    promptResetPeerIdentity: qt_signal!(),

    actor: Option<Addr<ClientActor>>,
}

enum SessionState {
    Running(Addr<SessionActor>),
    Unconnected,
}

/// ClientActor keeps track of the connection state.
pub struct ClientActor {
    inner: QObjectBox<ClientWorker>,

    state: SessionState,
    storage: Option<Storage>,
}

impl ClientActor {
    pub fn new(app: &mut SailfishApp) -> Self {
        let inner = QObjectBox::new(ClientWorker::default());
        app.set_object_property("ClientWorker".into(), inner.pinned());

        Self {
            inner,
            state: SessionState::Unconnected,
            storage: None,
        }
    }
}

impl Actor for ClientActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
    }
}

impl Handler<StorageReady> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        StorageReady(storage): StorageReady,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage.clone());
        // FIXME: retry connecting, check whether there's a plausible connection, wait for a
        // connection, ...
        Box::pin(
            SessionActor::new(ctx.address().recipient(), storage)
                .into_actor(self)
                .map(|session, act, _ctx| {
                    act.state = SessionState::Running(
                        session.expect("FIXME: could not immediately connect."),
                    );
                }),
        )
    }
}

impl Handler<SessionStopped> for ClientActor {
    type Result = ();

    fn handle(&mut self, _msg: SessionStopped, _ctx: &mut Self::Context) -> Self::Result {
        log::debug!("SessionActor stopped");
        // FIXME: restart
    }
}
