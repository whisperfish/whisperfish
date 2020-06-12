use std::collections::HashMap;

extern crate diesel;

use crate::actor;
use crate::model::session;
use crate::model::*;
use crate::store;

use actix::prelude::*;
use diesel::prelude::*;
use qmetaobject::*;
use std::fs;

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
        MimeType(mimetype via qstring_from_option):     "mimetype",
        HasAttachment(hasattachment):                   "hasattachment",
        Outgoing(outgoing):                             "outgoing",
        Queued(queued):                                 "queued",
    }
}

#[derive(QObject, Default)]
#[allow(non_snake_case)] // XXX: QML expects these as-is; consider changing later
pub struct MessageModel {
    base: qt_base_class!(trait QAbstractListModel),
    pub actor: Option<Addr<actor::MessageActor>>,

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

    load: qt_method!(fn(&self, sid: i64, peer_name: QString)),
    add: qt_method!(fn(&self, id: i32)),
    markSent: qt_method!(fn(&self, id: i32)),
    markReceived: qt_method!(fn(&self, id: i32)),
    remove: qt_method!(fn(&self, id: usize)),
    createMessage: qt_method!(fn(&self, source: String, msg: String, group_name: String, attachment: String, add: bool) -> i32),
}

impl MessageModel {
    fn row_count(&self) -> i32 {
        log::trace!("rowCount called, returning {}", self.messages.len());
        self.messages.len() as i32
    }

    fn load(&mut self, sid: i64, peer_name: QString) {
        (self as &mut dyn QAbstractListModel).begin_reset_model();

        self.messages.clear();

        (self as &mut dyn QAbstractListModel).end_reset_model();

        use futures::prelude::*;
        Arbiter::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchSession(sid))
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
        use futures::prelude::*;
        Arbiter::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(actor::FetchMessage(id))
                .map(Result::unwrap)
        );
        log::trace!("Dispatched actor::FetchMessage({})", id);
    }

    /// Mark a message sent in QML.
    ///
    /// Called through QML. Maybe QML doesn't know how
    /// to pass booleans, because this and `mark_received`
    /// simply wrap the real workhorse.
    ///
    /// Note that the id argument was i64 in Go.
    #[allow(non_snake_case)] // XXX: QML expects these as-is; consider changing later]
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
    #[allow(non_snake_case)] // XXX: QML expects these as-is; consider changing later]
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

        if let Some((i, msg)) = self
            .messages
            .iter()
            .enumerate()
            .find(|(_, msg)| msg.id == id)
        {
            // let index = (self as &mut dyn QAbstractItemModel).create_index(i as i32, 0, 0 as usize);
            if mark_sent {
                log::trace!("Mark message {} sent '{}'", id, mark_sent);

                // msg.sent = true;
                // msg.queued = false;
                // (self as &mut dyn QAbstractListModel).data_changed(index, index);  // , MessageRoles::Sent);
                // (self as &mut dyn QAbstractListModel).data_changed(index, index);  // , MessageRoles::Queued);
            } else if mark_received {
                log::trace!("Mark message {} received '{}'", id, mark_received);

                // msg.received = true;
                // (self as &mut dyn QAbstractListModel).data_changed(index, index);  // , MessageRoles::Received);
            }
        } else {
            log::error!("Message not found");
        }
    }

    /// Remove a message from both QML and database
    ///
    /// Note the Go code said main thread only. This is
    /// satisfied in Rust by sending the request to the
    /// main thread.
    pub fn remove(&self, idx: usize) {
        let msg = &self.messages[idx];

        if let Some((i, msg)) = self
            .messages
            .iter()
            .enumerate()
            .find(|(i, msg)| *i == idx)
        {
            use futures::prelude::*;
            Arbiter::spawn(
                self.actor
                    .as_ref()
                    .unwrap()
                    .send(actor::DeleteMessage(msg.id, i))
                    .map(Result::unwrap),
            );
            log::trace!("Dispatched actor::DeleteMessage({}, {})", msg.id, i);
        } else {
            log::error!("[remove] Message not found at index {}", idx);
        }
    }

    /// Create a new outgoing message, save to database and queue for delivery.
    ///
    /// If `add` is true then the new message will be appended to the model.
    /// When called from the `NewMessage` page, `add` should be set to `false`
    /// because there is no active session.
    ///
    /// Returns the session ID the message was created under.
    #[allow(non_snake_case)] // XXX: QML expects these as-is; consider changing later]
    pub fn createMessage(&self, source: String, msg: String, group_name: String, attachment: String, add: bool) -> i32 {
        // If group name or source is a comma separated list then create a group.
        let recipients: Vec<&str> = source.split(",").collect();

        if group_name.len() > 0 || recipients.len() > 1 {
            log::trace!("TODO textsecure.NewGroup(groupName, recipients) but in Rust");
            // let group = libsignal_protocol.new_group(group_name, recipients);
            // source = group.hex_id();
        }

        log::trace!("queue message {}", msg);
	    let new_msg = self.queue_message(source, msg, attachment /* , &'aal group */);

        // if add {
        // }

        // self.send_message(new_msg.id);

        // return new_msg.id;
        0
    }

    /// Prepare outgoing message for delivery to Signal and save message to queue.
    ///
    /// The message will be fetched from the queue by the SendWorker in a separate
    /// actix-qt thread and sent to Signal
    ///
    /// This is synchronous because we don't want to add the message to QML or
    /// emit the SendMessage signal if adding to the database fails.
    pub fn queue_message(&self, src: String, msg: String, attachment: String /*, group: &'aal TextSecure.Group */) -> Option<store::NewMessage> {
        let mut new_msg = store::NewMessage {
            // XXX: Diesel, by documentation, wants lifetimed references but nothing works that way
            source: src,
            text: msg,
            timestamp: Local::now().timestamp_nanos(),

            outgoing: &true,

            // TODO: Attachment dealth with in a future squash
            has_attachment: &false,
            attachment: Some(String::new()),
            mime_type: Some(String::new()),

            // Not sent until queue is processed
            sent: &false,
            received: &false,
            flags: &0,
        };

        /*
        if attachment.len() > 0 {
            if !fs::metadata(attachment).is_ok() {
                return None;
            }
            new_msg.attachment = Option<attachment::String>;
            new_msg.mime_type = Some(String::from("application/binary"));  // TODO: Not really
            new_msg.has_attachment = &true;
        } else {
            new_msg.attachment = Option<attachment = None>;
        };
        */

        return Some(new_msg);
    }

    // Event handlers below this line

    pub fn handle_fetch_session(&mut self, sess: session::Session) {
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
        use futures::prelude::*;
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

        let tail = self.row_count();

        (self as &mut dyn QAbstractListModel).begin_insert_rows(tail, tail + 1);

        self.messages.insert(tail as usize, message);

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

        // XXX testing
        log::trace!("TEST_ADD msg len: {}", self.row_count());
        log::trace!("TEST_ADD last msg id: {}", self.messages[self.row_count() as usize - 1].id);
        self.add(self.messages[self.row_count() as usize - 1].id);
        log::trace!("TEST_ADD afterwards len: {}", self.row_count());
    }

    pub fn handle_delete_message(&mut self, id: i32, idx: usize, del_rows: usize) {
        log::trace!("handle_delete_message({}) deleted {} rows, remove qml idx {}",
                    id, del_rows, idx);

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
