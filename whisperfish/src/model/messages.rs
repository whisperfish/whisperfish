#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::Storage;
use qmetaobject::QObjectBox;
use qmetaobject::{prelude::*, QMetaType};
use qttypes::{QVariantList, QVariantMap};
use std::collections::HashMap;
use whisperfish_store::schema;
use whisperfish_store::store::orm;

/// QML-constructable object that interacts with a single session.
#[derive(Default, QObject)]
pub struct MessageImpl {
    base: qt_base_class!(trait QObject),
    message_id: Option<i32>,
    message: Option<orm::AugmentedMessage>,

    attachments: QObjectBox<AttachmentListModel>,
    visual_attachments: QObjectBox<AttachmentListModel>,
    detail_attachments: QObjectBox<AttachmentListModel>,
}

crate::observing_model! {
    pub struct Message(MessageImpl) {
        messageId: i32; READ get_message_id WRITE set_message_id,
        valid: bool; READ get_valid,
        attachments: QVariant; READ attachments,
        reactions_: u32; READ reactions,
        thumbsAttachments: QVariant; READ visual_attachments,
        detailAttachments: QVariant; READ detail_attachments,
    } WITH OPTIONAL PROPERTIES FROM message WITH ROLE MessageRoles {
        sessionId SessionId,
        message Message,
        styledMessage StyledMessage,
        timestamp Timestamp,

        senderRecipientId SenderRecipientId,
        delivered Delivered,
        read Read,
        viewed Viewed,

        sent Sent,
        flags Flags,
        messageType MessageType,
        outgoing Outgoing,
        queued Queued,
        failed Failed,
        remoteDeleted RemoteDeleted,

        unidentifiedSender Unidentified,
        quotedMessageId QuotedMessageId,
        hasSpoilers HasSpoilers,
        hasStrikeThrough HasStrikeThrough,

        expiresIn ExpiresIn,
    }
}

impl EventObserving for MessageImpl {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, event: crate::store::observer::Event) {
        if let Some(id) = self.message_id {
            if let Some(attachment_id) = event.relation_key_for(schema::attachments::table) {
                if event.is_delete() {
                    // XXX This could also be implemented efficiently
                    self.fetch(ctx.storage(), id);
                } else {
                    // Only reload the attachments.
                    // We could also just reload the necessary attachment, but we're lazy today.
                    self.load_attachment(ctx.storage(), id, attachment_id.as_i32().unwrap());
                }
            } else {
                self.fetch(ctx.storage(), id);
            }
        }
    }

    fn interests(&self) -> Vec<Interest> {
        self.message
            .iter()
            .flat_map(orm::AugmentedMessage::interests)
            .chain(self.message_id.iter().map(|mid| {
                Interest::whole_table_with_relation(
                    schema::attachments::table,
                    schema::messages::table,
                    *mid,
                )
            }))
            .collect()
    }
}

impl MessageImpl {
    fn get_message_id(&self) -> i32 {
        self.message_id.unwrap_or(-1)
    }

    fn get_valid(&self) -> bool {
        self.message_id.is_some() && self.message.is_some()
    }

    fn attachments(&self) -> QVariant {
        self.attachments.pinned().into()
    }

    fn reactions(&self) -> u32 {
        tracing::trace!(
            "reactions (mid {:?}): {:?}",
            self.message.as_ref().map(|m| m.id),
            self.message.as_ref().map(|m| m.reactions)
        );
        self.message.as_ref().map(|m| m.reactions).unwrap_or(0) as _
    }

    fn detail_attachments(&self) -> QVariant {
        self.detail_attachments.pinned().into()
    }

    fn visual_attachments(&self) -> QVariant {
        self.visual_attachments.pinned().into()
    }

    fn fetch(&mut self, storage: Storage, id: i32) {
        self.message = storage.fetch_augmented_message(id);
        self.fetch_attachments(storage, id);
    }

    fn fetch_attachments(&mut self, storage: Storage, id: i32) {
        let attachments = storage.fetch_attachments_for_message(id);
        self.attachments
            .pinned()
            .borrow_mut()
            .set(attachments.clone());

        let (visual, detail) = attachments
            .into_iter()
            .partition(|x| x.content_type.contains("image") || x.content_type.contains("video"));

        self.detail_attachments.pinned().borrow_mut().set(detail);
        self.visual_attachments.pinned().borrow_mut().set(visual);
    }

    fn load_attachment(&mut self, storage: Storage, _id: i32, attachment_id: i32) {
        let attachment = storage
            .fetch_attachment(attachment_id)
            .expect("existing attachment");

        for container in &[
            &self.attachments,
            if attachment.content_type.contains("image")
                || attachment.content_type.contains("video")
            {
                &self.visual_attachments
            } else {
                &self.detail_attachments
            },
        ] {
            container
                .pinned()
                .borrow_mut()
                .update_attachment(attachment.clone());
        }
    }

    fn set_message_id(&mut self, ctx: Option<ModelContext<Self>>, id: i32) {
        if id >= 0 {
            self.message_id = Some(id);
            if let Some(ctx) = ctx {
                self.fetch(ctx.storage(), id);
            }
        } else {
            self.message_id = None;
            self.message = None;
            self.attachments.pinned().borrow_mut().set(Vec::new());
        }
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if let Some(id) = self.message_id {
            self.fetch(ctx.storage(), id);
        }
    }
}

/// QML-constructable object that interacts with a single session.
#[derive(Default, QObject)]
pub struct SessionImpl {
    base: qt_base_class!(trait QObject),
    session_id: Option<i32>,
    session: Option<orm::AugmentedSession>,
    message_list: QObjectBox<MessageListModel>,
}

crate::observing_model! {
    pub struct Session(SessionImpl) {
        sessionId: i32; READ get_session_id WRITE set_session_id,
        valid: bool; READ get_valid,
        messages: QVariant; READ messages,
    } WITH OPTIONAL PROPERTIES FROM session WITH ROLE SessionRoles {
        recipientId RecipientId,

        isGroup IsGroup,
        isGroupV2 IsGroupV2,
        isRegistered IsRegistered,

        groupId GroupId,
        groupName GroupName,
        groupDescription GroupDescription,

        message Message,
        section Section,
        timestamp Timestamp,
        read IsRead,
        sent Sent,
        deliveryCount Delivered,
        readCount Read,
        isMuted IsMuted,
        isArchived IsArchived,
        isPinned IsPinned,
        viewCount Viewed,
        draft Draft,
        expiringMessageTimeout ExpiringMessageTimeout,
    }
}

impl EventObserving for SessionImpl {
    type Context = ModelContext<Self>;

    #[tracing::instrument(level = "trace", skip(self, ctx))]
    fn observe(&mut self, ctx: Self::Context, event: crate::store::observer::Event) {
        let storage = ctx.storage();
        if let Some(session_id) = self.session_id {
            let message_id = event
                .relation_key_for(schema::messages::table)
                .and_then(|x| x.as_i32());

            if event.for_table(schema::attachments::table) && event.is_update() {
                // AugmentedMessage only cares about the number of attachments.
                tracing::trace!("Skipping attachment update");
            } else if event.relation_key_for(schema::reactions::table).is_some() {
                // Reactions update themselves.
                tracing::trace!("Skipping reaction update");
            } else if event.for_row(schema::sessions::table, session_id) {
                self.session = storage.fetch_session_by_id_augmented(session_id);
                // XXX how to trigger a Qt signal now?
            } else if message_id.is_some() {
                self.session = storage.fetch_session_by_id_augmented(session_id);
                self.message_list
                    .pinned()
                    .borrow_mut()
                    .observe(storage, session_id, event);
                // XXX how to trigger a Qt signal now?
            } else if event.for_table(schema::recipients::table) {
                let Some(new_recipient) = event
                    .relation_key_for(schema::recipients::table)
                    .and_then(|x| x.as_i32())
                    .and_then(|recipient_id| storage.fetch_recipient_by_id(recipient_id))
                else {
                    // Only refresh session - messages update themselves.
                    self.session = storage.fetch_session_by_id_augmented(session_id);
                    return;
                };
                if let Some(session) = &mut self.session {
                    match &mut session.inner.r#type {
                        orm::SessionType::DirectMessage(recipient) => {
                            assert!(recipient.id == new_recipient.id);
                            *recipient = new_recipient
                        }
                        orm::SessionType::GroupV1(_) => {
                            // Groups currently don't list recipients in this model.
                        }
                        orm::SessionType::GroupV2(_) => {
                            // Groups currently don't list recipients in this model.
                        }
                    }
                }
                // XXX how to trigger a Qt signal now?
            } else {
                tracing::debug!(
                    "Falling back to reloading the whole Session for event {:?}",
                    event
                );
                self.fetch(storage, session_id);
            }
        }
    }

    fn interests(&self) -> Vec<Interest> {
        self.session
            .iter()
            .flat_map(orm::AugmentedSession::interests)
            .chain(
                self.message_list
                    .pinned()
                    .borrow()
                    .messages
                    .iter()
                    .flat_map(orm::AugmentedMessage::interests),
            )
            .collect()
    }
}

impl SessionImpl {
    fn get_session_id(&self) -> i32 {
        self.session_id.unwrap_or(-1)
    }

    fn get_valid(&self) -> bool {
        self.session_id.is_some() && self.session.is_some()
    }

    fn fetch(&mut self, storage: Storage, id: i32) {
        self.session = storage.fetch_session_by_id_augmented(id);
        self.message_list
            .pinned()
            .borrow_mut()
            .load_all(storage, id);
    }

    fn set_session_id(&mut self, ctx: Option<ModelContext<Self>>, id: i32) {
        self.session_id = Some(id);
        if let Some(ctx) = ctx {
            self.fetch(ctx.storage(), id);
        }
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if let Some(id) = self.session_id {
            self.fetch(ctx.storage(), id);
        }
    }

    fn messages(&self) -> QVariant {
        self.message_list.pinned().into()
    }
}

define_model_roles! {
    enum MessageRoles for orm::AugmentedMessage {
        Id(id):                                               "id",
        SessionId(session_id):                                "sessionId",
        Message(text via qstring_from_option):                "message",
        StyledMessage(fn styled_message(&self) via qstring_from_cow): "styledMessage",
        Timestamp(server_timestamp via qdatetime_from_naive): "timestamp",

        SenderRecipientId(sender_recipient_id via qvariant_from_option): "senderRecipientId",

        Delivered(fn delivered(&self)):                       "delivered",
        Read(fn read(&self)):                                 "read", // How many recipient have received the message
        IsRead(is_read):                                      "isRead", // Is the message unread or read by self
        Viewed(fn viewed(&self)):                             "viewed",

        Sent(fn sent(&self)):                                 "sent",
        Flags(flags):                                         "flags",
        MessageType(message_type via qstring_from_option):    "messageType",
        Outgoing(is_outbound):                                "outgoing",
        Queued(fn queued(&self)):                             "queued",
        Failed(sending_has_failed):                           "failed",
        RemoteDeleted(is_remote_deleted):                     "remoteDeleted",

        Unidentified(use_unidentified):                       "unidentifiedSender",
        QuotedMessageId(quote_id via qvariant_from_option):   "quotedMessageId",
        HasSpoilers(fn has_spoilers(&self)):                  "hasSpoilers",
        HasStrikeThrough(fn has_strike_through(&self)):       "hasStrikeThrough",

        ExpiresIn(expires_in via int_from_i32_option):        "expiresIn",
        ExpiryStarted(expiry_started via qdatetime_from_naive_option): "expiryStarted",

        BodyRanges(fn body_ranges(&self) via body_ranges_qvariantlist): "bodyRanges",

        SpoilerTag(fn spoiler_tag(&self) via QString::from):  "spoilerTag",
        RevealedTag(fn revealed_tag(&self) via QString::from): "revealedTag",
        SpoilerLink(fn spoiler_link(&self) via QString::from): "spoilerLink",
        RevealedLink(fn revealed_link(&self) via QString::from): "revealedLink",

        Attachments(fn attachments(&self)):                   "attachments",
        Reactions(fn reactions(&self)):                       "reactions",
        IsVoiceNote(is_voice_note):                           "isVoiceNote",

        IsLatestRevision(fn is_latest_revision(&self)):       "isLatestRevision",
        IsEdited(fn is_edited(&self)):                        "isEdited",
    }
}

fn body_ranges_qvariantlist(
    body_ranges: &[whisperfish_store::body_ranges::BodyRange],
) -> QVariantList {
    body_ranges
        .iter()
        .map(|range| {
            use whisperfish_store::body_ranges::AssociatedValue;

            let mut qrange = QVariantMap::default();
            qrange.insert("start".into(), range.start.into());
            qrange.insert("length".into(), range.length.into());
            let mut associated_value = QVariantMap::default();
            match &range.associated_value {
                None => {}
                Some(AssociatedValue::MentionUuid(mention_aci)) => {
                    associated_value.insert("type".into(), QString::from("mention").to_qvariant());
                    associated_value.insert(
                        "mention".into(),
                        QString::from(mention_aci as &str).to_qvariant(),
                    );
                }
                Some(AssociatedValue::Style(style)) => {
                    associated_value.insert("type".into(), QString::from("style").to_qvariant());
                    associated_value.insert("style".into(), style.to_qvariant());
                }
                _ => {
                    tracing::warn!(
                        "unimplemented associated value: {:?}",
                        range.associated_value
                    );
                }
            }
            qrange.insert("associatedValue".into(), associated_value.to_qvariant());
            qrange.to_qvariant()
        })
        .collect()
}

#[derive(QObject, Default)]
pub struct MessageListModel {
    base: qt_base_class!(trait QAbstractListModel),
    messages: Vec<orm::AugmentedMessage>,
}

impl MessageListModel {
    fn load_all(&mut self, storage: Storage, id: i32) {
        self.begin_reset_model();
        self.messages = storage
            .fetch_all_messages_augmented(id, true)
            .into_iter()
            .map(Into::into)
            .collect();
        self.end_reset_model();
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn observe(&mut self, storage: Storage, session_id: i32, event: crate::store::observer::Event) {
        // Waterfall handling of event.  If we cannot find a good specialized way of handling
        // the event, we'll reload the whole model.
        let message_id = event
            .relation_key_for(schema::messages::table)
            .and_then(|x| x.as_i32())
            .expect("message-related event observation");
        if event.is_delete() && event.for_table(schema::messages::table) {
            if let Some((pos, _msg)) = self
                .messages
                .iter()
                .enumerate()
                .find(|(_, msg)| msg.id == message_id)
            {
                self.begin_remove_rows(pos as i32, pos as i32);
                self.messages.remove(pos);
                self.end_remove_rows();
                return;
            }
        } else if event.is_update_or_insert() {
            let message = storage
                .fetch_augmented_message(message_id)
                .expect("inserted message");
            if message.session_id != session_id {
                tracing::trace!("Ignoring message insert/update for different session.");
                return;
            }
            let pos = self.messages.binary_search_by_key(
                &std::cmp::Reverse((message.server_timestamp, message.id)),
                |message| std::cmp::Reverse((message.server_timestamp, message.id)),
            );
            match pos {
                Ok(existing_index) if !message.is_latest_revision() => {
                    // Update, but message is not the latest revision. Remove it.
                    tracing::debug!("Handling message edit. Removing edited message from view.");
                    self.begin_remove_rows(existing_index as i32, existing_index as i32);
                    self.messages.remove(existing_index);
                    self.end_remove_rows();
                }
                Ok(existing_index) => {
                    // Update, and message is the latest revision. Update it.
                    tracing::debug!("Handling update event.");
                    self.messages[existing_index] = message;
                    let idx = self.row_index(existing_index as i32);
                    self.data_changed(idx, idx);
                }
                Err(_insertion_index) if !message.is_latest_revision() => {
                    // Don't insert old revisions.
                    tracing::debug!("Handling message edit for an old edit, no-op.");
                }
                Err(insertion_index) => {
                    // Insert the message, because it's the latest revision.
                    tracing::debug!("Handling insertion event");
                    self.begin_insert_rows(insertion_index as i32, insertion_index as i32);
                    self.messages.insert(insertion_index, message);
                    self.end_insert_rows();
                }
            }
            return;
        }

        tracing::debug!(
            "Falling back to reloading the whole MessageListModel for event {:?}",
            event
        );
        self.load_all(storage, session_id);
    }
}

impl QAbstractListModel for MessageListModel {
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
