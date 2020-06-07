use actix::prelude::*;
use awc::error::WsProtocolError;
use awc::ws;
use futures::prelude::*;

use super::ClientActor;

use crate::store::Storage;

// XXX: attach a reason?
#[derive(Message)]
#[rtype(result = "()")]
pub struct SessionStopped;

#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error("Could not connect to the Signal server.")]
    ConnectionError,
}

const WS_URL: &str = "wss://textsecure-service.whispersystems.org/v1/websocket/";
const ROOT_CA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", "rootCA.crt"));

pub struct SessionActor {
    client_actor: actix::Recipient<SessionStopped>,
    storage: Storage,
    // XXX: in principle, this type is completely known...
    sink: Box<dyn Sink<ws::Message, Error = WsProtocolError>>,
}

impl SessionActor {
    pub async fn new(
        caller: actix::Recipient<SessionStopped>,
        storage: Storage,
        // tel: String,
        // secret: String,
    ) -> Result<Addr<Self>, SessionError> {
        use awc::{ClientBuilder, Connector};
        use std::sync::Arc;
        use std::time::Duration;

        let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));

        let mut ssl_config = rustls::ClientConfig::new();
        // FIXME
        // Forcing HTTP 1.1 because of:
        //   - https://github.com/hyperium/h2/issues/347
        //   - https://github.com/actix/actix-web/issues/1069
        // Currently, Signal does not yet server over 2.0, so this is merely a safeguard.
        ssl_config.alpn_protocols = vec![b"http/1.1".to_vec()];
        ssl_config
            .root_store
            .add_pem_file(&mut std::io::Cursor::new(ROOT_CA))
            .unwrap();

        let client = ClientBuilder::new()
            .connector(
                Connector::new()
                    .rustls(Arc::new(ssl_config))
                    .timeout(Duration::from_secs(10)) // https://github.com/actix/actix-web/issues/1047
                    .finish(),
            )
            .timeout(Duration::from_secs(65)) // as in Signal-Android
            .header("X-Signal-Agent", useragent)
            .finish();
        let ws = client.ws(WS_URL);

        let (_response, framed) = ws.connect().await.map_err(|e| {
            log::warn!("SessionActor has WS error: {:?}", e);
            SessionError::ConnectionError
        })?;
        log::info!("WebSocket connected: {:?}", _response);

        let (sink, stream) = framed.split();

        Ok(SessionActor::create(move |ctx| {
            ctx.add_stream(stream);

            Self {
                client_actor: caller,
                storage,
                sink: Box::new(sink),
            }
        }))
    }
}

impl Actor for SessionActor {
    type Context = Context<Self>;
}

impl StreamHandler<Result<ws::Frame, WsProtocolError>> for SessionActor {
    fn handle(&mut self, _: Result<ws::Frame, WsProtocolError>, _ctx: &mut Self::Context) {
        log::trace!("Message on the WS");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct ClientMock {
        active: bool,
    }

    impl Actor for ClientMock {
        type Context = Context<ClientMock>;
    }
    impl Handler<SessionStopped> for ClientMock {
        type Result = ();

        fn handle(&mut self, _: SessionStopped, _ctx: &mut Self::Context) {
            self.active = false;
        }
    }

    #[actix_rt::test]
    async fn connect_to_ows() -> Result<(), failure::Error> {
        let mock = ClientMock { active: true }.start();
        let storage = Storage::open(&crate::store::memory())?;

        let _client = SessionActor::new(mock.recipient(), storage).await?;

        Ok(())
    }
}
