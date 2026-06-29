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
        Interest::Subject {
            subject: Subject::of::<P>(),
            relation: Some(Relation {
                subject: Subject::of::<U>(),
                key: relation_key.into(),
            }),
        }
    }

    /// Watch process marker `P` for any event, regardless of relation.
    pub fn process<P: 'static>(_process: P) -> Self {
        Interest::Subject {
            subject: Subject::of::<P>(),
            relation: None,
        }
    }
}

/// Construct a process [`Event`] for marker `P`.
/// Construct a process [`Event`] for marker `P`.
///
/// Carries `()` as the payload; markers that send data use
/// [`process_event_with_payload`]. The verb lives entirely in the typed
/// payload, never in a diesel [`EventType`].
pub fn process_event<P: 'static>(key: impl Into<PrimaryKey>, relations: Vec<Relation>) -> Event {
    Event {
        subject: Subject::of::<P>(),
        key: key.into(),
        relations,
        payload: Arc::new(()),
    }
}

/// Construct a process [`Event`] for marker `P` carrying a typed payload.
pub fn process_event_with_payload<P: EventPayload>(
    key: impl Into<PrimaryKey>,
    relations: Vec<Relation>,
    payload: P::Payload,
) -> Event {
    Event {
        subject: Subject::of::<P>(),
        key: key.into(),
        relations,
        payload: Arc::new(payload),
    }
}

impl<O: Observable> crate::store::Storage<O> {
    /// Emit a process event for marker `P`.
    pub fn observe_process_event<P: 'static>(
        &self,
        key: impl Into<PrimaryKey>,
        relations: Vec<Relation>,
    ) {
        self.distribute_event(process_event::<P>(key, relations));
    }

    /// Emit a process event for marker `P` carrying a typed payload.
    pub fn observe_process_event_with_payload<P: EventPayload>(
        &self,
        key: impl Into<PrimaryKey>,
        relations: Vec<Relation>,
        payload: P::Payload,
    ) {
        self.distribute_event(process_event_with_payload::<P>(key, relations, payload));
    }
}
