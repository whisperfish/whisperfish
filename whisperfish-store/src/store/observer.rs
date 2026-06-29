//! Storage observer subsystem

mod orm_interests;

use std::any::{Any, TypeId};

use std::sync::Arc;

use uuid::Uuid;

/// Type-erased routing token identifying *what kind of thing* an [`Event`] or
/// [`Interest`] pertains to.
///
/// `Subject` is the generalization of the former closed `Table` enum: it routes
/// by [`TypeId`] rather than an enumerated variant list, so new kinds of
/// subjects (database tables today; future ephemeral/process subjects such as
/// typing notifications and username-resolution progress) register transparently
/// via [`Subject::of::<T>()`] without the observer core enumerating them. The
/// [`Eq`]/[`PartialEq`] impl compares only `tid`; `name` exists purely for
/// `tracing`/`Debug` legibility and is derived from [`std::any::type_name`].
///
/// For database rows, `T` is the diesel table type (e.g.
/// `schema::messages::table`); the diesel-table plumbing in this module
/// constructs subjects via [`Subject::of::<T>()`]. For future ephemeral/process
/// subjects, `T` will be a zero-sized marker type defined in the consumer crate.
#[derive(Clone, Debug)]
pub struct Subject {
    tid: TypeId,
    name: &'static str,
}

impl Subject {
    /// Construct a [`Subject`] for any `'static` type `T`.
    pub fn of<T: 'static>() -> Self {
        Self {
            tid: TypeId::of::<T>(),
            name: std::any::type_name::<T>(),
        }
    }

    /// Stable name suitable for `tracing`. Not part of equality.
    pub fn name(&self) -> &'static str {
        self.name
    }
}

impl PartialEq for Subject {
    fn eq(&self, other: &Self) -> bool {
        self.tid == other.tid
    }
}

impl Eq for Subject {}

impl std::hash::Hash for Subject {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.tid.hash(state);
    }
}

/// Links a process marker type to the typed payload carried by its events.
///
/// This is the consumer-side "downcasting infrastructure" that minimizes
/// syntactic overhead at `observe()` sites: instead of `event.payload
/// .as_ref()?.downcast_ref::<TypingTyper>()`, a model keyed off `Typing` writes
/// `event.payload_of::<Typing>()`. The observer core is oblivious to payload
/// shapes — it stores `Option<Arc<dyn Any + Send + Sync>>` and never downcasts;
/// only the consumer, which knows `P`, downcasts via [`Event::payload_of`].
///
/// Diesel-row events carry no payload and have no marker, so they never
/// participate in this trait. For process events that carry no payload on a
/// given lifecycle step (e.g. a typing `Delete`), the emitter simply constructs
/// a payloadless event (`process_event` or `observe_process_event`), and
/// `payload_of::<P>()` returns `None` — the consumer falls back to `key()`/
/// `relations` for the data it needs.
pub trait EventPayload: 'static {
    type Payload: Send + Sync + 'static;
}

#[derive(Debug, Clone)]
pub enum Interest {
    All,
    Row {
        subject: Subject,
        key: PrimaryKey,
    },
    Table {
        subject: Subject,
        relation: Option<Relation>,
    },
    /// Interest in an ephemeral/process [`Subject`] (a consumer-defined marker
    /// type, e.g. `Typing`), optionally scoped to a real DB row via `relation`.
    ///
    /// Structurally identical to [`Interest::Table`] but kept distinct so that
    /// [`Interest::Table`] cleanly means "a diesel table" and process subjects
    /// have an honest home. The matcher treats both the same: it compares
    /// [`Subject`] identity and, when `relation` is `Some`, requires the event to
    /// carry a matching relation edge.
    Process {
        subject: Subject,
        relation: Option<Relation>,
    },
}

impl Interest {
    pub fn whole_table<T: diesel::Table + 'static>(_table: T) -> Self {
        Interest::Table {
            subject: Subject::of::<T>(),
            relation: None,
        }
    }

    /// Watches a table T for changes related to a row in table U identified by a key
    /// `relation_key`.
    pub fn whole_table_with_relation<T, U>(
        _table: T,
        _related_table: U,
        relation_key: impl Into<PrimaryKey>,
    ) -> Self
    where
        T: diesel::Table + 'static,
        U: diesel::Table + 'static,
        U: diesel::JoinTo<T>,
    {
        Interest::Table {
            subject: Subject::of::<T>(),
            relation: Some(Relation {
                subject: Subject::of::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    pub fn row<T: diesel::Table + 'static>(_table: T, key: impl Into<PrimaryKey>) -> Self {
        Interest::Row {
            subject: Subject::of::<T>(),
            key: key.into(),
        }
    }

    /// Watch an ephemeral/process marker `P` for events related to a row in
    /// diesel table `U` identified by `relation_key`.
    ///
    /// `P` is the consumer-defined process marker (e.g. `Typing`); `U` is a real
    /// diesel table whose row scopes the interest. Observers declared this way
    /// receive process events whose `relations` include `(Subject::of::<U>(),
    /// relation_key)`.
    pub fn process_with_relation<P, U>(
        _process: P,
        _related_table: U,
        relation_key: impl Into<PrimaryKey>,
    ) -> Self
    where
        P: 'static,
        U: diesel::Table + 'static,
    {
        Interest::Process {
            subject: Subject::of::<P>(),
            relation: Some(Relation {
                subject: Subject::of::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    /// Watch an ephemeral/process marker `P` for *any* of its events, regardless
    /// of relation. Rarely useful (most observers want a specific scope) but
    /// provided for symmetry with [`Interest::whole_table`].
    pub fn process<P: 'static>(_process: P) -> Self {
        Interest::Process {
            subject: Subject::of::<P>(),
            relation: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Relation {
    subject: Subject,
    key: PrimaryKey,
}

impl Relation {
    /// The subject (table or process kind) this relation points at.
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    /// The primary key of the related row.
    pub fn key(&self) -> &PrimaryKey {
        &self.key
    }

    /// Construct a relation edge to a [`Subject`] (a diesel table or a process
    /// marker) at `key`. Used by process emitters to attach real DB rows to an
    /// ephemeral event so session-/row-scoped observers receive it.
    pub fn new(subject: Subject, key: impl Into<PrimaryKey>) -> Self {
        Relation {
            subject,
            key: key.into(),
        }
    }
}

/// Why an [`EventObserving::observe`] call fired, from the observer's point of
/// view.
///
/// `interest_index` is the positional index of the matching [`Interest`] in
/// the `Vec` the observer returned from [`EventObserving::interests`]. Order is
/// stable because `#[observing_model]` generates that `Vec`. `via_relation` is
/// `Some(declared)` when the matched interest was a
/// [`Interest::Table`] carrying a `relation` that matched through that relation;
/// `None` for [`Interest::All`], [`Interest::Row`], and relation-less
/// [`Interest::Table`] matches. It echoes the *declared* relation (which the
/// observer already knows) purely as a convenience so the observer can react in
/// context without re-deriving the path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchedInterest {
    interest_index: usize,
    via_relation: Option<Relation>,
}

impl MatchedInterest {
    pub fn interest_index(&self) -> usize {
        self.interest_index
    }

    pub fn via_relation(&self) -> Option<&Relation> {
        self.via_relation.as_ref()
    }
}

#[derive(Clone)]
pub struct Event {
    r#type: EventType,
    subject: Subject,
    key: PrimaryKey,
    relations: Vec<Relation>,
    /// Typed, type-erased payload carried by process events. `None` for all
    /// diesel-row events (and for payloadless process events such as a typing
    /// `Delete`). Consumers recover the concrete type via
    /// [`Event::payload_of::<P>()`] keyed off the process marker `P`, which
    /// keeps the observer core oblivious to payload shapes — it never
    /// downcasts, only routes on [`Subject`] + [`relations`].
    payload: Option<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Event")
            .field("type", &self.r#type)
            .field("subject", &self.subject)
            .field("key", &self.key)
            .field("relations", &self.relations)
            .field("payload", &self.payload.as_ref().map(|_| "<opaque>"))
            .finish()
    }
}

#[derive(Clone, Debug)]
pub enum EventType {
    Insert,
    Upsert,
    Update,
    Delete,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrimaryKey {
    Unknown,
    RowId(i32),
    StringRowId(String),
}

impl PrimaryKey {
    fn implies(&self, rhs: &PrimaryKey) -> bool {
        *self == PrimaryKey::Unknown || *self == *rhs
    }

    pub fn as_i32(&self) -> Option<i32> {
        match self {
            PrimaryKey::Unknown => None,
            PrimaryKey::RowId(i) => Some(*i),
            PrimaryKey::StringRowId(_) => None,
        }
    }
}

impl From<i32> for PrimaryKey {
    fn from(x: i32) -> Self {
        Self::RowId(x)
    }
}

impl From<String> for PrimaryKey {
    fn from(x: String) -> Self {
        Self::StringRowId(x)
    }
}

impl Event {
    pub fn for_table<T: diesel::Table + 'static>(&self, _table: T) -> bool {
        let subject = Subject::of::<T>();
        self.subject == subject
    }

    pub fn for_row<T: diesel::Table + 'static>(
        &self,
        _table: T,
        key_test: impl Into<PrimaryKey>,
    ) -> bool {
        let subject = Subject::of::<T>();
        self.subject == subject && self.key.implies(&key_test.into())
    }

    pub fn is_insert(&self) -> bool {
        matches!(self.r#type, EventType::Insert)
    }

    pub fn is_update_or_insert(&self) -> bool {
        matches!(
            self.r#type,
            EventType::Upsert | EventType::Insert | EventType::Update
        )
    }

    pub fn is_update(&self) -> bool {
        matches!(self.r#type, EventType::Update)
    }

    pub fn is_delete(&self) -> bool {
        matches!(self.r#type, EventType::Delete)
    }

    pub fn key(&self) -> &PrimaryKey {
        &self.key
    }

    pub fn relation_key_for<T: diesel::Table + 'static>(&self, _table: T) -> Option<&PrimaryKey> {
        if self.for_table(_table) {
            Some(&self.key)
        } else {
            let subject = Subject::of::<T>();
            self.relations
                .iter()
                .find(|relation| relation.subject == subject)
                .map(|relation| &relation.key)
        }
    }

    /// Typed access to the payload, keyed off a process marker `P` whose
    /// [`EventPayload::Payload`] is what it carries.
    ///
    /// Returns `None` for events with no payload (all diesel-row events, and
    /// payloadless process events such as a typing `Delete`) or whose subject is
    /// not `P`. The observer core is oblivious to payload shapes; only the
    /// consumer downcasts via this helper.
    pub fn payload_of<P: EventPayload>(&self) -> Option<&P::Payload> {
        self.payload.as_ref()?.downcast_ref::<P::Payload>()
    }
}

/// Shared matching logic for [`Interest::Table`] and [`Interest::Process`]:
/// match if `subject == ev_subject`, and (when `relation` is `Some`) require the
/// event to carry a matching relation edge — or, preserving legacy behaviour,
/// match when the event carries no relations at all. Returns `Some(None)` for a
/// relation-less match, `Some(Some(declared))` when matched through the relation.
fn match_relation(
    subject: &Subject,
    relation: Option<&Relation>,
    ev_subject: &Subject,
    relations: &[Relation],
) -> Option<Option<Relation>> {
    if subject != ev_subject {
        return None;
    }
    match relation {
        // Relation-less interest: any event on the subject matches.
        None => Some(None),
        Some(declared) => {
            let matched = relations.is_empty()
                || relations.iter().any(|event_relation| {
                    event_relation.subject == declared.subject && event_relation.key == declared.key
                });
            if matched {
                Some(Some(declared.clone()))
            } else {
                None
            }
        }
    }
}

impl Interest {
    /// Test whether `self` matches `ev`, returning `Some(via_relation)` if so.
    ///
    /// `via_relation` is `Some(declared_relation)` when the matched interest is
    /// a [`Interest::Table`] or [`Interest::Process`] that carried a `relation`
    /// and matched through it; `None` for [`Interest::All`], [`Interest::Row`],
    /// and relation-less [`Interest::Table`] / [`Interest::Process`] matches.
    /// `None` (the outer `Option`) means "no match."
    ///
    /// This is the structured counterpart of [`is_interesting`]; [`is_interesting`]
    /// is defined in terms of it.
    pub fn match_against(&self, ev: &Event) -> Option<Option<Relation>> {
        match (self, ev) {
            (Interest::All, _) => Some(None),
            (
                Interest::Table { subject, relation },
                Event {
                    subject: ev_subject,
                    relations,
                    ..
                },
            ) => match_relation(subject, relation.as_ref(), ev_subject, relations),

            // Ephemeral/process interests route structurally identically to
            // table interests; kept a distinct variant so `Interest::Table`
            // cleanly means "a diesel table". See [`Interest::Process`].
            (
                Interest::Process { subject, relation },
                Event {
                    subject: ev_subject,
                    relations,
                    ..
                },
            ) => match_relation(subject, relation.as_ref(), ev_subject, relations),

            (
                Interest::Row { subject, key },
                Event {
                    subject: ev_subject,
                    key: ev_key,
                    ..
                },
            ) => {
                if subject != ev_subject {
                    return None;
                }
                match ev_key {
                    // Legacy behaviour: an event on an unknown row matches any
                    // row-scoped interest on the same subject.
                    PrimaryKey::Unknown => Some(None),
                    ke if key == ke => Some(None),
                    _ => None,
                }
            }

            #[allow(unreachable_patterns)] // XXX should one of the enums be non-exhaustive instead?
            _ => {
                tracing::debug!(
                    "Unhandled event-interest pair; assuming interesting. {:?} {:?}",
                    ev,
                    self
                );
                // Preserves the legacy "assume interesting" fallback.
                Some(None)
            }
        }
    }
}

/// Compute the [`MatchedInterest`]s for an observer that declared `interests`,
/// against a single [`Event`]. Returned in ascending `interest_index` order; an
/// event may satisfy more than one declared interest.
pub fn matched_interests(interests: &[Interest], ev: &Event) -> Vec<MatchedInterest> {
    interests
        .iter()
        .enumerate()
        .filter_map(|(interest_index, interest)| {
            interest
                .match_against(ev)
                .map(|via_relation| MatchedInterest {
                    interest_index,
                    via_relation,
                })
        })
        .collect()
}

/// Construct an ephemeral/process [`Event`] for the marker type `P`.
///
/// `P` is a consumer-defined `'static` marker type (e.g. `Typing`, a future
/// `UsernameLookup`). The event's [`Subject`] is `Subject::of::<P>()`; its
/// `relations` carry the real DB rows it pertains to, so observers already
/// scoped to those rows (via [`Interest::whole_table_with_relation`] or a
/// hand-built [`Interest::Table`] with a `relation`) receive it through the
/// normal [`matched_interests`] dispatch — the matcher is oblivious to what
/// `P` is.
///
/// Typed payloads (`ProcessState` / `ProcessFailure`, R3), the `ProcessKind`
/// trait, and a process-state registry for late-observer replay are deferred to
/// the username-lookup MR, where they are first exercised; typing (binary,
/// encoded in [`EventType`]) needs none of them.
pub fn process_event<P: 'static>(
    r#type: EventType,
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
) -> Event {
    Event {
        r#type,
        subject: Subject::of::<P>(),
        key: key.into(),
        relations,
        payload: None,
    }
}

/// Construct an ephemeral/process [`Event`] for the marker `P`, carrying a
/// typed `payload` recoverable via [`Event::payload_of::<P>()`] at the consumer
/// site. [`EventPayload`] links `P` to its [`EventPayload::Payload`] type; the
/// observer core stores the payload as `Arc<dyn Any + Send + Sync>` and downcasts
/// only at the consumer's request.
///
/// Use this for process events whose data the consumer needs (e.g. a typing
/// `Insert` carrying the typer). Use [`process_event`] for payloadless steps
/// (e.g. a typing `Delete`, where the consumer keys off [`Event::key`] and
/// [`relations`] instead).
pub fn process_event_with_payload<P: EventPayload>(
    r#type: EventType,
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
    payload: P::Payload,
) -> Event {
    Event {
        r#type,
        subject: Subject::of::<P>(),
        key: key.into(),
        relations,
        payload: Some(Arc::new(payload)),
    }
}

impl Interest {
    pub fn is_interesting(&self, ev: &Event) -> bool {
        self.match_against(ev).is_some()
    }
}

pub trait Observatory {
    type Subscriber;

    fn register(&self, id: Uuid, interests: Vec<Interest>, subscriber: Self::Subscriber);
    fn update_interests(&self, id: Uuid, interests: Vec<Interest>);
    fn distribute_event(&self, event: Event);
}

pub trait Observable: Observatory + Clone {}

impl<O: Observatory + Clone> Observable for O {}

pub trait EventObserving {
    type Context;

    fn observe(&mut self, ctx: Self::Context, event: Event, matched: &[MatchedInterest])
    where
        Self: Sized;
    fn interests(&self) -> Vec<Interest>;
}

pub struct ObservationBuilder<'a, T, O>
where
    O: Observable,
{
    storage: &'a super::Storage<O>,
    event: Event,
    _table: T,
}

impl<T, O> Drop for ObservationBuilder<'_, T, O>
where
    O: Observable,
{
    fn drop(&mut self) {
        self.storage.distribute_event(self.event.clone());
    }
}

impl<'a, T, O> ObservationBuilder<'a, T, O>
where
    T: diesel::Table + 'static,
    O: Observable,
{
    pub fn with_relation<U>(mut self, _table: U, relation_key: impl Into<PrimaryKey>) -> Self
    where
        U: diesel::Table + 'static,
        U: diesel::JoinTo<T>,
    {
        self.event.relations.push(Relation {
            subject: Subject::of::<U>(),
            key: relation_key.into(),
        });
        self
    }

    /// Declare a two-hop transitive relation on the emitted event: the primary table
    /// `T` joins to `Via` via a declared FK, and `Via` joins to `Target` via a declared
    /// FK. Both edges are proven at compile time through `JoinTo` bounds, so this is as
    /// honest as [`with_relation`] without requiring a direct `T -> Target` FK.
    ///
    /// The pushed relation looks identical to a direct `with_relation(Target, key)` to
    /// consumers; only the *path* by which it was justified is two-hop.
    pub fn with_transitive_relation<Via, Target>(
        mut self,
        _via: Via,
        _target: Target,
        relation_key: impl Into<PrimaryKey>,
    ) -> Self
    where
        Via: diesel::Table + 'static,
        Target: diesel::Table + 'static,
        Via: diesel::JoinTo<T>,
        Target: diesel::JoinTo<Via>,
    {
        self.event.relations.push(Relation {
            subject: Subject::of::<Target>(),
            key: relation_key.into(),
        });
        self
    }
}

#[derive(Copy, Clone)]
pub struct ObserverHandle {
    id: Uuid,
}

impl<O: Observable> super::Storage<O> {
    pub fn register_observer(
        &mut self,
        interests: Vec<Interest>,
        subscriber: O::Subscriber,
    ) -> ObserverHandle {
        let id = Uuid::new_v4();
        self.observatory.register(id, interests, subscriber);
        ObserverHandle { id }
    }

    pub fn update_interests(&mut self, handle: ObserverHandle, interests: Vec<Interest>) {
        self.observatory.update_interests(handle.id, interests);
    }

    pub(super) fn distribute_event(&self, event: Event) {
        self.observatory.distribute_event(event);
    }

    pub fn observe_insert<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Insert,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_upsert<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Upsert,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_update<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Update,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_delete<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Delete,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    /// Emit an ephemeral/process event for the marker type `P`. Thin wrapper
    /// over [`process_event`] + [`distribute_event`]; provided on `Storage`
    /// so process coordinators use the same surface as DB emitters. The
    /// relations should name the real DB rows the event pertains to so
    /// session-/row-scoped observers receive it.
    pub fn observe_process_event<P: 'static>(
        &self,
        r#type: EventType,
        key: impl Into<PrimaryKey>,
        relations: Vec<Relation>,
    ) {
        self.distribute_event(process_event::<P>(r#type, key, relations));
    }

    /// Emit an ephemeral/process event for the marker `P` carrying a typed
    /// payload (see [`process_event_with_payload`]). Use this for process steps
    /// whose data the consumer needs; use [`observe_process_event`] for
    /// payloadless steps.
    pub fn observe_process_event_with_payload<P: EventPayload>(
        &self,
        r#type: EventType,
        key: impl Into<PrimaryKey>,
        relations: Vec<Relation>,
        payload: P::Payload,
    ) {
        self.distribute_event(process_event_with_payload::<P>(
            r#type, key, relations, payload,
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema;

    fn messages() -> Subject {
        Subject::of::<schema::messages::table>()
    }
    fn sessions() -> Subject {
        Subject::of::<schema::sessions::table>()
    }
    fn recipients() -> Subject {
        Subject::of::<schema::recipients::table>()
    }

    #[test]
    fn relation_event_generates_interest() {
        let interest = Interest::whole_table_with_relation(
            schema::messages::table,
            schema::sessions::table,
            1,
        );

        let event_on_session_0 = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 52.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: None,
        };
        let event_on_session_1 = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 66.into(),
            relations: vec![
                Relation {
                    subject: recipients(),
                    key: 26.into(),
                },
                Relation {
                    subject: sessions(),
                    key: 1.into(),
                },
            ],
            payload: None,
        };

        assert!(!interest.is_interesting(&event_on_session_0));
        assert!(interest.is_interesting(&event_on_session_1));
    }

    #[test]
    fn table_event_generates_interest() {
        let interest = Interest::whole_table(schema::messages::table);

        let event = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 52.into(),
            relations: vec![],
            payload: None,
        };

        assert!(interest.is_interesting(&event));
    }

    #[test]
    fn row_event_generates_interest() {
        let interest = Interest::row(schema::messages::table, 2);

        let negative = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 1.into(),
            relations: vec![],
            payload: None,
        };
        let positive = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 2.into(),
            relations: vec![],
            payload: None,
        };

        assert!(!interest.is_interesting(&negative));
        assert!(interest.is_interesting(&positive));
    }

    #[test]
    fn event() {
        let e = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 1.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: None,
        };
        assert!(e.for_table(crate::schema::messages::dsl::messages));
        assert!(!e.for_table(crate::schema::sessions::dsl::sessions));

        assert!(e.for_row(crate::schema::messages::dsl::messages, 1));
        assert!(!e.for_row(crate::schema::messages::dsl::messages, 2));

        assert!(e.is_insert());
        assert!(e.is_update_or_insert());
        assert!(!e.is_update());
        assert!(!e.is_delete());

        assert_eq!(*e.key(), PrimaryKey::RowId(1));

        assert_eq!(
            e.relation_key_for(crate::schema::sessions::dsl::sessions),
            Some(&PrimaryKey::RowId(0))
        );
        assert_eq!(
            e.relation_key_for(crate::schema::messages::dsl::messages),
            Some(&PrimaryKey::RowId(1))
        );
    }

    #[test]
    fn interest() {
        let i_all = Interest::All;
        let i_row = Interest::Row {
            subject: messages(),
            key: 2.into(),
        };
        let e_1 = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 1.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: None,
        };
        let e_2 = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 2.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: None,
        };
        let e_u = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: PrimaryKey::Unknown,
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: None,
        };
        let e_s = Event {
            r#type: EventType::Insert,
            subject: sessions(),
            key: PrimaryKey::Unknown,
            relations: vec![],
            payload: None,
        };
        assert!(i_all.is_interesting(&e_1));
        assert!(!i_row.is_interesting(&e_1));
        assert!(i_row.is_interesting(&e_2));
        assert!(i_row.is_interesting(&e_u));
        assert!(!i_row.is_interesting(&e_s));

        let i_cln = i_row.clone();
        match (i_cln, i_row) {
            (
                Interest::Row {
                    subject: t1,
                    key: k1,
                },
                Interest::Row {
                    subject: t2,
                    key: k2,
                },
            ) => {
                assert_eq!(t1, t2);
                assert_eq!(k1, k2);
            }
            _ => unreachable!(),
        }
    }

    #[test]
    fn primary_key() {
        let pk_u = PrimaryKey::Unknown;
        let pk_i = PrimaryKey::RowId(5);
        let pk_s = PrimaryKey::StringRowId("uuid".into());

        assert!(pk_i.as_i32().is_some());
        assert!(pk_s.as_i32().is_none());
        assert!(pk_u.implies(&pk_i));
        assert!(!pk_i.implies(&pk_s));
        assert!(pk_i.implies(&PrimaryKey::RowId(5)));
        assert!(!pk_s.implies(&PrimaryKey::StringRowId("other".into())));
    }

    #[test]
    fn subject_equality_is_typeid_based() {
        // Same type → equal regardless of the fact that the names differ on
        // re-parameterization of the *type*; here we only have one concrete
        // type per call, so this is the trivial positive case.
        let a = Subject::of::<schema::messages::table>();
        let b = Subject::of::<schema::messages::table>();
        assert_eq!(a, b);
        assert_eq!(a.name(), b.name());

        // Different diesel tables → not equal.
        let s = Subject::of::<schema::sessions::table>();
        assert_ne!(a, s);
    }

    #[test]
    fn subject_distinguishes_non_diesel_types() {
        // Marker types (the future shape of ephemeral/process subjects) route
        // distinctly from diesel tables and from each other, even though they
        // carry no data. This is the property the typing retrofit and
        // username-lookup will lean on.
        struct Typing;
        struct UsernameLookup;

        assert_ne!(Subject::of::<Typing>(), Subject::of::<UsernameLookup>());
        assert_ne!(
            Subject::of::<Typing>(),
            Subject::of::<schema::messages::table>()
        );
        assert_eq!(Subject::of::<Typing>(), Subject::of::<Typing>());
    }

    #[test]
    fn process_subject_routes_via_relation_to_session_scoped_interest() {
        // A process subject (here a `Typing` marker) is opaque to the matcher:
        // it routes purely on `Subject` identity + the relation edge to a real
        // DB row. This is the routing shape the typing retrofit will use — an
        // observer scoped to `sessions(sid)` by [`Interest::process_with_relation`]
        // receives a `Typing` event that carries `sessions(sid)` as its
        // relation, *without* the matcher knowing what `Typing` is.
        struct Typing;

        let on_session_1 = Event {
            r#type: EventType::Update,
            subject: Subject::of::<Typing>(),
            key: 7.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 1.into(),
            }],
            payload: None,
        };
        let on_session_2 = Event {
            r#type: EventType::Update,
            subject: Subject::of::<Typing>(),
            key: 8.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 2.into(),
            }],
            payload: None,
        };

        // `process_with_relation` is the ergonomic constructor for process
        // interests scoped to a real diesel row; it's what a SessionModel-like
        // observer will declare for typing.
        let scoped_to_session_1 =
            Interest::process_with_relation(Typing, schema::sessions::table, 1);

        assert!(scoped_to_session_1.is_interesting(&on_session_1));
        assert!(!scoped_to_session_1.is_interesting(&on_session_2));
    }

    #[test]
    fn match_against_returns_via_relation_for_table_with_relation() {
        // A `Table { relation: Some(_) }` interest that matches through its
        // declared relation echoes that relation back as `via_relation`.
        let interest = Interest::whole_table_with_relation(
            schema::messages::table,
            schema::sessions::table,
            1,
        );
        let event = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 9.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 1.into(),
            }],
            payload: None,
        };

        let matched = interest.match_against(&event).expect("should match");
        let via = matched.expect("relation-scoped interest should carry a via_relation");
        assert_eq!(via.subject(), &sessions());
        assert_eq!(via.key(), &PrimaryKey::RowId(1));

        // A relation-scoped interest that doesn't match the event's relation
        // key must yield no match.
        let event_other_session = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 9.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 2.into(),
            }],
            payload: None,
        };
        assert!(interest.match_against(&event_other_session).is_none());
    }

    #[test]
    fn match_against_relation_less_interest_yields_none_via() {
        // `All`, row-scoped, and relation-less table interests all match with
        // `via_relation == None`.
        let all = Interest::All;
        let table_plain = Interest::whole_table(schema::messages::table);
        let row = Interest::row(schema::messages::table, 5);
        let ev = Event {
            r#type: EventType::Update,
            subject: messages(),
            key: 5.into(),
            relations: vec![],
            payload: None,
        };

        for interest in [all, table_plain, row] {
            assert_eq!(
                interest.match_against(&ev),
                Some(None),
                "relation-less interests should match with via=None"
            );
        }
    }

    #[test]
    fn matched_interests_reports_index_and_dedups_per_interest() {
        // An event can satisfy multiple declared interests; `matched_interests`
        // reports one entry per satisfying interest, with correct positional
        // index, in declaration order.
        let interests: Vec<Interest> = vec![
            // index 0: matches (table on messages)
            Interest::whole_table(schema::messages::table),
            // index 1: does not match (row on sessions, event is on messages)
            Interest::row(schema::sessions::table, 1),
            // index 2: matches (messages scoped to sessions(1))
            Interest::whole_table_with_relation(
                schema::messages::table,
                schema::sessions::table,
                1,
            ),
            // index 3: does not match (messages scoped to sessions(2))
            Interest::whole_table_with_relation(
                schema::messages::table,
                schema::sessions::table,
                2,
            ),
        ];
        let ev = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 42.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 1.into(),
            }],
            payload: None,
        };

        let matched = matched_interests(&interests, &ev);
        let indices: Vec<usize> = matched.iter().map(|m| m.interest_index()).collect();
        assert_eq!(indices, vec![0, 2]);

        // The relation-scoped match (index 2) carries the declared relation;
        // the plain table match (index 0) carries None.
        let m0 = &matched[0];
        let m2 = &matched[1];
        assert!(m0.via_relation().is_none());
        let via2 = m2
            .via_relation()
            .expect("relation-scoped match has via_relation");
        assert_eq!(via2.subject(), &sessions());
        assert_eq!(via2.key(), &PrimaryKey::RowId(1));
    }

    #[test]
    fn matched_interests_empty_when_no_interest_matches() {
        let interests = vec![Interest::row(schema::messages::table, 1)];
        let ev = Event {
            r#type: EventType::Insert,
            subject: sessions(),
            key: 1.into(),
            relations: vec![],
            payload: None,
        };
        assert!(matched_interests(&interests, &ev).is_empty());
    }

    #[test]
    fn process_event_routes_to_session_scoped_interest_via_relation() {
        // The emit entry point that the typing retrofit (L4) will use: a process
        // marker `Typing` emits an Insert scoped to a session by carrying the
        // `sessions(sid)` relation. A hand-built session-scoped interest (the
        // shape `whole_table_with_relation` produces for diesel tables, but for
        // a non-diesel subject) receives it with `via_relation` echoing the
        // declared `sessions(sid)` relation.
        struct Typing;

        let sid = 7;
        let ev =
            process_event::<Typing>(EventType::Insert, sid, vec![Relation::new(sessions(), sid)]);

        assert_eq!(ev.subject, Subject::of::<Typing>());
        assert_eq!(ev.key(), &PrimaryKey::RowId(sid));

        let interest = Interest::process_with_relation(Typing, schema::sessions::table, sid);
        let matched = matched_interests(std::slice::from_ref(&interest), &ev);
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0].interest_index(), 0);
        let via = matched[0]
            .via_relation()
            .expect("relation-scoped process match carries via_relation");
        assert_eq!(via.subject(), &sessions());
        assert_eq!(via.key(), &PrimaryKey::RowId(sid));
    }

    #[test]
    fn process_event_delete_propagates_to_same_observer() {
        // Lifecycle symmetry: Delete reaches the same session-scoped interest
        // that Insert created, so the observer can tear down on timeout/stop.
        struct Typing;
        let sid = 7;
        let interest = Interest::process_with_relation(Typing, schema::sessions::table, sid);
        let interests = [interest];

        let insert =
            process_event::<Typing>(EventType::Insert, sid, vec![Relation::new(sessions(), sid)]);
        let delete =
            process_event::<Typing>(EventType::Delete, sid, vec![Relation::new(sessions(), sid)]);

        assert_eq!(matched_interests(&interests, &insert).len(), 1);
        assert_eq!(matched_interests(&interests, &delete).len(), 1);
    }

    #[test]
    fn process_payload_round_trips_via_payload_of() {
        // The marker `Typing` declares its payload type via `EventPayload`;
        // `payload_of::<Typing>()` is the one-line typed access at the
        // consumer site. Diesel events and payloadless process events return
        // `None`.
        struct Typing;
        #[derive(Debug, PartialEq)]
        struct Typer {
            id: i32,
            name: String,
        }
        impl EventPayload for Typing {
            type Payload = Typer;
        }

        let sid = 7;
        let typer = Typer {
            id: 42,
            name: "alice".to_string(),
        };
        let ev = process_event_with_payload::<Typing>(
            EventType::Insert,
            42,
            vec![Relation::new(sessions(), sid)],
            Typer {
                id: typer.id,
                name: typer.name.clone(),
            },
        );

        assert_eq!(ev.payload_of::<Typing>(), Some(&typer));

        // A payloadless Delete has no payload to downcast — consumer reads key().
        let delete =
            process_event::<Typing>(EventType::Delete, 42, vec![Relation::new(sessions(), sid)]);
        assert_eq!(delete.payload_of::<Typing>(), None);

        // Diesel events carry no payload.
        let db_ev = Event {
            r#type: EventType::Insert,
            subject: messages(),
            key: 1.into(),
            relations: vec![],
            payload: None,
        };
        assert_eq!(db_ev.payload_of::<Typing>(), None);
    }

    #[test]
    fn process_interest_without_relation_matches_any_process_event() {
        // `Interest::process` (relation-less) matches any event on that process
        // subject, in parallel with `Interest::whole_table` for diesel rows.
        struct Typing;

        let interest = Interest::process(Typing);
        let ev = process_event::<Typing>(EventType::Update, 9, vec![Relation::new(sessions(), 3)]);
        assert!(interest.is_interesting(&ev));

        // And reports None as via_relation (no declared relation to echo).
        let matched = interest.match_against(&ev).expect("should match");
        assert!(matched.is_none());
    }

    #[test]
    fn process_interest_is_distinct_from_table_interest() {
        // `Interest::Process` and `Interest::Table` with the same (Subject,
        // relation) shape behave identically for routing — the distinction is
        // semantic (process vs diesel table), enforced by type-level
        // constructors. A diesel-table interest must NOT route a process event
        // of the same subject, and vice versa — but since `Subject` identity is
        // what's compared, this reduces to: different subjects don't match.
        struct Typing;

        let process_interest = Interest::process_with_relation(Typing, schema::sessions::table, 1);
        // A diesel-table interest on `sessions` is a different subject from the
        // `Typing` marker, so a Typing event must not match it.
        let db_interest = Interest::whole_table(schema::sessions::table);
        let typing_ev =
            process_event::<Typing>(EventType::Insert, 1, vec![Relation::new(sessions(), 1)]);
        assert!(process_interest.is_interesting(&typing_ev));
        assert!(!db_interest.is_interesting(&typing_ev));
    }
}
