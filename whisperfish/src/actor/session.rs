mod typing_notifications;

pub use self::typing_notifications::*;

mod methods;
use methods::*;

use crate::gui::StorageReady;
use crate::platform::QmlApp;
use crate::store::{orm, Storage};
use actix::prelude::*;
use libsignal_service::protocol::{DeviceId, ProtocolAddress};
use qmetaobject::prelude::*;
use std::collections::{HashMap, VecDeque};

#[derive(actix::Message)]
#[rtype(result = "()")]
// XXX this should be called *per message* instead of per session,
//     probably.
pub struct MarkSessionRead {
    pub sid: i32,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MarkSessionMuted {
    pub sid: i32,
    pub muted: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MarkSessionArchived {
    pub sid: i32,
    pub archived: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct MarkSessionPinned {
    pub sid: i32,
    pub pinned: bool,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteSession {
    pub id: i32,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct LoadAllSessions;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct RemoveIdentities {
    pub recipient_id: i32,
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct SaveDraft {
    pub sid: i32,
    pub draft: String,
}

pub struct SessionActor {
    inner: QObjectBox<SessionMethods>,
    storage: Option<Storage>,

    typing_queue: VecDeque<TypingQueueItem>,
}

impl SessionActor {
    pub fn new(app: &mut QmlApp) -> Self {
        let inner = QObjectBox::new(SessionMethods::default());
        app.set_object_property("SessionModel".into(), inner.pinned());

        Self {
            inner,
            storage: None,
            typing_queue: VecDeque::new(),
        }
    }

    pub fn handle_update_typing(&mut self, typings: &HashMap<i32, Vec<orm::Recipient>>) {
        let session = self.inner.pinned();
        let session = session.borrow();
        session.handle_typings(typings);
    }
}

impl Actor for SessionActor {
    type Context = Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
    }
}

impl Handler<StorageReady> for SessionActor {
    type Result = ();

    fn handle(&mut self, storageready: StorageReady, _ctx: &mut Self::Context) -> Self::Result {
        self.storage = Some(storageready.storage);
        tracing::trace!("SessionActor has a registered storage");
    }
}

impl Handler<MarkSessionRead> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionRead { sid }: MarkSessionRead,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_ref().unwrap().mark_session_read(sid);
    }
}

impl Handler<MarkSessionArchived> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionArchived { sid, archived }: MarkSessionArchived,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage
            .as_ref()
            .unwrap()
            .mark_session_archived(sid, archived);
    }
}

impl Handler<MarkSessionPinned> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionPinned { sid, pinned }: MarkSessionPinned,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage
            .as_ref()
            .unwrap()
            .mark_session_pinned(sid, pinned);
    }
}

impl Handler<MarkSessionMuted> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionMuted { sid, muted }: MarkSessionMuted,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage
            .as_ref()
            .unwrap()
            .mark_session_muted(sid, muted);
    }
}

impl Handler<DeleteSession> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteSession { id }: DeleteSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_ref().unwrap().delete_session(id);
    }
}

impl Handler<SaveDraft> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        SaveDraft { sid, draft }: SaveDraft,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_ref().unwrap().save_draft(sid, draft);
    }
}

impl Handler<RemoveIdentities> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        RemoveIdentities { recipient_id }: RemoveIdentities,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let storage = self.storage.as_ref().unwrap();
        let recipient = if let Some(r) = storage.fetch_recipient_by_id(recipient_id) {
            r
        } else {
            tracing::warn!(
                "Requested removal of identities for recipient {}, but recipient not found.",
                recipient_id
            );
            return;
        };

        let identities = match (recipient.e164, recipient.uuid) {
            (None, None) => {
                tracing::debug!("No identities to remove");
                return;
            }
            (None, Some(uuid)) => vec![uuid.to_string()],
            (Some(e164), None) => vec![e164.to_string()],
            (Some(e164), Some(uuid)) => vec![e164.to_string(), uuid.to_string()],
        };

        let mut successes = 0;
        for identity in identities {
            tracing::debug!("Removing identity {}", identity);
            let addr = ProtocolAddress::new(identity.clone(), DeviceId::from(1));
            if !storage.delete_identity_key(&addr) {
                tracing::trace!("Could not remove identity {}.", identity);
            } else {
                successes += 1;
            }
        }

        if successes == 0 {
            tracing::warn!("Could not successfully remove any identity keys.  Please file a bug.");
        }
    }
}
