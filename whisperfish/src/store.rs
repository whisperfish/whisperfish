use actix::prelude::*;
use std::sync::Arc;
use uuid::Uuid;
use whisperfish_store::store::observer::{
    Event, Interest, MatchedInterest, Observatory, matched_interests,
};
pub use whisperfish_store::store::{
    AciOrPniStorage as CoreAciOrPniStorage, Storage as CoreStorage, *,
};

pub type Storage = CoreStorage<ActixObservatory>;
pub type AciOrPniStorage = CoreAciOrPniStorage<ActixObservatory>;

pub struct Subscription {
    id: Uuid,
    interests: Vec<Interest>,
    subscriber: actix::WeakRecipient<ActixEvent>,
}

#[derive(Clone, Debug, Message)]
#[rtype(result = "Vec<Interest>")]
pub struct ActixEvent {
    pub(crate) event: Event,
    pub(crate) matched: Vec<MatchedInterest>,
}

impl ActixEvent {
    pub fn new(event: Event, matched: Vec<MatchedInterest>) -> Self {
        ActixEvent { event, matched }
    }

    pub fn event(&self) -> &Event {
        &self.event
    }

    pub fn matched(&self) -> &[MatchedInterest] {
        &self.matched
    }
}

impl From<ActixEvent> for Event {
    fn from(value: ActixEvent) -> Self {
        value.event
    }
}

#[derive(Clone, Default)]
pub struct ActixObservatory {
    subscriptions: Arc<tokio::sync::RwLock<Vec<Subscription>>>,
}

impl Observatory for ActixObservatory {
    type Subscriber = actix::WeakRecipient<ActixEvent>;

    fn register(&self, id: Uuid, interests: Vec<Interest>, subscriber: Self::Subscriber) {
        let subscriptions = self.subscriptions.clone();
        actix::spawn(async move {
            let mut subscriptions = subscriptions.write().await;
            subscriptions.push(Subscription {
                id,
                interests,
                subscriber,
            });
        });
    }

    fn update_interests(&self, id: Uuid, interests: Vec<Interest>) {
        let subscriptions = self.subscriptions.clone();
        actix::spawn(async move {
            let mut subscriptions = subscriptions.write().await;
            if let Some(sub) = subscriptions.iter_mut().find(|sub| sub.id == id) {
                sub.interests = interests;
            }
        });
    }

    fn distribute_event(&self, event: Event) {
        let subscriptions = self.subscriptions.clone();
        actix::spawn(async move {
            let mut subscriptions = subscriptions.write().await;
            distribute_event(&mut subscriptions, event).await;
        });
    }
}

async fn distribute_event(subscriptions: &mut Vec<Subscription>, event: Event) {
    // Remove stale subscriptions
    subscriptions.retain(|x| x.subscriber.upgrade().is_some());

    for subscription in subscriptions {
        let matched = matched_interests(&subscription.interests, &event);
        if !matched.is_empty() {
            match subscription.subscriber.upgrade() {
                Some(subscriber) => {
                    let event = ActixEvent::new(event.clone(), matched);
                    match subscriber.send(event).await {
                        Ok(interests) => {
                            subscription.interests = interests;
                        }
                        Err(MailboxError::Timeout) => {
                            tracing::warn!(
                                "Dropping an event for a subscriber because of a timeout."
                            );
                        }
                        Err(MailboxError::Closed) => {
                            tracing::warn!(
                                "Mailbox has closed meanwhile. Dropping with next event."
                            );
                        }
                    }
                }
                None => {
                    tracing::warn!("Subscriber got dropped while processing.");
                }
            }
        }
    }
}
