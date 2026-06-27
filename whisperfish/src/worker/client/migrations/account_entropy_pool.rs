use super::*;
use actix::prelude::*;
use libsignal_service::libsignal_account_keys::AccountEntropyPool;
use libsignal_service::push_service::DEFAULT_DEVICE_ID;

#[derive(Message)]
#[rtype(result = "()")]
pub struct CheckAccountEntropyPool;

impl Handler<CheckAccountEntropyPool> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: CheckAccountEntropyPool, ctx: &mut Self::Context) -> Self::Result {
        let connectable = self.migration_state.connectable();

        let storage = self
            .storage
            .as_ref()
            .expect("storage in check account entropy pool")
            .clone();

        if storage.fetch_account_entropy_pool().is_some() {
            tracing::debug!("Account entropy pool is set.");
            return Box::pin(future::ready(()).into_actor(self).map(
                move |_res, act: &mut Self, _ctx| {
                    act.migration_state.notify_check_account_entropy_pool();
                },
            ));
        };

        let sender = self.message_sender();
        let is_primary = self.config.as_ref().get_device_id() == *DEFAULT_DEVICE_ID;
        let ctx_addr = ctx.address();

        Box::pin(
            async move {
                connectable.await;
                if is_primary {
                    tracing::debug!("Whisperfish is primary");

                    tracing::debug!("Generate account entropy pool...");
                    let pool = AccountEntropyPool::generate(&mut rand::rng());
                    storage.store_account_entropy_pool(&pool);

                    tracing::debug!("Derive master key...");
                    let master_key =
                        MasterKey::from_slice(pool.derive_svr_key().as_slice()).unwrap();
                    // Note: This is safe, since if Whisperfish is primary,
                    // we can't possibly have any server-side storage yet.
                    storage.store_master_key(Some(&master_key));

                    tracing::debug!("Derive storage key...");
                    let storage_key = StorageServiceKey::from_master_key(&master_key);
                    storage.store_storage_service_key(Some(&storage_key));

                    // XXX Storage Manifest
                    Ok(true)
                } else {
                    tracing::debug!("Whisperfish is secondary. Synchronize keys...");

                    match sender.await {
                        Ok(mut sender) => {
                            let request_keys = SyncMessage {
                                request: Some(sync_message::Request {
                                    r#type: Some(RequestType::Keys.into()),
                                }),
                                ..SyncMessage::with_padding(&mut rand::rng())
                            };
                            if let Err(e) = sender.send_sync_message(request_keys).await {
                                tracing::error!("Error syncing Keys: {e:?}; continuing...");
                                return Ok(true);
                                // return Ok(false);
                            }
                            Ok(true)
                        }
                        Err(e) => {
                            tracing::error!("Try syncing Keys in 10 seconds: {e:?}");
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            Ok(false)
                        }
                    }
                }
            }
            .instrument(tracing::debug_span!("Initialize Account Entropy Pool"))
            .into_actor(self)
            .map(
                move |result: anyhow::Result<bool>, act: &mut Self, _ctx| match result {
                    Err(e) => tracing::error!("Error initializing Account Entropy Pool: {e:#}"),
                    Ok(true) => act.migration_state.notify_check_account_entropy_pool(),
                    Ok(false) => ctx_addr.do_send(CheckAccountEntropyPool),
                },
            ),
        )
    }
}
