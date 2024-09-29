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
        let curr_master_key = storage.fetch_master_key();
        let _config = self.config.as_ref();
        let sender = self.message_sender();
        let is_primary = self.config.as_ref().get_device_id() == DEFAULT_DEVICE_ID.into();
        let addr = ctx.address();

        Box::pin(
            async move {
                connectable.await;
                let mut success = true;
                match (is_primary, curr_master_key) {
                    (true, None) => {
                        tracing::debug!(
                            "Whisperfish is primary and doesn't have master key. Generating..."
                        );
                        let master_key = MasterKey::generate();
                        let storage_key = StorageServiceKey::from_master_key(&master_key);
                        storage.store_master_key(Some(&master_key));
                        storage.store_storage_service_key(Some(&storage_key));
                    }
                    (false, None) => {
                        tracing::debug!(
                            "Whisperfish is linked and doesn't have master key. Fetching..."
                        );

                        match sender.await {
                            Ok(mut sender) => {
                                let self_recipient = storage
                                    .fetch_self_recipient()
                                    .expect("self recipient in initialize MasterKey");
                                let self_address = self_recipient
                                    .to_aci_service_address()
                                    .expect("self ACI ServiceAddress in initialize MasterKey");
                                sender
                                    .send_sync_message_request(&self_address, RequestType::Keys)
                                    .await
                                    .unwrap();
                            }
                            Err(e) => {
                                tracing::error!(
                                    "Could not send MasterKey request: {:?} Retrying in 10 seconds.",
                                    e
                                );
                                tokio::time::sleep(Duration::from_secs(10)).await;
                                success = false;
                            }
                        }
                    }
                    (_, Some(_)) => tracing::debug!("Whisperfish has master key in database"),
                };

                Ok(success)
            }
            .instrument(tracing::debug_span!("Initialize MasterKey"))
            .into_actor(self)
            .map(
                move |result: anyhow::Result<bool>, act: &mut Self, _ctx| match result {
                    Err(e) => {
                        tracing::error!("Error initializing MasterKey: {e:#}");
                    }
                    Ok(true) => {
                        act.migration_state.notify_check_master_key();
                    }
                    Ok(false) => {
                        // Delay was awaited where success was set to false
                        addr.do_send(CheckMasterKey);
                    }
                },
            ),
        )
    }
}
