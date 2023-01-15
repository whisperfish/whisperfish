#![allow(non_snake_case)]

use super::*;
use actix::prelude::*;
use futures::prelude::*;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;

#[derive(QObject, Default)]
pub struct SessionMethods {
    base: qt_base_class!(trait QObject),
    pub actor: Option<Addr<SessionActor>>,

    remove: qt_method!(fn(&self, id: i32)),

    markRead: qt_method!(fn(&mut self, id: i32)),
    markMuted: qt_method!(fn(&self, id: i32, muted: bool)),
    markArchived: qt_method!(fn(&self, id: i32, archived: bool)),
    markPinned: qt_method!(fn(&self, id: i32, pinned: bool)),

    removeIdentities: qt_method!(fn(&self, session_id: i32)),
}

impl SessionMethods {
    /// Removes session by id from the database.
    #[with_executor]
    fn remove(&self, id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(DeleteSession { id })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched DeleteSession({})", id);
    }

    #[with_executor]
    fn markRead(&mut self, id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(MarkSessionRead { sid: id })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    fn markMuted(&self, id: i32, muted: bool) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(MarkSessionMuted { sid: id, muted })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched MarkSessionMuted({}, {})", id, muted);
    }

    #[with_executor]
    fn markArchived(&self, id: i32, archived: bool) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(MarkSessionArchived { sid: id, archived })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched MarkSessionArchived({}, {})", id, archived);
    }

    #[with_executor]
    fn markPinned(&self, id: i32, pinned: bool) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(MarkSessionPinned { sid: id, pinned })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched MarkSessionPinned({}, {})", id, pinned);
    }

    #[with_executor]
    fn removeIdentities(&self, session_id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(RemoveIdentities { session_id })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched SessionRemoveIdentities({})", session_id);
    }
}