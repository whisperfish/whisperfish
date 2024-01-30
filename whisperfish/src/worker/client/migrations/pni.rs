use super::*;
use actix::prelude::*;

#[derive(Message)]
#[rtype(result = "()")]
pub struct InitializePni;

impl Handler<InitializePni> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: InitializePni, _ctx: &mut Self::Context) -> Self::Result {
        let service = self.authenticated_service();
        let whoami = self.migration_state.self_uuid_is_known();
        let storage = self.storage.clone().expect("initialized storage");
        let local_addr = self.local_addr.expect("local addr");

        Box::pin(
            async move {
                whoami.await;

                if storage.pni_storage().get_identity_key_pair().await.is_ok() {
                    tracing::trace!(
                        "PNI identity key pair already exists, assuming PNI is initialized"
                    );
                    return Ok(());
                }
                tracing::info!("PNI identity key pair is not set. Initializing PNI. Hold my beer.");

                let self_recipient = storage.fetch_self_recipient().expect("self recipient");
                let mut am = AccountManager::new(
                    service.clone(),
                    self_recipient.profile_key.as_ref().map(|bytes| {
                        let mut key = [0u8; 32];
                        key.copy_from_slice(&bytes);
                        ProfileKey::create(key)
                    }),
                );

                am.pnp_initialize_devices(
                    &mut storage.aci_storage(),
                    &mut storage.pni_storage(),
                    local_addr,
                    &mut rand::thread_rng(),
                )
                .await?;

                Ok(())
            }
            .instrument(tracing::debug_span!("initialize PNI"))
            .into_actor(self)
            .map(move |result: anyhow::Result<()>, act: &mut Self, _ctx| {
                if let Err(e) = result {
                    tracing::error!("Error initializing PNI: {e:#}");
                } else {
                    act.migration_state.notify_pni_distributed();
                }
            }),
        )
    }
}
