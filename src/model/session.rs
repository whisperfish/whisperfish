#![allow(non_snake_case)]

use std::collections::HashMap;

use crate::actor;
use crate::model::*;
use crate::store::orm;

use actix::prelude::*;
use chrono::prelude::*;
use futures::prelude::*;
use itertools::Itertools;
use qmetaobject::*;

// XXX attachments and receipts could be a compressed form.
struct AugmentedSession {
    session: orm::Session,
    group_members: Vec<orm::Recipient>,
    last_message: orm::Message,
    last_message_receipts: Vec<(orm::Receipt, orm::Recipient)>,
    last_message_attachments: Vec<orm::Attachment>,
}

impl std::ops::Deref for AugmentedSession {
    type Target = orm::Session;

    fn deref(&self) -> &orm::Session {
        &self.session
    }
}

#[derive(QObject, Default)]
pub struct SessionModel {
    base: qt_base_class!(trait QAbstractListModel),
    pub actor: Option<Addr<actor::SessionActor>>,

    content: Vec<AugmentedSession>,

    count: qt_method!(fn(&self) -> usize),
    add: qt_method!(fn(&self, id: i32, mark_read: bool)),
    remove: qt_method!(fn(&self, idx: usize)),
    removeById: qt_method!(fn(&self, id: i32)),
    reload: qt_method!(fn(&self)),

    markRead: qt_method!(fn(&mut self, id: usize)),
    markReceived: qt_method!(fn(&self, id: usize)),
    markSent: qt_method!(fn(&self, id: usize, message: QString)),
}

impl SessionModel {
    fn count(&self) -> usize {
        self.content.len()
    }

    /// Add or replace a Session in the model.
    fn add(&self, id: i32, mark_read: bool) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchSession { id, mark_read })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::FetchSession({})", id);
    }

    /// Removes session at index. This removes the session from the list model and
    /// deletes it from the database.
    fn remove(&mut self, idx: usize) {
        if idx > self.content.len() - 1 {
            log::error!("Invalid index for session model");
            return;
        }

        let sid = self.content[idx].id;

        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::DeleteSession { id: sid, idx })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::DeleteSession({})", idx);
    }

    /// Removes session by id. This removes the session from the list model and
    /// deletes it from the database.
    fn removeById(&self, id: i32) {
        let idx = self
            .content
            .iter()
            .position(|x| x.id == id)
            .expect("Session ID not found in session model");

        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::DeleteSession { id, idx })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::DeleteSession({})", idx);
    }

    fn reload(&self) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::LoadAllSessions)
                .map(Result::unwrap),
        );
    }

    fn markRead(&mut self, id: usize) {
        if let Some((idx, session)) = self
            .content
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.id == id as i32)
        {
            actix::spawn(
                self.actor
                    .as_ref()
                    .unwrap()
                    .send(actor::MarkSessionRead {
                        sid: session.id,
                        already_unread: !session.last_message.is_read,
                    })
                    .map(Result::unwrap),
            );

            session.last_message.is_read = true;
            let idx = (self as &mut dyn QAbstractListModel).row_index(idx as i32);
            (self as &mut dyn QAbstractListModel).data_changed(idx, idx);
        }

        // XXX: don't forget sync messages
    }

    fn markReceived(&self, _id: usize) {
        log::trace!("STUB: Mark received called");
        // XXX: don't forget sync messages
    }

    fn markSent(&self, _id: usize, _message: QString) {
        log::trace!("STUB: Mark sent called");
        // XXX: don't forget sync messages
    }

    // Event handlers below this line

    /// Handle loaded session
    ///
    /// The session is accompanied by the last message that was sent on this session.
    #[allow(clippy::type_complexity)]
    pub fn handle_sessions_loaded(
        &mut self,
        sessions: Vec<(
            orm::Session,
            Vec<orm::Recipient>,
            orm::Message,
            Vec<orm::Attachment>,
            Vec<(orm::Receipt, orm::Recipient)>,
        )>,
    ) {
        // XXX: maybe this should be called before even accessing the db?
        (self as &mut dyn QAbstractListModel).begin_reset_model();
        self.content = sessions
            .into_iter()
            .map(
                |(
                    session,
                    group_members,
                    last_message,
                    last_message_attachments,
                    last_message_receipts,
                )| {
                    AugmentedSession {
                        session,
                        group_members,
                        last_message,
                        last_message_attachments,
                        last_message_receipts,
                    }
                },
            )
            .collect();
        // XXX This could be solved through a sub query.
        self.content.sort_unstable_by(|a, b| {
            a.last_message
                .server_timestamp
                .cmp(&b.last_message.server_timestamp)
                .reverse()
        });
        (self as &mut dyn QAbstractListModel).end_reset_model();
    }

    /// Handle add-or-replace session
    pub fn handle_fetch_session(
        &mut self,
        sess: orm::Session,
        group_members: Vec<orm::Recipient>,
        mut last_message: orm::Message,
        last_message_attachments: Vec<orm::Attachment>,
        last_message_receipts: Vec<(orm::Receipt, orm::Recipient)>,
        mark_read: bool,
    ) {
        let sid = sess.id;
        let mut already_unread = false;

        let found = self
            .content
            .iter()
            .enumerate()
            .find(|(_i, s)| s.id == sess.id);

        if let Some((idx, _session)) = found {
            if !last_message.is_read {
                already_unread = true;
            }

            // Remove from this place so it can be added back in later
            (self as &mut dyn QAbstractListModel).begin_remove_rows(idx as i32, idx as i32);
            self.content.remove(idx);
            (self as &mut dyn QAbstractListModel).end_remove_rows();
        };

        if !last_message.is_read && mark_read {
            actix::spawn(
                self.actor
                    .as_ref()
                    .unwrap()
                    .send(actor::MarkSessionRead {
                        sid: sess.id,
                        already_unread,
                    })
                    .map(Result::unwrap),
            );
            log::trace!(
                "Dispatched actor::MarkSessionRead({}, {})",
                sid,
                already_unread
            );
            last_message.is_read = true;

        // unimplemented!();
        } else if last_message.is_read && !already_unread {
            // TODO: model.session.go:181
            // let count = self.unread() + 1;

            // self.set_unread(count);
            // self.unread_changed(count);
        }

        log::trace!("Inserting the message back in qml");

        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, 0);
        self.content.insert(
            0,
            AugmentedSession {
                session: sess,
                group_members,
                last_message,
                last_message_attachments,
                last_message_receipts,
            },
        );
        (self as &mut dyn QAbstractListModel).end_insert_rows();
    }

    /// When a session is marked as read and this handler called, implicitly
    /// the session will be set at the top of the QML list.
    pub fn handle_mark_session_read(&mut self, sid: i32, already_unread: bool) {
        if let Some((i, session)) = self
            .content
            .iter_mut()
            .enumerate()
            .find(|(_, s)| s.session.id == sid)
        {
            session.last_message.is_read = true;
            let idx = (self as &mut dyn QAbstractListModel).row_index(i as i32);
            (self as &mut dyn QAbstractListModel).data_changed(idx, idx);
        } else {
            log::warn!("Could not call data_changed for non-existing session!");
        }

        if already_unread {
            // TODO: model.session.go:173
            // let count = std::cmp::min(0, self.unread() - 1);

            // self.set_unread(count);
            // self.unread_changed(count);
        }
    }

    /// Remove deleted session from QML
    pub fn handle_delete_session(&mut self, idx: usize) {
        (self as &mut dyn QAbstractListModel).begin_remove_rows(idx as i32, idx as i32);
        self.content.remove(idx);
        (self as &mut dyn QAbstractListModel).end_remove_rows();
    }
}

impl AugmentedSession {
    fn timestamp(&self) -> NaiveDateTime {
        self.last_message.server_timestamp
    }

    fn group_name(&self) -> Option<&str> {
        match &self.session.r#type {
            orm::SessionType::GroupV1(group) => Some(&group.name),
            orm::SessionType::GroupV2(group) => Some(&group.name),
            orm::SessionType::DirectMessage(_) => None,
        }
    }

    // FIXME we have them separated now... Get QML to understand it.
    fn group_members(&self) -> Option<String> {
        match &self.session.r#type {
            orm::SessionType::GroupV1(_group) => Some(
                self.group_members
                    .iter()
                    .map(|r| r.e164_or_uuid())
                    .join(","),
            ),
            orm::SessionType::GroupV2(_group) => Some(
                self.group_members
                    .iter()
                    .map(|r| r.e164_or_uuid())
                    .join(","),
            ),
            orm::SessionType::DirectMessage(_) => None,
        }
    }

    fn sent(&self) -> bool {
        self.last_message.sent_timestamp.is_some()
    }

    fn source(&self) -> &str {
        match &self.session.r#type {
            orm::SessionType::GroupV1(_group) => "",
            orm::SessionType::GroupV2(_group) => "",
            orm::SessionType::DirectMessage(recipient) => recipient.e164_or_uuid(),
        }
    }

    fn has_attachment(&self) -> bool {
        !self.last_message_attachments.is_empty()
    }

    fn section(&self) -> String {
        // XXX: stub
        let now = chrono::Utc::now();
        let today = Utc
            .ymd(now.year(), now.month(), now.day())
            .and_hms(0, 0, 0)
            .naive_utc();
        let diff = today.signed_duration_since(self.last_message.server_timestamp);

        if diff.num_seconds() <= 0 {
            String::from("today")
        } else if diff.num_seconds() > 0 && diff.num_hours() <= 24 {
            String::from("yesterday")
        } else if diff.num_seconds() > 0 && diff.num_hours() <= (7 * 24) {
            let wd = self
                .last_message
                .server_timestamp
                .weekday()
                .number_from_monday()
                % 7;
            wd.to_string()
        } else {
            String::from("older")
        }
    }

    fn delivered(&self) -> u32 {
        self.last_message_receipts
            .iter()
            .filter(|(r, _)| r.delivered.is_some())
            .count() as _
    }

    fn read(&self) -> u32 {
        self.last_message_receipts
            .iter()
            .filter(|(r, _)| r.read.is_some())
            .count() as _
    }

    fn viewed(&self) -> u32 {
        self.last_message_receipts
            .iter()
            .filter(|(r, _)| r.viewed.is_some())
            .count() as _
    }
}

define_model_roles! {
    // FIXME: many of these are now functions because of backwards compatibility.
    //        swap them around for real fields, and fixup QML instead.
    enum SessionRoles for AugmentedSession {
        Id(id):                                                            "id",
        Source(fn source(&self) via QString::from):                        "source",
        IsGroup(fn is_group(&self)):                                       "isGroup",
        IsGroupV2(fn is_group_v2(&self)):                                  "isGroupV2",
        GroupName(fn group_name(&self) via qstring_from_option):           "groupName",
        GroupMembers(fn group_members(&self) via qstring_from_option):     "groupMembers",
        Message(last_message.text via qstring_from_option):                "message",
        Section(fn section(&self) via QString::from):                      "section",
        Timestamp(fn timestamp(&self) via qdatetime_from_naive):           "timestamp",
        IsRead(last_message.is_read):                                      "read",
        Sent(fn sent(&self)):                                              "sent",
        Delivered(fn delivered(&self)):                                    "delivered",
        Read(fn read(&self)):                                              "read",
        Viewed(fn viewed(&self)):                                          "viewed",
        HasAttachment(fn has_attachment(&self)):                           "hasAttachment"
    }
}

impl QAbstractListModel for SessionModel {
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
