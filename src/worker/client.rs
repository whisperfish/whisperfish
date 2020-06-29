use actix::prelude::*;
use qmetaobject::*;

use crate::sfos::SailfishApp;
use crate::store::{StorageReady, Storage};

mod socket;
use socket::SessionActor;

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

const ROOT_CA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", "rootCA.crt"));

/// ClientActor keeps track of the connection state.
pub struct ClientActor {
    inner: QObjectBox<ClientWorker>,
    client: awc::Client,

    state: SessionState,
    storage: Option<Storage>,
}

impl ClientActor {
    pub fn new(app: &mut SailfishApp) -> Self {
        use awc::{ClientBuilder, Connector};
        use std::sync::Arc;

        let inner = QObjectBox::new(ClientWorker::default());
        app.set_object_property("ClientWorker".into(), inner.pinned());

        let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));

        let mut ssl_config = rustls::ClientConfig::new();
        ssl_config
            .root_store
            .add_pem_file(&mut std::io::Cursor::new(ROOT_CA))
            .unwrap();

        let client = ClientBuilder::new()
            .connector(Connector::new().rustls(Arc::new(ssl_config)).finish())
            .header("X-Signal-Agent", useragent)
            .finish();

        Self {
            inner,
            client,
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

// XXX: attach a reason?
#[derive(Message)]
#[rtype(result = "()")]
struct SessionStopped;

impl Handler<StorageReady> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        StorageReady(storage): StorageReady,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage);
        // FIXME: retry connecting, check whether there's a plausible connection, wait for a
        // connection, ...
        Box::pin(
            SessionActor::new(ctx.address(), self.client.clone())
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
