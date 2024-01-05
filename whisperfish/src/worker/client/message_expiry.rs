use super::*;
use crate::worker::message_expiry::ExpiredMessages;
use actix::prelude::*;

impl StreamHandler<ExpiredMessages> for ClientActor {
    fn handle(&mut self, _messages: ExpiredMessages, _ctx: &mut Self::Context) {
        self.storage.as_mut().unwrap().delete_expired_messages();
    }
}
