#![allow(non_snake_case)]

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::orm;
use futures::TryFutureExt;
use libsignal_service::protocol::SessionStore;
use libsignal_service::session_store::SessionStoreExt;
use libsignal_service::ServiceAddress;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use std::collections::HashMap;
use uuid::Uuid;

/// QML-constructable object that interacts with a single recipient.
#[observing_model(
    properties_from_role(recipient: Option<RecipientWithAnalyzedSessionRoles> NOTIFY recipient_changed {
        id Id,
        externalId ExternalId,
        directMessageSessionId DirectMessageSessionId,
        uuid Uuid,
        // These two are aliases
        e164 E164,
        phoneNumber PhoneNumber,
        username Username,
        email Email,

        blocked Blocked,

        name JoinedName,
        familyName FamilyName,
        givenName GivenName,

        about About,
        emoji Emoji,

        unidentifiedAccessMode UnidentifiedAccessMode,
        profileSharing ProfileSharing,

        isRegistered IsRegistered,
    })
)]
#[derive(Default, QObject)]
pub struct Recipient {
    base: qt_base_class!(trait QObject),
    recipient_id: Option<i32>,
    // XXX What about PNI?
    recipient_uuid: Option<Uuid>,
    recipient: Option<RecipientWithAnalyzedSession>,

    #[qt_property(
        READ: get_recipient_id,
        WRITE: set_recipient_id,
        NOTIFY: recipient_changed,
    )]
    recipientId: i32,

    #[qt_property(
        READ: get_recipient_uuid,
        WRITE: set_recipient_uuid,
        NOTIFY: recipient_changed,
    )]
    recipientUuid: String,

    #[qt_property(
        READ: get_fingerprint_needed,
        WRITE: set_fingerprint_needed,
        ALIAS: fingerprintNeeded,
        NOTIFY: recipient_changed,
    )]
    fingerprint_needed: bool,
    #[qt_property(
        READ: get_valid,
        NOTIFY: recipient_changed,
    )]
    valid: bool,
    force_init: bool,

    #[qt_property(
        NOTIFY: fingerprint_changed,
    )]
    fingerprint: String,
    versions: Vec<(u32, u32)>,

    #[qt_property(
        READ: session_is_post_quantum,
        NOTIFY: fingerprint_changed,
        ALIAS: sessionIsPostQuantum,
    )]
    session_is_post_quantum: bool,

    recipient_changed: qt_signal!(),
    fingerprint_changed: qt_signal!(),
}

impl EventObserving for Recipient {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, _event: crate::store::observer::Event) {
        tracing::trace!("Observer recipient re-init");
        self.force_init = true;
        self.init(ctx);
    }

    fn interests(&self) -> Vec<Interest> {
        self.recipient
            .iter()
            .flat_map(|r| r.inner.interests())
            .collect()
    }
}

impl Recipient {
    fn get_recipient_id(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
        self.recipient_id.unwrap_or(-1)
    }

    fn get_recipient_uuid(&self, _ctx: Option<ModelContext<Self>>) -> String {
        self.recipient_uuid
            .as_ref()
            .map(Uuid::to_string)
            .unwrap_or("".into())
    }

    fn get_valid(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.recipient_id.is_some() && self.recipient.is_some()
    }

    #[tracing::instrument(skip(self, ctx))]
    fn set_recipient_id(&mut self, ctx: Option<ModelContext<Self>>, id: i32) {
        if self.recipient_id == Some(id) {
            return;
        }
        self.recipient_id = Some(id);

        // Set in init()
        if self.recipient_uuid.take().is_some() {
            self.recipient_changed();
        }
        if let Some(ctx) = ctx {
            self.init(ctx);
        }
    }

    #[tracing::instrument(skip(self, ctx))]
    fn set_recipient_uuid(&mut self, ctx: Option<ModelContext<Self>>, uuid: String) {
        if self.recipient_uuid.map(|u| u.to_string()).as_ref() == Some(&uuid) {
            return;
        }
        if self.recipient_id.take().is_some() {
            // Set in init()
            self.recipient_changed();
        }
        if let Ok(uuid) = Uuid::parse_str(&uuid) {
            self.recipient_uuid = Some(uuid);
        } else {
            tracing::warn!("QML requested unparsable UUID");
            self.recipient_uuid = None;
        }
        if let Some(ctx) = ctx {
            self.init(ctx);
        }
    }

    #[with_executor]
    #[tracing::instrument(skip(self, ctx))]
    fn set_fingerprint_needed(&mut self, ctx: Option<ModelContext<Self>>, needed: bool) {
        self.fingerprint_needed = needed;
        if let Some(ctx) = ctx {
            self.compute_fingerprint(ctx);
        }
    }

    fn get_fingerprint_needed(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.fingerprint_needed
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if self.recipient.is_none() || self.force_init {
            let storage = ctx.storage();
            let recipient = if let Some(uuid) = self.recipient_uuid {
                storage
                    .fetch_recipient(&ServiceAddress::from_aci(uuid))
                    .map(|inner| {
                        let direct_message_recipient_id = storage
                            .fetch_session_by_recipient_id(inner.id)
                            .map(|session| session.id)
                            .unwrap_or(-1);
                        self.recipient_id = Some(inner.id);
                        // XXX trigger Qt signal for this?
                        RecipientWithAnalyzedSession {
                            inner,
                            direct_message_recipient_id,
                        }
                    })
            } else if let Some(id) = self.recipient_id {
                if id >= 0 {
                    storage.fetch_recipient_by_id(id).map(|inner| {
                        let direct_message_recipient_id = storage
                            .fetch_session_by_recipient_id(inner.id)
                            .map(|session| session.id)
                            .unwrap_or(-1);
                        // XXX Clean this up after #532
                        self.recipient_uuid = inner.uuid.or(Some(Uuid::nil()));
                        RecipientWithAnalyzedSession {
                            inner,
                            direct_message_recipient_id,
                        }
                    })
                } else {
                    None
                }
            } else {
                None
            };

            self.recipient = recipient;
            self.recipient_changed();

            self.update_interests();
        }

        if self.force_init {
            self.force_init = false;
            if self.recipient.is_some() {
                self.fingerprint = String::default();
                self.fingerprint_changed();
            }
        }

        if self.fingerprint_needed {
            self.compute_fingerprint(ctx);
        }
    }

    fn compute_fingerprint(&mut self, ctx: ModelContext<Self>) {
        let qptr = QPointer::from(&*self);

        if self.recipient.is_none() || !self.fingerprint.is_empty() {
            tracing::trace!("Not computing fingerprint");
            return;
        }

        tracing::trace!("Computing fingerprint");
        // If an ACI recipient was found, attempt to compute the fingerprint
        let recipient = self.recipient.as_ref().unwrap();
        if let Some(recipient_svc) = recipient.to_aci_service_address() {
            let storage = ctx.storage();
            let recipient_id = recipient.id;
            let compute = async move {
                let local_svc = storage.fetch_self_service_address_aci().expect("self ACI");
                let fingerprint = storage
                    .aci_storage()
                    .compute_safety_number(&local_svc, &recipient_svc)
                    .await?;
                let sessions = storage
                    .aci_storage()
                    .get_sub_device_sessions(&recipient_svc)
                    .await?;
                let mut versions = Vec::new();
                for device_id in sessions {
                    let session = storage
                        .aci_storage()
                        .load_session(&recipient_svc.to_protocol_address(device_id))
                        .await?;
                    let version = session
                        .map(|x| x.session_version())
                        .transpose()?
                        .unwrap_or(0);
                    versions.push((device_id, version));
                }

                // XXX This is possibly not alive anymore
                let Some(recipient) = qptr.as_pinned() else {
                    tracing::warn!("Recipient object is gone, dropping fingerprint");
                    return anyhow::Result::Ok(());
                };
                let mut recipient_model = recipient.borrow_mut();

                if let Some(recipient) = &mut recipient_model.recipient {
                    if recipient.id != recipient_id {
                        // Skip and drop data
                        return anyhow::Result::Ok(());
                    }
                    recipient_model.fingerprint = fingerprint;
                    recipient_model.versions = versions;
                    recipient_model.fingerprint_changed();
                }

                Result::<_, anyhow::Error>::Ok(())
            }
            .map_ok_or_else(|e| tracing::error!("Computing fingerprint: {}", e), |_| ());
            actix::spawn(compute);
        }
    }

    fn session_is_post_quantum(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        const KYBER_AWARE_MESSAGE_VERSION: u32 = 4;

        self.versions
            .iter()
            .all(|(_, version)| *version >= KYBER_AWARE_MESSAGE_VERSION)
    }
}

#[derive(QObject, Default)]
pub struct RecipientListModel {
    base: qt_base_class!(trait QAbstractListModel),
    content: Vec<orm::Recipient>,
}

pub struct RecipientWithAnalyzedSession {
    inner: orm::Recipient,
    direct_message_recipient_id: i32,
}

impl std::ops::Deref for RecipientWithAnalyzedSession {
    type Target = orm::Recipient;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl RecipientListModel {}

define_model_roles! {
    pub(super) enum RecipientWithAnalyzedSessionRoles for RecipientWithAnalyzedSession {
        Id(id): "id",
        ExternalId(external_id via qstring_from_option): "externalId",
        DirectMessageSessionId(direct_message_recipient_id): "directMessageSessionId",
        Uuid(uuid via qstring_from_optional_to_string): "uuid",
        // These two are aliases
        E164(e164 via qstring_from_optional_to_string): "e164",
        PhoneNumber(e164 via qstring_from_optional_to_string): "phoneNumber",
        Username(username via qstring_from_option): "username",
        Email(email via qstring_from_option): "email",
        IsRegistered(is_registered): "isRegistered",

        Blocked(blocked): "blocked",

        JoinedName(profile_joined_name via qstring_from_option): "name",
        FamilyName(profile_family_name via qstring_from_option): "familyName",
        GivenName(profile_given_name via qstring_from_option): "givenName",

        About(about via qstring_from_option): "about",
        Emoji(about_emoji via qstring_from_option): "emoji",

        UnidentifiedAccessMode(unidentified_access_mode via Into<i32>::into): "unidentifiedAccessMode",
        ProfileSharing(profile_sharing): "profileSharing",
    }
}

define_model_roles! {
    pub(super) enum RecipientRoles for orm::Recipient {
        Id(id): "id",
        ExternalId(external_id via qstring_from_option): "externalId",
        Uuid(uuid via qstring_from_optional_to_string): "uuid",
        // These two are aliases
        E164(e164 via qstring_from_optional_to_string): "e164",
        PhoneNumber(e164 via qstring_from_optional_to_string): "phoneNumber",
        Username(username via qstring_from_option): "username",
        Email(email via qstring_from_option): "email",

        Blocked(blocked): "blocked",

        JoinedName(profile_joined_name via qstring_from_option): "name",
        FamilyName(profile_family_name via qstring_from_option): "familyName",
        GivenName(profile_given_name via qstring_from_option): "givenName",

        About(about via qstring_from_option): "about",
        Emoji(about_emoji via qstring_from_option): "emoji",

        UnidentifiedAccessMode(unidentified_access_mode via Into<i32>::into): "unidentifiedAccessMode",
        ProfileSharing(profile_sharing): "profileSharing",

        IsRegistered(is_registered): "isRegistered",
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
