use actix::prelude::*;

use libsignal_service::profile_name::ProfileName;
use rand::Rng;
use zkgroup::profiles::ProfileKey;

use crate::store::TrustLevel;

use super::*;

/// Generate and upload a profile for the self recipient.
#[derive(Message)]
#[rtype(result = "()")]
pub struct GenerateEmptyProfileIfNeeded;

/// Synchronize multi-device profile information.
#[derive(Message)]
#[rtype(result = "()")]
pub struct MultideviceSyncProfile;

impl Handler<MultideviceSyncProfile> for ClientActor {
    type Result = ResponseFuture<()>;
    fn handle(&mut self, _: MultideviceSyncProfile, _ctx: &mut Self::Context) -> Self::Result {
        let _storage = self.storage.clone().unwrap();
        let _cfg = _storage.read_config().expect("read config");
        let _service = self.authenticated_service();
        let _context = libsignal_protocol::Context::default();

        Box::pin(async move {
            // STUB
        })
    }
}

impl Handler<GenerateEmptyProfileIfNeeded> for ClientActor {
    type Result = ResponseFuture<()>;
    fn handle(&mut self, _: GenerateEmptyProfileIfNeeded, ctx: &mut Self::Context) -> Self::Result {
        let storage = self.storage.clone().unwrap();
        let cfg = storage.read_config().expect("read config");
        let service = self.authenticated_service();
        let context = libsignal_protocol::Context::default();
        let client = ctx.address();

        Box::pin(async move {
            let e164 = if let Some(e164) = cfg.tel.as_deref() {
                e164
            } else {
                log::warn!("No uuid set, cannot generate empty profile.");
                // XXX retry?
                return;
            };
            let uuid_str = if let Some(uuid) = cfg.uuid.as_deref() {
                uuid
            } else {
                log::warn!("No uuid set, cannot generate empty profile.");
                // XXX retry?
                return;
            };
            let uuid = match uuid::Uuid::parse_str(uuid_str) {
                Ok(uuid) => uuid,
                Err(e) => {
                    log::error!("Not creating profile, because uuid unparsable: {}", e);
                    return;
                }
            };
            let self_recipient =
                storage.merge_and_fetch_recipient(Some(e164), Some(uuid_str), TrustLevel::Certain);
            if let Some(key) = self_recipient.profile_key {
                log::trace!(
                    "Profile key is already set ({} bytes); not overwriting",
                    key.len()
                );
                return;
            }

            log::info!("Generating profile key");
            let profile_key = ProfileKey::generate(rand::thread_rng().gen());
            let mut am = AccountManager::new(context, service, Some(profile_key.get_bytes()));
            am.upload_versioned_profile_without_avatar(uuid, ProfileName::empty(), None, None)
                .await
                .expect("upload profile");

            // Now also set the database
            storage.update_profile_key(
                Some(e164),
                Some(uuid_str),
                &profile_key.get_bytes(),
                TrustLevel::Certain,
            );

            client.send(MultideviceSyncProfile).await.unwrap();
        })
    }
}
