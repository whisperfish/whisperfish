use std::collections::HashMap;

use crate::actor;
use crate::model::session;
use crate::model::*;
use crate::store;

use actix::prelude::*;
use diesel::prelude::*;
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
}

impl MessageModel {
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
