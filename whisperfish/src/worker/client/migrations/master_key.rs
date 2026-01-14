use super::*;
use actix::prelude::*;
use libsignal_service::push_service::DEFAULT_DEVICE_ID;

#[derive(Message)]
#[rtype(result = "()")]
pub struct CheckMasterKey;

impl Handler<CheckMasterKey> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: CheckMasterKey, ctx: &mut Self::Context) -> Self::Result {
        let connectable = self.migration_state.connectable();

        let storage = self
            .storage
            .as_ref()
            .expect("storage in check master key migration")
            .clone();

        if storage.fetch_master_key().is_some() {
            tracing::debug!("Whisperfish has master key.");
            return Box::pin(future::ready(()).into_actor(self).map(
                move |_res, act: &mut Self, _ctx| {
                    act.migration_state.notify_check_master_key();
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
                    tracing::debug!("Whisperfish is primary. Generating master key...");
                    let master_key = MasterKey::generate(&mut rand::rng());
                    let storage_key = StorageServiceKey::from_master_key(&master_key);
                    storage.store_master_key(Some(&master_key));
                    storage.store_storage_service_key(Some(&storage_key));
                    Ok(true)
                } else {
                    tracing::debug!("Whisperfish is secondary. Fetching master key...");

                    match sender.await {
                        Ok(mut sender) => {
                            let request_keys = SyncMessage {
                                request: Some(sync_message::Request {
                                    r#type: Some(RequestType::Keys.into()),
                                }),
                                ..SyncMessage::with_padding(&mut rand::rng())
                            };
                            if let Err(e) = sender.send_sync_message(request_keys).await {
                                tracing::error!("Error fetching master key: {e:?}; continuing...");
                                return Ok(true);
                                // return Ok(false);
                            }
                            Ok(true)
                        }
                        Err(e) => {
                            tracing::error!("Try refetching master key in 10 seconds: {e:?}");
                            tokio::time::sleep(Duration::from_secs(10)).await;
                            Ok(false)
                        }
                    }
                }
            }
            .instrument(tracing::debug_span!("Initialize MasterKey"))
            .into_actor(self)
            .map(
                move |result: anyhow::Result<bool>, act: &mut Self, _ctx| match result {
                    Err(e) => tracing::error!("Error initializing MasterKey: {e:#}"),
                    Ok(true) => act.migration_state.notify_check_master_key(),
                    Ok(false) => ctx_addr.do_send(CheckMasterKey),
                },
            ),
        )
    }
}
