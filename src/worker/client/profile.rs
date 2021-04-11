use actix::prelude::*;

use libsignal_service::profile_name::ProfileName;
use libsignal_service::push_service::DeviceCapabilities;
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

/// Synchronize profile attributes.
#[derive(Message)]
#[rtype(result = "()")]
pub struct RefreshProfileAttributes;

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
        let service = self.authenticated_service();
        let context = libsignal_protocol::Context::default();
        let client = ctx.address();

        Box::pin(async move {
            let uuid = storage.self_uuid().expect("self uuid");
            let self_recipient = storage
                .fetch_self_recipient()
                .expect("self recipient should be set by now");
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
                None,
                Some(&uuid.to_string()),
                &profile_key.get_bytes(),
                TrustLevel::Certain,
            );

            client.send(MultideviceSyncProfile).await.unwrap();
            client.send(RefreshProfileAttributes).await.unwrap();
        })
    }
}

impl Handler<RefreshProfileAttributes> for ClientActor {
    type Result = ResponseFuture<()>;
    fn handle(&mut self, _: RefreshProfileAttributes, _ctx: &mut Self::Context) -> Self::Result {
        let storage = self.storage.clone().unwrap();
        let registration_id = storage.protocol_store.lock().unwrap().regid;
        let service = self.authenticated_service();
        let context = libsignal_protocol::Context::default();

        Box::pin(async move {
            let self_recipient = storage.fetch_self_recipient().expect("self set by now");

            let mut am = AccountManager::new(context, service, self_recipient.profile_key());
            am.set_account_attributes(
                None,            // signaling key
                registration_id, // regid
                false,           // voice
                false,           // video
                true,            // fetches_messages
                None,            // pin
                None,            // reg lock
                None,            // unidentified_access_key
                false,           //unresticted UA
                true,            // discoverable by phone
                DeviceCapabilities {
                    uuid: true,
                    gv2: true,
                    storage: false,
                    gv1_migration: true,
                },
            )
            .await
            .expect("upload profile");
            log::info!("Profile attributes refreshed");
        })
    }
}
