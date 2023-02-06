#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::{orm, Storage};
use libsignal_service::prelude::protocol::SessionStoreExt;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use std::collections::HashMap;

/// QML-constructable object that interacts with a single recipient.
#[derive(Default, QObject)]
pub struct RecipientImpl {
    base: qt_base_class!(trait QObject),
    recipient_id: Option<i32>,
    recipient: Option<RecipientWithFingerprint>,
}

crate::observing_model! {
    pub struct Recipient(RecipientImpl) {
        recipientId: i32; READ get_recipient_id WRITE set_recipient_id,
        valid: bool; READ get_valid,
    } WITH OPTIONAL PROPERTIES FROM recipient WITH ROLE RecipientWithFingerprintRoles {
        id Id,
        uuid Uuid,
        // These two are aliases
        e164 E164,
        phoneNumber PhoneNumber,
        username Username,
        email Email,

        sessionFingerprint SessionFingerprint,

        blocked Blocked,

        name JoinedName,
        familyName FamilyName,
        givenName GivenName,

        about About,
        emoji Emoji,

        unidentifiedAccessMode UnidentifiedAccessModel,
        profileSharing ProfileSharing,
    }
}

impl EventObserving for RecipientImpl {
    fn observe(&mut self, storage: Storage, _event: crate::store::observer::Event) {
        if self.recipient_id.is_some() {
            self.init(storage);
        }
    }

    fn interests(&self) -> Vec<Interest> {
        self.recipient
            .iter()
            .flat_map(|r| r.inner.interests())
            .collect()
    }
}

impl RecipientImpl {
    fn get_recipient_id(&self) -> i32 {
        self.recipient_id.unwrap_or(-1)
    }

    fn get_valid(&self) -> bool {
        self.recipient_id.is_some() && self.recipient.is_some()
    }

    #[with_executor]
    fn set_recipient_id(&mut self, storage: Option<Storage>, id: i32) {
        self.recipient_id = Some(id);
        if let Some(storage) = storage {
            self.init(storage);
        }
    }

    fn init(&mut self, storage: Storage) {
        if let Some(id) = self.recipient_id {
            let recipient = if id >= 0 {
                storage
                    .fetch_recipient_by_id(id)
                    .map(|inner| RecipientWithFingerprint {
                        inner,
                        fingerprint: None,
                    })
            } else {
                None
            };
            self.recipient = recipient;
            // XXX trigger Qt signal for this?
        }
    }
}

#[derive(QObject, Default)]
pub struct RecipientListModel {
    base: qt_base_class!(trait QAbstractListModel),
    content: Vec<orm::Recipient>,
}

pub struct RecipientWithFingerprint {
    inner: orm::Recipient,
    fingerprint: Option<String>,
}

impl std::ops::Deref for RecipientWithFingerprint {
    type Target = orm::Recipient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RecipientListModel {}

define_model_roles! {
    pub(super) enum RecipientWithFingerprintRoles for RecipientWithFingerprint {
        Id(id): "id",
        Uuid(uuid via qstring_from_option): "uuid",
        // These two are aliases
        E164(e164 via qstring_from_option): "e164",
        PhoneNumber(e164 via qstring_from_option): "phoneNumber",
        Username(username via qstring_from_option): "username",
        Email(email via qstring_from_option): "email",

        Blocked(blocked): "blocked",

        JoinedName(profile_joined_name via qstring_from_option): "name",
        FamilyName(profile_family_name via qstring_from_option): "familyName",
        GivenName(profile_given_name via qstring_from_option): "givenName",

        About(about via qstring_from_option): "about",
        Emoji(about_emoji via qstring_from_option): "emoji",

        UnidentifiedAccessModel(unidentified_access_mode): "unidentifiedAccessMode",
        ProfileSharing(profile_sharing): "profileSharing",

        SessionFingerprint(fingerprint via qstring_from_option): "sessionFingerprint",
    }
}

define_model_roles! {
    pub(super) enum RecipientRoles for orm::Recipient {
        Id(id): "id",
        Uuid(uuid via qstring_from_option): "uuid",
        // These two are aliases
        E164(e164 via qstring_from_option): "e164",
        PhoneNumber(e164 via qstring_from_option): "phoneNumber",
        Username(username via qstring_from_option): "username",
        Email(email via qstring_from_option): "email",

        Blocked(blocked): "blocked",

        JoinedName(profile_joined_name via qstring_from_option): "name",
        FamilyName(profile_family_name via qstring_from_option): "familyName",
        GivenName(profile_given_name via qstring_from_option): "givenName",

        About(about via qstring_from_option): "about",
        Emoji(about_emoji via qstring_from_option): "emoji",

        UnidentifiedAccessModel(unidentified_access_mode): "unidentifiedAccessMode",
        ProfileSharing(profile_sharing): "profileSharing",
    }
}

impl QAbstractListModel for RecipientListModel {
    fn row_count(&self) -> i32 {
        self.content.len() as _
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        let role = RecipientRoles::from(role);
        role.get(&self.content[index.row() as usize])
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        RecipientRoles::role_names()
    }
}
