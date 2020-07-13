use actix::prelude::*;
use qmetaobject::*;

use crate::gui::StorageReady;
use crate::sfos::SailfishApp;
use crate::store::Storage;

use libsignal_service::prelude::*;
use libsignal_service_actix::prelude::*;

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
    Running(AwcPushService),
    Unconnected,
}

impl SessionState {
    fn unwrap_service(self) -> AwcPushService {
        match self {
            SessionState::Running(ref s) => s.clone(),
            SessionState::Unconnected => panic!("unwrap_service"),
        }
    }
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
        StorageReady(storage, config): StorageReady,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage.clone());

        let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));
        let service_cfg = ServiceConfiguration {
            service_urls: vec![SERVICE_URL.to_string()],
            cdn_urls: vec![],
            contact_discovery_url: vec![],
        };
        Box::pin(
            async move {
                let phonenumber = phonenumber::parse(None, config.tel).unwrap();
                let e164 = phonenumber
                    .format()
                    .mode(phonenumber::Mode::E164)
                    .to_string();
                log::info!("E164: {}", e164);
                let password = Some(storage.signal_password().await.unwrap());
                let signaling_key = storage.signaling_key().await.unwrap();
                let credentials = Credentials {
                    uuid: None,
                    e164,
                    password,
                    signaling_key,
                };

                let service =
                    AwcPushService::new(service_cfg, credentials.clone(), &useragent, &ROOT_CA);
                let mut receiver = MessageReceiver::new(service.clone());

                let pipe = receiver.create_message_pipe(credentials).await.unwrap();
                let stream = pipe.stream();
                (service, stream)
            }
            .into_actor(self)
            .map(|(service, pipe), act, ctx| {
                act.state = SessionState::Running(service);
                ctx.add_stream(pipe);
            }),
        )
    }
}

impl StreamHandler<Result<Envelope, ServiceError>> for ClientActor {
    fn handle(&mut self, _msg: Result<Envelope, ServiceError>, _ctx: &mut Self::Context) {
        unimplemented!()
    }
}
