use std::collections::HashMap;

use crate::actor;
use crate::model::*;
use crate::store;
use crate::worker::{ClientActor, SendMessage};

use actix::prelude::*;
use futures::prelude::*;
use qmetaobject::*;

define_model_roles! {
    enum MessageRoles for store::Message {
        ID(id):                                         "id",
        SID(sid):                                       "sid",
        Source(source via QString::from):               "source",
        Message(message via QString::from):             "message",
        Timestamp(timestamp via qdatetime_from_i64):    "timestamp",
        Sent(sent):                                     "sent",
        Received(received):                             "received",
        Flags(flags):                                   "flags",
        Attachment(attachment via qstring_from_option): "attachment",
        MimeType(mimetype via qstring_from_option):     "mimeType",
        HasAttachment(hasattachment):                   "hasAttachment",
        Outgoing(outgoing):                             "outgoing",
        Queued(queued):                                 "queued",
    }
}

#[derive(QObject, Default)]
#[allow(non_snake_case)] // XXX: QML expects these as-is; consider changing later
pub struct MessageModel {
    base: qt_base_class!(trait QAbstractListModel),
    pub actor: Option<Addr<actor::MessageActor>>,
    pub client_actor: Option<Addr<ClientActor>>,

    messages: Vec<store::Message>,

    peerIdentity: qt_property!(QString; NOTIFY peerIdentityChanged),
    peerName: qt_property!(QString; NOTIFY peerNameChanged),
    peerTel: qt_property!(QString; NOTIFY peerTelChanged),
    groupMembers: qt_property!(QString; NOTIFY groupMembersChanged),
    sessionId: qt_property!(i64; NOTIFY sessionIdChanged),
    group: qt_property!(bool; NOTIFY groupChanged),

    peerIdentityChanged: qt_signal!(),
    peerNameChanged: qt_signal!(),
    peerTelChanged: qt_signal!(),
    groupMembersChanged: qt_signal!(),
    sessionIdChanged: qt_signal!(),
    groupChanged: qt_signal!(),

    createMessage: qt_method!(
        fn(
            &self,
            source: QString,
            message: QString,
            groupName: QString,
            attachment: QString,
            add: bool,
        ) -> i64
    ),

    sendMessage: qt_method!(fn(&self, mid: i32)),

    load: qt_method!(fn(&self, sid: i64, peer_name: QString)),
    add: qt_method!(fn(&self, id: i32)),
    remove: qt_method!(fn(&self, id: usize)),

    numericFingerprint: qt_method!(fn(&self, localId: QString, remoteId: QString) -> QString),
}

impl MessageModel {
    #[allow(non_snake_case)]
    fn createMessage(
        &mut self,
        source: QString,
        message: QString,
        groupName: QString,
        attachment: QString,
        _add: bool,
    ) -> i64 {
        let source = source.to_string();
        let message = message.to_string();
        let group = groupName.to_string();
        let attachment = attachment.to_string();

        Arbiter::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::QueueMessage {
                    source,
                    message,
                    attachment,
                    group,
                })
                .map(Result::unwrap),
        );

        // TODO: QML should *not* synchronously wait for a session ID to be returned.
        -1
    }

    #[allow(non_snake_case)]
    /// Called when a message should be queued to be sent to OWS
    fn sendMessage(&mut self, mid: i32) {
        Arbiter::spawn(
            self.client_actor
                .as_mut()
                .unwrap()
                .send(SendMessage(mid))
                .map(Result::unwrap),
        );
    }

    pub fn handle_queue_message(&mut self, msg: crate::store::Message) {
        self.sendMessage(msg.id);

        // TODO: Go version modified the `self` model appropriately,
        //       with the `add`/`_add` parameter from createMessage.
        // if add {
        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, 0);
        self.messages.insert(0, msg);
        (self as &mut dyn QAbstractListModel).end_insert_rows();
        // }
    }

    fn load(&mut self, sid: i64, _peer_name: QString) {
        (self as &mut dyn QAbstractListModel).begin_reset_model();

        self.messages.clear();

        (self as &mut dyn QAbstractListModel).end_reset_model();

        Arbiter::spawn(
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
        Arbiter::spawn(
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
    pub fn remove(&self, idx: usize) {
        let msg = if let Some(msg) = self.messages.get(idx) {
            msg
        } else {
            log::error!("[remove] Message not found at index {}", idx);
            return;
        };

        Arbiter::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::DeleteMessage(msg.id, idx))
                .map(Result::unwrap),
        );

        log::trace!("Dispatched actor::DeleteMessage({}, {})", msg.id, idx);
    }

    #[allow(non_snake_case)]
    fn numericFingerprint(&self, _localId: QString, _remoteId: QString) -> QString {
        // XXX
        "unimplemented".into()
    }

    // Event handlers below this line

    /// Handle a fetched session from message's point of view
    pub fn handle_fetch_session(&mut self, sess: store::Session) {
        log::trace!("handle_fetch_session({})", sess.message);
        self.sessionId = sess.id;
        self.sessionIdChanged();

        self.group = sess.is_group;
        self.groupChanged();

        let group_name = sess.group_name.unwrap_or(String::new());
        if sess.is_group && group_name != "" {
            self.peerName = QString::from(group_name);
        } else {
            self.peerName = QString::from(sess.source.clone());
        }
        self.peerNameChanged();

        self.peerTel = QString::from(sess.source);
        self.peerTelChanged();

        self.groupMembers = QString::from(sess.group_members.unwrap_or(String::new()));
        self.groupMembersChanged();

        // TODO: contact identity key
        Arbiter::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchAllMessages(sess.id))
                .map(Result::unwrap),
        );
        log::trace!("Dispatched actor::FetchAllMessages({})", sess.id);
    }

    pub fn handle_fetch_message(&mut self, message: store::Message) {
        log::trace!("handle_fetch_message({})", message.id);

        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, 0);
        self.messages.insert(0, message);
        (self as &mut dyn QAbstractListModel).end_insert_rows();
    }

    pub fn handle_fetch_all_messages(&mut self, messages: Vec<store::Message>) {
        log::trace!(
            "handle_fetch_all_messages({}) count {}",
            messages[0].sid,
            messages.len()
        );

        (self as &mut dyn QAbstractListModel).begin_insert_rows(0, messages.len() as i32);

        self.messages.extend(messages);

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
