use crate::store::{orm::Recipient, Storage};
use actix::prelude::*;
use anyhow::Context;
use chrono::prelude::*;
use diesel::prelude::*;
use libsignal_service::{
    configuration::SignalServers, prelude::*, profile_cipher::ProfileCipher, protocol::Aci,
    push_service::SignalServiceProfile,
};
use std::{
    collections::{hash_map, HashMap},
    time::Duration,
};
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

        recipients
            .filter(
                // Keep this filter in sync with the one above
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
struct ScheduledWakeUp;

#[derive(actix::Message)]
// TODO: maybe return a more processed variant.
#[rtype(result = "anyhow::Result<Option<SignalServiceProfile>>")]
pub struct FetchProfile(pub Aci, pub Option<ProfileKey>);

pub struct ProfileUpdater {
    storage: Storage,
    back_off_until: DateTime<Utc>,

    local_aci: Aci,

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

impl actix::Handler<ScheduledWakeUp> for ProfileUpdater {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: ScheduledWakeUp, ctx: &mut Self::Context) -> Self::Result {
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
                let recipient_key = if let Some(key) = recipient.profile_key {
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
                    key_bytes.copy_from_slice(&key);
                    Some(ProfileKey::create(key_bytes))
                } else {
                    None
                };
                Some(FetchProfile(recipient_aci, recipient_key))
            })
            .collect::<Vec<_>>();

        Box::pin(
            async move {
                // We execute the send's in a closure (as opposed to try_send),
                // such that we can wait for the commands to return before scheduling our next
                // action.
                for fetch_command in fetch_commands {
                    let _ = addr.send(fetch_command).await;
                }
            }
            .into_actor(self)
            .map(|(), act, ctx| {
                // Done: update schedule
                tracing::debug!("ProfileUpdater scheduled wake finished");

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
        let mut service = self.unauthenticated_service();

        // If our own Profile is outdated, schedule a profile refresh
        let is_own_profile_refresh = self.local_aci == aci;

        Box::pin(
            async move { (aci, service.retrieve_profile_by_id(aci, key).await) }
                .into_actor(self)
                .map(move |(recipient_aci, profile), act, ctx| -> anyhow::Result<Option<SignalServiceProfile>>{
                    let _span = tracing::info_span!("processing profile fetch", recipient=%Uuid::from(recipient_aci)).entered();
                    match profile {
                        Ok(profile) => {
                            act.handle_profile_fetched(ctx, recipient_aci, Some(profile.clone()))?;
                            Ok(Some(profile))
                        },
                        Err(e) => match e {
                            ServiceError::NotFoundError => {
                                if !is_own_profile_refresh {
                                    // Set the profile to None
                                    act.handle_profile_fetched(ctx, recipient_aci, None)?;
                                }

                                Ok(None)
                            }
                            ServiceError::Unauthorized => {
                                // Set the profile to None
                                tracing::warn!("profile fetch was unauthorized");
                                if !is_own_profile_refresh {
                                    act.handle_profile_fetched(ctx, recipient_aci, None)?;
                                }

                                Err(e.into())
                            }
                            ServiceError::RateLimitExceeded { retry_after: Some(retry_after) } => {
                                tracing::warn!(%retry_after, "rate limit exceeded, stopping profile refresh process");
                                act.back_off_until = Utc::now() + retry_after;

                                Err(e.into())
                            }
                            ServiceError::RateLimitExceeded { retry_after: None } => {
                                tracing::error!("rate limit exceeded, stopping profile refresh process, without Retry-After header.");
                                act.back_off_until = Utc::now() + REYIELD_DELAY;

                                Err(e.into())
                            }
                            _ => {
                                tracing::error!(error=%e, "error refreshing outdated profile");
                                // We mark the profile as fetched *anyway* in order to avoid rate
                                // limiting errors.
                                if !is_own_profile_refresh {
                                    act.handle_profile_fetched(ctx, recipient_aci, None)?;
                                }

                                Err(e).context("unknown profile refresh error")
                            }
                        },
                    }
                }),
        )
    }
}

impl ProfileUpdater {
    pub fn new(storage: Storage, local_aci: Aci) -> Self {
        Self {
            storage,
            back_off_until: Utc::now() + REYIELD_DELAY,

            local_aci,

            ignore_map: HashMap::new(),

            next_wake_handle: None,
        }
    }

    fn service_cfg(&self) -> ServiceConfiguration {
        // XXX: read the configuration files!
        SignalServers::Production.into()
    }

    // XXX somehow dedupe this with the client ector.
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

        self.next_wake_handle = Some(ctx.notify_later(ScheduledWakeUp, duration));
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
        skip(self, _ctx, profile),
        fields(profile = profile.as_ref().map(debug_signal_service_profile))
    )]
    fn handle_profile_fetched(
        &mut self,
        _ctx: &mut <Self as Actor>::Context,
        recipient_aci: Aci,
        profile: Option<SignalServiceProfile>,
    ) -> anyhow::Result<()> {
        let storage = self.storage.clone();
        let recipient = storage
            .fetch_recipient(&recipient_aci.into())
            .context("could not find recipient for which we fetched a profile")?;
        let key = &recipient.profile_key;
        let service_address = recipient
            .to_service_address()
            .context("profile recipient has valid service address")?;

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

            storage.mark_recipient_registered(service_address, true);

            storage.save_profile(profile_data);

            // TODO: update avatar. Previously:
            // ctx.notify(ProfileCreated(profile_data));
        } else {
            tracing::trace!(
                "Recipient {service_address:?} doesn't have a profile on the server, assuming unregistered user",
            );

            storage.mark_recipient_registered(service_address, false);

            // If updating self, invalidate the cache
            if recipient_aci == self.local_aci {
                storage.invalidate_self_recipient();
            }
        }

        Ok(())
    }
}
