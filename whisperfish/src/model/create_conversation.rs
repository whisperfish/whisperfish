#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::Storage;
use libsignal_protocol::ServiceId;
use qmetaobject::prelude::*;
use whisperfish_store::schema;

/// QML-constructable object that queries a session based on a ServiceId,
/// and creates it if necessary.
#[observing_model]
#[derive(Default, QObject)]
pub struct CreateConversation {
    base: qt_base_class!(trait QObject),
    session_id: Option<i32>,
    service_id: Option<ServiceId>,
    name: Option<String>,

    #[qt_property(
        READ: get_session_id,
        NOTIFY: conversation_changed,
    )]
    sessionId: i32,

    #[qt_property(
        READ: get_service_id,
        WRITE: set_service_id,
        NOTIFY: conversation_changed,
        ALIAS: serviceId,
    )]
    service_id_: QString,

    #[qt_property(
        READ: get_name,
        WRITE: set_name,
        NOTIFY: conversation_changed,
    )]
    name_: QString,

    #[qt_property(
        READ: get_ready,
        NOTIFY: conversation_changed,
    )]
    ready: bool,
    #[qt_property(
        READ: get_invalid,
        NOTIFY: conversation_changed,
    )]
    invalid: bool,
    #[qt_property(
        READ: has_name,
        NOTIFY: conversation_changed,
    )]
    hasName: bool,

    conversation_changed: qt_signal!(),
}

impl EventObserving for CreateConversation {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, _event: crate::store::observer::Event) {
        let storage = ctx.storage();

        // If something changed
        self.fetch(storage);
    }

    fn interests(&self) -> Vec<Interest> {
        vec![Interest::whole_table(schema::sessions::table)]
    }
}

impl CreateConversation {
    fn get_session_id(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
        self.session_id.unwrap_or(-1)
    }

    fn has_name(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.name.is_some()
    }

    fn get_ready(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.session_id.is_some()
    }

    fn get_invalid(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.service_id.is_none()
    }

    fn fetch(&mut self, storage: Storage) {
        let recipient = if let Some(id) = &self.service_id {
            storage.fetch_recipient(id)
        } else {
            tracing::trace!("No service_id set; not fetching.");
            return;
        };

        let session = if let Some(recipient) = recipient {
            if let Some(name) = &recipient.profile_joined_name {
                self.name = Some(name.clone());
            } else if let Some(e164) = &recipient.e164 {
                self.name = Some(e164.to_string());
            }

            storage.fetch_or_insert_session_by_recipient_id(recipient.id)
        } else {
            // XXX This most probably requires interaction.
            tracing::warn!("Not creating new recipients through this method.");
            return;
        };
        self.session_id = Some(session.id);
        self.conversation_changed();
    }

    fn get_service_id(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.service_id
            .as_ref()
            .map(ServiceId::service_id_string)
            .unwrap_or_default()
            .into()
    }

    fn set_service_id(&mut self, ctx: Option<ModelContext<Self>>, uuid: QString) {
        self.service_id = ServiceId::parse_from_service_id_string(&uuid.to_string());
        self.session_id = None;
        if self.service_id.is_none() {
            tracing::error!("parsing service id");
        }

        if let Some(ctx) = ctx {
            self.fetch(ctx.storage());
        }
    }

    fn get_name(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.name.as_deref().unwrap_or_default().into()
    }

    fn set_name(&mut self, _ctx: Option<ModelContext<Self>>, name: QString) {
        self.name = Some(name.to_string());
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if self.service_id.is_some() {
            self.fetch(ctx.storage());
        }
    }
}
