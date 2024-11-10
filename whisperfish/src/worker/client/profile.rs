use super::*;
use crate::worker::profile_refresh::OutdatedProfile;
use actix::prelude::*;
use libsignal_service::profile_cipher::ProfileCipher;
use libsignal_service::push_service::SignalServiceProfile;
use tokio::io::AsyncWriteExt;
use whisperfish_store::StoreProfile;

impl StreamHandler<OutdatedProfile> for ClientActor {
    fn handle(&mut self, OutdatedProfile(aci, key): OutdatedProfile, ctx: &mut Self::Context) {
        tracing::trace!(
            "Received OutdatedProfile({}, [..]), fetching.",
            Uuid::from(aci)
        );
        // XXX: this should actually be unauthenticated and use sealed sender access:
        // PushServiceSocket::retrieveProfile(SignalServiceId target, @Nullable SealedSenderAccess sealedSenderAccess, Locale locale)
        let mut service = self.authenticated_service();

        // If our own Profile is outdated, schedule a profile refresh
        if self.config.get_aci() == Some(Uuid::from(aci)) {
            tracing::trace!("Scheduling a refresh for our own profile");
            ctx.notify(RefreshOwnProfile { force: false });
            return;
        }

        ctx.spawn(
            async move { (aci, service.retrieve_profile_by_id(aci, key).await) }
                .into_actor(self)
                .map(|(recipient_aci, profile), _act, ctx| {
                    match profile {
                        Ok(profile) => ctx.notify(ProfileFetched(recipient_aci, Some(profile))),
                        Err(e) => {
                            if let ServiceError::NotFoundError = e {
                                ctx.notify(ProfileFetched(recipient_aci, None))
                            } else {
                                tracing::error!("Error refreshing outdated profile: {}", e);
                            }
                        }
                    };
                }),
        );
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub(super) struct ProfileFetched(pub Aci, pub Option<SignalServiceProfile>);

impl Handler<ProfileFetched> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        ProfileFetched(aci, profile): ProfileFetched,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        match self.handle_profile_fetched(ctx, aci, profile) {
            Ok(()) => {}
            Err(e) => {
                tracing::warn!("Error with fetched profile: {}", e);
            }
        }
    }
}

fn debug_signal_service_profile(p: &SignalServiceProfile) -> String {
    format!(
        "SignalServiceProfile {{ identity_key: {:?}, name: {:?}, about: {:?}, about_emoji: {:?}, avatar: {:?}, unidentified_access: {:?}, unrestricted_unidentified_access: {:?}, capabilities: {:?} }}",
        p.identity_key.as_ref().map(|_| "..."),
        p.name.as_ref().map(|_| "..."),
        p.about.as_ref().map(|_| "..."),
        p.about_emoji.as_ref().map(|_| "..."),
        p.avatar.as_ref().map(|_| "..."),
        p.unidentified_access.as_ref().map(|_| "..."),
        p.unrestricted_unidentified_access,
        &p.capabilities,
    )
}

impl ClientActor {
    #[tracing::instrument(
        skip(self, ctx, profile),
        fields(profile = profile.as_ref().map(debug_signal_service_profile))
    )]
    fn handle_profile_fetched(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        recipient_aci: Aci,
        profile: Option<SignalServiceProfile>,
    ) -> anyhow::Result<()> {
        let storage = self.storage.clone().unwrap();
        let recipient = storage
            .fetch_recipient(&recipient_aci.into())
            .ok_or_else(|| {
                anyhow::anyhow!("could not find recipient for which we fetched a profile")
            })?;
        let key = &recipient.profile_key;

        if let Some(profile) = profile {
            let cipher = if let Some(key) = key {
                let mut bytes = [0u8; 32];
                bytes.copy_from_slice(key);
                ProfileCipher::new(zkgroup::profiles::ProfileKey::create(bytes))
            } else {
                anyhow::bail!(
                    "Fetched a profile for a contact that did not share the profile key."
                );
            };

            let unrestricted_unidentified_access = profile.unrestricted_unidentified_access;
            let profile_decrypted = cipher.decrypt(profile)?;

            tracing::info!("Decrypted profile {:?}", profile_decrypted);

            let profile_data = StoreProfile {
                given_name: profile_decrypted
                    .name
                    .as_ref()
                    .map(|x| x.given_name.to_owned()),
                family_name: profile_decrypted
                    .name
                    .as_ref()
                    .and_then(|x| x.family_name.to_owned()),
                joined_name: profile_decrypted.name.as_ref().map(|x| x.to_string()),
                about_text: profile_decrypted.about,
                emoji: profile_decrypted.about_emoji,
                unidentified: if unrestricted_unidentified_access {
                    UnidentifiedAccessMode::Unrestricted
                } else {
                    recipient.unidentified_access_mode
                },
                avatar: profile_decrypted.avatar,
                last_fetch: Utc::now().naive_utc(),
                r_uuid: recipient.uuid.unwrap(),
                r_id: recipient.id,
                r_key: recipient.profile_key,
            };

            ctx.notify(ProfileCreated(profile_data));
        } else {
            // XXX: We came here through 404 error, can that mean unregistered user?
            tracing::trace!(
                "Recipient {} doesn't have a profile on the server",
                recipient.e164_or_address()
            );
            let mut db = storage.db();

            use diesel::prelude::*;
            use whisperfish_store::schema::recipients::dsl::*;

            diesel::update(recipients)
                .set((last_profile_fetch.eq(Utc::now().naive_utc()),))
                .filter(uuid.nullable().eq(&Uuid::from(recipient_aci).to_string()))
                .execute(&mut *db)
                .expect("db");

            // If updating self, invalidate the cache
            if Some(Uuid::from(recipient_aci)) == self.config.get_aci() {
                storage.invalidate_self_recipient();
            }
        }

        Ok(())
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
struct ProfileCreated(StoreProfile);

impl Handler<ProfileCreated> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        ProfileCreated(store_profile): ProfileCreated,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let mut service = self.authenticated_service();
        let storage = self.storage.clone().unwrap();
        ctx.spawn(
            async move {
                let settings = crate::config::SettingsBridge::default();
                let avatar_dir = settings.get_string("avatar_dir");
                let avatar_dir = Path::new(&avatar_dir);
                if !avatar_dir.exists() {
                    std::fs::create_dir(avatar_dir)?;
                }
                let avatar_path = avatar_dir.join(store_profile.r_uuid.to_string());

                match store_profile.avatar.as_ref() {
                    Some(avatar) => {
                        let mut bytes = [0u8; 32];
                        bytes.copy_from_slice(store_profile.r_key.as_ref().unwrap());
                        let key = zkgroup::profiles::ProfileKey::create(bytes);
                        let cipher = ProfileCipher::new(key);
                        let mut avatar = service.retrieve_profile_avatar(avatar).await?;
                        // 10MB is what Signal Android allocates
                        let mut contents = Vec::with_capacity(10 * 1024 * 1024);
                        let len = avatar.read_to_end(&mut contents).await?;
                        contents.truncate(len);

                        let avatar_bytes = cipher.decrypt_avatar(&contents)?;

                        let mut f = tokio::fs::File::create(avatar_path).await?;
                        f.write_all(&avatar_bytes).await?;
                        tracing::info!("Profile avatar saved!");
                    }
                    None => match avatar_path.exists() {
                        true => {
                            std::fs::remove_file(avatar_path)?;
                            tracing::trace!("Profile avatar removed!");
                        }
                        false => tracing::trace!("Profile has no avatar to remove."),
                    },
                };

                let uuid = store_profile.r_uuid.to_owned();
                storage.save_profile(store_profile);
                Ok(uuid)
            }
            .into_actor(self)
            .map(|res: anyhow::Result<_>, _act, _ctx| {
                match res {
                    Ok(uuid) => tracing::info!("Profile for {} saved!", uuid),
                    Err(e) => tracing::error!("Error fetching profile avatar: {}", e),
                };
            }),
        );
    }
}
