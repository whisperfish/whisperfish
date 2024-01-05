//! Storage observer subsystem

use crate::schema;

mod orm_interests;

use actix::prelude::*;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum Interest {
    All,
    Row {
        table: Table,
        key: PrimaryKey,
    },
    Table {
        table: Table,
        relation: Option<Relation>,
    },
}

impl Interest {
    pub fn whole_table<T: diesel::Table + 'static>(_table: T) -> Self {
        let table = Table::from_diesel::<T>();
        Interest::Table {
            table,
            relation: None,
        }
    }

    /// Watches a table T for changes related to a row in table U identified by a key
    /// `relation_key`.
    pub fn whole_table_with_relation<T: diesel::Table + 'static, U: diesel::Table + 'static>(
        _table: T,
        _related_table: U,
        relation_key: impl Into<PrimaryKey>,
    ) -> Self
    where
        U: diesel::JoinTo<T>,
    {
        let table = Table::from_diesel::<T>();
        Interest::Table {
            table,
            relation: Some(Relation {
                table: Table::from_diesel::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    pub fn row<T: diesel::Table + 'static>(_table: T, key: impl Into<PrimaryKey>) -> Self {
        let table = Table::from_diesel::<T>();
        Interest::Row {
            table,
            key: key.into(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum Table {
    Attachments,
    GroupV1Members,
    GroupV1s,
    GroupV2Members,
    GroupV2s,
    IdentityRecords,
    Messages,
    Prekeys,
    Reactions,
    Receipts,
    Recipients,
    SenderKeyRecords,
    SessionRecords,
    Sessions,
    SignedPrekeys,
    Stickers,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Relation {
    table: Table,
    key: PrimaryKey,
}

#[derive(Clone, Message, Debug)]
#[rtype(result = "Vec<Interest>")]
pub struct Event {
    r#type: EventType,
    table: Table,
    key: PrimaryKey,
    relations: Vec<Relation>,
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
        let table = Table::from_diesel::<T>();
        self.table == table
    }

    pub fn for_row<T: diesel::Table + 'static>(
        &self,
        _table: T,
        key_test: impl Into<PrimaryKey>,
    ) -> bool {
        let table = Table::from_diesel::<T>();
        self.table == table && self.key.implies(&key_test.into())
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
            let table = Table::from_diesel::<T>();
            self.relations
                .iter()
                .find(|relation| relation.table == table)
                .map(|relation| &relation.key)
        }
    }
}

impl Interest {
    pub fn is_interesting(&self, ev: &Event) -> bool {
        match (self, ev) {
            (Interest::All, _) => true,

            // Interested in a whole table, and an event on the table is triggered
            (
                Interest::Table {
                    table: ti,
                    relation,
                },
                Event {
                    table: te,
                    relations,
                    ..
                },
            ) => {
                ti == te
                    && if let Some(relation) = relation {
                        // Some means only interested in one particular relation.
                        // If there's no matching relation specified, we assume a match;
                        // if there's a relation that matches in table, we filter on the specified key.
                        // Assumes that event-mentioned relations are exhaustive.
                        relations.is_empty()
                            || relations.iter().any(|event_relation| {
                                event_relation.table == relation.table
                                    && event_relation.key == relation.key
                            })
                    } else {
                        // None means interested in any table update, so we match only the table
                        true
                    }
            }

            // Interested in a particular row, and an event is triggered on some unknown row
            (
                Interest::Row { table: ti, key: _ },
                Event {
                    table: te,
                    key: PrimaryKey::Unknown,
                    ..
                },
            ) => ti == te,

            // Interested in a particular row, and an event is triggered on that specific row
            (
                Interest::Row { table: ti, key: ki },
                Event {
                    table: te, key: ke, ..
                },
            ) => ti == te && ki == ke,
            #[allow(unreachable_patterns)] // XXX should one of the enums be non-exhaustive instead?
            _ => {
                log::debug!(
                    "Unhandled event-interest pair; assuming interesting. {:?} {:?}",
                    ev,
                    self
                );
                true
            }
        }
    }
}

impl Table {
    fn from_diesel<T: diesel::Table + 'static>() -> Self {
        macro_rules! diesel_to_observation_table {
            ($($table:ident => $variant:ident,)* $(,)?) => {
                $(if std::any::TypeId::of::<T>() == std::any::TypeId::of::<schema::$table::table>() {
                    return Self::$variant
                })*
            }
        }
        diesel_to_observation_table!(
            attachments => Attachments,
            group_v1_members => GroupV1Members,
            group_v1s => GroupV1s,
            group_v2_members => GroupV2Members,
            group_v2s => GroupV2s,
            identity_records => IdentityRecords,
            messages => Messages,
            prekeys => Prekeys,
            reactions => Reactions,
            receipts => Receipts,
            recipients => Recipients,
            sender_key_records => SenderKeyRecords,
            session_records => SessionRecords,
            sessions => Sessions,
            signed_prekeys => SignedPrekeys,
            stickers => Stickers,
        );

        unimplemented!("Unknown diesel table")
    }
}

pub struct Subscription {
    id: Uuid,
    interests: Vec<Interest>,
    subscriber: actix::WeakRecipient<Event>,
}

/// The Observatory watches the database for changes, and dispatches it to interested [Observer]s.
#[derive(Default)]
pub struct Observatory {
    subscriptions: Vec<Subscription>,
}

impl Observatory {
    pub fn register(
        &mut self,
        id: Uuid,
        interests: Vec<Interest>,
        subscriber: actix::WeakRecipient<Event>,
    ) {
        self.subscriptions.push(Subscription {
            id,
            interests,
            subscriber,
        });
    }

    pub async fn distribute_event(&mut self, event: Event) {
        // Remove stale subscriptions
        self.subscriptions
            .retain(|x| x.subscriber.upgrade().is_some());

        for subscription in &mut self.subscriptions {
            if subscription
                .interests
                .iter()
                .any(|x| x.is_interesting(&event))
            {
                match subscription.subscriber.upgrade() {
                    Some(subscriber) => match subscriber.send(event.clone()).await {
                        Ok(interests) => {
                            subscription.interests = interests;
                        }
                        Err(MailboxError::Timeout) => {
                            log::warn!("Dropping an event for a subscriber because of a timeout.");
                        }
                        Err(MailboxError::Closed) => {
                            log::warn!("Mailbox has closed meanwhile.  Dropping with next event.");
                        }
                    },
                    None => {
                        log::warn!("Subscriber got dropped while processing.");
                    }
                }
            }
        }
    }
}

pub trait EventObserving {
    type Context;

    fn observe(&mut self, ctx: Self::Context, event: Event)
    where
        Self: Sized;
    fn interests(&self) -> Vec<Interest>;
}

pub struct ObservationBuilder<'a, T> {
    storage: &'a super::Storage,
    event: Event,
    _table: T,
}

impl<T> Drop for ObservationBuilder<'_, T> {
    fn drop(&mut self) {
        self.storage.distribute_event(self.event.clone());
    }
}

impl<'a, T: diesel::Table + 'static> ObservationBuilder<'a, T> {
    pub fn with_relation<U: diesel::Table + 'static>(
        mut self,
        _table: U,
        relation_key: impl Into<PrimaryKey>,
    ) -> Self
    where
        U: diesel::JoinTo<T>,
    {
        self.event.relations.push(Relation {
            table: Table::from_diesel::<U>(),
            key: relation_key.into(),
        });
        self
    }
}

#[derive(Copy, Clone)]
pub struct ObserverHandle {
    id: Uuid,
}

impl super::Storage {
    pub fn register_observer(
        &mut self,
        interests: Vec<Interest>,
        subscriber: actix::WeakRecipient<Event>,
    ) -> ObserverHandle {
        let observatory = self.observatory.clone();
        let id = Uuid::new_v4();
        actix::spawn(async move {
            let mut observatory = observatory.write().await;
            observatory.register(id, interests, subscriber);
        });
        ObserverHandle { id }
    }

    pub fn update_interests(&mut self, handle: ObserverHandle, interests: Vec<Interest>) {
        let observatory = self.observatory.clone();
        actix::spawn(async move {
            let mut observatory = observatory.write().await;
            if let Some(sub) = observatory
                .subscriptions
                .iter_mut()
                .find(|sub| sub.id == handle.id)
            {
                sub.interests = interests;
            }
        });
    }

    pub(super) fn distribute_event(&self, event: Event) {
        let observatory = self.observatory.clone();
        actix::spawn(async move {
            let mut observatory = observatory.write().await;
            observatory.distribute_event(event).await;
        });
    }

    pub fn observe_insert<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T> {
        let table = Table::from_diesel::<T>();

        ObservationBuilder {
            storage: self,
            event: Event {
                table,
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Insert,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_upsert<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T> {
        let table = Table::from_diesel::<T>();

        ObservationBuilder {
            storage: self,
            event: Event {
                table,
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Upsert,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_update<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T> {
        let table = Table::from_diesel::<T>();

        ObservationBuilder {
            storage: self,
            event: Event {
                table,
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Update,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_delete<T: diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T> {
        let table = Table::from_diesel::<T>();

        ObservationBuilder {
            storage: self,
            event: Event {
                table,
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Delete,
            },
            _table: diesel_table,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relation_event_generates_interest() {
        let interest = Interest::whole_table_with_relation(
            schema::messages::table,
            schema::sessions::table,
            1,
        );

        let event_on_session_0 = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 52.into(),
            relations: vec![Relation {
                table: Table::Sessions,
                key: 0.into(),
            }],
        };
        let event_on_session_1 = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 66.into(),
            relations: vec![
                Relation {
                    table: Table::Recipients,
                    key: 26.into(),
                },
                Relation {
                    table: Table::Sessions,
                    key: 1.into(),
                },
            ],
        };

        assert!(!interest.is_interesting(&event_on_session_0));
        assert!(interest.is_interesting(&event_on_session_1));
    }

    #[test]
    fn table_event_generates_interest() {
        let interest = Interest::whole_table(schema::messages::table);

        let event = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 52.into(),
            relations: vec![],
        };

        assert!(interest.is_interesting(&event));
    }

    #[test]
    fn row_event_generates_interest() {
        let interest = Interest::row(schema::messages::table, 2);

        let negative = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 1.into(),
            relations: vec![],
        };
        let positive = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 2.into(),
            relations: vec![],
        };

        assert!(!interest.is_interesting(&negative));
        assert!(interest.is_interesting(&positive));
    }

    #[test]
    fn event() {
        let e = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 1.into(),
            relations: vec![Relation {
                table: Table::Sessions,
                key: 0.into(),
            }],
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
            table: Table::Messages,
            key: 2.into(),
        };
        let e_1 = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 1.into(),
            relations: vec![Relation {
                table: Table::Sessions,
                key: 0.into(),
            }],
        };
        let e_2 = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: 2.into(),
            relations: vec![Relation {
                table: Table::Sessions,
                key: 0.into(),
            }],
        };
        let e_u = Event {
            r#type: EventType::Insert,
            table: Table::Messages,
            key: PrimaryKey::Unknown,
            relations: vec![Relation {
                table: Table::Sessions,
                key: 0.into(),
            }],
        };
        let e_s = Event {
            r#type: EventType::Insert,
            table: Table::Sessions,
            key: PrimaryKey::Unknown,
            relations: vec![],
        };
        assert!(i_all.is_interesting(&e_1));
        assert!(!i_row.is_interesting(&e_1));
        assert!(i_row.is_interesting(&e_2));
        assert!(i_row.is_interesting(&e_u));
        assert!(!i_row.is_interesting(&e_s));

        let i_cln = i_row.clone();
        match (i_cln, i_row) {
            (Interest::Row { table: t1, key: k1 }, Interest::Row { table: t2, key: k2 }) => {
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
}
