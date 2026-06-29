//! Diesel-row observation support.
//!
//! Diesel tables are represented by [`DieselTable`] tokens. This module holds
//! the diesel-bound [`DieselTable::of`] constructor, the diesel constructors
//! for [`Interest`], the diesel predicates of [`Event`], the
//! [`ObservationBuilder`], and the `Storage::observe_*` row-event emitters.

use super::*;

impl DieselTable {
    /// Construct a [`DieselTable`] token for the diesel table `T`.
    pub fn of<T: ::diesel::Table + 'static>() -> Self {
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

impl Interest {
    pub fn whole_table<T: ::diesel::Table + 'static>(_table: T) -> Self {
        Interest::Table {
            table: DieselTable::of::<T>(),
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
        T: ::diesel::Table + 'static,
        U: ::diesel::Table + 'static,
        U: ::diesel::JoinTo<T>,
    {
        Interest::Table {
            table: DieselTable::of::<T>(),
            relation: Some(Relation {
                table: DieselTable::of::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    pub fn row<T: ::diesel::Table + 'static>(_table: T, key: impl Into<PrimaryKey>) -> Self {
        Interest::Row {
            table: DieselTable::of::<T>(),
            key: key.into(),
        }
    }
}

impl Event {
    pub fn for_table<T: ::diesel::Table + 'static>(&self, _table: T) -> bool {
        let table = DieselTable::of::<T>();
        matches!(self.subject, EventSubject::Table(ref t) if *t == table)
    }

    pub fn for_row<T: ::diesel::Table + 'static>(
        &self,
        _table: T,
        key_test: impl Into<PrimaryKey>,
    ) -> bool {
        let table = DieselTable::of::<T>();
        match &self.subject {
            EventSubject::Table(t) if *t == table => self.key.implies(&key_test.into()),
            _ => false,
        }
    }

    pub fn relation_key_for<T: ::diesel::Table + 'static>(&self, _table: T) -> Option<&PrimaryKey> {
        let table = DieselTable::of::<T>();
        match &self.subject {
            EventSubject::Table(t) if *t == table => Some(&self.key),
            _ => self
                .relations
                .iter()
                .find(|relation| relation.table == table)
                .map(|relation| &relation.key),
        }
    }
}

pub struct ObservationBuilder<'a, T, O>
where
    O: Observable,
{
    storage: &'a crate::store::Storage<O>,
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
    T: ::diesel::Table + 'static,
    O: Observable,
{
    pub fn with_relation<U>(mut self, _table: U, relation_key: impl Into<PrimaryKey>) -> Self
    where
        U: ::diesel::Table + 'static,
        U: ::diesel::JoinTo<T>,
    {
        self.event.relations.push(Relation {
            table: DieselTable::of::<U>(),
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
        Via: ::diesel::Table + 'static,
        Target: ::diesel::Table + 'static,
        Via: ::diesel::JoinTo<T>,
        Target: ::diesel::JoinTo<Via>,
    {
        self.event.relations.push(Relation {
            table: DieselTable::of::<Target>(),
            key: relation_key.into(),
        });
        self
    }
}

impl<O: Observable> crate::store::Storage<O> {
    pub fn observe_insert<T: ::diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: EventSubject::Table(DieselTable::of::<T>()),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Insert,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_upsert<T: ::diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: EventSubject::Table(DieselTable::of::<T>()),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Upsert,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_update<T: ::diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: EventSubject::Table(DieselTable::of::<T>()),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Update,
                payload: None,
            },
            _table: diesel_table,
        }
    }

    pub fn observe_delete<T: ::diesel::Table + 'static>(
        &self,
        diesel_table: T,
        key: impl Into<PrimaryKey>,
    ) -> ObservationBuilder<'_, T, O> {
        ObservationBuilder {
            storage: self,
            event: Event {
                subject: EventSubject::Table(DieselTable::of::<T>()),
                key: key.into(),
                relations: Vec::new(),
                r#type: EventType::Delete,
                payload: None,
            },
            _table: diesel_table,
        }
    }
}
