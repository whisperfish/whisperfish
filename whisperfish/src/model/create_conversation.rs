#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::{schema, Storage};
use phonenumber::PhoneNumber;
use qmetaobject::prelude::*;

/// QML-constructable object that queries a session based on e164 or uuid, and creates it if
/// necessary.
#[derive(Default, QObject)]
pub struct CreateConversationImpl {
    base: qt_base_class!(trait QObject),
    session_id: Option<i32>,
    uuid: Option<uuid::Uuid>,
    e164: Option<phonenumber::PhoneNumber>,
    name: Option<String>,
}

crate::observing_model! {
    pub struct CreateConversation(CreateConversationImpl) {
        sessionId: i32; READ get_session_id,
        uuid: QString; READ get_uuid WRITE set_uuid,
        e164: QString; READ get_e164 WRITE set_e164,
        ready: bool; READ get_ready,
        invalid: bool; READ get_invalid,
        hasName: bool; READ has_name,
        name: QString; READ get_name,
    }
}

impl EventObserving for CreateConversationImpl {
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

impl CreateConversationImpl {
    fn get_session_id(&self) -> i32 {
        self.session_id.unwrap_or(-1)
    }

    fn has_name(&self) -> bool {
        self.name.is_some()
    }

    fn get_ready(&self) -> bool {
        self.session_id.is_some()
    }

    fn get_invalid(&self) -> bool {
        // XXX Also invalid when lookup failed
        self.e164.is_none() && self.uuid.is_none()
    }

    fn fetch(&mut self, storage: Storage) {
        let recipient = if let Some(uuid) = self.uuid {
            storage.fetch_recipient_by_uuid(uuid)
        } else if let Some(e164) = &self.e164 {
            storage.fetch_recipient_by_phonenumber(e164)
        } else {
            log::trace!("Neither e164 nor uuid set; not fetching.");
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
            log::warn!("Not creating new recipients through this method.");
            return;
        };
        self.session_id = Some(session.id);
    }

    fn get_uuid(&self) -> QString {
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
                log::error!("Parsing uuid: {}", e);
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
                log::error!("Parsing phone number: {}", e);
                e
            })
            .ok();
        self.uuid = None;
        if let Some(ctx) = ctx {
            self.fetch(ctx.storage());
        }
    }

    fn get_e164(&self) -> QString {
        self.e164
            .as_ref()
            .map(PhoneNumber::to_string)
            .unwrap_or_default()
            .into()
    }

    fn get_name(&self) -> QString {
        self.name.as_deref().unwrap_or_default().into()
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if self.e164.is_some() || self.uuid.is_some() {
            self.fetch(ctx.storage());
        }
    }
}
