#![allow(non_snake_case)]

use std::sync::OnceLock;

use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use whisperfish_store::NewMessage;
use whisperfish_store::orm::MessageType;

use crate::store::Storage;

#[derive(QObject, Default)]
pub struct SessionMethods {
    base: qt_base_class!(trait QObject),
    /// Set exactly once, from the GUI side, when storage becomes ready.
    /// QML may call into these methods before storage is set (during the
    /// boot window before `StorageReady`); in that case the calls are
    /// silently dropped — the same risk profile as the old actor, which
    /// would have panicked if a mutation beat `StorageReady`.
    pub storage: OnceLock<Storage>,

    remove: qt_method!(fn(&self, id: i32)),

    markRead: qt_method!(fn(&self, id: i32)),
    markMuted: qt_method!(fn(&self, id: i32, muted: bool)),
    markArchived: qt_method!(fn(&self, id: i32, archived: bool)),
    markPinned: qt_method!(fn(&self, id: i32, pinned: bool)),

    removeIdentities: qt_method!(fn(&self, recipients_id: i32)),

    saveDraft: qt_method!(fn(&self, sid: i32, draft: String)),
}

impl SessionMethods {
    /// Returns the storage, or `None` if storage is not yet initialized.
    /// The latter only happens during the narrow boot window before
    /// `StorageReady` is dispatched; calls are silently dropped.
    fn storage(&self) -> Option<&Storage> {
        self.storage.get()
    }
}

impl SessionMethods {
    /// Removes session by id from the database.
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn remove(&self, id: i32) {
        let Some(storage) = self.storage() else {
            tracing::warn!(session_id = id, "DeleteSession dropped: storage not ready");
            return;
        };
        storage.delete_session(id);
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn markRead(&self, id: i32) {
        let Some(storage) = self.storage() else {
            tracing::warn!(
                session_id = id,
                "MarkSessionRead dropped: storage not ready"
            );
            return;
        };
        storage.mark_session_read(id);
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn markMuted(&self, id: i32, muted: bool) {
        let Some(storage) = self.storage() else {
            tracing::warn!(
                session_id = id,
                "MarkSessionMuted dropped: storage not ready"
            );
            return;
        };
        storage.mark_session_muted(id, muted);
        tracing::trace!("MarkSessionMuted({}, {})", id, muted);
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn markArchived(&self, id: i32, archived: bool) {
        let Some(storage) = self.storage() else {
            tracing::warn!(
                session_id = id,
                "MarkSessionArchived dropped: storage not ready"
            );
            return;
        };
        storage.mark_session_archived(id, archived);
        tracing::trace!("MarkSessionArchived({}, {})", id, archived);
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn markPinned(&self, id: i32, pinned: bool) {
        let Some(storage) = self.storage() else {
            tracing::warn!(
                session_id = id,
                "MarkSessionPinned dropped: storage not ready"
            );
            return;
        };
        storage.mark_session_pinned(id, pinned);
        tracing::trace!("MarkSessionPinned({}, {})", id, pinned);
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn removeIdentities(&self, recipient_id: i32) {
        let Some(storage) = self.storage() else {
            tracing::warn!(recipient_id, "RemoveIdentities dropped: storage not ready");
            return;
        };

        let _span =
            tracing::debug_span!("Removing identities for recipient ID {}", recipient_id).entered();

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

        let session = storage.fetch_or_insert_session_by_recipient_id(recipient_id);

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

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn saveDraft(&self, sid: i32, draft: String) {
        let Some(storage) = self.storage() else {
            tracing::warn!(session_id = sid, "SaveDraft dropped: storage not ready");
            return;
        };
        storage.save_draft(sid, draft);
        tracing::trace!("SaveDraft for {}", sid);
    }
}
