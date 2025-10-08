use super::*;
use actix::prelude::*;
use anyhow::Context;
use libsignal_service::push_service::DEFAULT_DEVICE_ID;

#[derive(Message)]
#[rtype(result = "()")]
pub struct InitializePni;

impl Handler<InitializePni> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: InitializePni, ctx: &mut Self::Context) -> Self::Result {
        if self.ws.is_none() {
            // This should be triggered after restart,
            // not during the initial connection.
            tracing::warn!(
                "Not connected to server, cannot initialize PNI. Retrying in 10 seconds."
            );
            ctx.notify_later(InitializePni, Duration::from_secs(10));
            return Box::pin(async {}.into_actor(self));
        }
        let service = self.authenticated_service();
        let whoami = self.migration_state.self_uuid_is_known();
        let storage = self.storage.clone().expect("initialized storage");
        let local_addr = self.self_aci.expect("local addr");
        let local_e164 = self.config.get_tel().expect("phone number");
        let sender = self.message_sender();
        let device_id = self.config.get_device_id();

        Box::pin(
            async move {
                whoami.await;

                if storage.pni_storage().get_identity_key_pair().await.is_ok() {
                    // XXX: this is not a great way to check if PNI is initialized
                    tracing::trace!(
                        "PNI identity key pair already exists, assuming PNI is initialized"
                    );
                    return Ok(false);
                }

                if device_id != *DEFAULT_DEVICE_ID {
                    tracing::info!("Not initializing PNI on linked device");
                    return Ok(false);
                }
                tracing::info!("PNI identity key pair is not set. Initializing PNI. Hold my beer.");

                let profile_key =
                    storage
                        .fetch_self_recipient_profile_key()
                        .as_ref()
                        .map(|bytes| {
                            let mut key = [0u8; 32];
                            key.copy_from_slice(bytes);
                            ProfileKey::create(key)
                        });
                let mut am = AccountManager::new(service.clone(), profile_key);

                let identity_key_pair = protocol::IdentityKeyPair::generate(&mut rand::rng());
                let mut pni = storage.pni_storage();
                pni.write_identity_key_pair(identity_key_pair).await?;

                let res = am
                    .pnp_initialize_devices::<rand::rngs::ThreadRng, _, _, _>(
                        &mut storage.aci_storage(),
                        &mut storage.pni_storage(),
                        sender.await?,
                        local_addr,
                        local_e164,
                        &mut rand::rng(),
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

                Ok(true)
            }
            .instrument(tracing::debug_span!("initialize PNI"))
            .into_actor(self)
            .map(
                move |result: anyhow::Result<bool>, act: &mut Self, _ctx| match result {
                    Err(e) => {
                        tracing::error!("Error initializing PNI: {e:#}");
                    }
                    Ok(initialized) => {
                        act.migration_state.notify_pni_distributed();
                        if initialized {
                            tracing::info!("PNI initialized successfully");
                        }
                    }
                },
            ),
        )
    }
}
