#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::Storage;
use qmetaobject::prelude::*;
use std::collections::HashMap;
use whisperfish_store::observer::Event;
use whisperfish_store::schema;
use whisperfish_store::store::orm;

/// QML-constructable object that interacts with a list of sessions.
///
/// Currently, this object will list all sessions unfiltered, ordered by the last message received
/// timestamp.
/// In the future, it should be possible to install filters and change the ordering.
#[observing_model]
#[derive(Default, QObject)]
pub struct Sessions {
    base: qt_base_class!(trait QObject),
    session_list: QObjectBox<SessionListModel>,

    #[qt_property(READ: sessions, NOTIFY: model_changed)]
    sessions: QVariant,
    #[qt_property(READ: count, NOTIFY: count_changed)]
    count: usize,
    #[qt_property(READ: unread, NOTIFY: count_changed)]
    unread: usize,

    model_changed: qt_signal!(),
    count_changed: qt_signal!(),
}

impl Sessions {
    fn init(&mut self, ctx: ModelContext<Self>) {
        self.session_list
            .pinned()
            .borrow_mut()
            .load_all(ctx.storage());
        self.count_changed();
    }

    fn sessions(&self, _ctx: Option<ModelContext<Self>>) -> QVariant {
        self.session_list.pinned().into()
    }

    fn count(&self, _ctx: Option<ModelContext<Self>>) -> usize {
        self.session_list.pinned().borrow().count()
    }

    fn unread(&self, _ctx: Option<ModelContext<Self>>) -> usize {
        self.session_list.pinned().borrow().unread()
    }
}

impl EventObserving for Sessions {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, event: Event) {
        let storage = ctx.storage();
        self.session_list
            .pinned()
            .borrow_mut()
            .observe(storage, event);
        self.count_changed();
        self.update_interests();
    }

    fn interests(&self) -> Vec<crate::store::observer::Interest> {
        std::iter::once(Interest::whole_table(schema::sessions::table))
            .chain(
                self.session_list
                    .pinned()
                    .borrow()
                    .content
                    .iter()
                    .flat_map(|session| session.interests()),
            )
            .collect()
    }
}

#[derive(QObject, Default)]
pub struct SessionListModel {
    base: qt_base_class!(trait QAbstractListModel),
    content: Vec<orm::AugmentedSession>,

    count: qt_property!(usize; READ count NOTIFY countChanged),
    unread: qt_property!(usize; READ unread NOTIFY countChanged),

    countChanged: qt_signal!(),
}

impl SessionListModel {
    fn load_all(&mut self, storage: Storage) {
        self.begin_reset_model();
        self.content = storage.fetch_all_sessions_augmented();

        // Stable sort, such that this retains the above ordering.
        self.content.sort_by_key(|k| !k.is_pinned);
        self.end_reset_model();
        self.countChanged();
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn observe(&mut self, storage: Storage, event: Event) {
        // Find the correct session and update the latest message
        let session_id = event
            .relation_key_for(schema::sessions::table)
            .and_then(|x| x.as_i32());
        let message_id = event
            .relation_key_for(schema::messages::table)
            .and_then(|x| x.as_i32());
        let attachment_id = event
            .relation_key_for(schema::attachments::table)
            .and_then(|x| x.as_i32());
        let recipient_id = event
            .relation_key_for(schema::recipients::table)
            .and_then(|x| x.as_i32());
        if session_id.is_none()
            && message_id.is_none()
            && attachment_id.is_none()
            && recipient_id.is_none()
        {
            tracing::trace!(
                "Falling back to reloading the whole Sessions model for event {:?}",
                event
            );
            self.load_all(storage);
            return;
        }

        if event.for_table(schema::recipients::table) && event.is_update() {
            if let Some(recipient_id) = recipient_id {
                if let Some(new_recipient) = storage.fetch_recipient_by_id(recipient_id) {
                    let mut updates = Vec::new();
                    for (idx, session) in self.content.iter_mut().enumerate() {
                        match &mut session.inner.r#type {
                            orm::SessionType::DirectMessage(recipient) => {
                                if recipient.id == recipient_id {
                                    *recipient = new_recipient.clone();
                                    updates.push(idx);
                                }
                            }
                            orm::SessionType::GroupV1(_group) => {
                                // Groups don't have recipients in this model
                            }
                            orm::SessionType::GroupV2(_) => {
                                // Groups don't have recipients in this model
                            }
                        }
                    }
                    if session_id.is_none() && message_id.is_none() && attachment_id.is_none() {
                        return;
                    }
                    for idx in updates {
                        let idx = self.row_index(idx as i32);
                        self.data_changed(idx, idx);
                    }
                }
            }
        }

        if attachment_id.is_some()
            && event.for_table(schema::attachments::table)
            && event.is_update()
        {
            // Don't care, because SessionListModel only takes into account the number of
            // attachments.
            // Furthermore, inserts will have an associated message_id, and deletes don't occur
            // (so we fall back in that case).
            return;
        }

        if let Some(session_id) = session_id {
            if let Some(session) = storage.fetch_session_by_id_augmented(session_id) {
                let new_idx = self.content.binary_search_by_key(
                    &std::cmp::Reverse((
                        session.is_pinned,
                        session.last_message.as_ref().map(|m| &m.server_timestamp),
                        session.id,
                    )),
                    |session| {
                        std::cmp::Reverse((
                            session.is_pinned,
                            session.last_message.as_ref().map(|m| &m.server_timestamp),
                            session.id,
                        ))
                    },
                );
                let found = self
                    .content
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.id == session_id);

                match (new_idx, found) {
                    // Session time/id matches exactly, replace
                    (Ok(idx), _) => {
                        // Replace session
                        self.content[idx] = session;
                        let idx = self.row_index(idx as i32);
                        self.data_changed(idx, idx);
                    }
                    (Err(mut dest_idx), Some((src_idx, _))) => {
                        if dest_idx == src_idx {
                            // Moving "up above itself"
                            self.content[dest_idx] = session;
                            let m_idx = self.row_index(dest_idx as i32);
                            self.data_changed(m_idx, m_idx);
                        } else if dest_idx == (src_idx + 1) {
                            // Moving "down below itself"
                            self.content[src_idx] = session;
                            let m_idx = self.row_index(src_idx as i32);
                            self.data_changed(m_idx, m_idx);
                        } else {
                            // Moving somewhere else
                            self.begin_remove_rows(src_idx as i32, src_idx as i32);
                            self.content.remove(src_idx);
                            self.end_remove_rows();

                            if dest_idx > src_idx {
                                dest_idx -= 1;
                            }

                            self.begin_insert_rows(dest_idx as i32, dest_idx as i32);
                            self.content.insert(dest_idx, session);
                            self.end_insert_rows();
                        }
                    }
                    (Err(idx), None) => {
                        // Insert session at idx
                        self.begin_insert_rows(idx as i32, idx as i32);
                        self.content.insert(idx, session);
                        self.end_insert_rows();
                    }
                }
                // countChanged is also for unread count, so just fire it every time. It's cheap.
                self.countChanged();
            } else {
                assert!(event.for_table(schema::sessions::table));
                assert!(event.is_delete());

                let found = self
                    .content
                    .iter()
                    .enumerate()
                    .find(|(_, s)| s.id == session_id);

                if let Some((idx, _)) = found {
                    self.begin_remove_rows(idx as i32, idx as i32);
                    self.content.remove(idx);
                    self.end_remove_rows();

                    self.countChanged();
                } else {
                    tracing::warn!("Could not find session in model for deletion event");
                }
            }
        } else if let Some(message_id) = message_id {
            // There's no relation to a session, so that means that (data related to) an augmented message was
            // updated.
            if let Some((idx, session)) =
                self.content.iter_mut().enumerate().find(|(_, session)| {
                    session.last_message.as_ref().map(|x| x.id) == Some(message_id)
                })
            {
                if let Some(message) = &mut session.last_message {
                    if message.id == message_id {
                        // XXX This can in principle fetch a message with another timestamp,
                        // but I think all those cases are handled with a session_id
                        session.last_message =
                            storage.fetch_last_message_by_session_id_augmented(session.id);
                        let idx = self.row_index(idx as i32);
                        self.data_changed(idx, idx);
                    }
                }
            } else {
                tracing::warn!("Could not find session in model for message update event");
            }
        } else {
            tracing::warn!(
                "Unimplemented: Sessions model observe without message_id or session_id"
            );
        }
    }

    fn count(&self) -> usize {
        self.content.len()
    }

    fn unread(&self) -> usize {
        self.content
            .iter()
            .map(|session| usize::from(!session.is_read()))
            .sum()
    }
}

define_model_roles! {
    pub(super) enum SessionRoles for orm::AugmentedSession {
        Id(id):                                                            "id",
        SessionId(id):                                                     "sessionId",
        RecipientId(fn recipient_id(&self)):                               "recipientId",
        RecipientUuid(fn recipient_uuid(&self) via qstring_from_cow):      "recipientUuid",

        IsGroup(fn is_group(&self)):                                       "isGroup", // GroupV1 or GroupV2 actually
        IsGroupV2(fn is_group_v2(&self)):                                  "isGroupV2",
        IsRegistered(fn is_registered(&self)):                             "isRegistered",
        IsBlocked(fn is_blocked(&self)):                                   "isBlocked",
        GroupId(fn group_id(&self) via qstring_from_option):               "groupId",
        GroupName(fn group_name(&self) via qstring_from_option):           "groupName",
        GroupDescription(fn group_description(&self) via qstring_from_option):
                                                                           "groupDescription",
        Message(fn last_message_text(&self) via qstring_from_option):      "message",
        MessageId(fn last_message_id(&self)):                              "messageId",

        Section(fn section(&self) via QString::from):                      "section",
        Timestamp(fn timestamp(&self) via qdatetime_from_naive_option):    "timestamp",
        IsRead(fn is_read(&self)):                                         "read", // TODO Give session its own timestamp?
        Sent(fn sent(&self)):                                              "sent", // TODO cf. isPreviewReceived (#151)
        Delivered(fn delivered(&self)):                                    "deliveryCount",
        Read(fn read(&self)):                                              "readCount",
        IsMuted(fn is_muted(&self)):                                       "isMuted",
        IsArchived(fn is_archived(&self)):                                 "isArchived",
        IsPinned(fn is_pinned(&self)):                                     "isPinned",
        Viewed(fn viewed(&self)):                                          "viewCount",

        Draft(fn draft(&self) via QString::from):                          "draft",
        ExpiringMessageTimeout(expiring_message_timeout via int_from_duration_option): "expiringMessageTimeout",
    }
}

impl QAbstractListModel for SessionListModel {
    fn row_count(&self) -> i32 {
        self.content.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let role = SessionRoles::from(role);
        role.get(&self.content[index.row() as usize])
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        SessionRoles::role_names()
    }
}
