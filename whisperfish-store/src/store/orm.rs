use super::schema::*;
use chrono::prelude::*;
use diesel::sql_types::Integer;
use libsignal_service::prelude::*;
use libsignal_service::proto::GroupContextV2;
use phonenumber::PhoneNumber;
use std::borrow::Cow;
use std::fmt::{Display, Error, Formatter};
use std::time::Duration;

mod sql_types;
use sql_types::{OptionPhoneNumberString, OptionUuidString, UuidString};

#[derive(Queryable, Insertable, Debug, Clone)]
pub struct GroupV1 {
    pub id: String,
    pub name: String,
    pub expected_v2_id: Option<String>,
}

impl Display for GroupV1 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "GroupV1 {{ id: \"{}\", name: \"{}\" }}",
            shorten(&self.id, 12),
            &self.name
        )
    }
}

#[derive(Queryable, Insertable, Debug, Clone)]
pub struct GroupV1Member {
    pub group_v1_id: String,
    pub recipient_id: i32,
    pub member_since: Option<NaiveDateTime>,
}

#[derive(Queryable, Insertable, Debug, Clone)]
pub struct GroupV2 {
    pub id: String,
    pub name: String,

    pub master_key: String,
    pub revision: i32,

    pub invite_link_password: Option<Vec<u8>>,

    pub access_required_for_attributes: i32,
    pub access_required_for_members: i32,
    pub access_required_for_add_from_invite_link: i32,

    pub avatar: Option<String>,
    pub description: Option<String>,
}

impl Display for GroupV2 {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match &self.description {
            Some(desc) => write!(
                f,
                "GroupV2 {{ id: \"{}\", name: \"{}\", description: \"{}\" }}",
                shorten(&self.id, 12),
                &self.name,
                desc
            ),
            None => write!(
                f,
                "GroupV2 {{ id: \"{}\", name: \"{}\" }}",
                shorten(&self.id, 12),
                &self.name
            ),
        }
    }
}

#[derive(Queryable, Insertable, Debug, Clone)]
pub struct GroupV2Member {
    pub group_v2_id: String,
    pub recipient_id: i32,
    pub member_since: NaiveDateTime,
    pub joined_at_revision: i32,
    pub role: i32,
}

impl Display for GroupV2Member {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "GroupV2Member {{ group_v2_id: \"{}\", recipient_id: {}, member_since: \"{}\" }}",
            shorten(&self.group_v2_id, 12),
            &self.recipient_id,
            &self.member_since
        )
    }
}

#[derive(Queryable, Identifiable, Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub id: i32,
    pub session_id: i32,

    pub text: Option<String>,
    pub sender_recipient_id: Option<i32>,

    pub received_timestamp: Option<NaiveDateTime>,
    pub sent_timestamp: Option<NaiveDateTime>,
    pub server_timestamp: NaiveDateTime,
    pub is_read: bool,
    pub is_outbound: bool,
    pub flags: i32,
    pub expires_in: Option<i32>,
    pub expiry_started: Option<NaiveDateTime>,
    pub schedule_send_time: Option<NaiveDateTime>,
    pub is_bookmarked: bool,
    pub use_unidentified: bool,
    pub is_remote_deleted: bool,

    pub sending_has_failed: bool,

    pub quote_id: Option<i32>,

    pub story_type: StoryType,

    #[diesel(deserialize_as = OptionUuidString, serialize_as = OptionUuidString)]
    pub server_guid: Option<Uuid>,
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match (&self.text, &self.quote_id) {
            (Some(text), Some(quote_id)) => write!(
                f,
                "Message {{ id: {}, session_id: {}, text: \"{}\", quote_id: {} }}",
                &self.id,
                &self.session_id,
                shorten(text, 9),
                quote_id
            ),
            (None, Some(quote_id)) => write!(
                f,
                "Message {{ id: {}, session_id: {}, quote_id: {} }}",
                &self.id, &self.session_id, quote_id
            ),
            (Some(text), None) => write!(
                f,
                "Message {{ id: {}, session_id: {}, text: \"{}\" }}",
                &self.id,
                &self.session_id,
                shorten(text, 9),
            ),
            (None, None) => write!(
                f,
                "Message {{ id: {}, session_id: {} }}",
                &self.id, &self.session_id
            ),
        }
    }
}

impl Default for Message {
    fn default() -> Self {
        Self {
            id: Default::default(),
            session_id: Default::default(),
            text: Default::default(),
            sender_recipient_id: Default::default(),
            received_timestamp: Default::default(),
            sent_timestamp: Default::default(),
            server_timestamp: NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
            is_read: Default::default(),
            is_outbound: Default::default(),
            flags: Default::default(),
            expires_in: Default::default(),
            expiry_started: Default::default(),
            schedule_send_time: Default::default(),
            is_bookmarked: Default::default(),
            use_unidentified: Default::default(),
            is_remote_deleted: Default::default(),
            sending_has_failed: Default::default(),
            quote_id: Default::default(),
            story_type: StoryType::None,
            server_guid: Default::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, FromSqlRow, PartialEq, Eq, AsExpression)]
#[diesel(sql_type = Integer)]
#[repr(i32)]
pub enum UnidentifiedAccessMode {
    Unknown = 0,
    Disabled = 1,
    Enabled = 2,
    Unrestricted = 3,
}

impl std::convert::TryFrom<i32> for UnidentifiedAccessMode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::Unknown),
            1 => Ok(Self::Disabled),
            2 => Ok(Self::Enabled),
            3 => Ok(Self::Unrestricted),
            _ => Err(()),
        }
    }
}

impl From<UnidentifiedAccessMode> for i32 {
    fn from(value: UnidentifiedAccessMode) -> Self {
        value as i32
    }
}

#[derive(Queryable, Identifiable, Debug, Clone)]
pub struct Recipient {
    pub id: i32,
    #[diesel(deserialize_as = OptionPhoneNumberString)]
    pub e164: Option<PhoneNumber>,
    #[diesel(deserialize_as = OptionUuidString)]
    pub uuid: Option<Uuid>,
    pub username: Option<String>,
    pub email: Option<String>,
    pub blocked: bool,

    pub profile_key: Option<Vec<u8>>,
    pub profile_key_credential: Option<Vec<u8>>,

    pub profile_given_name: Option<String>,
    pub profile_family_name: Option<String>,
    pub profile_joined_name: Option<String>,
    pub signal_profile_avatar: Option<String>,
    pub profile_sharing: bool,

    pub last_profile_fetch: Option<NaiveDateTime>,

    pub storage_service_id: Option<Vec<u8>>,
    pub storage_proto: Option<Vec<u8>>,

    pub capabilities: i32,
    pub last_gv1_migrate_reminder: Option<NaiveDateTime>,
    pub last_session_reset: Option<NaiveDateTime>,

    pub about: Option<String>,
    pub about_emoji: Option<String>,

    pub is_registered: bool,
    pub unidentified_access_mode: UnidentifiedAccessMode,

    #[diesel(deserialize_as = OptionUuidString)]
    pub pni: Option<Uuid>,
    pub needs_pni_signature: bool,
}

impl Display for Recipient {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        let profile_joined_name = self.profile_joined_name.as_deref().unwrap_or_default();
        match (&self.e164, &self.uuid, &self.pni) {
            (Some(e164), Some(r_uuid), pni) => write!(
                f,
                "Recipient {{ id: {}, name: \"{}\", e164: \"{}\", uuid: \"{}\", pni: {} }}",
                &self.id,
                profile_joined_name,
                shorten(&e164.to_string(), 6),
                shorten(&r_uuid.to_string(), 9),
                if pni.is_some() {
                    "available"
                } else {
                    "unavailable"
                },
            ),
            (None, Some(r_uuid), pni) => write!(
                f,
                "Recipient {{ id: {}, name: \"{}\", uuid: \"{}\", pni: {} }}",
                &self.id,
                profile_joined_name,
                shorten(&r_uuid.to_string(), 9),
                if pni.is_some() {
                    "available"
                } else {
                    "unavailable"
                },
            ),
            // XXX: is this invalid?  PNI without ACI and E164 might actually be valid.
            (None, None, Some(pni)) => write!(
                f,
                "Recipient {{ id: {}, name: \"{}\", pni: \"{}\", INVALID }}",
                &self.id,
                profile_joined_name,
                shorten(&pni.to_string(), 9),
            ),
            // XXX: is this invalid?  PNI without ACI might actually be valid.
            (Some(e164), None, Some(pni)) => write!(
                f,
                "Recipient {{ id: {}, name: \"{}\", e164: \"{}\", pni: \"{}\", INVALID }}",
                &self.id,
                profile_joined_name,
                shorten(&e164.to_string(), 6),
                shorten(&pni.to_string(), 9),
            ),
            // XXX: is this invalid?  Phonenumber without ACI/PNI is unreachable atm,
            //      but only because of current technical limitations in WF
            (Some(e164), None, None) => write!(
                f,
                "Recipient {{ id: {}, name: \"{}\", e164: \"{}\", INVALID }}",
                &self.id,
                profile_joined_name,
                shorten(&e164.to_string(), 6),
            ),
            (None, None, None) => write!(
                f,
                "Recipient {{ id: {}, name: \"{}\", INVALID }}",
                &self.id, profile_joined_name
            ),
        }
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
#[diesel(primary_key(address, device_id))]
pub struct SessionRecord {
    pub address: String,
    pub device_id: i32,
    pub record: Vec<u8>,
}

impl Display for SessionRecord {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "SessionRecord {{ address: \"{}\", device_id: {} }}",
            shorten(&self.address, 9),
            &self.device_id
        )
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
#[diesel(primary_key(address))]
pub struct IdentityRecord {
    pub address: String,
    pub record: Vec<u8>,
}

impl Display for IdentityRecord {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "IdentityRecord {{ address: \"{}\" }}",
            shorten(&self.address, 9)
        )
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
pub struct SignedPrekey {
    pub id: i32,
    pub record: Vec<u8>,
}

impl Display for SignedPrekey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "SignedPrekey {{ id: {} }}", &self.id)
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
pub struct Prekey {
    pub id: i32,
    pub record: Vec<u8>,
}

impl Display for Prekey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "Prekey {{ id: {} }}", &self.id)
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
pub struct KyberPrekey {
    pub id: i32,
    pub record: Vec<u8>,
}

impl Display for KyberPrekey {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "KyberPrekey {{ id: {} }}", &self.id)
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
#[diesel(primary_key(address, device, distribution_id))]
pub struct SenderKeyRecord {
    pub address: String,
    pub device: i32,
    pub distribution_id: String,
    pub record: Vec<u8>,
    pub created_at: NaiveDateTime,
}

impl Display for SenderKeyRecord {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "SenderKeyRecord {{ address: \"{}\", device: {}, created_at: \"{}\" }}",
            shorten(&self.address, 9),
            &self.device,
            &self.created_at
        )
    }
}

impl Recipient {
    pub fn unidentified_access_key(&self) -> Option<[u8; 16]> {
        self.profile_key()
            .map(ProfileKey::create)
            .as_ref()
            .map(ProfileKey::derive_access_key)
    }

    // XXX should become ProfileKey
    pub fn profile_key(&self) -> Option<[u8; 32]> {
        if let Some(pk) = self.profile_key.as_ref() {
            if pk.len() != 32 {
                tracing::warn!("Profile key is {} bytes", pk.len());
                None
            } else {
                let mut key = [0u8; 32];
                key.copy_from_slice(pk);
                Some(key)
            }
        } else {
            None
        }
    }

    pub fn to_service_address(&self) -> Option<libsignal_service::ServiceAddress> {
        self.uuid
            .map(|uuid| libsignal_service::ServiceAddress { uuid })
    }

    pub fn uuid(&self) -> String {
        self.uuid.as_ref().map(Uuid::to_string).unwrap_or_default()
    }

    pub fn e164(&self) -> String {
        self.e164
            .as_ref()
            .map(PhoneNumber::to_string)
            .unwrap_or_default()
    }

    pub fn e164_or_uuid(&self) -> String {
        self.e164
            .as_ref()
            .map(PhoneNumber::to_string)
            .or_else(|| self.uuid.as_ref().map(Uuid::to_string))
            .expect("either uuid or e164")
    }

    pub fn name(&self) -> Cow<'_, str> {
        self.profile_joined_name
            .as_deref()
            .map(Cow::Borrowed)
            .unwrap_or_else(|| Cow::Owned(self.e164_or_uuid()))
    }
}

#[derive(Queryable, Debug, Clone)]
pub struct DbSession {
    pub id: i32,

    pub direct_message_recipient_id: Option<i32>,
    pub group_v1_id: Option<String>,
    pub group_v2_id: Option<String>,

    pub is_archived: bool,
    pub is_pinned: bool,

    pub is_silent: bool,
    pub is_muted: bool,

    pub draft: Option<String>,

    pub expiring_message_timeout: Option<i32>,
}

impl Display for DbSession {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match (&self.direct_message_recipient_id, &self.group_v2_id) {
            (Some(r_id), Some(g_id)) => write!(
                f,
                "DbSession {{ id: {}, direct_message_recipient_id: {}, group_v2_id: \"{}\", INVALID }}",
                &self.id, r_id, shorten(g_id, 12)
            ),
            (Some(r_id), None) => write!(
                f,
                "DbSession {{ id: {}, direct_message_recipient_id: {} }}",
                &self.id, r_id
            ),
            (_, Some(g_id)) => write!(
                f,
                "DbSession {{ id: {}, group_v2_id: \"{}\" }}",
                &self.id, shorten(g_id, 12)
            ),
            _ => write!(f, "DbSession {{ id: {}, INVALID }}", &self.id),
        }
    }
}

#[derive(Queryable, Debug, Clone)]
pub struct Attachment {
    pub id: i32,
    pub json: Option<String>,
    pub message_id: i32,
    pub content_type: String,
    pub name: Option<String>,
    pub content_disposition: Option<String>,
    pub content_location: Option<String>,
    pub attachment_path: Option<String>,
    pub is_pending_upload: bool,
    pub transfer_file_path: Option<String>,
    pub size: Option<i32>,
    pub file_name: Option<String>,
    pub unique_id: Option<String>,
    pub digest: Option<String>,
    pub is_voice_note: bool,
    pub is_borderless: bool,
    pub is_quote: bool,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub sticker_pack_id: Option<String>,
    pub sticker_pack_key: Option<Vec<u8>>,
    pub sticker_id: Option<i32>,
    pub sticker_emoji: Option<String>,
    pub data_hash: Option<Vec<u8>>,
    pub visual_hash: Option<String>,
    pub transform_properties: Option<String>,
    pub transfer_file: Option<String>,
    pub display_order: i32,
    pub upload_timestamp: NaiveDateTime,
    pub cdn_number: Option<i32>,
    pub caption: Option<String>,
    pub pointer: Option<Vec<u8>>,
}

impl Display for Attachment {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match (&self.size, &self.file_name) {
            (Some(size), Some(file_name)) => write!(
                f,
                "Attachment {{ id: {}, message_id: {}, content_type: \"{}\", size: {}, file_name: \"{}\", is_voice_note: {}, _is_sticker_pack: {} }}",
                &self.id, &self.message_id, &self.content_type, size, file_name, &self.is_voice_note, &self.sticker_pack_id.is_some()
            ),
            (Some(size), _) => write!(
                f,
                "Attachment {{ id: {}, message_id: {}, content_type: \"{}\", size: {}, is_voice_note: {}, _is_sticker_pack: {} }}",
                &self.id, &self.message_id, &self.content_type, size, &self.is_voice_note, &self.sticker_pack_id.is_some()
            ),
            (_, Some(file_name)) => write!(
                f,
                "Attachment {{ id: {}, message_id: {}, content_type: \"{}\", file_name: \"{}\", is_voice_note: {}, _is_sticker_pack: {} }}",
                &self.id, &self.message_id, &self.content_type, file_name, &self.is_voice_note, &self.sticker_pack_id.is_some()
            ),
            _ => write!(
                f,
                "Attachment {{ id: {}, message_id: {}, content_type: \"{}\", is_voice_note: {}, _is_sticker_pack: {} }}",
                &self.id, &self.message_id, &self.content_type, &self.is_voice_note, &self.sticker_pack_id.is_some()
            ),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Session {
    pub id: i32,

    pub is_archived: bool,
    pub is_pinned: bool,

    pub is_silent: bool,
    pub is_muted: bool,

    pub expiring_message_timeout: Option<Duration>,

    pub draft: Option<String>,
    pub r#type: SessionType,
}

impl Display for Session {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "Session {{ id: {}, _has_draft: {}, type: {} }}",
            &self.id,
            &self.draft.is_some(),
            &self.r#type,
        )
    }
}

#[derive(Queryable, Debug, Clone)]
pub struct Reaction {
    pub reaction_id: i32,
    pub message_id: i32,
    pub author: i32,
    pub emoji: String,
    pub sent_time: NaiveDateTime,
    pub received_time: NaiveDateTime,
}

impl Display for Reaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "Reaction {{ reaction_id: {}, message_id: {}, author: {}, emoji: \"{}\" }}",
            &self.reaction_id, &self.message_id, &self.author, &self.emoji,
        )
    }
}

#[derive(Queryable, Debug, Clone)]
pub struct GroupedReaction {
    pub message_id: i32,
    pub emoji: String,
    pub count: i64,
}

impl Display for GroupedReaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(f, "{{ \"{}\": {} }}", &self.emoji, &self.count,)
    }
}

#[derive(Queryable, Debug, Clone)]
pub struct Receipt {
    pub message_id: i32,
    pub recipient_id: i32,
    pub delivered: Option<NaiveDateTime>,
    pub read: Option<NaiveDateTime>,
    pub viewed: Option<NaiveDateTime>,
}

impl Session {
    pub fn is_dm(&self) -> bool {
        self.r#type.is_dm()
    }

    pub fn is_group(&self) -> bool {
        self.r#type.is_group_v1() || self.r#type.is_group_v2()
    }

    pub fn is_group_v1(&self) -> bool {
        self.r#type.is_group_v1()
    }

    pub fn is_group_v2(&self) -> bool {
        self.r#type.is_group_v2()
    }

    pub fn unwrap_dm(&self) -> &Recipient {
        self.r#type.unwrap_dm()
    }

    pub fn unwrap_group_v1(&self) -> &GroupV1 {
        self.r#type.unwrap_group_v1()
    }

    pub fn unwrap_group_v2(&self) -> &GroupV2 {
        self.r#type.unwrap_group_v2()
    }

    pub fn group_context_v2(&self) -> Option<GroupContextV2> {
        if let SessionType::GroupV2(group) = &self.r#type {
            let master_key = hex::decode(&group.master_key).expect("hex group id in db");
            Some(GroupContextV2 {
                master_key: Some(master_key),
                revision: Some(group.revision as u32),
                group_change: None,
            })
        } else {
            None
        }
    }
}

impl
    From<(
        DbSession,
        Option<Recipient>,
        Option<GroupV1>,
        Option<GroupV2>,
    )> for Session
{
    fn from(
        (session, recipient, groupv1, groupv2): (
            DbSession,
            Option<Recipient>,
            Option<GroupV1>,
            Option<GroupV2>,
        ),
    ) -> Session {
        assert_eq!(
            session.direct_message_recipient_id.is_some(),
            recipient.is_some(),
            "direct session requires recipient"
        );
        assert_eq!(
            session.group_v1_id.is_some(),
            groupv1.is_some(),
            "groupv1 session requires groupv1"
        );
        assert_eq!(
            session.group_v2_id.is_some(),
            groupv2.is_some(),
            "groupv2 session requires groupv2"
        );

        let t = match (recipient, groupv1, groupv2) {
            (Some(recipient), None, None) => SessionType::DirectMessage(recipient),
            (None, Some(gv1), None) => SessionType::GroupV1(gv1),
            (None, None, Some(gv2)) => SessionType::GroupV2(gv2),
            _ => unreachable!("case handled above"),
        };

        let DbSession {
            id,

            direct_message_recipient_id: _,
            group_v1_id: _,
            group_v2_id: _,

            is_archived,
            is_pinned,

            is_silent,
            is_muted,

            draft,

            expiring_message_timeout,
        } = session;
        Session {
            id,

            is_archived,
            is_pinned,

            is_silent,
            is_muted,

            draft,

            expiring_message_timeout: expiring_message_timeout
                .and_then(|i| if i == 0 { None } else { Some(i) })
                .map(|i| i as u64)
                .map(Duration::from_secs),

            r#type: t,
        }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::large_enum_variant)]
pub enum SessionType {
    // XXX clippy suggests to put Recipient, 322 bytes, on the heap.
    DirectMessage(Recipient),
    GroupV1(GroupV1),
    GroupV2(GroupV2),
}

impl Display for SessionType {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match self {
            SessionType::DirectMessage(recipient) => {
                write!(f, "DirectMessage {{ recipient: {} }}", recipient)
            }
            SessionType::GroupV1(group) => {
                write!(f, "GroupV1 {{ group: {} }}", group)
            }
            SessionType::GroupV2(group) => {
                write!(f, "GroupV2 {{ group: {} }}", group)
            }
        }
    }
}

impl SessionType {
    pub fn is_dm(&self) -> bool {
        matches!(self, Self::DirectMessage(_))
    }

    pub fn is_group_v1(&self) -> bool {
        matches!(self, Self::GroupV1(_))
    }

    pub fn is_group_v2(&self) -> bool {
        matches!(self, Self::GroupV2(_))
    }

    pub fn unwrap_dm(&self) -> &Recipient {
        assert!(self.is_dm(), "unwrap panicked at unwrap_dm()");
        match self {
            Self::DirectMessage(r) => r,
            _ => unreachable!(),
        }
    }

    pub fn unwrap_group_v1(&self) -> &GroupV1 {
        assert!(self.is_group_v1(), "unwrap panicked at unwrap_group_v1()");
        match self {
            Self::GroupV1(g) => g,
            _ => unreachable!(),
        }
    }

    pub fn unwrap_group_v2(&self) -> &GroupV2 {
        assert!(self.is_group_v2(), "unwrap panicked at unwrap_group_v2()");
        match self {
            Self::GroupV2(g) => g,
            _ => unreachable!(),
        }
    }
}

// Some extras

/// [`Message`] augmented with its sender, attachment count and receipts.
#[derive(Clone, Default)]
pub struct AugmentedMessage {
    pub inner: Message,
    pub attachments: usize,
    pub is_voice_note: bool,
    pub receipts: Vec<(Receipt, Recipient)>,
}

impl Display for AugmentedMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "AugmentedMessage {{ attachments: {}, _receipts: {}, inner: {} }}",
            &self.attachments,
            &self.receipts.len(),
            &self.inner
        )
    }
}

impl std::ops::Deref for AugmentedMessage {
    type Target = Message;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl AugmentedMessage {
    pub fn sent(&self) -> bool {
        self.inner.sent_timestamp.is_some()
    }

    pub fn delivered(&self) -> u32 {
        self.receipts
            .iter()
            .filter(|(r, _)| r.delivered.is_some())
            .count() as _
    }

    pub fn read(&self) -> u32 {
        self.receipts
            .iter()
            .filter(|(r, _)| r.read.is_some())
            .count() as _
    }

    pub fn viewed(&self) -> u32 {
        self.receipts
            .iter()
            .filter(|(r, _)| r.viewed.is_some())
            .count() as _
    }

    pub fn queued(&self) -> bool {
        self.is_outbound && self.sent_timestamp.is_none() && !self.sending_has_failed
    }

    pub fn attachments(&self) -> u32 {
        self.attachments as _
    }
}

pub struct AugmentedSession {
    pub inner: Session,
    pub last_message: Option<AugmentedMessage>,
}

impl Display for AugmentedSession {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        match &self.last_message {
            Some(message) => write!(
                f,
                "AugmentedSession {{ inner: {}, last_message: {} }}",
                &self.inner, message
            ),
            None => write!(
                f,
                "AugmentedSession {{ inner: {}, last_message: None }}",
                &self.inner
            ),
        }
    }
}

impl std::ops::Deref for AugmentedSession {
    type Target = Session;

    fn deref(&self) -> &Session {
        &self.inner
    }
}

impl AugmentedSession {
    pub fn timestamp(&self) -> Option<NaiveDateTime> {
        self.last_message.as_ref().map(|m| m.inner.server_timestamp)
    }

    pub fn group_name(&self) -> Option<&str> {
        match &self.inner.r#type {
            SessionType::GroupV1(group) => Some(&group.name),
            SessionType::GroupV2(group) => Some(&group.name),
            SessionType::DirectMessage(_) => None,
        }
    }

    pub fn group_description(&self) -> Option<String> {
        match &self.inner.r#type {
            SessionType::GroupV1(_) => None,
            SessionType::GroupV2(group) => group.description.to_owned(),
            SessionType::DirectMessage(_) => None,
        }
    }

    pub fn group_id(&self) -> Option<&str> {
        match &self.inner.r#type {
            SessionType::GroupV1(group) => Some(&group.id),
            SessionType::GroupV2(group) => Some(&group.id),
            SessionType::DirectMessage(_) => None,
        }
    }

    pub fn sent(&self) -> bool {
        if let Some(m) = &self.last_message {
            m.sent_timestamp.is_some()
        } else {
            false
        }
    }

    pub fn recipient_id(&self) -> i32 {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => -1,
            SessionType::GroupV2(_group) => -1,
            SessionType::DirectMessage(recipient) => recipient.id,
        }
    }

    pub fn recipient_name(&self) -> &str {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => "",
            SessionType::GroupV2(_group) => "",
            SessionType::DirectMessage(recipient) => {
                recipient.profile_joined_name.as_deref().unwrap_or_default()
            }
        }
    }

    pub fn recipient_uuid(&self) -> Cow<'_, str> {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => "".into(),
            SessionType::GroupV2(_group) => "".into(),
            SessionType::DirectMessage(recipient) => recipient.uuid().into(),
        }
    }

    pub fn recipient_e164(&self) -> Cow<'_, str> {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => "".into(),
            SessionType::GroupV2(_group) => "".into(),
            SessionType::DirectMessage(recipient) => recipient.e164().into(),
        }
    }

    pub fn recipient_emoji(&self) -> &str {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => "",
            SessionType::GroupV2(_group) => "",
            SessionType::DirectMessage(recipient) => {
                recipient.about_emoji.as_deref().unwrap_or_default()
            }
        }
    }

    pub fn recipient_about(&self) -> &str {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => "",
            SessionType::GroupV2(_group) => "",
            SessionType::DirectMessage(recipient) => recipient.about.as_deref().unwrap_or_default(),
        }
    }

    pub fn has_avatar(&self) -> bool {
        match &self.r#type {
            SessionType::GroupV1(_) => false,
            SessionType::GroupV2(group) => group.avatar.is_some(),
            SessionType::DirectMessage(recipient) => recipient.signal_profile_avatar.is_some(),
        }
    }

    pub fn is_registered(&self) -> bool {
        match &self.inner.r#type {
            SessionType::GroupV1(_group) => true,
            SessionType::GroupV2(_group) => true,
            SessionType::DirectMessage(recipient) => recipient.is_registered,
        }
    }

    pub fn has_attachment(&self) -> bool {
        if let Some(m) = &self.last_message {
            m.attachments > 0
        } else {
            false
        }
    }

    pub fn is_voice_note(&self) -> bool {
        if let Some(m) = &self.last_message {
            m.is_voice_note
        } else {
            false
        }
    }

    pub fn draft(&self) -> String {
        self.draft.clone().unwrap_or_default()
    }

    pub fn last_message_text(&self) -> Option<&str> {
        self.last_message.as_ref().and_then(|m| m.text.as_deref())
    }

    pub fn section(&self) -> String {
        if self.is_pinned {
            return String::from("pinned");
        }

        // XXX: stub
        let now = chrono::Utc::now();
        let today = Utc
            .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
            .unwrap()
            .naive_utc();

        let last_message = if let Some(m) = &self.last_message {
            &m.inner
        } else {
            return String::from("today");
        };
        let diff = today.signed_duration_since(last_message.server_timestamp);

        if diff.num_seconds() <= 0 {
            String::from("today")
        } else if diff.num_hours() <= 24 {
            String::from("yesterday")
        } else if diff.num_hours() <= (7 * 24) {
            let wd = last_message.server_timestamp.weekday().number_from_monday() % 7;
            wd.to_string()
        } else {
            String::from("older")
        }
    }

    pub fn is_remote_deleted(&self) -> bool {
        self.last_message
            .as_ref()
            .map(|m| m.is_remote_deleted)
            .unwrap_or(false)
    }

    pub fn is_read(&self) -> bool {
        self.last_message
            .as_ref()
            .map(|m| m.is_read)
            .unwrap_or(true)
    }

    pub fn delivered(&self) -> u32 {
        if let Some(m) = &self.last_message {
            m.receipts
                .iter()
                .filter(|(r, _)| r.delivered.is_some())
                .count() as _
        } else {
            0
        }
    }

    pub fn read(&self) -> u32 {
        if let Some(m) = &self.last_message {
            m.receipts.iter().filter(|(r, _)| r.read.is_some()).count() as _
        } else {
            0
        }
    }

    pub fn is_muted(&self) -> bool {
        self.is_muted
    }

    pub fn is_archived(&self) -> bool {
        self.is_archived
    }

    pub fn is_pinned(&self) -> bool {
        self.is_pinned
    }

    pub fn viewed(&self) -> u32 {
        if let Some(m) = &self.last_message {
            m.receipts
                .iter()
                .filter(|(r, _)| r.viewed.is_some())
                .count() as _
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, Debug, FromSqlRow, PartialEq, Eq, AsExpression)]
#[diesel(sql_type = Integer)]
#[repr(i32)]
pub enum StoryType {
    None = 0,
    StoryWithReplies = 1,
    StoryWithoutReplies = 2,
    TextStoryWithReplies = 3,
    TextStoryWithoutReplies = 4,
}

impl std::convert::TryFrom<i32> for StoryType {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::None),
            1 => Ok(Self::StoryWithReplies),
            2 => Ok(Self::StoryWithoutReplies),
            3 => Ok(Self::TextStoryWithReplies),
            4 => Ok(Self::TextStoryWithoutReplies),
            _ => Err(()),
        }
    }
}

impl From<StoryType> for i32 {
    fn from(value: StoryType) -> Self {
        value as i32
    }
}

impl StoryType {
    pub fn from_params(allows_replies: bool, text_attachment: bool) -> Self {
        match (allows_replies, text_attachment) {
            (false, false) => Self::StoryWithoutReplies,
            (true, false) => Self::StoryWithReplies,
            (false, true) => Self::TextStoryWithoutReplies,
            (true, true) => Self::TextStoryWithReplies,
        }
    }
}

#[derive(Clone, Copy, Debug, FromSqlRow, PartialEq, Eq, AsExpression)]
#[diesel(sql_type = Integer)]
#[repr(i32)]
pub enum DistributionListPrivacyMode {
    OnlyWith = 0,
    AllExcept = 1,
    All = 2,
}

impl std::convert::TryFrom<i32> for DistributionListPrivacyMode {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            0 => Ok(Self::OnlyWith),
            1 => Ok(Self::AllExcept),
            2 => Ok(Self::All),
            _ => Err(()),
        }
    }
}

impl From<DistributionListPrivacyMode> for i32 {
    fn from(value: DistributionListPrivacyMode) -> Self {
        value as i32
    }
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
#[diesel(primary_key(distribution_id))]
pub struct DistributionList {
    pub name: String,
    #[diesel(deserialize_as = UuidString, serialize_as = UuidString)]
    pub distribution_id: Uuid,
    pub session_id: Option<i32>,
    pub allows_replies: bool,
    pub deletion_timestamp: Option<NaiveDateTime>,
    pub is_unknown: bool,
    pub privacy_mode: DistributionListPrivacyMode,
}

#[derive(Queryable, Identifiable, Insertable, Debug, Clone)]
#[diesel(primary_key(distribution_id, session_id))]
pub struct DistributionListMember {
    #[diesel(deserialize_as = UuidString, serialize_as = UuidString)]
    pub distribution_id: Uuid,
    pub session_id: i32,
    pub privacy_mode: DistributionListPrivacyMode,
}

pub fn shorten(text: &str, limit: usize) -> std::borrow::Cow<'_, str> {
    let limit = text
        .char_indices()
        .map(|(i, _)| i)
        .nth(limit)
        .unwrap_or(text.len());
    if text.len() > limit {
        format!("{}...", &text[..limit]).into()
    } else {
        text.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Helpers //

    fn get_group_v1() -> GroupV1 {
        GroupV1 {
            id: "cba".into(),
            name: "G1".into(),
            expected_v2_id: None,
        }
    }

    fn get_group_v2() -> GroupV2 {
        GroupV2 {
            id: "abc".into(),
            name: "G2".into(),
            master_key: "123".into(),
            revision: 42,
            invite_link_password: None,
            access_required_for_add_from_invite_link: 0,
            access_required_for_attributes: 0,
            access_required_for_members: 0,
            avatar: None,
            description: Some("desc".into()),
        }
    }

    fn get_message() -> Message {
        Message {
            id: 71,
            text: Some("msg text".into()),
            session_id: 66,
            server_timestamp: NaiveDateTime::parse_from_str(
                "2023-03-31 14:51:25",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            ..Default::default()
        }
    }

    fn get_recipient() -> Recipient {
        Recipient {
            id: 981,
            e164: Some(phonenumber::parse(None, "+358401010101").unwrap()),
            uuid: Some(Uuid::parse_str("bff93979-a0fa-41f5-8ccf-e319135384d8").unwrap()),
            pni: None,
            username: Some("nick".into()),
            email: None,
            blocked: false,
            profile_key: None,
            profile_key_credential: None,
            profile_given_name: None,
            profile_family_name: None,
            profile_joined_name: Some("Nick Name".into()),
            signal_profile_avatar: None,
            profile_sharing: true,
            last_profile_fetch: None,
            unidentified_access_mode: UnidentifiedAccessMode::Enabled,
            storage_service_id: None,
            storage_proto: None,
            capabilities: 0,
            last_gv1_migrate_reminder: None,
            last_session_reset: None,
            about: Some("About me!".into()),
            about_emoji: Some("🦊".into()),
            is_registered: true,
            needs_pni_signature: false,
        }
    }

    fn get_attachment() -> Attachment {
        Attachment {
            id: 24,
            json: None,
            message_id: 313,
            content_type: "image/jpeg".into(),
            name: Some("Cat!".into()),
            content_disposition: None,
            content_location: None,
            attachment_path: None,
            is_pending_upload: false,
            transfer_file_path: None,
            size: Some(963012),
            file_name: Some("cat.jpg".into()),
            unique_id: None,
            digest: None,
            is_voice_note: false,
            is_borderless: false,
            is_quote: false,
            width: Some(1024),
            height: Some(768),
            sticker_pack_id: None,
            sticker_pack_key: None,
            sticker_id: None,
            sticker_emoji: None,
            data_hash: None,
            visual_hash: None,
            transform_properties: None,
            transfer_file: None,
            display_order: 1,
            upload_timestamp: NaiveDateTime::parse_from_str(
                "2023-04-01 07:01:32",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
            cdn_number: None,
            caption: Some("Funny cat!".into()),
            pointer: None,
        }
    }

    fn get_dm_session() -> Session {
        Session {
            id: 2,
            is_archived: false,
            is_pinned: false,
            is_silent: false,
            is_muted: false,
            expiring_message_timeout: None,
            draft: None,
            r#type: SessionType::DirectMessage(get_recipient()),
        }
    }

    fn get_gv2_session() -> Session {
        Session {
            id: 2,
            is_archived: false,
            is_pinned: false,
            is_silent: false,
            is_muted: false,
            expiring_message_timeout: None,
            draft: None,
            r#type: SessionType::GroupV2(get_group_v2()),
        }
    }

    fn get_augmented_message() -> AugmentedMessage {
        let timestamp =
            NaiveDateTime::parse_from_str("2023-04-01 07:01:32", "%Y-%m-%d %H:%M:%S").unwrap();
        AugmentedMessage {
            attachments: 2,
            inner: get_message(),
            is_voice_note: false,
            receipts: vec![(
                Receipt {
                    message_id: 1,
                    recipient_id: 2,
                    delivered: Some(timestamp),
                    read: Some(timestamp),
                    viewed: Some(timestamp),
                },
                get_recipient(),
            )],
        }
    }

    // Tests //

    #[test]
    fn display_groupv1() {
        let g1 = get_group_v1();
        assert_eq!(format!("{}", g1), "GroupV1 { id: \"cba\", name: \"G1\" }");
    }

    #[test]
    fn display_groupv2() {
        let mut g2 = get_group_v2();
        assert_eq!(
            format!("{}", g2),
            "GroupV2 { id: \"abc\", name: \"G2\", description: \"desc\" }"
        );
        g2.description = None;
        assert_eq!(format!("{}", g2), "GroupV2 { id: \"abc\", name: \"G2\" }");
    }

    #[test]
    fn display_groupv2_member() {
        let datetime =
            NaiveDateTime::parse_from_str("2023-03-31 14:51:25", "%Y-%m-%d %H:%M:%S").unwrap();
        let g2m = GroupV2Member {
            group_v2_id: "id".into(),
            recipient_id: 22,
            member_since: datetime,
            joined_at_revision: 999,
            role: 2,
        };
        assert_eq!(format!("{}",g2m), "GroupV2Member { group_v2_id: \"id\", recipient_id: 22, member_since: \"2023-03-31 14:51:25\" }");
    }

    #[test]
    fn display_message() {
        let mut m = get_message();
        assert_eq!(
            format!("{}", m),
            "Message { id: 71, session_id: 66, text: \"msg text\" }"
        );
        m.text = None;
        assert_eq!(format!("{}", m), "Message { id: 71, session_id: 66 }");
        m.quote_id = Some(87);
        assert_eq!(
            format!("{}", m),
            "Message { id: 71, session_id: 66, quote_id: 87 }"
        );
        m.text = Some("wohoo".into());
        assert_eq!(
            format!("{}", m),
            "Message { id: 71, session_id: 66, text: \"wohoo\", quote_id: 87 }"
        );

        m.text = Some("Onks yhtää jäitä pakkases?".into());
        // Some characters are >1 bytes long
        assert_eq!(
            format!("{}", m),
            "Message { id: 71, session_id: 66, text: \"Onks yhtä...\", quote_id: 87 }"
        );
    }

    #[test]
    fn display_recipient() {
        let mut r = get_recipient();
        assert_eq!(format!("{}", r), "Recipient { id: 981, name: \"Nick Name\", e164: \"+35840...\", uuid: \"bff93979-...\", pni: unavailable }");
        r.e164 = None;
        assert_eq!(
            format!("{}", r),
            "Recipient { id: 981, name: \"Nick Name\", uuid: \"bff93979-...\", pni: unavailable }"
        );
        r.uuid = None;
        r.profile_joined_name = None;
        assert_eq!(
            format!("{}", r),
            "Recipient { id: 981, name: \"\", INVALID }"
        );
        r.e164 = Some(phonenumber::parse(None, "+358401010102").unwrap());
        assert_eq!(
            format!("{}", r),
            "Recipient { id: 981, name: \"\", e164: \"+35840...\", INVALID }"
        );
    }

    #[test]
    fn display_session_record() {
        let s = SessionRecord {
            address: "something".into(),
            device_id: 2,
            record: vec![65],
        };
        assert_eq!(
            format!("{}", s),
            "SessionRecord { address: \"something\", device_id: 2 }"
        )
    }

    #[test]
    fn display_identity_record() {
        let s = IdentityRecord {
            address: "something".into(),
            record: vec![65],
        };
        assert_eq!(
            format!("{}", s),
            "IdentityRecord { address: \"something\" }"
        )
    }

    #[test]
    fn display_signed_prekey() {
        let s = SignedPrekey {
            id: 2,
            record: vec![65],
        };
        assert_eq!(format!("{}", s), "SignedPrekey { id: 2 }")
    }

    #[test]
    fn display_prekey() {
        let s = Prekey {
            id: 2,
            record: vec![65],
        };
        assert_eq!(format!("{}", s), "Prekey { id: 2 }")
    }

    #[test]
    fn display_sender_key_record() {
        let datetime =
            NaiveDateTime::parse_from_str("2023-04-01 07:01:32", "%Y-%m-%d %H:%M:%S").unwrap();
        let s = SenderKeyRecord {
            address: "whateva".into(),
            record: vec![65],
            device: 13,
            distribution_id: "huh".into(),
            created_at: datetime,
        };
        assert_eq!(format!("{}", s), "SenderKeyRecord { address: \"whateva\", device: 13, created_at: \"2023-04-01 07:01:32\" }")
    }

    #[test]
    pub fn display_db_session() {
        let mut s = DbSession {
            id: 55,
            direct_message_recipient_id: Some(413),
            group_v1_id: None,
            group_v2_id: Some("gv2_id".into()),
            is_archived: false,
            is_pinned: false,
            is_silent: false,
            is_muted: false,
            draft: None,
            expiring_message_timeout: None,
        };
        assert_eq!(
            format!("{}", s),
            "DbSession { id: 55, direct_message_recipient_id: 413, group_v2_id: \"gv2_id\", INVALID }"
        );
        s.direct_message_recipient_id = None;
        assert_eq!(
            format!("{}", s),
            "DbSession { id: 55, group_v2_id: \"gv2_id\" }"
        );
        s.group_v2_id = None;
        assert_eq!(format!("{}", s), "DbSession { id: 55, INVALID }");
        s.direct_message_recipient_id = Some(777);
        assert_eq!(
            format!("{}", s),
            "DbSession { id: 55, direct_message_recipient_id: 777 }"
        );
    }

    #[test]
    fn display_attachment() {
        let mut a = get_attachment();
        assert_eq!(format!("{}", a), "Attachment { id: 24, message_id: 313, content_type: \"image/jpeg\", size: 963012, file_name: \"cat.jpg\", is_voice_note: false, _is_sticker_pack: false }");
        a.size = None;
        assert_eq!(format!("{}", a), "Attachment { id: 24, message_id: 313, content_type: \"image/jpeg\", file_name: \"cat.jpg\", is_voice_note: false, _is_sticker_pack: false }");
        a.file_name = None;
        assert_eq!(format!("{}", a), "Attachment { id: 24, message_id: 313, content_type: \"image/jpeg\", is_voice_note: false, _is_sticker_pack: false }");
        a.size = Some(0);
        assert_eq!(format!("{}", a), "Attachment { id: 24, message_id: 313, content_type: \"image/jpeg\", size: 0, is_voice_note: false, _is_sticker_pack: false }");
    }

    #[test]
    fn display_session() {
        let mut s = get_dm_session();
        assert_eq!(format!("{}", s), "Session { id: 2, _has_draft: false, type: DirectMessage { recipient: Recipient { id: 981, name: \"Nick Name\", e164: \"+35840...\", uuid: \"bff93979-...\", pni: unavailable } } }");
        s.r#type = SessionType::GroupV1(get_group_v1());
        assert_eq!(format!("{}", s), "Session { id: 2, _has_draft: false, type: GroupV1 { group: GroupV1 { id: \"cba\", name: \"G1\" } } }");
        s.r#type = SessionType::GroupV2(get_group_v2());
        assert_eq!(format!("{}", s), "Session { id: 2, _has_draft: false, type: GroupV2 { group: GroupV2 { id: \"abc\", name: \"G2\", description: \"desc\" } } }");
    }

    #[test]
    fn display_reaction() {
        let r = Reaction {
            reaction_id: 1,
            message_id: 86,
            author: 5,
            emoji: "🦊".into(),
            sent_time: NaiveDateTime::parse_from_str("2023-04-01 09:03:18", "%Y-%m-%d %H:%M:%S")
                .unwrap(),
            received_time: NaiveDateTime::parse_from_str(
                "2023-04-01 09:03:21",
                "%Y-%m-%d %H:%M:%S",
            )
            .unwrap(),
        };
        assert_eq!(
            format!("{}", r),
            "Reaction { reaction_id: 1, message_id: 86, author: 5, emoji: \"🦊\" }"
        );
    }

    #[test]
    fn display_augmented_message() {
        let m = get_augmented_message();
        assert_eq!(format!("{}", m), "AugmentedMessage { attachments: 2, _receipts: 1, inner: Message { id: 71, session_id: 66, text: \"msg text\" } }")
    }

    #[test]
    fn display_augmented_session() {
        let mut s = AugmentedSession {
            inner: get_dm_session(),
            last_message: Some(get_augmented_message()),
        };
        assert_eq!(format!("{}", s), "AugmentedSession { inner: Session { id: 2, _has_draft: false, type: DirectMessage { recipient: Recipient { id: 981, name: \"Nick Name\", e164: \"+35840...\", uuid: \"bff93979-...\", pni: unavailable } } }, last_message: AugmentedMessage { attachments: 2, _receipts: 1, inner: Message { id: 71, session_id: 66, text: \"msg text\" } } }");
        s.last_message = None;
        assert_eq!(format!("{}", s), "AugmentedSession { inner: Session { id: 2, _has_draft: false, type: DirectMessage { recipient: Recipient { id: 981, name: \"Nick Name\", e164: \"+35840...\", uuid: \"bff93979-...\", pni: unavailable } } }, last_message: None }");
    }

    #[test]
    fn recipient() {
        let mut r = get_recipient();
        let key_ok: [u8; 32] = [
            1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
            25, 26, 27, 28, 29, 30, 31, 32,
        ];
        let key_inv: [u8; 3] = [1, 2, 3];
        assert!(r.profile_key().is_none());
        r.profile_key = Some(key_inv.to_vec());
        assert!(r.profile_key().is_none());
        r.profile_key = Some(key_ok.to_vec());
        assert_eq!(r.profile_key(), Some(key_ok));
        assert_eq!(
            r.to_service_address(),
            Some(libsignal_service::ServiceAddress {
                uuid: uuid::Uuid::parse_str("bff93979-a0fa-41f5-8ccf-e319135384d8").unwrap(),
            })
        );
        assert_eq!(r.uuid(), "bff93979-a0fa-41f5-8ccf-e319135384d8");
        assert_eq!(r.e164_or_uuid(), "+358401010101");
        assert_eq!(r.name(), "Nick Name");
    }

    #[test]
    fn session() {
        let s = get_gv2_session();
        assert!(s.is_group());
        assert!(s.is_group_v2());
        assert!(s.unwrap_group_v2().id.eq("abc"));
    }

    #[test]
    fn session_type() {
        let mut s = SessionType::DirectMessage(get_recipient());
        assert!(!s.is_group_v2());
        s = SessionType::GroupV2(get_group_v2());
        assert!(s.unwrap_group_v2().id.eq("abc"));
    }

    #[test]
    fn augmented_message() {
        let a = get_augmented_message();
        assert!(!a.sent());
        assert!(!a.queued());
        assert_eq!(a.delivered(), 1);
        assert_eq!(a.read(), 1);
        assert_eq!(a.viewed(), 1);
        assert_eq!(a.attachments(), 2);
    }

    #[test]
    fn augmented_session() {
        let mut a = AugmentedSession {
            inner: get_gv2_session(),
            last_message: Some(get_augmented_message()),
        };
        a.inner.is_pinned = true;

        assert_eq!(a.id, get_gv2_session().id);
        assert_eq!(
            a.timestamp(),
            Some(
                NaiveDateTime::parse_from_str("2023-03-31 14:51:25", "%Y-%m-%d %H:%M:%S").unwrap()
            )
        );
        assert_eq!(a.recipient_name(), "");
        assert_eq!(a.recipient_uuid(), "");
        assert_eq!(a.recipient_e164(), "");
        assert_eq!(a.recipient_emoji(), "");
        assert_eq!(a.recipient_about(), "");
        assert_eq!(a.group_name(), Some("G2"));
        assert_eq!(a.group_description(), Some("desc".into()));
        assert_eq!(a.group_id(), Some("abc"));
        assert!(!a.sent());
        assert!(!a.has_avatar());
        assert!(a.has_attachment());
        assert_eq!(a.draft(), "".to_string());
        assert_eq!(a.last_message_text(), Some("msg text"));
        assert!(a.is_pinned());
        assert_eq!(a.section(), "pinned");
        assert!(!a.is_read());
        assert_eq!(a.read(), 1);
        assert_eq!(a.delivered(), 1);
        assert!(!a.is_muted());
        assert!(!a.is_archived());
        assert_eq!(a.viewed(), 1);

        a = AugmentedSession {
            inner: get_dm_session(),
            last_message: Some(get_augmented_message()),
        };
        a.inner.is_pinned = true;

        assert_eq!(a.group_name(), None);
        assert_eq!(a.group_description(), None);
        assert_eq!(a.group_id(), None);
        assert_eq!(a.recipient_id(), 981);
        assert_eq!(a.recipient_name(), "Nick Name");
        assert_eq!(a.recipient_uuid(), "bff93979-a0fa-41f5-8ccf-e319135384d8");
        assert_eq!(a.recipient_e164(), "+358401010101");
        assert_eq!(a.recipient_emoji(), "🦊");
        assert_eq!(a.recipient_about(), "About me!");
        assert!(!a.has_avatar());
    }

    #[test]
    fn text_shortener() {
        assert_eq!(shorten("abc", 4), "abc");
        assert_eq!(shorten("abcd", 4), "abcd");
        assert_eq!(shorten("abcde", 4), "abcd...");
        // Some characters are >1 bytes long.
        assert_eq!(shorten("Hyvää huomenta", 5), "Hyvää...");
        assert_eq!(shorten("Dobrý den", 5), "Dobrý...");
        assert_eq!(shorten("こんにちは", 3), "こんに...");
        assert_eq!(shorten("안녕하세요", 2), "안녕...");
        assert_eq!(shorten("Здравствуйте", 5), "Здрав...");
    }
}
