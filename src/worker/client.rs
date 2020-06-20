use actix::prelude::*;
use qmetaobject::*;

use crate::gui::StorageReady;
use crate::sfos::SailfishApp;
use crate::store::Storage;

use libsignal_service::prelude::*;
use libsignal_service_actix::prelude::*;

const WS_URL: &str = "wss://textsecure-service.whispersystems.org/v1/websocket/";
const SERVICE_URL: &str = "https://textsecure-service.whispersystems.org/";
const ROOT_CA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", "rootCA.crt"));

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
    Running(MessageReceiver<AwcPushService>),
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
    type Result = ResponseFuture<()>;

    fn handle(
        &mut self,
        StorageReady(storage, config): StorageReady,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage.clone());

        let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));
        let service_cfg = ServiceConfiguration {
            service_urls: vec![SERVICE_URL.to_string()],
            cdn_urls: vec![],
            contact_discovery_url: vec![],
        };
        Box::pin(async move {
            let phonenumber = phonenumber::parse(None, config.tel).unwrap();
            let e164 = phonenumber
                .format()
                .mode(phonenumber::Mode::E164)
                .to_string();
            log::info!("E164: {}", e164);
            let password = Some(storage.signal_password().await.unwrap());
            let credentials = Credentials {
                uuid: None,
                e164,
                password,
            };
            let service = AwcPushService::new(service_cfg, credentials, &useragent, &ROOT_CA);

            let mut receiver = MessageReceiver::new(service);
            let messages = receiver.retrieve_messages().await.unwrap();
            log::info!("{} pending messages received.", messages.len());
        })
    }
}
