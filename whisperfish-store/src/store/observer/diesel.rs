//! Diesel-row observation support.
//!
//! This module is the diesel construction boundary: it turns diesel table
//! types into [`Subject`]s (via [`Subject::of`]) and CRUD verbs into
//! [`EventType`] payloads. The bounds `diesel::Table` and `diesel::JoinTo`
//! live on these constructors, not on [`Subject`] itself — from the observer
//! core's view diesel rows are "just another subject."

use super::*;

impl Interest {
    pub fn whole_table<T: ::diesel::Table + 'static>(_table: T) -> Self {
        Interest::Any {
            subject: Subject::of::<T>(),
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
        Interest::Related {
            subject: Subject::of::<T>(),
            relation: Relation {
                subject: Subject::of::<U>(),
                key: relation_key.into(),
            },
        }
    }

    pub fn row<T: ::diesel::Table + 'static>(_table: T, key: impl Into<PrimaryKey>) -> Self {
        Interest::Keyed {
            subject: Subject::of::<T>(),
            key: key.into(),
        }
    }
}

impl Event {
    pub fn for_table<T: ::diesel::Table + 'static>(&self, _table: T) -> bool {
        self.subject == Subject::of::<T>()
    }

    pub fn for_row<T: ::diesel::Table + 'static>(
        &self,
        _table: T,
        key_test: impl Into<PrimaryKey>,
    ) -> bool {
        let subject = Subject::of::<T>();
        if self.subject != subject {
            return false;
        }
        self.key.implies(&key_test.into())
    }

    pub fn relation_key_for<T: ::diesel::Table + 'static>(&self, _table: T) -> Option<&PrimaryKey> {
        let subject = Subject::of::<T>();
        if self.subject == subject {
            Some(&self.key)
        } else {
            self.relations
                .iter()
                .find(|relation| relation.subject == subject)
                .map(|relation| &relation.key)
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
        Via: ::diesel::Table + 'static,
        Target: ::diesel::Table + 'static,
        Via: ::diesel::JoinTo<T>,
        Target: ::diesel::JoinTo<Via>,
    {
        self.event.relations.push(Relation {
            subject: Subject::of::<Target>(),
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
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                payload: Arc::new(EventType::Insert),
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
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                payload: Arc::new(EventType::Upsert),
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
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                payload: Arc::new(EventType::Update),
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
                subject: Subject::of::<T>(),
                key: key.into(),
                relations: Vec::new(),
                payload: Arc::new(EventType::Delete),
            },
            _table: diesel_table,
        }
    }
}
