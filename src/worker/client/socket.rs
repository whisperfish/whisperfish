use actix::prelude::*;
use awc::error::WsProtocolError;
use awc::ws;
use futures::prelude::*;

use super::ClientActor;

#[derive(thiserror::Error, Debug)]
pub enum SessionError {
    #[error("Could not connect to the Signal server.")]
    ConnectionError,
}

const BASE_URL: &str = "https://textsecure-service.whispersystems.org";

pub struct SessionActor {
    client_actor: Addr<ClientActor>,
    // XXX: in principle, this type is completely known...
    sink: Box<dyn Sink<ws::Message, Error = WsProtocolError>>,
}

impl SessionActor {
    pub async fn new(
        caller: Addr<ClientActor>,
        client: awc::Client,
        // tel: String,
        // secret: String,
    ) -> Result<Addr<Self>, SessionError> {
        let ws = client.ws(format!("{}/{}", BASE_URL, "v1/websocket/"));

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
