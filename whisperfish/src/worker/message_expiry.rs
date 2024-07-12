use crate::store::Storage;
use chrono::Utc;
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

pub struct ExpiredMessages;

impl ExpiredMessagesStream {
    pub fn new(storage: Storage, wake_channel: tokio::sync::mpsc::UnboundedReceiver<()>) -> Self {
        Self {
            storage,
            next_wake: None,
            wake_channel,
        }
    }

    #[tracing::instrument(skip(self, cx))]
    fn update_next_wake(&mut self, cx: &mut Context<'_>) {
        if let Some((message_id, time)) = self.storage.fetch_next_expiring_message_id() {
            tracing::info!(
                "message {} expires at {}; scheduling wake-up.",
                message_id,
                time
            );
            let delta = time - Utc::now();
            self.next_wake = Some(Box::pin(tokio::time::sleep(
                delta.to_std().unwrap_or(Duration::from_secs(1)),
            )));

            cx.waker().wake_by_ref();
        } else {
            self.next_wake = None;
        }
    }
}

impl Stream for ExpiredMessagesStream {
    type Item = ExpiredMessages;

    #[tracing::instrument(skip(self, cx))]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some(next_wake) = &mut self.next_wake {
            if Pin::new(next_wake).poll(cx).is_ready() {
                self.next_wake = None;
                if !self.storage.fetch_expired_message_ids().is_empty() {
                    return Poll::Ready(Some(ExpiredMessages));
                }
            }
        }

        let woken = self.wake_channel.poll_recv(cx).is_ready();

        if woken || self.next_wake.is_none() {
            let _span = if woken {
                tracing::trace_span!("woken by channel").entered()
            } else {
                tracing::trace_span!("no next wake, computing").entered()
            };

            self.update_next_wake(cx);
        }

        Poll::Pending
    }
}
