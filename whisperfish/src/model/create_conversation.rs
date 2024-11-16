#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::Storage;
use libsignal_service::protocol::Aci;
use phonenumber::PhoneNumber;
use qmetaobject::prelude::*;
use whisperfish_store::schema;

/// QML-constructable object that queries a session based on e164 or uuid, and creates it if
/// necessary.
#[observing_model]
#[derive(Default, QObject)]
pub struct CreateConversation {
    base: qt_base_class!(trait QObject),
    session_id: Option<i32>,
    // XXX What about PNI?
    uuid: Option<uuid::Uuid>,
    e164: Option<phonenumber::PhoneNumber>,
    name: Option<String>,

    #[qt_property(
        READ: get_session_id,
        NOTIFY: conversation_changed,
    )]
    sessionId: i32,
    #[qt_property(
        READ: get_uuid,
        WRITE: set_uuid,
        NOTIFY: conversation_changed,
        ALIAS: uuid,
    )]
    uuid_: QString,
    #[qt_property(
        READ: get_e164,
        WRITE: set_e164,
        NOTIFY: conversation_changed,
        ALIAS: e164,
    )]
    e164_: QString,
    #[qt_property(
        READ: get_name,
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
        // XXX Also invalid when lookup failed
        self.e164.is_none() && self.uuid.is_none()
    }

    fn fetch(&mut self, storage: Storage) {
        let recipient = if let Some(aci) = self.uuid {
            storage.fetch_recipient(&Aci::from(aci).into())
        } else if let Some(e164) = &self.e164 {
            storage.fetch_recipient_by_e164(e164)
        } else {
            tracing::trace!("Neither e164 nor uuid set; not fetching.");
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

    fn get_uuid(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.uuid
            .as_ref()
            .map(uuid::Uuid::to_string)
            .unwrap_or_default()
            .into()
    }

    fn set_uuid(&mut self, ctx: Option<ModelContext<Self>>, uuid: QString) {
        self.uuid = uuid::Uuid::parse_str(&uuid.to_string())
            // inspect_err https://github.com/rust-lang/rust/pull/91346 Rust 1.59 (unstable)
            //             https://github.com/rust-lang/rust/pull/116866 Rust 1.76 (stable)
            .map_err(|e| {
                tracing::error!("Parsing uuid: {}", e);
                e
            })
            .ok();
        self.e164 = None;
        if let Some(ctx) = ctx {
            self.fetch(ctx.storage());
        }
    }

    fn set_e164(&mut self, ctx: Option<ModelContext<Self>>, e164: QString) {
        self.e164 = phonenumber::parse(None, e164.to_string())
            // inspect_err https://github.com/rust-lang/rust/pull/91346 Rust 1.59 (unstable)
            //             https://github.com/rust-lang/rust/pull/116866 Rust 1.76 (stable)
            .map_err(|e| {
                tracing::error!("Parsing phone number: {}", e);
                e
            })
            .ok();
        self.uuid = None;
        if let Some(ctx) = ctx {
            self.fetch(ctx.storage());
        }
    }

    fn get_e164(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.e164
            .as_ref()
            .map(PhoneNumber::to_string)
            .unwrap_or_default()
            .into()
    }

    fn get_name(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.name.as_deref().unwrap_or_default().into()
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if self.e164.is_some() || self.uuid.is_some() {
            self.fetch(ctx.storage());
        }
    }
}
