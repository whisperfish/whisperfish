#![allow(non_snake_case)]

//! Per-session typing-indicator model. `Insert`/`Delete` deltas on the [`Typing`]
//! process channel drive an in-memory typer set; a self-arming sweep expires
//! stale entries. Nothing here is persisted.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use actix::prelude::*;
use chrono::prelude::*;
use qmetaobject::prelude::*;
use qttypes::QVariantList;
use whisperfish_store::{schema, store::orm};

use crate::model::active_model::{ModelContext, ObservingModelActor};
use crate::model::*;
use crate::store::observer::{Event, EventObserving, EventPayload, Interest};

/// How long a typing indicator stays valid after the sender's timestamp.
const TYPING_EXPIRY: chrono::Duration = chrono::Duration::seconds(5);

/// Process marker for typing notifications.
pub struct Typing;

impl EventPayload for Typing {
    type Payload = TypingEvent;
}

/// A typing lifecycle event carried on the [`Typing`] process channel.
///
/// Replaces the previous encoding where `EventType::Insert`/`Delete` stood in
/// for "started"/"stopped"; the observer core does not interpret this enum, it
/// only routes on [`Subject`] + [`Relation`]. The recipient id rides on
/// [`Event::key`].
#[derive(Clone, Debug, PartialEq)]
pub enum TypingEvent {
    /// The sender started (or is refreshing) a typing indicator.
    Started { sent_at: DateTime<Utc> },
    /// The sender stopped typing.
    Stopped,
}

/// Display name for a typer: prefer username, then profile given name, then the
/// E.164/address.
fn typer_display_name(r: &orm::Recipient) -> String {
    if let Some(username) = r.username.as_ref() {
        username.as_str().to_string()
    } else if let Some(name) = r.profile_given_name.as_ref() {
        name.as_str().to_string()
    } else {
        r.e164_or_address().as_str().to_string()
    }
}

/// Self-sweep message used to expire stale typing indicators.
#[derive(actix::Message)]
#[rtype(result = "()")]
struct TypingSweep;

#[observing_model]
#[derive(Default, QObject)]
pub struct TypingModel {
    base: qt_base_class!(trait QObject),

    #[qt_property(WRITE: set_session_id, NOTIFY: typing_names_changed)]
    sessionId: i32,

    #[qt_property(READ: typing_names_qml, NOTIFY: typing_names_changed)]
    typingNames: QVariantList,

    typing_names_changed: qt_signal!(),

    /// rid -> (display_name, expires_at). In-memory.
    state: HashMap<i32, (String, Instant)>,
    /// Earliest deadline currently armed with `notify_later`, if any.
    armed: Option<Instant>,
}

impl TypingModel {
    fn set_session_id(&mut self, ctx: Option<ModelContext<Self>>, sid: i32) {
        if self.sessionId == sid {
            return;
        }
        self.sessionId = sid;
        let changed = !self.state.is_empty();
        self.state.clear();
        self.armed = None;
        if changed {
            self.typing_names_changed();
        }
        // Only update interests once the model is initialized (`ctx` is set).
        if ctx.is_some() {
            self.update_interests();
        }
    }

    fn typing_names_qml(&self, _ctx: Option<ModelContext<Self>>) -> QVariantList {
        let mut list = QVariantList::default();
        for (name, _) in self.state.values() {
            list.push(QString::from(name.as_str()).into());
        }
        list
    }

    fn init(&mut self, _ctx: ModelContext<Self>) {}

    /// Insert or refresh a typer. Returns whether the visible name set changed
    /// (a same-name Start is a no-op for QML but still refreshes the expiry).
    fn insert_typer(&mut self, rid: i32, name: String, expires: Instant) -> bool {
        let changed = !matches!(self.state.get(&rid), Some((existing, _)) if existing == &name);
        self.state.insert(rid, (name, expires));
        changed
    }

    /// Drop entries whose expiry has passed. Returns whether any were removed.
    fn sweep_expired(&mut self, now: Instant) -> bool {
        let before = self.state.len();
        self.state.retain(|_, (_, expires)| *expires > now);
        before != self.state.len()
    }

    fn earliest_expiry(&self) -> Option<Instant> {
        self.state.values().map(|(_, e)| *e).min()
    }

    /// Re-arm the sweep to the earliest future expiry, avoiding stacked timers.
    fn maybe_rearm(&mut self, _now: Instant, ctx: &ModelContext<Self>) {
        let need = match (self.earliest_expiry(), self.armed) {
            (Some(_e), None) => true,
            (Some(e), Some(a)) if e < a => true,
            (None, _) => {
                self.armed = None;
                false
            }
            _ => false,
        };
        if need {
            ctx.addr.do_send(TypingSweep);
        }
    }
}

impl EventObserving for TypingModel {
    type Context = ModelContext<Self>;

    fn observe(
        &mut self,
        ctx: Self::Context,
        event: Event,
        _matched: &[crate::store::observer::MatchedInterest],
    ) {
        let Some(rid) = event.key().as_i32() else {
            return;
        };
        let now_utc = Utc::now();
        let now_instant = Instant::now();

        let names_changed = match event.payload_of::<Typing>() {
            Some(TypingEvent::Started { sent_at }) => {
                let expiry = *sent_at + TYPING_EXPIRY;
                if expiry <= now_utc {
                    return; // arrived too late
                }
                let expires_instant =
                    now_instant + (expiry - now_utc).to_std().unwrap_or(Duration::ZERO);
                let name = ctx
                    .storage()
                    .fetch_recipient_by_id(rid)
                    .map(|r| typer_display_name(&r))
                    .unwrap_or_default();
                let changed = self.insert_typer(rid, name, expires_instant);
                self.maybe_rearm(now_instant, &ctx);
                changed
            }
            Some(TypingEvent::Stopped) => {
                let changed = self.state.remove(&rid).is_some();
                if changed {
                    self.maybe_rearm(now_instant, &ctx);
                }
                changed
            }
            None => return,
        };

        if names_changed {
            self.typing_names_changed();
        }
    }

    fn interests(&self) -> Vec<Interest> {
        if self.sessionId >= 0 {
            vec![Interest::process_with_relation(
                Typing,
                schema::sessions::table,
                self.sessionId,
            )]
        } else {
            Vec::new()
        }
    }
}

impl Handler<TypingSweep> for ObservingModelActor<TypingModel> {
    type Result = ();

    fn handle(&mut self, _: TypingSweep, ctx: &mut Self::Context) -> Self::Result {
        let Some(model) = self.model.as_pinned() else {
            ctx.stop();
            return;
        };
        let mut model = model.borrow_mut();
        let now = Instant::now();
        if model.sweep_expired(now) {
            model.typing_names_changed();
        }
        match model.earliest_expiry() {
            Some(e) if model.armed != Some(e) => {
                ctx.notify_later(TypingSweep, e.saturating_duration_since(now));
                model.armed = Some(e);
            }
            None if model.armed.is_some() => model.armed = None,
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_adds_and_earliest_tracks_expiry() {
        let mut m = TypingModel::default();
        let now = Instant::now();
        let a = now + Duration::from_secs(5);
        assert!(m.insert_typer(1, "alice".into(), a));
        assert_eq!(m.earliest_expiry(), Some(a));

        // Same name: no visible change, but expiry refreshes.
        let a2 = now + Duration::from_secs(8);
        assert!(!m.insert_typer(1, "alice".into(), a2));
        assert_eq!(m.earliest_expiry(), Some(a2));

        let b = now + Duration::from_secs(6);
        assert!(m.insert_typer(2, "bob".into(), b));
        // Alice (refreshed to 8s) is still latest; bob earlier.
        assert_eq!(m.earliest_expiry(), Some(b));
    }

    #[test]
    fn insert_replacing_name_signals_change() {
        let mut m = TypingModel::default();
        let now = Instant::now();
        m.insert_typer(1, "alice".into(), now + Duration::from_secs(5));
        assert!(m.insert_typer(1, "alice2".into(), now + Duration::from_secs(5)));
    }

    #[test]
    fn sweep_removes_only_expired() {
        let mut m = TypingModel::default();
        let now = Instant::now();
        m.insert_typer(1, "alice".into(), now - Duration::from_secs(1));
        m.insert_typer(2, "bob".into(), now + Duration::from_secs(5));
        assert!(m.sweep_expired(now));
        assert_eq!(m.earliest_expiry(), Some(now + Duration::from_secs(5)));
        // Re-sweep is a no-op.
        assert!(!m.sweep_expired(now));
    }

    #[test]
    fn empty_has_no_expiry() {
        let m = TypingModel::default();
        assert_eq!(m.earliest_expiry(), None);
    }
}
