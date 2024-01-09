use super::*;
use crate::worker::message_expiry::ExpiredMessages;
use actix::prelude::*;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct StartMessageExpiry {
    pub message_id: i32,
}

impl Handler<StartMessageExpiry> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        StartMessageExpiry { message_id }: StartMessageExpiry,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage
            .as_ref()
            .unwrap()
            .start_message_expiry(message_id);
    }
}

impl StreamHandler<ExpiredMessages> for ClientActor {
    fn handle(&mut self, _messages: ExpiredMessages, _ctx: &mut Self::Context) {
        self.storage.as_mut().unwrap().delete_expired_messages();
    }
}

impl ClientWorker {
    #[allow(non_snake_case)]
    #[qmeta_async::with_executor]
    pub(super) fn startMessageExpiry(&self, message_id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(StartMessageExpiry { message_id })
                .map(Result::unwrap),
        );
        tracing::trace!(message_id, "dispatched StartMessageExpiry");
    }
}
