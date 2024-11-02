#![allow(non_snake_case)]

use qttypes::QVariantMap;
use whisperfish_store::observer::{Event, EventObserving, Interest};
use whisperfish_store::orm;

use crate::model::*;
use crate::store::Storage;
use std::collections::HashMap;
use std::process::Command;

#[observing_model(
    properties_from_role(attachment: Option<AttachmentRoles> NOTIFY attachment_changed {
        r#type MimeType,
        data Data,
        original_name OriginalName,
        visual_hash VisualHash,
        is_voice_note IsVoiceNote,

        transcription Transcription,
    })
)]
#[derive(Default, QObject)]
pub struct Attachment {
    base: qt_base_class!(trait QObject),
    attachment_id: Option<i32>,
    attachment: Option<orm::Attachment>,

    #[qt_property(
        READ: get_attachment_id,
        WRITE: set_attachment_id,
    )]
    attachmentId: i32,
    #[qt_property(
        READ: get_valid
    )]
    valid: bool,

    attachment_changed: qt_signal!(),
}

impl Attachment {
    fn init(&mut self, ctx: ModelContext<Self>) {
        if let Some(id) = self.attachment_id {
            self.fetch(ctx.storage(), id);
        }
    }

    fn get_valid(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.attachment_id.is_some() && self.attachment.is_some()
    }

    fn get_attachment_id(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
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
        self.attachment_changed();
    }
}

impl EventObserving for Attachment {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, _event: Event) {
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
        Data(fn absolute_attachment_path(&self) via qstring_from_option): "data",
        OriginalName(file_name via qstring_from_option): "original_name",
        VisualHash(visual_hash via qstring_from_option): "visual_hash",
        IsVoiceNote(is_voice_note):                      "is_voice_note",
        Transcription(transcription via qstring_from_option): "transcription",
        Size(size via Option::unwrap_or_default):        "size",
        DownloadLength(download_length via Option::unwrap_or_default): "download_length",
        DownloadedPercentage(fn downloaded_percentage(&self)): "downloaded_percentage",
        IsDownloading(fn is_downloading(&self)):         "is_downloading",
        IsDownloaded(fn is_downloaded(&self)):           "is_downloaded",
        CanRetry(fn can_retry(&self)):                   "can_retry",
    }
}

#[derive(QObject, Default)]
pub struct AttachmentListModel {
    base: qt_base_class!(trait QAbstractListModel),
    pub(super) attachments: Vec<orm::Attachment>,

    count: qt_property!(i32; NOTIFY rowCountChanged READ row_count),

    /// Gets the nth item of the model
    get: qt_method!(fn(&self, idx: i32) -> QVariantMap),

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

    fn get(&self, idx: i32) -> QVariantMap {
        let mut map = QVariantMap::default();
        let idx = self.row_index(idx);

        for (k, v) in self.role_names() {
            map.insert(QString::from(v.to_string()), self.data(idx, k));
        }
        map
    }

    fn open(&mut self, idx: i32) {
        let attachment = if let Some(attachment) = self.attachments.get(idx as usize) {
            attachment
        } else {
            tracing::error!("[attachment] Message not found at index {}", idx);
            return;
        };
        let Some(attachment) = attachment.absolute_attachment_path() else {
            tracing::error!("[attachment] Opening attachment without path (idx {})", idx);
            return;
        };

        match Command::new("xdg-open").arg(attachment.as_ref()).status() {
            Ok(status) => {
                if !status.success() {
                    tracing::error!("[attachment] fail");
                }
            }
            Err(e) => {
                tracing::error!("[attachment] Error {}", e);
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
