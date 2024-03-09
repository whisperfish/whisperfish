use super::*;
use actix::prelude::*;
use anyhow::Context;

#[derive(Message)]
#[rtype(result = "()")]
pub struct InitializePni;

impl Handler<InitializePni> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: InitializePni, _ctx: &mut Self::Context) -> Self::Result {
        let service = self.authenticated_service();
        let whoami = self.migration_state.self_uuid_is_known();
        let storage = self.storage.clone().expect("initialized storage");
        let local_addr = self.self_aci.expect("local addr");
        let local_e164 = self.config.get_tel().expect("phone number");
        let sender = self.message_sender();

        Box::pin(
            async move {
                whoami.await;

                if storage.pni_storage().get_identity_key_pair().await.is_ok() {
                    // XXX: this is not a great way to check if PNI is initialized
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

                let identity_key_pair =
                    protocol::IdentityKeyPair::generate(&mut rand::thread_rng());
                let mut pni = storage.pni_storage();
                pni.write_identity_key_pair(identity_key_pair).await?;

                let res = am
                    .pnp_initialize_devices(
                        &mut storage.aci_storage(),
                        &mut storage.pni_storage(),
                        sender.await?,
                        local_addr,
                        local_e164,
                        &mut rand::thread_rng(),
                    )
                    .await
                    .context("initializing linked devices for PNP");

                if let Err(e) = res {
                    pni.remove_identity_key_pair().await.with_context(|| {
                        format!(
                        "removing PNI identity because failed to initialize PNP on devices: {e:#}"
                    )
                    })?;
                    return Err(e);
                }

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
