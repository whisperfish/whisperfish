#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::{orm, Storage};
use std::collections::HashMap;
use std::process::Command;

#[derive(Default, QObject)]
pub struct AttachmentImpl {
    base: qt_base_class!(trait QObject),
    attachment_id: Option<i32>,
    attachment: Option<orm::Attachment>,
}

crate::observing_model! {
    pub struct Attachment(AttachmentImpl) {
        attachmentId: i32; READ get_attachment_id WRITE set_attachment_id,
        valid: bool; READ get_valid,
    } WITH OPTIONAL PROPERTIES FROM attachment WITH ROLE AttachmentRoles {
        r#type MimeType,
        data Data,
        original_name OriginalName,
        visual_hash VisualHash,
        is_voice_note IsVoiceNote,
    }
}

impl AttachmentImpl {
    fn init(&mut self, ctx: ModelContext<Self>) {
        if let Some(id) = self.attachment_id {
            self.fetch(ctx.storage(), id);
        }
    }

    fn get_valid(&self) -> bool {
        self.attachment_id.is_some() && self.attachment.is_some()
    }

    fn get_attachment_id(&self) -> i32 {
        self.attachment_id.unwrap_or(-1)
    }

    fn set_attachment_id(&mut self, ctx: Option<ModelContext<Self>>, id: i32) {
        self.attachment_id = Some(id);
        if let Some(ctx) = ctx {
            self.fetch(ctx.storage(), id);
        }
    }

    fn fetch(&mut self, storage: Storage, id: i32) {
        self.attachment = storage.fetch_attachment(id);
    }
}

impl EventObserving for AttachmentImpl {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, _event: crate::store::observer::Event) {
        if let Some(id) = self.attachment_id {
            self.fetch(ctx.storage(), id);
        }
    }

    fn interests(&self) -> Vec<Interest> {
        self.attachment
            .iter()
            .flat_map(orm::Attachment::interests)
            .collect()
    }
}

define_model_roles! {
    enum AttachmentRoles for orm::Attachment {
        // There's a lot more useful stuff to expose.
        Id(id):                                          "id",
        MimeType(content_type via QString::from):        "type",
        Data(attachment_path via qstring_from_option):   "data",
        OriginalName(file_name via qstring_from_option): "original_name",
        VisualHash(visual_hash via qstring_from_option): "visual_hash",
        IsVoiceNote(is_voice_note):                      "is_voice_note",
    }
}

#[derive(QObject, Default)]
pub struct AttachmentListModel {
    base: qt_base_class!(trait QAbstractListModel),
    pub(super) attachments: Vec<orm::Attachment>,

    count: qt_property!(i32; NOTIFY rowCountChanged READ row_count),

    /// Gets the nth item of the model, serialized as byte array
    get: qt_method!(fn(&self, idx: i32) -> QByteArray),

    open: qt_method!(fn(&self, idx: i32)),

    rowCountChanged: qt_signal!(),
}

impl AttachmentListModel {
    pub fn new(attachments: Vec<orm::Attachment>) -> Self {
        Self {
            attachments,
            ..Default::default()
        }
    }

    pub(super) fn set(&mut self, new: Vec<orm::Attachment>) {
        self.begin_reset_model();
        self.attachments = new;
        self.end_reset_model();

        self.rowCountChanged();
    }

    pub fn update_attachment(&mut self, attachment: orm::Attachment) {
        let result = self
            .attachments
            .iter_mut()
            .enumerate()
            .find(|(_i, a)| a.id == attachment.id);

        if let Some((idx, old_attachment)) = result {
            *old_attachment = attachment;

            let idx = self.row_index(idx as i32);
            self.data_changed(idx, idx);
        } else {
            self.begin_insert_rows(self.attachments.len() as i32, self.attachments.len() as i32);
            self.attachments.push(attachment);
            self.end_insert_rows();
        }
    }

    // XXX When we're able to run Rust 1.a-bit-more, with qmetaobject 0.2.7+, we have QVariantMap.
    fn get(&self, idx: i32) -> QByteArray {
        let mut map = qmetaobject::QJsonObject::default();

        for (k, v) in self.role_names() {
            let idx = self.row_index(idx);
            map.insert(
                v.to_str().expect("only utf8 role names"),
                self.data(idx, k).into(),
            );
        }

        map.to_json()
    }

    fn open(&mut self, idx: i32) {
        let attachment = if let Some(attachment) = self.attachments.get(idx as usize) {
            attachment
        } else {
            log::error!("[attachment] Message not found at index {}", idx);
            return;
        };
        let attachment = if let Some(path) = &attachment.attachment_path {
            path
        } else {
            log::error!("[attachment] Opening attachment without path (idx {})", idx);
            return;
        };

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
}

impl QAbstractListModel for AttachmentListModel {
    fn row_count(&self) -> i32 {
        self.attachments.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let role = AttachmentRoles::from(role);
        role.get(&self.attachments[index.row() as usize])
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        AttachmentRoles::role_names()
    }
}
