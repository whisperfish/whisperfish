#![allow(non_snake_case)]

use std::collections::HashMap;
use std::process::Command;

use crate::actor;
use crate::model::*;
use crate::store::orm;
use crate::worker::{ClientActor, SendMessage};

use actix::prelude::*;
use futures::prelude::*;
use itertools::Itertools;
use qmetaobject::*;

struct AugmentedMessage {
    inner: orm::Message,
    sender: Option<orm::Recipient>,
    attachments: Vec<orm::Attachment>,
    receipts: Vec<(orm::Receipt, orm::Recipient)>,
}

impl std::ops::Deref for AugmentedMessage {
    type Target = orm::Message;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl AugmentedMessage {
    pub fn source(&self) -> &str {
        if let Some(sender) = &self.sender {
            sender.e164_or_uuid()
        } else {
            ""
        }
    }

    pub fn sent(&self) -> bool {
        self.inner.sent_timestamp.is_some()
    }

    fn delivered(&self) -> u32 {
        self.receipts
            .iter()
            .filter(|(r, _)| r.delivered.is_some())
            .count() as _
    }

    fn read(&self) -> u32 {
        self.receipts
            .iter()
            .filter(|(r, _)| r.read.is_some())
            .count() as _
    }

    fn viewed(&self) -> u32 {
        self.receipts
            .iter()
            .filter(|(r, _)| r.viewed.is_some())
            .count() as _
    }

    pub fn attachments(&self) -> u32 {
        self.attachments.len() as _
    }

    pub fn first_attachment(&self) -> &str {
        if self.attachments.is_empty() {
            return "";
        }

        self.attachments[0].attachment_path.as_deref().unwrap_or("")
    }

    pub fn first_attachment_mime_type(&self) -> &str {
        if self.attachments.is_empty() {
            return "";
        }
        &self.attachments[0].content_type
    }
}

define_model_roles! {
    enum MessageRoles for AugmentedMessage {
        Id(id):                                               "id",
        Sid(session_id):                                      "sid",
        Source(fn source(&self) via QString::from):           "source",
        Message(text via qstring_from_option):                "message",
        Timestamp(server_timestamp via qdatetime_from_naive): "timestamp",

        Delivered(fn delivered(&self)):                       "delivered",
        Read(fn read(&self)):                                 "read",
        Viewed(fn viewed(&self)):                             "viewed",

        Sent(fn sent(&self)):                                 "sent",
        Flags(flags):                                         "flags",
        Attachments(fn attachments(&self)):                   "attachments",
        Outgoing(is_outbound):                                "outgoing",
        // FIXME
        // Queued(queued):                                       "queued",

        // FIXME issue #11 multiple attachments
        Attachment(fn first_attachment(&self) via QString::from): "attachment",
        AttachmentMimeType(fn first_attachment_mime_type(&self) via QString::from): "mimeType",
    }
}

#[derive(QObject, Default)]
pub struct MessageModel {
    base: qt_base_class!(trait QAbstractListModel),
    pub actor: Option<Addr<actor::MessageActor>>,
    pub client_actor: Option<Addr<ClientActor>>,

    messages: Vec<AugmentedMessage>,
    group_members: Vec<(orm::GroupV1Member, orm::Recipient)>,

    sessionId: qt_property!(i32; NOTIFY sessionIdChanged),

    peerIdentity: qt_property!(QString; NOTIFY peerIdentityChanged),
    peerName: qt_property!(QString; NOTIFY peerChanged),
    peerTel: qt_property!(QString; NOTIFY peerChanged),
    peerUuid: qt_property!(QString; NOTIFY peerChanged),

    groupMembers: qt_property!(QString; NOTIFY groupMembersChanged),
    groupId: qt_property!(QString; NOTIFY groupChanged),
    group: qt_property!(bool; NOTIFY groupChanged),

    peerIdentityChanged: qt_signal!(),
    peerChanged: qt_signal!(),
    groupMembersChanged: qt_signal!(),
    sessionIdChanged: qt_signal!(),
    groupChanged: qt_signal!(),

    openAttachment: qt_method!(fn(&self, index: usize)),
    createGroupMessage: qt_method!(
        fn(
            &self,
            group_id: QString,
            message: QString,
            groupName: QString,
            attachment: QString,
            add: bool,
        ) -> i32
    ),
    createMessage: qt_method!(
        fn(&self, source: QString, message: QString, attachment: QString, add: bool) -> i32
    ),

    sendMessage: qt_method!(fn(&self, mid: i32)),
    endSession: qt_method!(fn(&self, e164: QString)),

    load: qt_method!(fn(&self, sid: i32, peer_name: QString)),
    add: qt_method!(fn(&self, id: i32)),
    remove: qt_method!(
        fn(
            &self,
            id: usize, /* FIXME the implemented method takes an *index* but should take a message ID */
        )
    ),

    numericFingerprint: qt_method!(fn(&self, localId: QString, remoteId: QString) -> QString),

    markSent: qt_method!(fn(&self, id: i32)),
    markReceived: qt_method!(fn(&self, id: i32)),
}

impl MessageModel {
    fn openAttachment(&mut self, idx: usize) {
        let msg = if let Some(msg) = self.messages.get(idx) {
            msg
        } else {
            log::error!("[attachment] Message not found at index {}", idx);
            return;
        };

        // XXX move this method to its own model.
        let attachment = msg.first_attachment();

        log::debug!("[attachment] Open by index {:?}: {}", idx, &attachment);

        match Command::new("xdg-open").arg(attachment).status() {
            Ok(status) => {
                if !status.success() {
                    log::error!("[attachment] fail");
                }
            }
            Err(e) => {
                log::error!("[attachment] Error {}", e);
            }
        }
    }

    fn createGroupMessage(
        &mut self,
        group_id: QString,
        message: QString,
        _group_name: QString,
        attachment: QString,
        _add: bool,
    ) -> i32 {
        let group_id = group_id.to_string();
        let message = message.to_string();
        let attachment = attachment.to_string();

        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                // XXX hackermannnnnn
                .send(match group_id.len() {
                    32 => actor::QueueGroupMessage::GroupV1Message {
                        group_id,
                        message,
                        attachment,
                    },
                    64 => actor::QueueGroupMessage::GroupV2Message {
                        group_id,
                        message,
                        attachment,
                    },
                    _ => unreachable!("Illegal group ID"),
                })
                .map(Result::unwrap),
        );

        // TODO: QML should *not* synchronously wait for a session ID to be returned.
        -1
    }

    fn createMessage(
        &mut self,
        e164: QString,
        message: QString,
        attachment: QString,
        _add: bool,
    ) -> i32 {
        let e164 = e164.to_string();
        let message = message.to_string();
        let attachment = attachment.to_string();

        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::QueueMessage {
                    e164,
                    message,
                    attachment,
                })
                .map(Result::unwrap),
        );

        // TODO: QML should *not* synchronously wait for a session ID to be returned.
        -1
    }

    /// Called when a message should be queued to be sent to OWS
    fn sendMessage(&mut self, mid: i32) {
        actix::spawn(
            self.client_actor
                .as_mut()
                .unwrap()
                .send(SendMessage(mid))
                .map(Result::unwrap),
        );
    }

    /// Called when a message should be queued to be sent to OWS
    fn endSession(&mut self, e164: QString) {
        actix::spawn(
            self.actor
                .as_mut()
                .unwrap()
                .send(actor::EndSession(e164.into()))
                .map(Result::unwrap),
        );
    }

    pub fn handle_queue_message(&mut self, msg: orm::Message) {
        self.sendMessage(msg.id);

        // TODO: Go version modified the `self` model appropriately,
        //       with the `add`/`_add` parameter from createMessage.
        // if add {
        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, 0);
        self.messages.insert(
            0,
            AugmentedMessage {
                inner: msg,
                sender: None,
                // XXX
                attachments: Vec::new(),
                // No receipts yet.
                receipts: Vec::new(),
            },
        );
        (self as &mut dyn QAbstractListModel).end_insert_rows();
        // }
    }

    fn load(&mut self, sid: i32, _peer_name: QString) {
        (self as &mut dyn QAbstractListModel).begin_reset_model();

        self.messages.clear();

        (self as &mut dyn QAbstractListModel).end_reset_model();

        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchSession {
                    id: sid,
                    mark_read: false,
                })
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::FetchSession({})", sid);
    }

    /// Adds a message to QML list.
    ///
    /// This retrieves a `Message` by the given id and adds it to the UI.
    ///
    /// Note that the id argument was i64 in Go.
    fn add(&mut self, id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchMessage(id))
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::FetchMessage({})", id);
    }

    /// Remove a message from both QML and database
    ///
    /// Note the Go code said main thread only. This is
    /// satisfied in Rust by sending the request to the
    /// main thread.
    ///
    /// FIXME Take a message ID instead of an index.
    pub fn remove(&self, idx: usize) {
        let msg = if let Some(msg) = self.messages.get(idx) {
            msg
        } else {
            log::error!("[remove] Message not found at index {}", idx);
            return;
        };

        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::DeleteMessage(msg.id, idx))
                .map(Result::unwrap),
        );

        log::trace!("Dispatched actor::DeleteMessage({}, {})", msg.id, idx);
    }

    fn numericFingerprint(&self, _localId: QString, _remoteId: QString) -> QString {
        // XXX
        "unimplemented".into()
    }

    /// Mark a message sent in QML.
    ///
    /// Called through QML. Maybe QML doesn't know how
    /// to pass booleans, because this and `mark_received`
    /// simply wrap the real workhorse.
    ///
    /// Note that the id argument was i64 in Go.
    fn markSent(&mut self, id: i32) {
        self.mark(id, true, false)
    }

    /// Mark a message received in QML.
    ///
    /// Called through QML. Maybe QML doesn't know how
    /// to pass booleans, because this and `mark_sent`
    /// simply wrap the real workhorse.
    ///
    /// Note that the id argument was i64 in Go.
    fn markReceived(&mut self, id: i32) {
        self.mark(id, false, true)
    }

    /// Mark a message sent or received in QML. No database involved.
    ///
    /// Note that the id argument was i64 in Go.
    fn mark(&mut self, id: i32, mark_sent: bool, mark_received: bool) {
        if mark_sent && mark_received {
            log::trace!("Cannot mark message both sent and received");
            return;
        }

        if !mark_sent && !mark_received {
            log::trace!("Cannot mark message both not sent and not received");
            return;
        }

        if let Some((i, mut msg)) = self
            .messages
            .iter_mut()
            .enumerate()
            .find(|(_, msg)| msg.id == id)
        {
            if mark_sent {
                log::trace!("Mark message {} sent '{}'", id, mark_sent);

                // XXX
                msg.inner.sent_timestamp = Some(chrono::Utc::now().naive_utc());
            } else if mark_received {
                log::trace!("Mark message {} received '{}'", id, mark_received);

                // XXX needs something more in AugmentedMessage
                // msg.received = true;
            }
            // In fact, we should only update the necessary roles, but qmetaobject, in its current
            // state, does not allow this.
            // , MessageRoles::Received);
            // We'll also have troubles with the mutable borrow over `msg`, but that's nothing we
            // cannot solve.  We're saved by NLL here.
            let idx = (self as &mut dyn QAbstractListModel).row_index(i as i32);
            (self as &mut dyn QAbstractListModel).data_changed(idx, idx);
        } else {
            log::error!("Message not found");
        }
    }

    // Event handlers below this line

    /// Handle a fetched session from message's point of view
    pub fn handle_fetch_session(
        &mut self,
        sess: orm::Session,
        group_members: Vec<(orm::GroupV1Member, orm::Recipient)>,
        peer_identity: String,
    ) {
        log::trace!("handle_fetch_session({})", sess.id);
        self.sessionId = sess.id;
        self.sessionIdChanged();

        self.group_members = group_members;

        match sess.r#type {
            orm::SessionType::GroupV1(group) => {
                self.peerTel = QString::from("");
                self.peerUuid = QString::from("");
                self.peerName = QString::from(group.name.deref());
                self.peerChanged();

                self.group = true;
                self.groupId = QString::from(group.id);
                self.groupChanged();

                self.groupMembers = QString::from(
                    self.group_members
                        .iter()
                        .map(|(_, r)| r.e164_or_uuid())
                        .join(","),
                );
                self.groupMembersChanged();
            }
            orm::SessionType::GroupV2(group) => {
                self.peerTel = QString::from("");
                self.peerUuid = QString::from("");
                self.peerName = QString::from(group.name.deref());
                self.peerChanged();

                self.group = true;
                self.groupId = QString::from(group.id);
                self.groupChanged();

                self.groupMembers = QString::from(
                    self.group_members
                        .iter()
                        .map(|(_, r)| r.e164_or_uuid())
                        .join(","),
                );
                self.groupMembersChanged();
            }
            orm::SessionType::DirectMessage(recipient) => {
                self.group = false;
                self.groupId = QString::from("");
                self.groupChanged();

                self.peerTel = QString::from(recipient.e164.as_deref().unwrap_or(""));
                self.peerUuid = QString::from(recipient.uuid.as_deref().unwrap_or(""));
                self.peerName = QString::from(recipient.e164_or_uuid());
                self.peerChanged();
            }
        };

        self.peerIdentity = peer_identity.into();
        self.peerIdentityChanged();

        // TODO: contact identity key
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchAllMessages(sess.id))
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::FetchAllMessages({})", sess.id);
    }

    pub fn handle_fetch_message(
        &mut self,
        message: orm::Message,
        recipient: Option<orm::Recipient>,
        attachments: Vec<orm::Attachment>,
        receipts: Vec<(orm::Receipt, orm::Recipient)>,
    ) {
        log::trace!("handle_fetch_message({})", message.id);

        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, 0);
        self.messages.insert(
            0,
            AugmentedMessage {
                inner: message,
                sender: recipient,
                attachments,
                receipts,
            },
        );
        (self as &mut dyn QAbstractListModel).end_insert_rows();
    }

    #[allow(clippy::type_complexity)]
    pub fn handle_fetch_all_messages(
        &mut self,
        messages: Vec<(
            orm::Message,
            Option<orm::Recipient>,
            Vec<orm::Attachment>,
            Vec<(orm::Receipt, orm::Recipient)>,
        )>,
    ) {
        log::trace!(
            "handle_fetch_all_messages({}) count {}",
            messages[0].0.session_id,
            messages.len()
        );

        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, messages.len() as i32);

        self.messages.extend(messages.into_iter().map(
            |(message, sender, attachments, receipts)| AugmentedMessage {
                inner: message,
                sender,
                attachments,
                receipts,
            },
        ));

        (self as &mut dyn QAbstractListModel).end_insert_rows();
    }

    pub fn handle_delete_message(&mut self, id: i32, idx: usize, del_rows: usize) {
        log::trace!(
            "handle_delete_message({}) deleted {} rows, remove qml idx {}",
            id,
            del_rows,
            idx
        );

        (self as &mut dyn QAbstractListModel).begin_remove_rows(idx as i32, idx as i32);

        self.messages.remove(idx);

        (self as &mut dyn QAbstractListModel).end_remove_rows();
    }
}

impl QAbstractListModel for MessageModel {
    fn row_count(&self) -> i32 {
        self.messages.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let role = MessageRoles::from(role);
        role.get(&self.messages[index.row() as usize])
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        MessageRoles::role_names()
    }
}
