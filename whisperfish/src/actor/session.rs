mod typing_notifications;

pub use self::typing_notifications::*;

mod methods;
use methods::*;

use whisperfish_store::orm;
use whisperfish_store::orm::MessageType;
use whisperfish_store::NewMessage;

use crate::platform::QmlApp;
use crate::{gui::StorageReady, store::Storage};
use actix::prelude::*;

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

    /// Helper method to access storage reference
    ///
    /// Panics if storage is not initialized (should never happen after StorageReady)
    fn storage(&self) -> &Storage {
        self.storage.as_ref().expect("storage not initialized")
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
        self.storage().mark_session_read(sid);
    }
}

impl Handler<MarkSessionArchived> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionArchived { sid, archived }: MarkSessionArchived,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage().mark_session_archived(sid, archived);
    }
}

impl Handler<MarkSessionPinned> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionPinned { sid, pinned }: MarkSessionPinned,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage().mark_session_pinned(sid, pinned);
    }
}

impl Handler<MarkSessionMuted> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        MarkSessionMuted { sid, muted }: MarkSessionMuted,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage().mark_session_muted(sid, muted);
    }
}

impl Handler<DeleteSession> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteSession { id }: DeleteSession,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage().delete_session(id);
    }
}

impl Handler<SaveDraft> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        SaveDraft { sid, draft }: SaveDraft,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage().save_draft(sid, draft);
    }
}

impl Handler<RemoveIdentities> for SessionActor {
    type Result = ();

    fn handle(
        &mut self,
        RemoveIdentities { recipient_id }: RemoveIdentities,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let _span =
            tracing::debug_span!("Removing identities for recipient ID {}", recipient_id).entered();

        let storage = self.storage();
        let recipient = if let Some(r) = storage.fetch_recipient_by_id(recipient_id) {
            r
        } else {
            tracing::warn!("recipient not found");
            return;
        };

        let mut success = false;

        success |= recipient
            .to_aci_service_address()
            .map(|aci| storage.delete_identity_key(&aci))
            .unwrap_or(false);

        success |= recipient
            .to_pni_service_address()
            .map(|pni| storage.delete_identity_key(&pni))
            .unwrap_or(false);

        let session = storage.fetch_session_by_recipient_id(recipient_id).unwrap();

        storage.create_message(&NewMessage {
            session_id: session.id,
            sent: true,
            is_read: true,
            message_type: Some(MessageType::IdentityReset),
            ..NewMessage::new_outgoing()
        });

        if !success {
            tracing::warn!(
                "Could not find and remove any identities for recipient. Please file a bug."
            );
        }
    }
}
