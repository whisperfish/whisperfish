//! Username-resolution subactor.
//!
//! Resolves a Signal username or a `signal.me` username link to an ACI, using
//! the *unidentified* websocket (lookups don't require authentication), and
//! publishes the result onto the observer bus as a [`UsernameLookup`] payload
//! keyed on the submitted query string. A [`crate::model::create_conversation`]
//! model observes `Interest::on::<UsernameLookup>()` and reacts.
//!
//! Both the bare-username (`johndoe.99`) and username-link
//! (`https://signal.me/#eu/<payload>` or bare payload) forms are supported, per
//! the `libsignal-service-rs` `usernames` example. The legacy `sgnl://` link
//! scheme is intentionally *not* handled — Signal-Android's `UsernameUtil.kt`
//! no longer accepts it either.
//!
//! Mirrors [`crate::worker::profile_refresh::ProfileUpdater`]'s lightweight
//! unidentified-socket pattern: the WS is opened lazily per lookup and carries
//! no liveness bookkeeping of its own. Reconnecting/keepalive is the WS
//! transport's responsibility; if a longer-lived connection is needed later
//! (e.g. sharing one with `ProfileUpdater`), that's a follow-up.

use actix::prelude::*;
use libsignal_service::{
    configuration::SignalServers,
    prelude::*,
    protocol::Username,
    websocket::{SignalWebSocket, Unidentified},
};

use crate::gui::StorageReady;
use crate::model::username_lookup::{UsernameLookup, UsernameLookupResult};
use crate::store::Storage;

/// Resolve a username or `signal.me` username link to an ACI.
///
/// The actor emits a [`UsernameLookup`] event keyed on this string regardless
/// of outcome (resolved / not found / failed), so the subscribing model can
/// drive all three states from one observer channel.
#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct ResolveUsername(pub String);

pub struct UsernameResolver {
    storage: Option<Storage>,
    signal_server: SignalServers,
}

impl UsernameResolver {
    pub fn new(signal_server: SignalServers) -> Self {
        Self {
            storage: None,
            signal_server,
        }
    }

    // XXX The four helpers below are duplicated from ProfileUpdater / ClientActor.
    // See `whisperfish/src/worker/profile_refresh.rs`:204 (XXX comment) and
    // `client.rs:613`. Extracting a shared `UnidentifiedSocket` helper is a
    // follow-up that naturally falls out of the ongoing ClientActor split.
    fn user_agent(&self) -> String {
        crate::user_agent()
    }

    fn unauthenticated_service(&self) -> PushService {
        PushService::new(self.signal_server, None, self.user_agent())
    }

    fn unidentified_websocket(
        &self,
    ) -> impl Future<Output = Result<SignalWebSocket<Unidentified>, ServiceError>> + use<> {
        let mut u_service = self.unauthenticated_service();
        async move {
            u_service
                .ws("/v1/websocket/", "/v1/keepalive", &[], None)
                .await
        }
    }
}

impl actix::Actor for UsernameResolver {
    type Context = actix::Context<Self>;
}

impl Handler<StorageReady> for UsernameResolver {
    type Result = ();

    fn handle(
        &mut self,
        StorageReady { storage }: StorageReady,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.storage.is_some() {
            tracing::error!(
                "StorageReady dispatched twice; ignoring duplicate for UsernameResolverActor"
            );
            return;
        }
        self.storage = Some(storage);
        tracing::trace!("UsernameResolverActor has registered storage");
    }
}

impl Handler<ResolveUsername> for UsernameResolver {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        ResolveUsername(query): ResolveUsername,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        if self.storage.is_none() {
            // Lookups can arrive during the narrow boot window before
            // `StorageReady` is dispatched, exactly like `SessionMethods`.
            // We can't emit an observer event without storage, so drop; the
            // model's `busy` flag will clear on the next user retry.
            tracing::warn!(%query, "ResolveUsername dropped: storage not ready");
            return Box::pin(async {}.into_actor(self));
        }
        let storage = self.storage.clone().expect("storage set above");

        let ws_fut = self.unidentified_websocket();
        Box::pin(
            async move {
                let result = resolve(ws_fut, &query).await;
                persist_and_emit(&storage, query.clone(), result).await
            }
            .into_actor(self),
        )
    }
}

/// Drives the network classification + lookup. Kept free of actor state so it
/// reads as a straight port of the `libsignal-service-rs` `usernames` example.
async fn resolve(
    ws_fut: impl Future<Output = Result<SignalWebSocket<Unidentified>, ServiceError>>,
    query: &str,
) -> UsernameLookupResult {
    let mut ws = match ws_fut.await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::warn!(%query, error=%e, "username lookup: websocket setup failed");
            return generic_failure();
        }
    };

    tracing::info!(%query, "username lookup: classified query");

    let username = if let Ok(u) = Username::new(query) {
        tracing::info!(%query, username=%u, "username lookup: bare-username path");
        u
    } else {
        let Ok(query) = url::Url::parse(query) else {
            tracing::info!(%query, "username lookup: not a username or link");
            return generic_failure();
        };
        tracing::info!(%query, "username lookup: link path");
        match ws.look_up_username_link(&query).await {
            Ok(Some(u)) => {
                tracing::info!(%query, "username lookup: link decrypted");
                u
            }
            Ok(None) => {
                tracing::info!(%query, "username lookup: link not found");
                return UsernameLookupResult::NotFound;
            }
            Err(e) => {
                tracing::warn!(%query, error=%e, "username lookup: link fetch failed");
                return generic_failure();
            }
        }
    };

    match ws.look_up_username(&username).await {
        Ok(Some(aci)) => {
            tracing::info!(%query, aci = %aci.service_id_string(), "username lookup: resolved");
            UsernameLookupResult::Resolved {
                aci,
                username: username.to_string(),
            }
        }
        Ok(None) => {
            tracing::info!(%query, "username lookup: hash not found");
            UsernameLookupResult::NotFound
        }
        Err(e) => {
            tracing::warn!(%query, error=%e, "username hash lookup failed");
            generic_failure()
        }
    }
}

/// Sanitized failure string. The detailed `ServiceError` is logged at the
/// call site; we never hand protocol internals to QML.
fn generic_failure() -> UsernameLookupResult {
    UsernameLookupResult::Failed("whisperfish-username-lookup-failed".into())
}

/// On success, ensure a recipient exists for the ACI and stash the decrypted
/// username (if any) on it; then publish the lookup outcome regardless of
/// variant, so the subscribing model drives all three states.
async fn persist_and_emit(storage: &Storage, query: String, result: UsernameLookupResult) {
    if let UsernameLookupResult::Resolved { aci, username } = &result {
        let recipient = storage.fetch_or_insert_recipient_by_address(&(*aci).into());
        storage.set_recipient_username(recipient.id, username.as_str());
    }
    emit_lookup(storage, &query, result);
}

fn emit_lookup(storage: &Storage, query: &str, result: UsernameLookupResult) {
    storage.observe_event(
        UsernameLookup::primary_key(query),
        vec![],
        UsernameLookup {
            query: query.to_string(),
            result,
        },
    );
}
