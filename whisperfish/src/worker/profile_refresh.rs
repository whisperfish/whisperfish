use crate::{store::orm::Recipient, store::Storage};
use chrono::prelude::*;
use diesel::prelude::*;
use futures::Stream;
use libsignal_service::protocol::Aci;
use std::{
    collections::{hash_map, HashMap},
    pin::Pin,
    task::{Context, Poll},
    time::{Duration, Instant},
};
use uuid::Uuid;
use zkgroup::profiles::ProfileKey;

const REYIELD_DELAY: Duration = Duration::from_secs(5 * 60);

/// Stream that yields UUIDs of outdated profiles that require an update.
///
/// Only yields a UUID once every 5 minutes.
pub struct OutdatedProfileStream {
    ignore_map: HashMap<Aci, Instant>,
    storage: Storage,
    next_wake: Option<Pin<Box<tokio::time::Sleep>>>,
}

pub struct OutdatedProfile(pub Aci, pub Option<ProfileKey>);

impl OutdatedProfileStream {
    pub fn new(storage: Storage) -> Self {
        Self {
            ignore_map: HashMap::new(),
            storage,
            next_wake: None,
        }
    }

    fn clean_ignore_set(&mut self) {
        // XXX The ignore set should also get cleaned if an external trigger is fired for
        // refreshing a profile.  Currently, this external trigger will only be able to fire every
        // 5 minutes.
        self.ignore_map.retain(|_uuid, time| *time > Instant::now());
    }

    fn next_out_of_date_profile(&mut self) -> Option<OutdatedProfile> {
        use whisperfish_store::schema::recipients::dsl::*;

        // https://github.com/signalapp/Signal-Android/blob/09b9349f6c0cf02688a79d8c2c9edeb8b32dd3cf/app/src/main/java/org/thoughtcrime/securesms/database/RecipientDatabase.kt#L3209
        let _last_interaction_threshold = Utc::now() - chrono::Duration::days(30);
        let last_fetch_threshold = Utc::now() - chrono::Duration::days(1);

        let mut db = self.storage.db();
        let out_of_date_profiles: Vec<Recipient> = recipients
            .filter(
                // Keep this filter in sync with the one below
                profile_key
                    .is_not_null()
                    .and(uuid.is_not_null())
                    .and(is_registered.eq(true))
                    .and(
                        last_profile_fetch
                            .is_null()
                            .or(last_profile_fetch.le(last_fetch_threshold.naive_utc())),
                    ),
            )
            .order_by(last_profile_fetch.asc())
            .load(&mut *db)
            .expect("db");

        for recipient in out_of_date_profiles {
            let recipient_aci = Aci::from(recipient.uuid.expect("database precondition"));
            let recipient_key = if let Some(key) = recipient.profile_key {
                if key.len() != 32 {
                    tracing::warn!("Invalid profile key in db. Skipping.");
                    continue;
                }
                if let hash_map::Entry::Vacant(e) = self.ignore_map.entry(recipient_aci) {
                    e.insert(Instant::now() + REYIELD_DELAY);
                } else {
                    continue;
                }
                let mut key_bytes = [0u8; 32];
                key_bytes.copy_from_slice(&key);
                Some(ProfileKey::create(key_bytes))
            } else {
                None
            };
            return Some(OutdatedProfile(recipient_aci, recipient_key));
        }

        None
    }

    fn compute_next_wake(&mut self) -> bool {
        // Either the next wake is because of the ignore set, or if that's empty, the next one in
        // the database.
        if let Some((_, time)) = self.ignore_map.iter().min_by_key(|(_, time)| *time) {
            self.next_wake = Some(Box::pin(tokio::time::sleep_until(
                tokio::time::Instant::from_std(*time),
            )));
            return true;
        }

        // No immediate updates needed at this point,
        // so we look at the next recipient,
        // and schedule a wake.
        use whisperfish_store::schema::recipients::dsl::*;

        let mut db = self.storage.db();
        let next_wake: Option<Recipient> = recipients
            .filter(
                // Keep this filter in sync with the one above
                profile_key
                    .is_not_null()
                    .and(uuid.is_not_null())
                    .and(is_registered.eq(true))
                    .and(last_profile_fetch.is_not_null()),
            )
            .order_by(last_profile_fetch.asc())
            .first(&mut *db)
            .optional()
            .expect("db");
        if let Some(recipient) = next_wake {
            let time = recipient
                .last_profile_fetch
                .expect("recipient with last_profile_fetch==null should be in ignore set");
            let time = chrono::offset::Utc.from_utc_datetime(&time);
            let delta = Utc::now() - time;
            self.next_wake = Some(Box::pin(tokio::time::sleep(
                delta.to_std().unwrap_or(REYIELD_DELAY),
            )));
            return true;
        }

        false
    }
}

impl Stream for OutdatedProfileStream {
    type Item = OutdatedProfile;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.clean_ignore_set();

        if let Some(out_of_date_profile) = self.next_out_of_date_profile() {
            tracing::trace!(
                "Yielding out-of-date profile {}",
                Uuid::from(out_of_date_profile.0)
            );
            return Poll::Ready(Some(out_of_date_profile));
        }

        self.compute_next_wake();
        if let Some(next_wake) = self.next_wake.as_mut() {
            let next_wake: Pin<_> = next_wake.as_mut();
            futures::ready!(std::future::Future::poll(next_wake, cx));
        } else {
            // XXX inefficient consumers of a stream will poll this independently of a timer.
            // We could add some artificial timeout of a few minutes to ensure the stream does not
            // die...
            tracing::warn!("Profile refresh worker has nothing to wake to.");
        }

        Poll::Pending
    }
}
