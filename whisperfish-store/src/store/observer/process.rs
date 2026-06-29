//! Process/ephemeral event observation support.

use super::*;

impl Interest {
    /// Watch process marker `P` for events related to row `relation_key` in diesel table `U`.
    pub fn process_with_relation<P, U>(
        _process: P,
        _related_table: U,
        relation_key: impl Into<PrimaryKey>,
    ) -> Self
    where
        P: 'static,
        U: ::diesel::Table + 'static,
    {
        Interest::Process {
            subject: Subject::of::<P>(),
            relation: Some(Relation {
                table: DieselTable::of::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    /// Watch process marker `P` for any event, regardless of relation.
    pub fn process<P: 'static>(_process: P) -> Self {
        Interest::Process {
            subject: Subject::of::<P>(),
            relation: None,
        }
    }
}

/// Construct a process [`Event`] for marker `P`.
pub fn process_event<P: 'static>(
    r#type: EventType,
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
) -> Event {
    Event {
        r#type,
        subject: EventSubject::Process(Subject::of::<P>()),
        key: key.into(),
        relations,
        payload: None,
    }
}

/// Construct a process [`Event`] for marker `P` carrying a typed payload.
pub fn process_event_with_payload<P: EventPayload>(
    r#type: EventType,
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
    payload: P::Payload,
) -> Event {
    Event {
        r#type,
        subject: EventSubject::Process(Subject::of::<P>()),
        key: key.into(),
        relations,
        payload: Some(Arc::new(payload)),
    }
}

impl<O: Observable> crate::store::Storage<O> {
    /// Emit a process event for marker `P`.
    pub fn observe_process_event<P: 'static>(
        &self,
        r#type: EventType,
        key: impl Into<PrimaryKey>,
        relations: Vec<Relation>,
    ) {
        self.distribute_event(process_event::<P>(r#type, key, relations));
    }

    /// Emit a process event for marker `P` carrying a typed payload.
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
