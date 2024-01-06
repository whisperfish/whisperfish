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
                if let (Some(aci), Some(pni)) = (config.get_uuid(), config.get_pni()) {
                    tracing::trace!("ACI ({}) and PNI ({}) already set.", aci, pni);
                    return Ok(None);
                }

                let response = service.whoami().await?;

                Ok::<_, anyhow::Error>(Some(response))
            }
            .into_actor(self)
            .map(
                move |result: Result<Option<WhoAmIResponse>, _>, act, _ctx| {
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
                        credentials.uuid = Some(result.uuid);
                        config2.set_uuid(result.uuid);
                        config2.set_pni(result.pni);
                        config2.write_to_file().expect("write config");
                    } else {
                        tracing::error!("Credentials was none while setting UUID");
                    }

                    act.migration_state.notify_whoami();
                },
            ),
        )
    }
}
