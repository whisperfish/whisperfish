#![allow(non_snake_case)]

use crate::model::username_lookup::{UsernameLookup, UsernameLookupResult};
use crate::model::*;
use crate::store::Storage;
use crate::store::observer::{Event, EventObserving, Interest};
use crate::worker::username::ResolveUsername;
use libsignal_protocol::ServiceId;
use qmetaobject::prelude::*;
use whisperfish_store::schema;

/// QML-constructable object that resolves a conversation by username or
/// `signal.me` username link, or directly by `ServiceId`.
///
/// Two entry points:
/// - `serviceId` (direct): an ACI/PNI the caller already holds (e.g. a group
///   member). `fetch` resolves it locally immediately, no network.
/// - `query`: a bare username (`johndoe.99`) or username link. Dispatches a
///   [`ResolveUsername`] to the [`crate::worker::username::UsernameResolverActor`],
///   which resolves it over the unidentified websocket and publishes a
///   [`UsernameLookup`] event on the observer bus. This model observes
///   `Interest::on::<UsernameLookup>()` (unscoped) and filters by the submitted
///   query string — a keyed-interest migration would need no payload change
///   since the key already carries the query.
#[observing_model]
#[derive(Default, QObject)]
pub struct CreateConversation {
    base: qt_base_class!(trait QObject),
    session_id: Option<i32>,
    service_id: Option<ServiceId>,
    name: Option<String>,
    /// The submitted username/link query; the "is this mine?" identity token
    /// for inbound [`UsernameLookup`] events.
    query: String,
    error: Option<String>,
    /// True while a `ResolveUsername` is outstanding. Cleared by `observe()`
    /// on every terminal state (resolved / not found / failed).
    busy: bool,

    #[qt_property(
        READ: get_session_id,
        NOTIFY: conversation_changed,
    )]
    sessionId: i32,

    #[qt_property(
        READ: get_service_id,
        WRITE: set_service_id,
        NOTIFY: conversation_changed,
        ALIAS: serviceId,
    )]
    service_id_: QString,

    #[qt_property(
        READ: get_query,
        WRITE: set_query,
        NOTIFY: conversation_changed,
        ALIAS: query,
    )]
    query_: QString,

    #[qt_property(
        READ: get_name,
        WRITE: set_name,
        NOTIFY: conversation_changed,
        ALIAS: name,
    )]
    name_: QString,

    #[qt_property(
        READ: get_error,
        NOTIFY: conversation_changed,
        ALIAS: error,
    )]
    error_: QString,

    #[qt_property(
        READ: get_ready,
        NOTIFY: conversation_changed,
    )]
    ready: bool,
    #[qt_property(
        READ: get_invalid,
        NOTIFY: conversation_changed,
    )]
    invalid: bool,
    #[qt_property(
        READ: get_busy,
        NOTIFY: conversation_changed,
        ALIAS: busy,
    )]
    busy_: bool,
    #[qt_property(
        READ: has_name,
        NOTIFY: conversation_changed,
    )]
    hasName: bool,

    conversation_changed: qt_signal!(),
}

impl EventObserving for CreateConversation {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, event: Event) {
        // UsernameLookup events drive the async resolution states. Routing is
        // unscoped (Interest::on); the "is this mine?" check is the submitted
        // query string, which both sides carry identically.
        if let Some(lookup) = event.payload_of::<UsernameLookup>() {
            if lookup.query != self.query {
                return;
            }
            self.busy = false;
            match &lookup.result {
                UsernameLookupResult::Resolved { aci, username } => {
                    self.error = None;
                    self.service_id = Some((*aci).into());
                    self.name = Some(username.clone());
                    // The resolver already inserted the recipient; this local
                    // fetch finds it, fills the session, and emits.
                    self.fetch(ctx.storage());
                    // `fetch` emits `conversation_changed`; don't double-emit.
                    return;
                }
                UsernameLookupResult::NotFound => {
                    self.error = Some(
                        // Translatable id; QML may override the rendered text.
                        "whisperfish-username-not-found".to_string(),
                    );
                }
                UsernameLookupResult::Failed(msg) => {
                    self.error = Some(msg.clone());
                }
            }
            self.conversation_changed();
            return;
        }

        // A session-table event (e.g. profile refresh filled a joined name):
        // re-run the local name-fallback fetch. No-op when no service_id is set.
        self.fetch(ctx.storage());
    }

    fn interests(&self) -> Vec<Interest> {
        vec![
            Interest::on::<UsernameLookup>(),
            Interest::whole_table(schema::sessions::table),
        ]
    }
}

impl CreateConversation {
    fn get_session_id(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
        self.session_id.unwrap_or(-1)
    }

    fn has_name(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.name.is_some()
    }

    fn get_ready(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.session_id.is_some()
    }

    fn get_busy(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.busy
    }

    /// Nothing actionable entered yet. The QML spinner runs on
    /// `!invalid && !ready`, so this stays true at idle and turns false as
    /// soon as a `query` or `serviceId` is set (covering the busy and
    /// error states too, since those imply a query was entered).
    fn get_invalid(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.query.is_empty() && self.service_id.is_none()
    }

    fn fetch(&mut self, storage: Storage) {
        let recipient = if let Some(id) = &self.service_id {
            storage.fetch_recipient(id)
        } else {
            tracing::trace!("No service_id set; not fetching.");
            return;
        };

        let session = if let Some(recipient) = recipient {
            if let Some(name) = &recipient.profile_joined_name {
                self.name = Some(name.clone());
            } else if let Some(username) = &recipient.username {
                self.name = Some(username.clone());
            } else if let Some(e164) = &recipient.e164 {
                self.name = Some(e164.to_string());
            }

            storage.fetch_or_insert_session_by_recipient_id(recipient.id)
        } else {
            // The caller holds a ServiceId with no local row. The direct
            // (`serviceId`) path no longer creates recipients here — the
            // username-resolver does, on a confirmed ACI. For the username
            // path, the recipient is always inserted before `observe()` runs.
            tracing::warn!("Recipient for service_id not found; not creating through this path.");
            return;
        };
        self.session_id = Some(session.id);
        self.conversation_changed();
    }

    fn get_service_id(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.service_id
            .as_ref()
            .map(ServiceId::service_id_string)
            .unwrap_or_default()
            .into()
    }

    fn set_service_id(&mut self, ctx: Option<ModelContext<Self>>, uuid: QString) {
        self.service_id = ServiceId::parse_from_service_id_string(&uuid.to_string());
        self.session_id = None;
        // Direct entry point; clear any pending-query state.
        self.query.clear();
        self.error = None;
        self.busy = false;
        if self.service_id.is_none() {
            tracing::error!("parsing service id");
        }

        if let Some(ctx) = ctx {
            self.fetch(ctx.storage());
        }
    }

    fn get_query(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.query.clone().into()
    }

    fn set_query(&mut self, _ctx: Option<ModelContext<Self>>, query: QString) {
        let query = query.to_string();
        // Reset all resolution state; only `query` carries forward.
        self.query = query.clone();
        self.service_id = None;
        self.session_id = None;
        self.name = None;
        self.error = None;

        if query.is_empty() {
            self.busy = false;
            self.conversation_changed();
            return;
        }

        self.busy = true;
        self.conversation_changed();

        // Fire the lookup through the resolver subactor. Lookups that arrive
        // during the boot window (resolver addr absent, or its storage not yet
        // ready) surface as a local error so the UI can retry.
        let resolver = self
            ._app
            .as_pinned()
            .and_then(|app| app.borrow().username_resolver.borrow().clone());
        match resolver {
            Some(addr) => addr.do_send(ResolveUsername(query)),
            None => {
                tracing::error!("UsernameResolverActor not available when query set");
                self.busy = false;
                self.error = Some("whisperfish-username-resolver-unavailable".to_string());
                self.conversation_changed();
            }
        }
    }

    fn get_name(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.name.as_deref().unwrap_or_default().into()
    }

    fn set_name(&mut self, _ctx: Option<ModelContext<Self>>, name: QString) {
        self.name = Some(name.to_string());
    }

    fn get_error(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.error.as_deref().unwrap_or_default().into()
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if self.service_id.is_some() {
            self.fetch(ctx.storage());
        }
    }
}
