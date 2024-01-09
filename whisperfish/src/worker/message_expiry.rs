use crate::store::Storage;
use chrono::{DateTime, Utc};
use futures::{Future, Stream};
use std::{
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

pub struct ExpiredMessagesStream {
    storage: Storage,
    next_wake: Option<Pin<Box<tokio::time::Sleep>>>,
    wake_channel: tokio::sync::mpsc::UnboundedReceiver<()>,
}

pub struct ExpiredMessages {
    pub messages: Vec<(i32, DateTime<Utc>)>,
}

impl ExpiredMessagesStream {
    pub fn new(storage: Storage, wake_channel: tokio::sync::mpsc::UnboundedReceiver<()>) -> Self {
        Self {
            storage,
            next_wake: None,
            wake_channel,
        }
    }

    fn update_next_wake(&mut self) {
        if let Some((message_id, time)) = self.storage.fetch_next_expiring_message_id() {
            tracing::info!(
                "Message {} expires at {}; scheduling wake-up.",
                message_id,
                time
            );
            let delta = time - Utc::now();
            self.next_wake = Some(Box::pin(tokio::time::sleep(
                delta.to_std().unwrap_or(Duration::from_secs(1)),
            )));
        } else {
            self.next_wake = None;
        }
    }
}

impl Stream for ExpiredMessagesStream {
    type Item = ExpiredMessages;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.wake_channel.poll_recv(cx).is_ready() {
            self.update_next_wake();
        }

        if let Some(next_wake) = &mut self.next_wake {
            if Pin::new(next_wake).poll(cx).is_ready() {
                self.next_wake = None;
                // This does not take the first expired message, but that shouldn't really matter.
                let messages = self.storage.fetch_expired_message_ids();
                if !messages.is_empty() {
                    return Poll::Ready(Some(ExpiredMessages { messages }));
                }
            }
        }

        if self.next_wake.is_none() {
            self.update_next_wake();
        }

        Poll::Pending
    }
}
