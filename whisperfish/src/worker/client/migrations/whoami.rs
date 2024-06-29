use super::*;
use actix::prelude::*;
use libsignal_service::push_service::WhoAmIResponse;

#[derive(Message)]
#[rtype(result = "()")]
pub struct WhoAmI;

impl Handler<WhoAmI> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: WhoAmI, _ctx: &mut Self::Context) -> Self::Result {
        let mut service = self.authenticated_service();
        let config = std::sync::Arc::clone(&self.config);
        let config2 = std::sync::Arc::clone(&self.config);

        Box::pin(
            async move {
                if let (Some(aci), Some(pni)) = (config.get_aci(), config.get_pni()) {
                    tracing::trace!("ACI ({}) and PNI ({}) already set.", aci, pni);
                    return Ok(None);
                }

                let response = service.whoami().await?;

                Ok::<_, anyhow::Error>(Some(response))
            }
            .instrument(tracing::debug_span!("whoami"))
            .into_actor(self)
            .map(
                move |result: Result<Option<WhoAmIResponse>, _>, act, _ctx| {
                    if result.is_ok() {
                        act.migration_state.notify_whoami();
                    }
                    let result = match result {
                        Ok(Some(result)) => result,
                        Ok(None) => return,
                        Err(e) => {
                            tracing::error!("fetching UUID: {}", e);
                            return;
                        }
                    };
                    tracing::info!("Retrieved ACI ({}) and PNI ({})", result.uuid, result.pni);

                    if let Some(credentials) = act.credentials.as_mut() {
                        credentials.aci = Some(result.uuid);
                        config2.set_aci(result.uuid);
                        config2.set_pni(result.pni);
                        config2.write_to_file().expect("write config");
                    } else {
                        tracing::error!("Credentials was none while setting UUID");
                    }
                    act.self_pni = Some(ServiceAddress::new_pni(result.pni));
                },
            ),
        )
    }
}
