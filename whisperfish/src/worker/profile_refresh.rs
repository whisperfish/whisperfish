use crate::store::{orm::Recipient, Storage};
use actix::prelude::*;
use anyhow::Context;
use chrono::prelude::*;
use diesel::prelude::*;
use futures::AsyncReadExt;
use libsignal_service::{
    configuration::SignalServers, prelude::*, profile_cipher::ProfileCipher, protocol::Aci,
    push_service::SignalServiceProfile,
};
use std::{
    collections::{hash_map, HashMap},
    time::Duration,
};
use tokio::io::AsyncWriteExt;
use tracing_futures::Instrument;
use uuid::Uuid;
use whisperfish_store::{orm::UnidentifiedAccessMode, StoreProfile};
use zkgroup::profiles::ProfileKey;

const MAX_CONCURRENT_UPDATES: usize = 5;
const REYIELD_DELAY: chrono::Duration = chrono::Duration::seconds(5 * 60);
#[allow(unused)]
const LAST_INTERACTION_THRESHOLD: chrono::Duration = chrono::Duration::days(30);
const LAST_PROFILE_FETCH_THRESHOLD: chrono::Duration = chrono::Duration::days(1);

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

macro_rules! recipients_filtered {
    ($ignore_map:expr) => {{
        use whisperfish_store::schema::recipients::dsl::*;
        let last_fetch_threshold = Utc::now() - LAST_PROFILE_FETCH_THRESHOLD;

        let ignored = $ignore_map
            .keys()
            .cloned()
            .map(|aci| Uuid::from(aci).to_string());

        // We're looking for recipients such that:
        // 1. The profile key *is known*; AND
        // 2. Their ACI *is known*; AND
        // 3. either:
        //    a. Their profile has never been fetched
        //    b. The last time the profile was fetched is more than LAST_PROFILE_FETCH_THRESHOLD
        //       ago AND the user is a known registered user.
        //    ; AND
        // 4. The ACI is not in the ignore map
        recipients
            .filter(
                profile_key
                    .is_not_null()
                    .and(uuid.is_not_null())
                    .and(
                        last_profile_fetch.is_null().or(last_profile_fetch
                            .le(last_fetch_threshold.naive_utc())
                            .and(is_registered.eq(true))),
                    )
                    .and(uuid.ne_all(ignored)),
            )
            .order_by(last_profile_fetch.asc())
    }};
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct WakeUp;

#[derive(actix::Message)]
// TODO: maybe return a more processed variant.
#[rtype(result = "anyhow::Result<Option<SignalServiceProfile>>")]
pub struct FetchProfile(pub Aci, pub Option<ProfileKey>);

#[derive(actix::Message)]
#[rtype(result = "()")]
struct FetchAvatar {
    recipient_uuid: Uuid,
    profile_key: zkgroup::profiles::ProfileKey,
    avatar_attachment_path: String,
}

pub struct ProfileUpdater {
    storage: Storage,
    back_off_until: DateTime<Utc>,

    local_aci: Aci,
    credentials: ServiceCredentials,

    // TODO: store the ignore reason
    ignore_map: HashMap<Aci, DateTime<Utc>>,

    next_wake_handle: Option<actix::SpawnHandle>,
}

impl actix::Actor for ProfileUpdater {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut <Self as actix::Actor>::Context) {
        // Schedule first wake
        self.update_scheduled_wake(ctx);
    }
}

impl actix::Handler<WakeUp> for ProfileUpdater {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: WakeUp, ctx: &mut Self::Context) -> Self::Result {
        // Cancel any remaining wake-ups.
        if let Some(handle) = self.next_wake_handle.take() {
            ctx.cancel_future(handle);
        }

        self.update_ignore_set();

        let storage = self.storage.clone();
        let mut db = storage.db();
        let out_of_date_profile_recipients: Vec<Recipient> = recipients_filtered!(self.ignore_map)
            .limit(MAX_CONCURRENT_UPDATES as i64)
            .load(&mut *db)
            .expect("db");

        let addr = ctx.address();

        let fetch_commands = out_of_date_profile_recipients
            .into_iter()
            .filter_map(|recipient| {
                // TODO: Filter out OOD-profiles without recent interaction (i.e., place them in the ignore map)
                let recipient_aci = Aci::from(recipient.uuid.expect("database precondition"));
                let recipient_key = if let Some(key) = &recipient.profile_key {
                    if key.len() != 32 {
                        tracing::warn!("Invalid profile key in db. Skipping.");
                        return None;
                    }
                    if let hash_map::Entry::Vacant(e) = self.ignore_map.entry(recipient_aci) {
                        e.insert(Utc::now() + REYIELD_DELAY);
                    } else {
                        return None;
                    }
                    let mut key_bytes = [0u8; 32];
                    key_bytes.copy_from_slice(key);
                    Some(ProfileKey::create(key_bytes))
                } else {
                    None
                };
                tracing::debug!(%recipient, "scheduling profile update");
                Some(FetchProfile(recipient_aci, recipient_key))
            })
            .collect::<Vec<_>>();

        Box::pin(
            async move {
                // We execute the send's in a closure (as opposed to try_send),
                // such that we can wait for the commands to return before scheduling our next
                // action.
                let mut successes = 0;
                let mut empties = 0;
                let mut errors = 0;
                for fetch_command in fetch_commands {
                    let FetchProfile(ref recipient, _) = fetch_command;
                    let span = tracing::info_span!("fetching profile", ?recipient);
                    match addr.send(fetch_command).instrument(span.clone()).await {
                        Ok(Ok(profile)) => {
                            let _span = span.enter();
                            if profile.is_some() {
                                tracing::debug!("fetched profile");
                                successes += 1;
                            } else {
                                tracing::debug!("returned empty profile");
                                empties += 1;
                            }
                        }
                        Ok(Err(e)) => {
                            let _span = span.enter();
                            tracing::error!("{e}");
                            errors += 1;
                        }
                        Err(_) => {
                            let _span = span.enter();
                            // mailbox closed no-op and wait for shutdown
                            errors += 1;
                        }
                    }
                }
                (successes, empties, errors)
            }
            .into_actor(self)
            .map(|(successes, empties, errors), act, ctx| {
                // Done: update schedule
                tracing::debug!(updates=%successes, empty_profiles=%empties, errors=%errors, "ProfileUpdater scheduled wake finished");

                // Wait at least five minutes for the next batch
                let earliest_wake = Utc::now() + REYIELD_DELAY;
                act.back_off_until = std::cmp::max(act.back_off_until, earliest_wake);

                act.update_scheduled_wake(ctx);
            }),
        )
    }
}

impl Handler<FetchProfile> for ProfileUpdater {
    type Result = ResponseActFuture<Self, anyhow::Result<Option<SignalServiceProfile>>>;

    fn handle(
        &mut self,
        FetchProfile(aci, key): FetchProfile,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        tracing::trace!(
            "Received FetchProfile({}, {:?}), fetching.",
            Uuid::from(aci),
            key.as_ref().map(|_| "[..]"),
        );
        // XXX: this should actually be unauthenticated and use sealed sender access:
        // PushServiceSocket::retrieveProfile(SignalServiceId target, @Nullable SealedSenderAccess sealedSenderAccess, Locale locale)
        let mut service = self.authenticated_service();

        Box::pin(
            async move { (aci, service.retrieve_profile_by_id(aci, key).await) }
                .into_actor(self)
                .map(move |(recipient_aci, profile), act, ctx| -> anyhow::Result<Option<SignalServiceProfile>>{
                    let _span = tracing::info_span!("processing profile fetch", recipient=%Uuid::from(recipient_aci)).entered();
                    act.handle_profile_fetched(ctx, recipient_aci, profile)
                        .inspect_err(|e| tracing::error!("{e}"))
                }),
        )
    }
}

impl Handler<FetchAvatar> for ProfileUpdater {
    type Result = ();

    fn handle(
        &mut self,
        FetchAvatar {
            recipient_uuid,
            profile_key,
            avatar_attachment_path,
        }: FetchAvatar,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let mut service = self.unauthenticated_service();
        ctx.spawn(
            async move {
                let settings = crate::config::SettingsBridge::default();
                let avatar_dir = settings.get_string("avatar_dir");
                let avatar_dir = std::path::Path::new(&avatar_dir);
                if !avatar_dir.exists() {
                    std::fs::create_dir(avatar_dir)?;
                }
                let avatar_path = avatar_dir.join(recipient_uuid.to_string());

                let mut avatar = service
                    .retrieve_profile_avatar(&avatar_attachment_path)
                    .await?;
                // 10MB is what Signal Android allocates
                let mut contents = Vec::with_capacity(10 * 1024 * 1024);
                let len = avatar.read_to_end(&mut contents).await?;
                contents.truncate(len);

                let cipher = ProfileCipher::new(profile_key);
                let avatar_bytes = cipher.decrypt_avatar(&contents)?;

                let mut f = tokio::fs::File::create(avatar_path).await?;
                f.write_all(&avatar_bytes).await?;
                tracing::info!("Profile avatar saved!");

                Ok(())
            }
            .into_actor(self)
            .map(|res: anyhow::Result<_>, _act, _ctx| {
                if let Err(e) = res {
                    tracing::error!("Error fetching profile avatar: {}", e);
                }
            }),
        );
    }
}

impl ProfileUpdater {
    pub fn new(storage: Storage, local_aci: Aci, credentials: ServiceCredentials) -> Self {
        Self {
            storage,
            back_off_until: Utc::now() + REYIELD_DELAY,

            local_aci,
            credentials,

            ignore_map: HashMap::new(),

            next_wake_handle: None,
        }
    }

    fn service_cfg(&self) -> ServiceConfiguration {
        // XXX: read the configuration files!
        SignalServers::Production.into()
    }

    // XXX somehow dedupe this with the client ector.
    fn authenticated_service(&self) -> PushService {
        let service_cfg = self.service_cfg();
        PushService::new(
            service_cfg,
            Some(self.credentials.clone()),
            crate::user_agent(),
        )
    }

    fn unauthenticated_service(&self) -> PushService {
        let service_cfg = self.service_cfg();
        PushService::new(service_cfg, None, crate::user_agent())
    }

    fn update_scheduled_wake(&mut self, ctx: &mut <Self as actix::Actor>::Context) {
        // Cancel any remaining wake up
        if let Some(handle) = self.next_wake_handle.take() {
            ctx.cancel_future(handle);
        }

        // Compute the next one
        let Some(next_wake) = self.compute_next_wake() else {
            tracing::trace!("no profile update wakeup scheduled");
            return;
        };

        // Compute the delay for the future
        let delta = Utc::now() - next_wake;
        let duration = delta.to_std().unwrap_or(Duration::ZERO);

        if duration.is_zero() {
            tracing::trace!("wake-up scheduled immediately");
        }

        self.next_wake_handle = Some(ctx.notify_later(WakeUp, duration));
    }

    fn update_ignore_set(&mut self) {
        // XXX The ignore set should also get cleaned if an external trigger is fired for
        // refreshing a profile.  Currently, this external trigger will only be able to fire every
        // 5 minutes.
        self.ignore_map.retain(|_uuid, time| *time > Utc::now());

        // TODO: also clear Acis which got recent interaction (how?)
    }

    fn compute_next_wake(&mut self) -> Option<DateTime<Utc>> {
        // We look at the next recipient,
        // and schedule a wake.
        use whisperfish_store::schema::recipients::dsl::*;

        let mut db = self.storage.db();
        let next_wake: Option<Option<NaiveDateTime>> = recipients_filtered!(self.ignore_map)
            .select(last_profile_fetch)
            .first(&mut *db)
            .optional()
            .expect("db");

        next_wake
            .map(|val| {
                val.as_ref()
                    .map(NaiveDateTime::and_utc)
                    .unwrap_or_else(Utc::now)
            })
            .map(|time| std::cmp::max(time, self.back_off_until))
    }

    #[tracing::instrument(
        skip(self, ctx, profile),
        fields(profile = ?profile.as_ref().map(debug_signal_service_profile), is_own_profile_refresh),
    )]
    fn handle_profile_fetched(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        recipient_aci: Aci,
        profile: Result<SignalServiceProfile, ServiceError>,
    ) -> anyhow::Result<Option<SignalServiceProfile>> {
        let is_own_profile_refresh = self.local_aci == recipient_aci;
        tracing::Span::current().record("is_own_profile_refresh", is_own_profile_refresh);

        let storage = self.storage.clone();
        let recipient = storage
            .fetch_recipient(&recipient_aci.into())
            .context("could not find recipient for which we fetched a profile")?;
        let key = &recipient.profile_key;
        let service_address = recipient
            .to_service_address()
            .context("profile recipient has valid service address")?;

        let profile = match profile {
            Ok(profile) => profile,
            Err(e) => match e {
                ServiceError::NotFoundError => {
                    if !is_own_profile_refresh {
                        tracing::trace!(
                            "Recipient {service_address:?} is not a registered Signal user",
                        );
                        storage.mark_recipient_registered(service_address, false);
                    }

                    storage.mark_profile_updated(recipient_aci.into());
                    tracing::debug!("profile not found");
                    return Ok(None);
                }
                ServiceError::Unauthorized => {
                    // Set the profile to None
                    tracing::warn!("profile fetch was unauthorized");
                    if !is_own_profile_refresh {
                        storage.remove_profile(recipient_aci.into());
                    }

                    return Err(e.into());
                }
                ServiceError::RateLimitExceeded {
                    retry_after: Some(retry_after),
                } => {
                    tracing::warn!(%retry_after, "rate limit exceeded, stopping profile refresh process");
                    self.back_off_until = Utc::now() + retry_after;

                    return Err(e.into());
                }
                ServiceError::RateLimitExceeded { retry_after: None } => {
                    tracing::error!("rate limit exceeded, stopping profile refresh process, without Retry-After header.");
                    self.back_off_until = Utc::now() + REYIELD_DELAY;

                    return Err(e.into());
                }
                _ => {
                    tracing::error!(error=%e, "error refreshing outdated profile");
                    // We mark the profile as fetched *anyway* in order to avoid rate
                    // limiting errors.
                    if !is_own_profile_refresh {
                        // XXX Should we instead *just* update the time?
                        storage.remove_profile(recipient_aci.into());
                    }

                    return Err(e).context("unknown profile refresh error");
                }
            },
        };

        let profile_key = if let Some(key) = key {
            let mut bytes = [0u8; 32];
            bytes.copy_from_slice(key);
            zkgroup::profiles::ProfileKey::create(bytes)
        } else {
            anyhow::bail!("Fetched a profile for a contact that did not share the profile key.");
        };
        let cipher = ProfileCipher::new(profile_key);

        let unrestricted_unidentified_access = profile.unrestricted_unidentified_access;
        let profile_decrypted = cipher.decrypt(profile.clone())?;

        tracing::info!("Decrypted profile {:?}", profile_decrypted);

        if let Some(avatar_attachment_path) = profile_decrypted.avatar.clone() {
            ctx.notify(FetchAvatar {
                recipient_uuid: recipient_aci.into(),
                profile_key,
                avatar_attachment_path,
            });
        }

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

        storage.mark_recipient_registered(service_address, true);

        storage.save_profile(profile_data);

        Ok(Some(profile))
    }
}
