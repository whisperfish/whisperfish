//! Storage observer subsystem

mod diesel;
mod orm_interests;

use std::any::{Any, TypeId};

use std::sync::Arc;

use uuid::Uuid;

/// Type-erased routing token identifying what an [`Event`] or [`Interest`] is about.
///
/// `Subject` is the single identity primitive of the observer core: diesel
/// tables and process markers are both represented as `Subject`, distinguished
/// only by the Rust type they stand for. The diesel/process distinction lives
/// entirely at the construction boundaries (see [`diesel`] and [`process`]),
/// where diesel-specific bounds (`diesel::Table`, `diesel::JoinTo`) gate the
/// constructors; the core never inspects it.
#[derive(Clone)]
pub struct Subject {
    tid: TypeId,
    name: &'static str,
}

impl std::fmt::Debug for Subject {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Subject").field(&self.name).finish()
    }
}

impl Subject {
    /// Construct a [`Subject`] for any `'static` type `T`.
    ///
    /// For diesel tables the `diesel::Table` bound lives on the constructors in
    /// [`diesel`] (`Interest::whole_table`, etc.) rather than here.
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

#[derive(Debug, Clone)]
pub enum Interest {
    All,
    /// Row-scoped interest on a subject: matches a single row by key. The
    /// subject may be a diesel table or a process marker.
    Row {
        subject: Subject,
        key: PrimaryKey,
    },
    /// Subject-scoped interest, optionally scoped to a real DB row via
    /// `relation`. The subject may be a diesel table or a process marker;
    /// diesel is "just another subject" from the matcher's perspective.
    Subject {
        subject: Subject,
        relation: Option<Relation>,
    },
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Relation {
    subject: Subject,
    key: PrimaryKey,
}

impl Relation {
    pub fn subject(&self) -> &Subject {
        &self.subject
    }

    pub fn key(&self) -> &PrimaryKey {
        &self.key
    }

    /// Construct a relation edge to `subject` at `key`.
    pub fn new(subject: Subject, key: impl Into<PrimaryKey>) -> Self {
        Relation {
            subject,
            key: key.into(),
        }
    }
}

#[derive(Clone)]
pub struct Event {
    subject: Subject,
    key: PrimaryKey,
    relations: Vec<Relation>,
    /// Typed payload carried by all events. Diesel-row events carry an
    /// [`EventType`]; process events carry their domain verb (e.g.
    /// `TypingEvent`); payloadless process events carry `()`. Recovered by the
    /// consumer via [`Event::payload_of::<T>()`].
    payload: Arc<dyn Any + Send + Sync>,
}

impl std::fmt::Debug for Event {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let payload_label = if self.payload.is::<()>() {
            "<empty>"
        } else {
            "<opaque>"
        };
        f.debug_struct("Event")
            .field("subject", &self.subject)
            .field("key", &self.key)
            .field("relations", &self.relations)
            .field("payload", &payload_label)
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// Recover the payload as `T`, if this event carries one.
    ///
    /// Diesel-row events carry [`EventType`]; process events carry their
    /// domain verb (e.g. `TypingEvent`); payloadless process events carry `()`,
    /// which downcasts to any non-`()` `T` as `None`. This is a one-line
    /// `downcast_ref` over [`Event::payload`]; the type association lives at
    /// the call site, not in the event.
    pub fn payload_of<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.payload.downcast_ref::<T>()
    }

    pub fn is_insert(&self) -> bool {
        matches!(self.payload_of::<EventType>(), Some(EventType::Insert))
    }

    pub fn is_update_or_insert(&self) -> bool {
        matches!(
            self.payload_of::<EventType>(),
            Some(EventType::Upsert | EventType::Insert | EventType::Update)
        )
    }

    pub fn is_update(&self) -> bool {
        matches!(self.payload_of::<EventType>(), Some(EventType::Update))
    }

    pub fn is_delete(&self) -> bool {
        matches!(self.payload_of::<EventType>(), Some(EventType::Delete))
    }

    pub fn key(&self) -> &PrimaryKey {
        &self.key
    }
}

impl Interest {
    /// Watch subject `T` for any event, regardless of relation. `T` is the
    /// payload type for process events (subject = payload type under the fold
    /// that unified them), or a marker for any other subject.
    pub fn on<T: 'static>() -> Self {
        Interest::Subject {
            subject: Subject::of::<T>(),
            relation: None,
        }
    }

    /// Watch subject `T` for events related to row `relation_key` in subject
    /// `U`. The relation anchor may be a diesel table or any other subject;
    /// the matcher only routes on [`Subject`] identity.
    pub fn on_with_relation<T: 'static, U: 'static>(relation_key: impl Into<PrimaryKey>) -> Self {
        Interest::Subject {
            subject: Subject::of::<T>(),
            relation: Some(Relation {
                subject: Subject::of::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    /// Test whether `self` matches `ev`.
    ///
    /// The verb/domain the consumer needs is recovered separately from the
    /// event's typed payload (`payload_of::<T>()`); the matcher only decides
    /// routing on [`Subject`] identity + [`Relation`] edges.
    pub fn is_interesting(&self, ev: &Event) -> bool {
        match self {
            Interest::All => true,
            Interest::Subject { subject, relation } => {
                if subject != &ev.subject {
                    return false;
                }
                match relation {
                    // Relation-less interest: any event on the subject matches.
                    None => true,
                    Some(declared) => {
                        ev.relations.is_empty()
                            || ev.relations.iter().any(|event_relation| {
                                event_relation.subject == declared.subject
                                    && event_relation.key == declared.key
                            })
                    }
                }
            }
            Interest::Row { subject, key } => {
                if subject != &ev.subject {
                    return false;
                }
                match &ev.key {
                    // An unknown-key event matches any row-scoped interest on the same subject.
                    PrimaryKey::Unknown => true,
                    ke => key == ke,
                }
            }
        }
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

    fn observe(&mut self, ctx: Self::Context, event: Event)
    where
        Self: Sized;
    fn interests(&self) -> Vec<Interest>;
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

    /// Emit an event whose subject is `Subject::of::<T>()` and whose payload is
    /// `payload`. This is the general process constructor: for process
    /// events, subject and payload type are the same, so `T` is inferred from
    /// the `payload` value and no turbofish is needed at the call site.
    pub fn observe_event<T: Send + Sync + 'static>(
        &self,
        key: impl Into<PrimaryKey>,
        relations: Vec<Relation>,
        payload: T,
    ) {
        self.distribute_event(event(key, relations, payload));
    }
}

/// Construct an [`Event`] whose subject is `Subject::of::<T>()` and whose
/// payload is `payload`. For process events the subject and payload type are
/// the same, so `T` is inferred from `payload`.
///
/// Diesel-row events use `EventType` as the payload type but a diesel table
/// as the subject (subject ≠ payload by design); those events go through the
/// diesel constructors in [`diesel`] rather than this one.
pub fn event<T: Send + Sync + 'static>(
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
    payload: T,
) -> Event {
    Event {
        subject: Subject::of::<T>(),
        key: key.into(),
        relations,
        payload: Arc::new(payload),
    }
}

/// Construct a payloadless [`Event`] whose subject is `Subject::of::<T>()`.
/// The payload is `()`; consumers recover their payload type with
/// [`Event::payload_of::<T>()`] and get `None`.
pub fn event_without_payload<T: 'static>(
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
) -> Event {
    Event {
        subject: Subject::of::<T>(),
        key: key.into(),
        relations,
        payload: Arc::new(()),
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
            subject: messages(),
            key: 52.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: Arc::new(EventType::Insert),
        };
        let event_on_session_1 = Event {
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
            payload: Arc::new(EventType::Insert),
        };

        assert!(!interest.is_interesting(&event_on_session_0));
        assert!(interest.is_interesting(&event_on_session_1));
    }

    #[test]
    fn table_event_generates_interest() {
        let interest = Interest::whole_table(schema::messages::table);

        let event = Event {
            subject: messages(),
            key: 52.into(),
            relations: vec![],
            payload: Arc::new(EventType::Insert),
        };

        assert!(interest.is_interesting(&event));
    }

    #[test]
    fn row_event_generates_interest() {
        let interest = Interest::row(schema::messages::table, 2);

        let negative = Event {
            subject: messages(),
            key: 1.into(),
            relations: vec![],
            payload: Arc::new(EventType::Insert),
        };
        let positive = Event {
            subject: messages(),
            key: 2.into(),
            relations: vec![],
            payload: Arc::new(EventType::Insert),
        };

        assert!(!interest.is_interesting(&negative));
        assert!(interest.is_interesting(&positive));
    }

    #[test]
    fn event() {
        let e = Event {
            subject: messages(),
            key: 1.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: Arc::new(EventType::Insert),
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
            subject: messages(),
            key: 1.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: Arc::new(EventType::Insert),
        };
        let e_2 = Event {
            subject: messages(),
            key: 2.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: Arc::new(EventType::Insert),
        };
        let e_u = Event {
            subject: messages(),
            key: PrimaryKey::Unknown,
            relations: vec![Relation {
                subject: sessions(),
                key: 0.into(),
            }],
            payload: Arc::new(EventType::Insert),
        };
        let e_s = Event {
            subject: sessions(),
            key: PrimaryKey::Unknown,
            relations: vec![],
            payload: Arc::new(EventType::Insert),
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
        assert_eq!(
            Subject::of::<schema::messages::table>(),
            Subject::of::<schema::messages::table>()
        );
        assert_ne!(
            Subject::of::<schema::messages::table>(),
            Subject::of::<schema::sessions::table>()
        );
    }

    #[test]
    fn process_subject_routes_via_relation_to_session_scoped_interest() {
        // A process subject (here a `Typing` marker) is opaque to the matcher:
        // it routes purely on `Subject` identity + the relation edge to a real
        // DB row. This is the routing shape the typing retrofit will use — an
        // observer scoped to `sessions(sid)` by [`Interest::on_with_relation`]
        // receives a `Typing` event that carries `sessions(sid)` as its
        // relation, *without* the matcher knowing what `Typing` is.
        struct Typing;

        let on_session_1 = Event {
            subject: Subject::of::<Typing>(),
            key: 7.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 1.into(),
            }],
            payload: Arc::new(EventType::Update),
        };
        let on_session_2 = Event {
            subject: Subject::of::<Typing>(),
            key: 8.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 2.into(),
            }],
            payload: Arc::new(EventType::Update),
        };

        // `on_with_relation` is the ergonomic constructor for process
        // interests scoped to a real diesel row; it's what a SessionModel-like
        // observer will declare for typing.
        let scoped_to_session_1 = Interest::on_with_relation::<Typing, schema::sessions::table>(1);

        assert!(scoped_to_session_1.is_interesting(&on_session_1));
        assert!(!scoped_to_session_1.is_interesting(&on_session_2));
    }

    #[test]
    fn event_routes_to_session_scoped_interest_via_relation() {
        // The general `event_without_payload` emit entry point routes a typing
        // event to a session-scoped interest via the carried `sessions(sid)`
        // relation. The interest is scoped by [`Interest::on_with_relation`];
        // the matcher routes purely on `Subject` identity + the relation edge.
        struct Typing;

        let sid = 7;
        let ev = event_without_payload::<Typing>(sid, vec![Relation::new(sessions(), sid)]);

        assert_eq!(ev.subject, Subject::of::<Typing>());
        assert_eq!(ev.key(), &PrimaryKey::RowId(sid));

        let interest = Interest::on_with_relation::<Typing, schema::sessions::table>(sid);
        assert!(
            interest.is_interesting(&ev),
            "session-scoped interest should match a relation-routed event"
        );

        // A re-emit (e.g. a "stopped" delta) reaches the same interest.
        let delete = event_without_payload::<Typing>(sid, vec![Relation::new(sessions(), sid)]);
        assert!(interest.is_interesting(&delete));

        // And a different session does not.
        let other_session =
            event_without_payload::<Typing>(sid, vec![Relation::new(sessions(), sid + 1)]);
        assert!(!interest.is_interesting(&other_session));
    }

    #[test]
    fn process_payload_round_trips_via_payload_of() {
        // The general `event(payload)` constructor routes on `Subject::of::<T>()`
        // where `T` is the payload type (inferred from the payload value);
        // `payload_of::<T>()` is the one-line typed access at the consumer
        // site (a `downcast_ref` over `Event::payload`). Payloadless process
        // events and diesel-row events return `None`.
        #[derive(Debug, PartialEq)]
        struct Typer {
            id: i32,
            name: String,
        }

        let sid = 7;
        let typer = Typer {
            id: 42,
            name: "alice".to_string(),
        };
        let ev = super::event(
            42,
            vec![Relation::new(sessions(), sid)],
            Typer {
                id: typer.id,
                name: typer.name.clone(),
            },
        );

        assert_eq!(ev.payload_of::<Typer>(), Some(&typer));

        // A payloadless process event has no payload to downcast — consumer reads key().
        let delete = event_without_payload::<Typer>(42, vec![Relation::new(sessions(), sid)]);
        assert_eq!(delete.payload_of::<Typer>(), None);

        // Diesel events carry an `EventType` payload, not a `Typer`.
        let db_ev = Event {
            subject: messages(),
            key: 1.into(),
            relations: vec![],
            payload: Arc::new(EventType::Insert),
        };
        assert_eq!(db_ev.payload_of::<Typer>(), None);
        assert_eq!(db_ev.payload_of::<EventType>(), Some(&EventType::Insert));
    }

    #[test]
    fn on_interest_without_relation_matches_any_event() {
        // `Interest::on` (relation-less) matches any event on that process
        // subject, in parallel with `Interest::whole_table` for diesel rows.
        struct Typing;

        let interest = Interest::on::<Typing>();
        let ev = event_without_payload::<Typing>(9, vec![Relation::new(sessions(), 3)]);
        assert!(interest.is_interesting(&ev));
    }
}
