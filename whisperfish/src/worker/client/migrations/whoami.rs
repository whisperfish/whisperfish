use super::*;
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "()")]
pub struct WhoAmI;

impl Handler<WhoAmI> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: WhoAmI, _ctx: &mut Self::Context) -> Self::Result {
        let config = std::sync::Arc::clone(&self.config);
        let mut i_ws = self.i_ws.clone().unwrap();

        Box::pin(
            async move {
                if let (Some(aci), Some(pni)) = (config.get_aci(), config.get_pni()) {
                    tracing::trace!("ACI ({}) and PNI ({}) already set.", aci, pni);
                    return Ok(None);
                }

                let response = i_ws.whoami().await?;

                Ok::<_, anyhow::Error>(Some((response, config)))
            }
            .instrument(tracing::debug_span!("whoami"))
            .into_actor(self)
            .map(move |result, act, _ctx| {
                if result.is_ok() {
                    act.migration_state.notify_whoami();
                }
                let (result, config) = match result {
                    Ok(Some(result)) => result,
                    Ok(None) => return,
                    Err(e) => {
                        tracing::error!("fetching UUID: {}", e);
                        return;
                    }
                };
                tracing::info!("Retrieved ACI ({}) and PNI ({})", result.aci, result.pni);

                if let Some(credentials) = act.credentials.as_mut() {
                    credentials.aci = Some(result.aci);
                    config.set_aci(result.aci);
                    config.set_pni(result.pni);
                    config.write_to_file().expect("write config");
                } else {
                    tracing::error!("Credentials was none while setting UUID");
                }
                act.self_pni = Some(Pni::from(result.pni));
            }),
        )
    }
}
