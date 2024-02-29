#![allow(non_snake_case)]

use super::*;
use actix::prelude::*;
use futures::prelude::*;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use qttypes::{QVariantList, QVariantMap};

#[derive(QObject, Default)]
pub struct SessionMethods {
    base: qt_base_class!(trait QObject),
    pub actor: Option<Addr<SessionActor>>,

    remove: qt_method!(fn(&self, id: i32)),

    markRead: qt_method!(fn(&mut self, id: i32)),
    markMuted: qt_method!(fn(&self, id: i32, muted: bool)),
    markArchived: qt_method!(fn(&self, id: i32, archived: bool)),
    markPinned: qt_method!(fn(&self, id: i32, pinned: bool)),

    removeIdentities: qt_method!(fn(&self, recipients_id: i32)),

    saveDraft: qt_method!(fn(&self, sid: i32, draft: String)),
    sendTypings: qt_signal!(typing_data: QVariantMap),
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
        tracing::trace!("Dispatched DeleteSession({})", id);
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
        tracing::trace!("Dispatched MarkSessionMuted({}, {})", id, muted);
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
        tracing::trace!("Dispatched MarkSessionArchived({}, {})", id, archived);
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
        tracing::trace!("Dispatched MarkSessionPinned({}, {})", id, pinned);
    }

    #[with_executor]
    fn removeIdentities(&self, recipient_id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(RemoveIdentities { recipient_id })
                .map(Result::unwrap),
        );
        tracing::trace!("Dispatched RemoveIdentities({})", recipient_id);
    }

    #[with_executor]
    fn saveDraft(&self, sid: i32, draft: String) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(SaveDraft { sid, draft })
                .map(Result::unwrap),
        );
        tracing::trace!("Dispatched SafeDraft for {}", sid);
    }

    pub fn handle_typings(&self, typings: &HashMap<i32, Vec<orm::Recipient>>) {
        for (sid, recipients) in typings {
            let mut qnames = QVariantList::default();
            for r in recipients {
                qnames.push(if let Some(username) = r.username.as_ref() {
                    QVariant::from(QString::from(username.as_str()))
                } else if let Some(name) = r.profile_given_name.as_ref() {
                    QVariant::from(QString::from(name.as_str()))
                } else {
                    QVariant::from(QString::from(r.e164_or_uuid().as_str()))
                })
            }
            let mut item = QVariantMap::default();
            item.insert("sid".into(), QVariant::from(*sid));
            item.insert("names".into(), qnames.into());
            self.sendTypings(item);
        }
    }
}
