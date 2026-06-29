//! Storage observer subsystem

mod diesel;
mod orm_interests;
mod process;

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

/// Links a marker type `P` to the concrete payload type it carries.
///
/// The payload is stored as `Arc<dyn Any + Send + Sync>` and recovered by the
/// consumer through [`Event::payload_of::<P>()`].
///
/// - Diesel-row events use [`DieselRow`], whose payload is the CRUD
///   [`EventType`]; the table identity lives on [`Event::subject`].
/// - Process markers use their own marker type (e.g. `Typing`), whose payload
///   is the domain verb (e.g. `TypingEvent`). Payloadless markers declare
///   `type Payload = ()` and carry an `Arc::new(())`.
pub trait EventPayload: 'static {
    type Payload: Send + Sync + 'static;
}

/// Marker for diesel-row events. Its payload is the CRUD [`EventType`].
///
/// This is the diesel counterpart of a process marker like `Typing`: both are
/// just [`Subject`]s at the core, and the "verb" rides in the payload,
/// accessed via [`Event::payload_of::<DieselRow>()`] resp.
/// [`Event::payload_of::<P>()`]. The only thing that makes `DieselRow`
/// diesel-y is that its constructors live in [`diesel`] behind `diesel::Table`
/// bounds; the core treat it as "just another process".
pub struct DieselRow;

impl EventPayload for DieselRow {
    type Payload = EventType;
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

/// Information about the [`Interest`] that caused an observer to fire.
///
/// `interest_index` is the position of the matching interest in the observer's
/// declared list. `via_relation` is the relation edge through which the match
/// happened, if any.
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
    subject: Subject,
    key: PrimaryKey,
    relations: Vec<Relation>,
    /// Typed payload carried by all events. Diesel-row events carry an
    /// [`EventType`] (recovered via [`Event::payload_of::<DieselRow>()`]);
    /// process events carry their domain verb (recovered via
    /// [`Event::payload_of::<P>()`] for the marker `P`); payloadless markers
    /// carry `()`.
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
    /// Recover the payload for the marker type `P`, if any.
    ///
    /// For diesel-row events use [`DieselRow`]; for process markers use the
    /// marker type `P`. Returns `None` when the stored payload is not `P::Payload`
    /// (e.g. a `()`-stored event downcast to a typed payload).
    pub fn payload_of<P: EventPayload>(&self) -> Option<&P::Payload> {
        self.payload.downcast_ref::<P::Payload>()
    }

    pub fn is_insert(&self) -> bool {
        matches!(self.payload_of::<DieselRow>(), Some(EventType::Insert))
    }

    pub fn is_update_or_insert(&self) -> bool {
        matches!(
            self.payload_of::<DieselRow>(),
            Some(EventType::Upsert | EventType::Insert | EventType::Update)
        )
    }

    pub fn is_update(&self) -> bool {
        matches!(self.payload_of::<DieselRow>(), Some(EventType::Update))
    }

    pub fn is_delete(&self) -> bool {
        matches!(self.payload_of::<DieselRow>(), Some(EventType::Delete))
    }

    pub fn key(&self) -> &PrimaryKey {
        &self.key
    }
}

/// Match a table/process interest against an event.
///
/// Returns `Some(None)` for a relation-less match, `Some(Some(relation))` when
/// matched through the declared relation, and `None` when the subject differs.
fn match_relation<S: PartialEq>(
    subject: &S,
    relation: Option<&Relation>,
    ev_subject: &S,
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
    /// Test whether `self` matches `ev`.
    ///
    /// Returns `Some(via_relation)` on a match, where `via_relation` is `None`
    /// unless the match happened through a declared relation. Returns `None`
    /// when the event does not match.
    pub fn match_against(&self, ev: &Event) -> Option<Option<Relation>> {
        match (self, ev) {
            (Interest::All, _) => Some(None),
            (Interest::Subject { subject, relation }, Event { relations, .. }) => {
                match_relation(subject, relation.as_ref(), &ev.subject, relations)
            }

            (Interest::Row { subject, key }, Event { key: ev_key, .. }) => {
                if subject != &ev.subject {
                    return None;
                }
                match ev_key {
                    // An unknown-key event matches any row-scoped interest on the same subject.
                    PrimaryKey::Unknown => Some(None),
                    ke if key == ke => Some(None),
                    _ => None,
                }
            }
        }
    }
}

/// Compute the [`MatchedInterest`]s for an observer against a single [`Event`].
///
/// Results are returned in declaration order.
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

impl Interest {
    /// Convenience equivalent to `match_against(ev).is_some()`.
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
}

#[cfg(test)]
mod tests {
    use super::process::{process_event, process_event_with_payload};
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
        // observer scoped to `sessions(sid)` by [`Interest::process_with_relation`]
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
            subject: messages(),
            key: 9.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 1.into(),
            }],
            payload: Arc::new(EventType::Insert),
        };

        let matched = interest.match_against(&event).expect("should match");
        let via = matched.expect("relation-scoped interest should carry a via_relation");
        assert_eq!(via.subject(), &sessions());
        assert_eq!(via.key(), &PrimaryKey::RowId(1));

        // A relation-scoped interest that doesn't match the event's relation
        // key must yield no match.
        let event_other_session = Event {
            subject: messages(),
            key: 9.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 2.into(),
            }],
            payload: Arc::new(EventType::Insert),
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
            subject: messages(),
            key: 5.into(),
            relations: vec![],
            payload: Arc::new(EventType::Update),
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
            subject: messages(),
            key: 42.into(),
            relations: vec![Relation {
                subject: sessions(),
                key: 1.into(),
            }],
            payload: Arc::new(EventType::Insert),
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
    fn process_event_routes_to_session_scoped_interest_via_relation() {
        // The emit entry point that the typing retrofit (L4) will use: a process
        // marker `Typing` emits an Insert scoped to a session by carrying the
        // `sessions(sid)` relation. A hand-built session-scoped interest (the
        // shape `whole_table_with_relation` produces for diesel tables, but for
        // a non-diesel subject) receives it with `via_relation` echoing the
        // declared `sessions(sid)` relation.
        struct Typing;

        let sid = 7;
        let ev = process_event::<Typing>(sid, vec![Relation::new(sessions(), sid)]);

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

        // Delete reaches the same session-scoped interest.
        let delete = process_event::<Typing>(sid, vec![Relation::new(sessions(), sid)]);
        assert_eq!(
            matched_interests(std::slice::from_ref(&interest), &delete).len(),
            1
        );
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
            42,
            vec![Relation::new(sessions(), sid)],
            Typer {
                id: typer.id,
                name: typer.name.clone(),
            },
        );

        assert_eq!(ev.payload_of::<Typing>(), Some(&typer));

        // A payloadless Delete has no payload to downcast — consumer reads key().
        let delete = process_event::<Typing>(42, vec![Relation::new(sessions(), sid)]);
        assert_eq!(delete.payload_of::<Typing>(), None);

        // Diesel events carry no payload.
        let db_ev = Event {
            subject: messages(),
            key: 1.into(),
            relations: vec![],
            payload: Arc::new(EventType::Insert),
        };
        assert_eq!(db_ev.payload_of::<Typing>(), None);
    }

    #[test]
    fn process_interest_without_relation_matches_any_process_event() {
        // `Interest::process` (relation-less) matches any event on that process
        // subject, in parallel with `Interest::whole_table` for diesel rows.
        struct Typing;

        let interest = Interest::process(Typing);
        let ev = process_event::<Typing>(9, vec![Relation::new(sessions(), 3)]);
        assert!(interest.is_interesting(&ev));

        // And reports None as via_relation (no declared relation to echo).
        let matched = interest.match_against(&ev).expect("should match");
        assert!(matched.is_none());
    }
}
