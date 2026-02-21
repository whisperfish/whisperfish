// XXX maybe the session-to-db migration should move into the store module.
pub mod migrations;

mod attachment;
#[cfg(feature = "calling")]
mod call;
mod groupv2;
mod linked_devices;
mod message_expiry;
mod profile;
mod profile_upload;
mod unidentified;
mod voice_note_transcription;

pub use self::groupv2::*;
pub use self::linked_devices::*;
use self::migrations::MigrationCondVar;
pub use self::profile_upload::*;
use self::unidentified::UnidentifiedCertificates;
use anyhow::anyhow;
use attachment::FetchAttachment;
use image::GenericImageView;
use itertools::Itertools;
use libsignal_service::groups_v2::Role;
use libsignal_service::messagepipe::Incoming;
use libsignal_service::proto::data_message::{Delete, Quote};
use libsignal_service::proto::sync_message::fetch_latest::Type as LatestType;
use libsignal_service::proto::sync_message::message_request_response::Type as MessageRequestAction;
use libsignal_service::proto::sync_message::Blocked;
use libsignal_service::proto::sync_message::Configuration;
use libsignal_service::proto::sync_message::Keys;
use libsignal_service::proto::sync_message::MessageRequestResponse;
use libsignal_service::proto::sync_message::Read;
use libsignal_service::proto::sync_message::Sent;
use libsignal_service::proto::SyncMessage;
use libsignal_service::protocol::ServiceIdKind;
use libsignal_service::push_service::RegistrationMethod;
use libsignal_service::push_service::DEFAULT_DEVICE_ID;
use libsignal_service::sender::SendMessageResult;
use libsignal_service::sender::ThreadIdentifier;
use qmetaobject::QMetaType;
use qttypes::QVariantMap;
use tracing_futures::Instrument;
use uuid::Uuid;
use whisperfish_store::millis_to_naive_chrono;
use whisperfish_store::naive_chrono_rounded_down;
use whisperfish_store::naive_chrono_to_millis;
use whisperfish_store::orm;
use whisperfish_store::orm::shorten;
use whisperfish_store::orm::MessageType;
use whisperfish_store::orm::SessionType;
use whisperfish_store::orm::StoryType;
use whisperfish_store::TrustLevel;
use zkgroup::profiles::ProfileKey;

use super::message_expiry::ExpiredMessagesStream;
use super::profile_refresh::OutdatedProfileStream;
use crate::actor::SendReaction;
use crate::actor::SessionActor;
use crate::config::SettingsBridge;
use crate::gui::StorageReady;
#[cfg(feature = "calling")]
use crate::model::Calls;
use crate::model::DeviceModel;
use crate::platform::QmlApp;
use crate::store::orm::UnidentifiedAccessMode;
use crate::store::AciOrPniStorage;
use crate::store::Storage;
use crate::worker::client::unidentified::CertType;
use actix::prelude::*;
use anyhow::Context;
#[cfg(feature = "calling")]
pub use call::*;
use chrono::prelude::*;
use futures::prelude::*;
use libsignal_service::configuration::SignalServers;
use libsignal_service::content::sync_message::Request as SyncRequest;
use libsignal_service::content::DataMessageFlags;
use libsignal_service::content::{
    sync_message, ContentBody, DataMessage, GroupContextV2, Metadata, Reaction, TypingMessage,
};
use libsignal_service::prelude::*;
use libsignal_service::proto::receipt_message::Type as ReceiptType;
use libsignal_service::proto::typing_message::Action;
use libsignal_service::proto::ReceiptMessage;
use libsignal_service::protocol::{self, *};
use libsignal_service::push_service::{
    AccountAttributes, DeviceCapabilities, RegistrationSessionMetadataResponse, ServiceIds,
    VerificationTransport, VerifyAccountResponse,
};
use libsignal_service::sender::AttachmentSpec;
use libsignal_service::websocket::SignalWebSocket;
use libsignal_service::AccountManager;
use phonenumber::PhoneNumber;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use qttypes::QVariantList;
use std::collections::HashMap;
use std::collections::HashSet;
use std::convert::TryFrom;
use std::convert::TryInto;
use std::fmt::{Display, Error, Formatter};
use std::fs::remove_file;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;
use sync_message::request::Type as RequestType;

// Maximum theoretical TypingMessage send rate,
// plus some change for Reaction messages etc.
const TM_MAX_RATE: f32 = 30.0; // messages per minute
const TM_CACHE_CAPACITY: f32 = 5.0; // 5 min
const TM_CACHE_TRESHOLD: f32 = 4.5; // 4 min 30 sec

#[derive(Debug)]
pub struct NewAttachment {
    pub path: String,
    pub mime_type: String,
}

#[derive(actix::Message, Debug)]
#[rtype(result = "()")]
pub struct QueueMessage {
    pub session_id: i32,
    pub message: String,
    pub attachments: Vec<NewAttachment>,
    pub quote: i32,
    pub is_voice_note: bool,
}

impl Display for QueueMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "QueueMessage {{ session_id: {}, message: \"{}\", quote: {}, attachments: \"{:?}\", is_voice_note: {} }}",
            &self.session_id,
            shorten(&self.message, 9),
            &self.quote,
            &self.attachments,
            &self.is_voice_note,
        )
    }
}

#[derive(actix::Message, Debug)]
#[rtype(result = "()")]
pub struct QueueExpiryUpdate {
    pub session_id: i32,
    pub expires_in: Option<Duration>,
}

impl Display for QueueExpiryUpdate {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "QueueExpiryMessage {{ session_id: {}, expires_in: \"{}\" }}",
            &self.session_id,
            match &self.expires_in {
                Some(d) => format!("Some({}s)", d.as_secs()),
                _ => "None".into(),
            },
        )
    }
}

#[derive(Message)]
#[rtype(result = "()")]
/// Enqueue a message on socket by message id.
///
/// This will construct a DataMessage, and pass it to a DeliverMessage
pub struct SendMessage(pub i32);

/// Delivers a constructed `T: Into<ContentBody>` to a session.
///
/// Returns true when delivered via unidentified sending.
#[derive(Message)]
#[rtype(result = "Result<Vec<SendMessageResult>, anyhow::Error>")]
struct DeliverMessage<T> {
    content: T,
    timestamp: u64,
    online: bool,
    for_story: bool,
    session_type: SessionType,
}

#[derive(Message)]
#[rtype(result = "Result<(), MessageSenderError>")]
struct DeliverSyncMessage(SyncMessage);

#[derive(actix::Message)]
#[rtype(result = "()")]
/// Send a notification that we're typing on a certain session.
pub struct SendTypingNotification {
    pub session_id: i32,
    pub is_start: bool,
}

#[derive(Message)]
#[rtype(result = "()")]
struct ReactionSent {
    message_id: i32,
    sender_id: i32,
    emoji: String,
    remove: bool,
    timestamp: NaiveDateTime,
}

#[derive(Message)]
#[rtype(result = "usize")]
pub struct CompactDb;

#[derive(Message)]
#[rtype(result = "()")]
/// Reset a session with a certain recipient
pub struct EndSession(pub i32);

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct ClientWorker {
    base: qt_base_class!(trait QObject),
    messageReceived: qt_signal!(sid: i32, mid: i32),
    messageReactionReceived: qt_signal!(sid: i32, mid: i32),
    attachmentDownloaded: qt_signal!(sid: i32, mid: i32),
    attachmentDownloadProgress: qt_signal!(sid: i32, mid: i32, progress: usize),
    messageReceipt: qt_signal!(sid: i32, mid: i32),
    queueEmptyChanged: qt_signal!(),
    notifyMessage: qt_signal!(data: QVariantMap),
    promptResetPeerIdentity: qt_signal!(),
    messageSent: qt_signal!(sid: i32, mid: i32, message: QString),
    messageNotSent: qt_signal!(sid: i32, mid: i32),
    closeNotification: qt_signal!(sid: i32, mid: i32),
    proofRequested: qt_signal!(token: QString, kind: QString),
    proofCaptchaResult: qt_signal!(success: bool),

    search: qt_method!(fn(&self, search_text: String, session_id: i32)),
    clearSearch: qt_method!(fn(&self)),
    searchResults: qt_property!(QVariantList; NOTIFY searchResultsChanged),
    searchResultsChanged: qt_signal!(),

    send_typing_notification: qt_method!(fn(&self, id: i32, is_start: bool)),
    submit_proof_captcha: qt_method!(fn(&self, token: String, response: String)),

    transcribeVoiceNote: qt_method!(fn(&self, message_id: i32)),

    connected: qt_property!(bool; NOTIFY connectedChanged),
    queueEmpty: qt_property!(bool; NOTIFY queueEmptyChanged),
    connectedChanged: qt_signal!(),

    actor: Option<Addr<ClientActor>>,
    session_actor: Option<Addr<SessionActor>>,
    device_model: Option<QObjectBox<DeviceModel>>,

    // Linked device management
    link_device: qt_method!(fn(&self, tsurl: String)),
    unlink_device: qt_method!(fn(&self, id: i64)),
    reload_linked_devices: qt_method!(fn(&self)),
    renameLinkedDevice: qt_method!(fn(&self, device_id: i32, device_name: String)),

    compact_db: qt_method!(fn(&self)),

    refresh_group_v2: qt_method!(fn(&self, session_id: usize)),

    fetchAttachment: qt_method!(fn(&self, attachment_id: i32)),
    delete_file: qt_method!(fn(&self, file_name: String)),
    startMessageExpiry: qt_method!(fn(&self, message_id: i32)),

    reconnect: qt_method!(fn(&self)),

    refresh_profile: qt_method!(fn(&self, recipient_id: i32)),
    upload_profile: qt_method!(
        fn(&self, given_name: String, family_name: String, about: String, emoji: String)
    ),

    mark_messages_read: qt_method!(fn(&self, msg_id_list: QVariantList)),

    linkRecipient: qt_method!(fn(&self, recipient_id: i32, external_id: String)),
    unlinkRecipient: qt_method!(fn(&self, recipient_id: i32)),

    sendConfiguration: qt_method!(fn(&self)),
    handleMessageRequest: qt_method!(fn(&self, recipient_aci: String, action: String)),

    updateAnnouncementsOnlyMode: qt_method!(fn(&self, group_id: String, enabled: bool)),
}

/// State machine for keeping track of initial envelope delivery
///
/// On initial connect, Signal sends us a dump of all envelopes that are in the queue,
/// followed by a "done" signal.  We need to keep track of which envelopes we've decrypted and
/// processed, so that we can forward the "done" Signal to QML, to display the final notification.
///
/// All envelopes are identified by their server GUID, and initially stored in the `Processing` state.
/// When the "done" signal is received, we transition to the `SignalSeen` state, and we stop adding
/// any new envelopes to the state machine.  We then wait for all envelopes to be processed,
/// and move to the `Done` state when the last envelope is processed.
#[derive(Default, Debug)]
enum QueueProcessState {
    #[default]
    Starting,
    Processing {
        expected_envelopes: Vec<String>,
    },
    SignalSeen {
        expected_envelopes: Vec<String>,
    },
    Done,
}

impl QueueProcessState {
    #[tracing::instrument(level = "trace")]
    fn observe_guid(&mut self, envelope: &str) {
        match self {
            Self::Starting => {
                *self = Self::Processing {
                    expected_envelopes: vec![envelope.to_string()],
                };
            }
            Self::Processing { expected_envelopes } => {
                expected_envelopes.push(envelope.to_string());
            }
            Self::SignalSeen { .. } | Self::Done => {
                // no-op
            }
        }

        tracing::trace!(new_state = ?self);
    }

    #[tracing::instrument(level = "trace")]
    fn processed_guid(&mut self, processed_envelope: &str) {
        match self {
            Self::SignalSeen { expected_envelopes } => {
                expected_envelopes.retain(|e| e != processed_envelope);
                if expected_envelopes.is_empty() {
                    *self = Self::Done;
                }
            }
            Self::Processing { expected_envelopes } => {
                expected_envelopes.retain(|e| e != processed_envelope);
            }
            _ => {}
        }

        tracing::trace!(new_state = ?self);
    }

    #[tracing::instrument(level = "trace")]
    fn observe_signal(&mut self) {
        match self {
            Self::Processing { expected_envelopes } => {
                *self = Self::SignalSeen {
                    expected_envelopes: std::mem::take(expected_envelopes),
                };
            }
            Self::Starting => *self = Self::Done,
            _ => {}
        }

        tracing::trace!(new_state = ?self);
    }

    fn is_done(&self) -> bool {
        matches!(self, Self::Done)
    }
}

/// ClientActor keeps track of the connection state.
pub struct ClientActor {
    inner: QObjectBox<ClientWorker>,
    #[cfg(feature = "calling")]
    calls_model: QObjectBox<Calls>,

    migration_state: MigrationCondVar,

    unidentified_certificates: unidentified::UnidentifiedCertificates,
    credentials: Option<ServiceCredentials>,
    self_aci: Option<Aci>,
    self_pni: Option<Pni>,
    storage: Option<Storage>,
    ws: Option<SignalWebSocket>,
    config: std::sync::Arc<crate::config::SignalConfig>,

    message_stream_handle: Option<SpawnHandle>,

    transient_timestamps: HashSet<u64>,
    initial_queue_process_state: QueueProcessState,

    voice_note_transcription_queue: voice_note_transcription::VoiceNoteTranscriptionQueue,

    start_time: DateTime<Local>,

    outdated_profile_stream_handle: Option<SpawnHandle>,
    message_expiry_notification_handle: Option<tokio::sync::mpsc::UnboundedSender<()>>,

    registration_session: Option<RegistrationSessionMetadataResponse>,

    settings: SettingsBridge,

    #[cfg(feature = "calling")]
    call_state: Option<call::WhisperfishCallManager>,
}

fn whisperfish_device_capabilities() -> DeviceCapabilities {
    DeviceCapabilities {
        announcement_group: true,
        storage: false,
        sender_key: true,
        change_number: false,
        gift_badges: false,
        stories: false,
        pni: true,
        payment_activation: false,
    }
}

pub enum Notification {
    Message,
    AudioCall,
    VideoCall,
}

pub fn message_notification(
    notification_type: Notification,
    session: orm::Session,
    sender_recipient: Option<orm::Recipient>,
    message_id: Option<i32>,
    message_text: Option<String>,
) -> QVariantMap {
    let (session_name, is_group) = match session.r#type {
        SessionType::GroupV1(group) => (group.name, true),
        SessionType::GroupV2(group) => (group.name, true),
        SessionType::DirectMessage(recipient) => (
            match recipient.profile_joined_name {
                Some(name) => name,
                None => recipient.e164_or_address(),
            },
            false,
        ),
    };
    let sender_name = QString::from(
        sender_recipient
            .as_ref()
            .map(|x| x.name().as_ref().to_string())
            .unwrap_or_default(),
    );
    let sender_e164 = QString::from(
        sender_recipient
            .as_ref()
            .map(|x| x.e164_or_address().to_string())
            .unwrap_or_default(),
    );
    let message_text = QString::from(message_text.unwrap_or_default());
    let message_id = message_id.unwrap_or(-1);

    let mut notification = QVariantMap::default();
    notification.insert("sessionId".into(), QVariant::from(session.id));
    notification.insert("messageId".into(), QVariant::from(message_id));
    notification.insert("sessionName".into(), session_name.to_qvariant());
    notification.insert("senderName".into(), sender_name.to_qvariant());
    notification.insert("senderE164".into(), sender_e164.to_qvariant());
    notification.insert("isGroup".into(), QVariant::from(is_group));

    // JS object contains either "message" or "isVideoCall"
    match notification_type {
        Notification::Message => {
            notification.insert("message".into(), message_text.to_qvariant());
            tracing::debug!(
                "New message notification: session {}, sender '{}', message '{}'",
                session.id,
                sender_name,
                message_text
            );
        }
        Notification::AudioCall => {
            notification.insert("isVideoCall".into(), QVariant::from(false));
            tracing::debug!(
                "New missed call notification: session {}, caller '{}'",
                session.id,
                sender_name,
            );
        }
        Notification::VideoCall => {
            notification.insert("isVideoCall".into(), QVariant::from(true));
            tracing::debug!(
                "New missed video call notification: session {}, caller '{}'",
                session.id,
                sender_name,
            );
        }
    }

    notification
}

impl ClientActor {
    pub fn new(
        app: &mut QmlApp,
        session_actor: Addr<SessionActor>,
        config: std::sync::Arc<crate::config::SignalConfig>,
    ) -> Result<Self, anyhow::Error> {
        let inner = QObjectBox::new(ClientWorker::default());
        let device_model = QObjectBox::new(DeviceModel::default());

        #[cfg(feature = "calling")]
        let calls_model = QObjectBox::new(Calls::new());

        app.set_object_property("ClientWorker".into(), inner.pinned());
        app.set_object_property("DeviceModel".into(), device_model.pinned());
        #[cfg(feature = "calling")]
        app.set_object_property("calls".into(), calls_model.pinned());

        inner.pinned().borrow_mut().session_actor = Some(session_actor);
        inner.pinned().borrow_mut().device_model = Some(device_model);

        let transient_timestamps: HashSet<u64> =
            HashSet::with_capacity((TM_CACHE_CAPACITY * TM_MAX_RATE) as _);

        Ok(Self {
            inner,
            #[cfg(feature = "calling")]
            calls_model,
            migration_state: MigrationCondVar::new(),
            unidentified_certificates: UnidentifiedCertificates::default(),
            credentials: None,
            self_aci: None,
            self_pni: None,
            storage: None,
            ws: None,
            config,

            message_stream_handle: None,

            transient_timestamps,
            initial_queue_process_state: QueueProcessState::Starting,

            voice_note_transcription_queue:
                voice_note_transcription::VoiceNoteTranscriptionQueue::default(),

            start_time: Local::now(),

            outdated_profile_stream_handle: None,
            message_expiry_notification_handle: None,

            registration_session: None,

            settings: SettingsBridge::default(),

            #[cfg(feature = "calling")]
            call_state: None,
        })
    }

    fn service_ids(&self) -> Option<ServiceIds> {
        Some(ServiceIds {
            aci: self.config.get_aci()?,
            pni: self.config.get_pni()?,
        })
    }

    fn user_agent(&self) -> String {
        crate::user_agent()
    }

    fn unauthenticated_service(&self) -> PushService {
        let service_cfg = self.service_cfg();
        PushService::new(service_cfg, None, self.user_agent())
    }

    fn authenticated_service_with_credentials(
        &self,
        credentials: ServiceCredentials,
    ) -> PushService {
        let service_cfg = self.service_cfg();

        PushService::new(service_cfg, Some(credentials), self.user_agent())
    }

    /// Panics if no authentication credentials are set.
    fn authenticated_service(&self) -> PushService {
        self.authenticated_service_with_credentials(self.credentials.clone().unwrap())
    }

    fn message_sender(
        &self,
    ) -> impl Future<Output = Result<MessageSender<AciOrPniStorage>, ServiceError>> {
        let storage = self.storage.clone().unwrap();
        let service = self.authenticated_service();
        let mut u_service = self.unauthenticated_service();

        let ws = self.ws.clone();
        let cipher = self.cipher(ServiceIdKind::Aci);
        let local_aci = self.self_aci.unwrap();
        let local_pni = self.self_pni.unwrap();
        let device_id = self.config.get_device_id();

        async move {
            let Some(ws) = ws else {
                return Err(ServiceError::SendError {
                    reason: "SignalWebSocket is not open".into(),
                });
            };

            let aci_key = storage
                .aci_storage()
                .get_identity_key_pair()
                .await
                .expect("aci identity set");
            let pni_key = storage
                .pni_storage()
                .get_identity_key_pair()
                .await
                .map_err(|_e| {
                    tracing::warn!(
                        "PNI identity key pair not set. Assuming PNI is not initialized."
                    );
                })
                .ok();

            let u_ws = u_service
                .ws("/v1/websocket/", "/v1/keepalive", &[], None)
                .await?;
            Ok(MessageSender::new(
                ws,
                u_ws,
                service,
                cipher,
                storage.aci_or_pni(ServiceIdKind::Aci), // In what cases do we use the
                local_aci,
                local_pni,
                aci_key,
                pni_key,
                device_id,
            ))
        }
    }

    fn service_cfg(&self) -> ServiceConfiguration {
        // XXX: read the configuration files!
        SignalServers::Production.into()
    }

    pub fn clear_transient_timstamps(&mut self) {
        if self.transient_timestamps.len() > (TM_CACHE_CAPACITY * TM_MAX_RATE) as usize {
            // slots / slots_per_minute = minutes
            const DURATION: u64 = (TM_CACHE_TRESHOLD * 60.0 * 1000.0) as _;
            let limit = (Utc::now().timestamp_millis() as u64) - DURATION;

            let len_before = self.transient_timestamps.len();
            self.transient_timestamps.retain(|t| *t > limit);
            tracing::trace!(
                "Removed {}/{} cached transient timestamps",
                len_before - self.transient_timestamps.len(),
                self.transient_timestamps.len()
            );
        }
    }

    #[tracing::instrument(level = "debug", skip(self, ctx, message, metadata))]
    pub fn handle_needs_delivery_receipt(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        message: &DataMessage,
        metadata: &Metadata,
    ) -> Option<()> {
        let content = ReceiptMessage {
            r#type: Some(ReceiptType::Delivery as _),
            timestamp: vec![message.timestamp?],
        };

        let storage = self.storage.as_ref().unwrap();

        let session_type = SessionType::DirectMessage(
            storage
                .fetch_recipient(&metadata.sender)
                .expect("needs-receipt sender recipient"),
        );

        ctx.notify(DeliverMessage {
            content,
            timestamp: Utc::now().timestamp_millis() as u64,
            session_type,
            online: false,
            for_story: false,
        });

        Some(())
    }

    /// Sync read receipt messages to other devices.
    /// Send `ReceiptMessage` to recipients, if the option is enabled in settings
    pub fn handle_needs_read_receipts(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        message_ids: Vec<i32>,
        read_receipts_enabled: bool,
    ) {
        let storage = self.storage.as_ref().unwrap();
        let mut messages = storage.fetch_messages_by_ids(message_ids);
        messages.retain(|m| m.message_type.is_none());
        let mut sessions: HashMap<i32, orm::Session> = HashMap::new();

        // Iterate over messages

        for message in messages.iter() {
            sessions.entry(message.session_id).or_insert_with(|| {
                storage
                    .fetch_session_by_id(message.session_id)
                    .expect("existing session for message")
            });
        }

        tracing::trace!(
            "Sending read receipts for {} messages in {} sessions",
            messages.len(),
            sessions.len()
        );

        // Synchronize to other own devices (if any)
        let read: Vec<Read> = messages
            .iter()
            .filter_map(|m| {
                if let Some(r_id) = m.sender_recipient_id {
                    // XXX database query in a loop
                    if let Some(recipient) = storage.fetch_recipient_by_id(r_id) {
                        Some(Read {
                            sender_aci: recipient.uuid.map(|u| u.to_string()),
                            timestamp: Some(m.server_timestamp.and_utc().timestamp_millis() as u64),
                        })
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let sync = SyncMessage {
            read,
            ..SyncMessage::with_padding(&mut rand::rng())
        };
        ctx.notify(DeliverSyncMessage(sync));

        if read_receipts_enabled {
            for (session_id, session) in sessions {
                let timestamp: Vec<u64> = messages
                    .iter()
                    .filter(|m| m.session_id == session_id)
                    .map(|m| m.server_timestamp.and_utc().timestamp_millis() as u64)
                    .collect();

                let content = ReceiptMessage {
                    r#type: Some(ReceiptType::Read as _),
                    timestamp,
                };

                ctx.notify(DeliverMessage {
                    content,
                    timestamp: Utc::now().timestamp_millis() as u64,
                    session_type: session.r#type,
                    online: false,
                    for_story: false,
                });
            }
        }
    }

    /// Process incoming message from Signal
    ///
    /// This was `MessageHandler` in Go.
    ///
    /// TODO: consider putting this as an actor `Handle<>` implementation instead.
    #[tracing::instrument(
        level = "debug",
        skip(self, ctx, source_phonenumber, source_addr, msg, sync_sent, metadata),
        fields(
            source_phonenumber = %source_phonenumber.as_ref().map(|p| p.to_string()).as_deref().unwrap_or("None"),
            source_addr = %source_addr.as_ref().map(ServiceId::service_id_string).as_deref().unwrap_or("None"),
            is_edit = %edit.is_some(),
        ),
    )]
    #[allow(clippy::too_many_arguments)]
    pub fn handle_message(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        // XXX: remove this argument
        source_phonenumber: Option<PhoneNumber>,
        source_addr: Option<ServiceId>,
        msg: &DataMessage,
        sync_sent: Option<Sent>,
        metadata: &Metadata,
        edit: Option<NaiveDateTime>,
    ) {
        let timestamp = metadata.timestamp;
        let dest_identity = metadata.destination.kind();
        let is_sync_sent = sync_sent.is_some();

        let mut storage = self.storage.clone().expect("storage");
        let self_recipient = storage.fetch_self_recipient().expect("self recipient");

        let sender_recipient = if source_phonenumber.is_some() || source_addr.is_some() {
            Some(storage.merge_and_fetch_recipient(
                source_phonenumber.clone(),
                source_addr.map(Aci::try_from).transpose().ok().flatten(),
                source_addr.map(Pni::try_from).transpose().ok().flatten(),
                crate::store::TrustLevel::Certain,
            ))
        } else {
            None
        };

        let flags = msg
            .flags()
            .try_into()
            .expect("Message flags doesn't fit into i32");
        let mut message_type: Option<MessageType> = None;

        if flags & DataMessageFlags::EndSession as i32 != 0 {
            let storage = storage.clone();
            if let Some(svc_addr) = sender_recipient
                .as_ref()
                .and_then(|r| r.to_service_address())
            {
                actix::spawn(async move {
                    if let Err(e) = match dest_identity {
                        ServiceIdKind::Aci => {
                            storage.aci_storage().delete_all_sessions(&svc_addr).await
                        }
                        ServiceIdKind::Pni => {
                            storage.pni_storage().delete_all_sessions(&svc_addr).await
                        }
                    } {
                        tracing::error!(
                            "End session requested for {}, but could not end session: {:?}",
                            &svc_addr.service_id_string(),
                            e
                        );
                    };
                });
            } else {
                tracing::error!("Requested session reset but no service address associated");
            }
            message_type = Some(MessageType::EndSession);
        }

        if (source_phonenumber.is_some() || source_addr.is_some()) && !is_sync_sent {
            if let Some(key) = msg.profile_key.as_deref() {
                let (recipient, was_updated) = storage.update_profile_key(
                    source_phonenumber.clone(),
                    source_addr,
                    key,
                    crate::store::TrustLevel::Certain,
                );
                if was_updated {
                    ctx.notify(RefreshProfile::ByRecipientId(recipient.id));
                }
            }
        }

        if flags & DataMessageFlags::ProfileKeyUpdate as i32 != 0 {
            message_type = Some(MessageType::ProfileKeyUpdate);
        }

        if !msg.preview.is_empty() {
            tracing::warn!("Message contains preview data, which is not yet saved nor displayed. Please upvote issue #695");
        }

        let expiration_timer_update = flags & DataMessageFlags::ExpirationTimerUpdate as i32 != 0;
        let alt_body = if let Some(reaction) = &msg.reaction {
            match storage.process_reaction(
                sender_recipient.as_ref().unwrap_or(&self_recipient),
                msg,
                reaction,
            ) {
                Ok(Some((message, session))) => {
                    tracing::info!("Reaction saved for message {}/{}", session.id, message.id);
                    self.inner
                        .pinned()
                        .borrow_mut()
                        .messageReactionReceived(session.id, message.id);
                }
                Ok(None) => {
                    tracing::error!("No message or session for reaction. Dropping silently.");
                    tracing::warn!("This could indicate out-of-order receipt delivery (#260)");
                }
                Err(e) => {
                    tracing::error!("Could not process reaction: {e}");
                }
            }
            None
        } else if expiration_timer_update {
            message_type = Some(MessageType::ExpirationTimerUpdate);
            Some("".into())
        } else if let Some(GroupContextV2 {
            group_change: Some(ref _group_change),
            ..
        }) = msg.group_v2
        {
            message_type = Some(MessageType::GroupChange);
            None
        } else if !msg.attachments.is_empty() {
            tracing::trace!("Received an attachment without body, replacing with empty text.");
            Some("".into())
        } else if let Some(sticker) = &msg.sticker {
            tracing::warn!(
                "Received a sticker, but they are currently unsupported. Please upvote issue #14."
            );
            tracing::trace!("{:?}", sticker);
            Some(format!(
                "[Whisperfish] Received a sticker: {}",
                sticker.emoji.as_ref().unwrap()
            ))
        } else if msg.payment.is_some() {
            // TODO: Save some info about payments?
            message_type = Some(MessageType::Payment);
            Some("".into())
        } else if msg.group_call_update.is_some() {
            message_type = Some(MessageType::GroupCallUpdate);
            Some("".into())
        } else if !msg.contact.is_empty() {
            Some("".into())
        }
        // TODO: Add more message types
        else {
            None
        };

        if let Some(msg_delete) = &msg.delete {
            let target_sent_timestamp = millis_to_naive_chrono(
                msg_delete
                    .target_sent_timestamp
                    .expect("Delete message has no timestamp"),
            );
            let db_message = storage.fetch_message_by_timestamp(target_sent_timestamp);
            if let Some(db_message) = db_message {
                let db_sender_rcpt = db_message.sender_recipient_id;
                let msg_sender_rcpt = sender_recipient.as_ref().map(|r| r.id);
                if is_sync_sent || db_sender_rcpt == msg_sender_rcpt {
                    storage.delete_message(db_message.id);
                    self.inner
                        .pinned()
                        .borrow_mut()
                        .closeNotification(db_message.session_id, db_message.id);
                } else {
                    tracing::warn!("Received a delete message from a different user, ignoring it.");
                }
            } else {
                tracing::warn!(
                    "Message {} not found for deletion!",
                    naive_chrono_to_millis(target_sent_timestamp)
                );
            }
        }

        let group = if let Some(group) = msg.group_v2.as_ref() {
            let mut key_stack = [0u8; zkgroup::GROUP_MASTER_KEY_LEN];
            key_stack.clone_from_slice(group.master_key.as_ref().expect("group message with key"));
            let key = GroupMasterKey::new(key_stack);
            let secret = GroupSecretParams::derive_from_master_key(key);

            let store_v2 = crate::store::GroupV2 {
                secret,
                revision: group.revision(),
            };

            let existing_group = storage.group_v2_exists(&store_v2);
            let session = storage.fetch_or_insert_session_by_group_v2(&store_v2);

            if existing_group {
                if group.group_change.is_some() {
                    ctx.notify(GroupV2Update(group.clone(), session));
                }
            } else {
                tracing::info!(
                    "We don't know this group. We'll request it's structure from the server."
                );
                ctx.notify(RequestGroupV2Info(store_v2.clone(), key_stack));
            }

            Some(storage.fetch_or_insert_session_by_group_v2(&store_v2))
        } else {
            None
        };

        let body = msg.body.clone().or(alt_body);
        let text = if let Some(body) = body {
            body
        } else {
            tracing::debug!("Message without (alt) body, not inserting");
            return;
        };

        let is_unidentified = if let (Some(sent), Some(source_addr)) = (&sync_sent, &source_addr) {
            let source_service_id = source_addr.service_id_string();
            sent.unidentified_status
                .iter()
                .any(|x| x.unidentified() && x.destination_service_id() == source_service_id)
        } else {
            metadata.unidentified_sender
        };

        let original_message = edit.and_then(|ts| storage.fetch_message_by_timestamp(ts));
        // Some sanity checks
        if edit.is_some() {
            if let Some(original_message) = original_message.as_ref() {
                if original_message.sender_recipient_id != sender_recipient.as_ref().map(|x| x.id) {
                    tracing::warn!("Received an edit for a message that was not sent by the same sender. Continuing, but this is weird.");
                }
            } else {
                tracing::warn!("Received an edit for a message that does not exist. Inserting as is and praying.  This is most probably a bug regarding out-of-order delivery.");
            }
        }

        let body_ranges = crate::store::body_ranges::serialize(&msg.body_ranges);

        let mut session = group.unwrap_or_else(|| {
            let recipient = storage.merge_and_fetch_recipient(
                source_phonenumber.clone(),
                source_addr.map(Aci::try_from).transpose().ok().flatten(),
                source_addr.map(Pni::try_from).transpose().ok().flatten(),
                TrustLevel::Certain,
            );
            storage.fetch_or_insert_session_by_recipient_id(recipient.id)
        });

        // Group expiry timer handled via GroupChanges
        if session.is_dm() {
            storage.update_expiration_timer(&session, msg.expire_timer, msg.expire_timer_version);
            session.expire_timer_version = msg.expire_timer_version() as i32;
            session.expiring_message_timeout =
                msg.expire_timer.map(|v| Duration::from_secs(v as u64));
        }

        if message_type == Some(MessageType::GroupChange) {
            tracing::warn!("Inserting a generic GroupChange message after handling it. This should not happen.");
        }

        let new_message = crate::store::NewMessage {
            source_addr,
            text,
            flags,
            outgoing: is_sync_sent,
            is_unidentified,
            sent: is_sync_sent,
            timestamp: millis_to_naive_chrono(if is_sync_sent && timestamp > 0 {
                timestamp
            } else {
                msg.timestamp()
            }),
            received: false, // This is set true by a receipt handler
            session_id: session.id,
            is_read: is_sync_sent,
            quote_timestamp: msg.quote.as_ref().and_then(|x| x.id),
            expires_in: session.expiring_message_timeout,
            expire_timer_version: session.expire_timer_version,
            story_type: StoryType::None,
            server_guid: metadata.server_guid,
            body_ranges,
            message_type,

            edit: original_message.as_ref(),
        };

        let message = storage.create_message(&new_message);

        if let Some(h) = self.message_expiry_notification_handle.as_ref() {
            h.send(()).expect("send message expiry notification");
        }

        if self.settings.get_bool("attachment_log") && !msg.attachments.is_empty() {
            tracing::trace!("Logging message to the attachment log");
            // XXX Sync code, but it's not the only sync code in here...
            let mut log = self.attachment_log();

            writeln!(
                log,
                "[{}] {:?} for message ID {}",
                Utc::now(),
                msg,
                message.id
            )
            .expect("write to the attachment log");
        }

        for attachment in &msg.attachments {
            let attachment_id = storage.register_attachment(message.id, attachment.clone());

            if self.settings.get_bool("save_attachments") {
                ctx.notify(FetchAttachment { attachment_id });
            }
        }

        self.inner
            .pinned()
            .borrow_mut()
            .messageReceived(session.id, message.id);

        if !is_sync_sent
            && !session.is_muted
            && self.settings.get_notification_privacy() != "off"
            && sender_recipient.as_ref().map(|x| x.id) != Some(self_recipient.id)
        {
            if let Some(original_message) = original_message {
                self.inner
                    .pinned()
                    .borrow_mut()
                    .closeNotification(original_message.session_id, original_message.id);

                for (rct, rcp) in storage.fetch_reactions_for_message(original_message.id) {
                    // We already have these reactions in the database so there should
                    // not be any errors, and even if so, we can safely ignore them
                    let _ = storage.save_reaction(message.id, rcp.id, rct.emoji, rct.sent_time);
                }
            };

            let notification = message_notification(
                Notification::Message,
                session,
                sender_recipient,
                Some(message.id),
                message.text,
            );
            self.inner.pinned().borrow_mut().notifyMessage(notification);
        }
    }

    fn handle_sync_request(&mut self, meta: Metadata, req: SyncRequest) {
        tracing::trace!("Processing sync request {:?}", req.r#type());

        let local_addr = self.self_aci.unwrap();
        let storage = self.storage.clone().unwrap();
        let sender = self.message_sender();
        let configuration = self.get_configuration();

        actix::spawn(async move {
            let mut sender = sender.await?;
            match req.r#type() {
                RequestType::Unknown => {
                    tracing::warn!("Unknown sync request from {:?}:{}. Please upgrade Whisperfish or file an issue.", meta.sender, meta.sender_device);
                    tracing::trace!("Unknown sync request: {:#?}", req);
                    return Ok(());
                }
                RequestType::Contacts => {
                    use libsignal_service::sender::ContactDetails;
                    // In fact, we should query for registered contacts instead of sessions here.
                    // https://gitlab.com/whisperfish/whisperfish/-/issues/133
                    let recipients: Vec<orm::Recipient> = {
                        use whisperfish_store::schema::recipients::dsl::*;
                        use diesel::prelude::*;
                        let mut db = storage.db();
                        recipients.load(&mut *db)?
                    };

                    let contacts = recipients.into_iter().map(|recipient| {
                            ContactDetails {
                                // XXX: expire timer from dm session
                                number: recipient.e164.as_ref().map(PhoneNumber::to_string),
                                aci: recipient.uuid.as_ref().map(Uuid::to_string),
                                name: recipient.profile_joined_name.clone(),
                                // XXX other profile stuff
                                ..Default::default()
                            }
                    });

                    sender.send_contact_details(&local_addr.into(), None, contacts, false, true).await?;
                },
                RequestType::Configuration => {
                    sender.send_sync_message(SyncMessage {configuration: Some(configuration), ..SyncMessage::with_padding(&mut rand::rng())}).await?;
                },
                RequestType::Keys => {
                    let master = storage.fetch_master_key();
                    // XXX media root backup key, account entropy pool
                    let keys = Some(Keys { master: master.map(|k| k.into()), account_entropy_pool: None, media_root_backup_key: None });
                    sender.send_sync_message(SyncMessage {keys, ..SyncMessage::with_padding(&mut rand::rng())}).await?;
                }
                RequestType::Blocked => {
                    let blocked = Some(Blocked {
                        numbers: storage.fetch_blocked_numbers().into_iter().map(|e| e.to_string()).collect_vec(),
                        acis: storage.fetch_blocked_acis().into_iter().map(|e| e.to_string()).collect_vec(),
                        group_ids: Vec::new(), // Group V1
                    });
                    sender.send_sync_message(SyncMessage {blocked, ..SyncMessage::with_padding(&mut rand::rng())}).await?;
                }
                // RequestType::PniIdentity // RESERVED
                // RequestType::Groups // RESERVED
                _ => {
                    tracing::trace!("Unimplemented sync request: {:#?}", req);
                    anyhow::bail!("Unimplemented sync request type: {:?}", req.r#type());
                },
            };

            Ok::<_, anyhow::Error>(())
        }.map(|v| if let Err(e) = v {tracing::error!("{:?} in handle_sync_request()", e)}));
    }

    fn get_configuration(&self) -> Configuration {
        Configuration {
            read_receipts: Some(self.settings.get_enable_read_receipts()),
            unidentified_delivery_indicators: None,
            typing_indicators: Some(self.settings.get_enable_typing_indicators()),
            provisioning_version: None,
            link_previews: Some(self.settings.get_enable_link_previews()),
        }
    }

    #[tracing::instrument(level = "debug", skip(self, recipient), fields(recipient = recipient.service_id_string()))]
    fn handle_message_not_sealed(&mut self, recipient: ServiceId) {
        // TODO: if the contact should have our profile key already, send it again.
        //       if the contact should not yet have our profile key, this is ok, and we
        //       should offer the user a message request.
        //       Cfr. MessageContentProcessor, grep for handleNeedsDeliveryReceipt.
        tracing::warn!(
            "Received an unsealed message from {:?}. Assert that they have our profile key.",
            recipient
        );
    }

    #[tracing::instrument(level = "debug", skip(self))]
    fn handle_message_request_response(&mut self, response: &MessageRequestResponse) -> bool {
        let storage = self.storage.clone().expect("storage initialized");
        if let Some(aci) = &response.thread_aci {
            let addr = ServiceId::parse_from_service_id_string(aci.as_str())
                .expect("valid aci uuid in MessageRequestResponse");
            match response.r#type() {
                MessageRequestAction::Accept => storage.mark_recipient_accepted(&addr),
                MessageRequestAction::Block => storage.mark_recipient_blocked(&addr),
                MessageRequestAction::BlockAndDelete => {
                    // Is it a "thread delete" which we don't support yet either?
                    storage.mark_recipient_blocked(&addr)
                }
                MessageRequestAction::BlockAndSpam => {
                    tracing::warn!(
                        "Reporting spam for groups is not yet implemented. Please upvote bug #392"
                    );
                    storage.mark_recipient_blocked(&addr)
                }
                _ => {
                    tracing::warn!(
                        "unhandled response type {:?} for {}. Please upvote bug #324",
                        response.r#type(),
                        addr.service_id_string()
                    );
                    return false;
                }
            }
            true
        } else if let Some(group_id) = &response.group_id {
            tracing::warn!("Group message request responses are not yet implemented. {:?}. Please upvote bug #327", group_id);
            false
        } else {
            tracing::warn!(
                "Unhandle message request response: {:?}. Please upvote bug #324",
                response
            );
            false
        }
    }

    #[tracing::instrument(
        level = "debug",
        skip(self, ctx),
        fields(%metadata, %body),
    )]
    fn process_envelope(
        &mut self,
        Content { body, metadata }: Content,
        ctx: &mut <Self as Actor>::Context,
    ) {
        let storage = self.storage.clone().expect("storage initialized");

        match body {
            ContentBody::NullMessage(_message) => {
                tracing::trace!("Ignoring NullMessage");
            }
            ContentBody::DataMessage(message) => {
                self.handle_message(
                    ctx,
                    None,
                    Some(metadata.sender),
                    &message,
                    None,
                    &metadata,
                    None,
                );
                if metadata.needs_receipt {
                    self.handle_needs_delivery_receipt(ctx, &message, &metadata);
                }

                // XXX Maybe move this if test (and the one for edit) into handle_message?
                if !metadata.unidentified_sender {
                    self.handle_message_not_sealed(metadata.sender);
                }
            }
            ContentBody::EditMessage(edit) => {
                let message = edit
                    .data_message
                    .as_ref()
                    .expect("edit message contains data");
                self.handle_message(
                    ctx,
                    None,
                    Some(metadata.sender),
                    message,
                    None,
                    &metadata,
                    Some(millis_to_naive_chrono(
                        edit.target_sent_timestamp
                            .expect("edit message contains timestamp"),
                    )),
                );

                if metadata.needs_receipt {
                    self.handle_needs_delivery_receipt(ctx, message, &metadata);
                }

                if !metadata.unidentified_sender {
                    self.handle_message_not_sealed(metadata.sender);
                }
            }
            ContentBody::SynchronizeMessage(message) => {
                let mut handled = false;
                if let Some(sent) = message.sent {
                    handled = true;
                    tracing::trace!("Sync sent message");
                    // These are messages sent through a paired device.
                    let address =
                        ServiceId::parse_from_service_id_string(sent.destination_service_id());
                    if address.is_none() {
                        tracing::warn!("Unparsable ServiceId {}", sent.destination_service_id());
                    }
                    let phonenumber = sent
                        .destination_e164
                        .as_deref()
                        .map(|s| phonenumber::parse(None, s))
                        .transpose()
                        .map_err(|_| {
                            tracing::warn!("Unparsable phonenumber {}", sent.destination_e164())
                        })
                        .ok()
                        .flatten();

                    if let Some(message) = &sent.message {
                        self.handle_message(
                            ctx,
                            // Empty string mainly when groups,
                            // but maybe needs a check. TODO
                            phonenumber,
                            address,
                            message,
                            Some(sent.clone()),
                            &metadata,
                            None,
                        );
                    } else if let Some(edit) = &sent.edit_message {
                        let message = edit.data_message.as_ref().expect("edit message");
                        let edit = edit.target_sent_timestamp.expect("edit timestamp");

                        self.handle_message(
                            ctx,
                            // Empty string mainly when groups,
                            // but maybe needs a check. TODO
                            phonenumber,
                            address,
                            message,
                            Some(sent.clone()),
                            &metadata,
                            Some(millis_to_naive_chrono(edit)),
                        );
                    } else {
                        tracing::warn!(
                            "Dropping sync-sent without message; probably Stories related: {:?}",
                            sent
                        );
                    }
                }
                if let Some(request) = message.request {
                    handled = true;
                    tracing::trace!("Sync request message");
                    self.handle_sync_request(metadata, request);
                }
                if !message.read.is_empty() {
                    handled = true;
                    tracing::trace!("Sync read message");
                    for read in &message.read {
                        // Signal uses timestamps in milliseconds, chrono has nanoseconds
                        // XXX: this should probably not be based on ts alone.
                        if let Some(timestamp) = read.timestamp.map(millis_to_naive_chrono) {
                            let source = read.sender_aci();
                            tracing::trace!(
                                "Marking message from {} at {} ({}) as read.",
                                source,
                                timestamp,
                                naive_chrono_rounded_down(timestamp),
                            );
                            if let Some(updated) = storage.mark_message_read(timestamp) {
                                self.inner
                                    .pinned()
                                    .borrow_mut()
                                    .messageReceipt(updated.session_id, updated.message_id)
                            }
                        }
                    }
                }
                if let Some(fetch) = message.fetch_latest {
                    handled = true;
                    match fetch.r#type() {
                        LatestType::Unknown => {
                            tracing::warn!("Sync FetchLatest with unknown type")
                        }
                        LatestType::LocalProfile => {
                            tracing::trace!("Scheduling local profile refresh");
                            ctx.notify(RefreshOwnProfile { force: true });
                        }
                        LatestType::StorageManifest => {
                            // XXX
                            tracing::warn!(
                                "Unimplemented: synchronize fetch request StorageManifest"
                            )
                        }
                        LatestType::SubscriptionStatus => {
                            tracing::warn!(
                                "Unimplemented: synchronize fetch request SubscriptionStatus"
                            )
                        }
                    }
                }
                if let Some(response) = message.message_request_response {
                    handled = true;
                    self.handle_message_request_response(&response);
                }
                if let Some(keys) = message.keys {
                    handled = true;
                    tracing::debug!("Sync Keys message");
                    // Note: storage_key is deprecated; it's generated from master_key
                    if let Some(bytes) = &keys.master {
                        if let Ok(master_key) = MasterKey::from_slice(bytes) {
                            storage.store_master_key(Some(&master_key));
                            let storage_key = StorageServiceKey::from_master_key(&master_key);
                            storage.store_storage_service_key(Some(&storage_key));
                            tracing::info!("Keys sync message handled successfully");
                        } else {
                            tracing::error!("Keys sync message with invalid data");
                        };
                    } else {
                        tracing::error!("Keys sync message without data");
                    }
                }
                if !handled {
                    tracing::warn!("Sync message without known sync type");
                }
            }
            ContentBody::TypingMessage(typing) => {
                if self.settings.get_enable_typing_indicators() {
                    tracing::info!("{:?} is typing.", metadata.sender.service_id_string());
                    let res = self
                        .inner
                        .pinned()
                        .borrow()
                        .session_actor
                        .as_ref()
                        .expect("session actor running")
                        .try_send(crate::actor::TypingNotification {
                            typing,
                            sender: metadata.sender,
                        });
                    if let Err(e) = res {
                        tracing::error!(
                            "Could not send typing notification to SessionActor: {}",
                            e
                        );
                    }
                } else {
                    tracing::debug!("Ignoring TypingMessage");
                }
            }
            ContentBody::ReceiptMessage(receipt) => {
                if let Some(receipt_type_i32) = receipt.r#type {
                    if let Ok(receipt_type) = ReceiptType::try_from(receipt_type_i32) {
                        let timestamps = receipt
                            .timestamp
                            .into_iter()
                            .map(millis_to_naive_chrono)
                            .collect();
                        let rcpt_timestamp = millis_to_naive_chrono(metadata.timestamp);
                        match receipt_type {
                            ReceiptType::Delivery => {
                                tracing::info!(
                                    "{:?} received a message.",
                                    metadata.sender.service_id_string()
                                );
                                for updated in storage.mark_messages_delivered(
                                    metadata.sender,
                                    timestamps,
                                    rcpt_timestamp,
                                ) {
                                    self.inner
                                        .pinned()
                                        .borrow_mut()
                                        .messageReceipt(updated.session_id, updated.message_id)
                                }
                            }
                            ReceiptType::Read => {
                                if self.settings.get_enable_read_receipts() {
                                    tracing::info!(
                                        "{:?} read a message.",
                                        metadata.sender.service_id_string()
                                    );
                                    for updated in storage.mark_messages_read(
                                        metadata.sender,
                                        timestamps,
                                        rcpt_timestamp,
                                    ) {
                                        self.inner
                                            .pinned()
                                            .borrow_mut()
                                            .messageReceipt(updated.session_id, updated.message_id)
                                    }
                                } else {
                                    tracing::debug!("Ignoring DeliveryMessage(Read)");
                                }
                            }
                            ReceiptType::Viewed => {
                                tracing::warn!(
                                    "Viewed receipts are not yet implemented. Please upvote issue #670"
                                );
                            }
                        }
                    }
                }
            }
            ContentBody::CallMessage(call) => {
                #[cfg(feature = "calling")]
                self.handle_call_message(ctx, metadata, call);

                #[cfg(not(feature = "calling"))]
                {
                    tracing::error!("Received CallMessage, but calling feature is not enabled.");
                    tracing::trace!("{call:?}");
                }
            }
            ContentBody::StoryMessage(story) => {
                tracing::error!("Received a Story, which is not yet implemented.");
                tracing::trace!("{story:?}");
            }
            ContentBody::PniSignatureMessage(pni) => {
                tracing::error!("Received a PniSignatureMessage, which is not yet implemented.");
                tracing::trace!("{pni:?}");
            }
        }
    }

    fn attachment_log(&self) -> std::fs::File {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(self.storage.as_ref().unwrap().path().join(format!(
                "attachments-{}.log",
                self.start_time.format("%Y-%m-%d_%H-%M")
            )))
            .expect("open attachment log")
    }

    fn cipher(&self, service_identity: ServiceIdKind) -> ServiceCipher<AciOrPniStorage> {
        let service_cfg = self.service_cfg();
        let device_id = self.config.get_device_id();
        ServiceCipher::new(
            self.storage.as_ref().unwrap().aci_or_pni(service_identity),
            service_cfg.unidentified_sender_trust_roots.clone(),
            Uuid::from(self.self_aci.unwrap()),
            device_id,
        )
    }
}

impl Actor for ClientActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
        #[cfg(feature = "calling")]
        {
            self.call_state = Some(call::WhisperfishCallManager::new(ctx.address()));
            self.calls_model
                .pinned()
                .borrow_mut()
                .set_client(ctx.address());
        }
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();
    }
}

impl Handler<QueueMessage> for ClientActor {
    type Result = ();

    fn handle(&mut self, msg: QueueMessage, ctx: &mut Self::Context) -> Self::Result {
        let _span = tracing::trace_span!("QueueMessage", %msg).entered();
        let storage = self.storage.as_mut().unwrap();

        let session = storage
            .fetch_session_by_id(msg.session_id)
            .expect("existing session when sending");

        let quote = if msg.quote >= 0 {
            Some(
                storage
                    .fetch_message_by_id(msg.quote)
                    .expect("existing quote id"),
            )
        } else {
            None
        };

        let inserted_msg = storage.create_message(&crate::store::NewMessage {
            session_id: msg.session_id,
            source_addr: storage.fetch_self_service_address_aci(),
            text: msg.message,
            quote_timestamp: quote.map(|msg| naive_chrono_to_millis(msg.server_timestamp)),
            expires_in: session.expiring_message_timeout,
            expire_timer_version: session.expire_timer_version,
            ..crate::store::NewMessage::new_outgoing()
        });

        for attachment in &msg.attachments {
            storage.insert_local_attachment(
                inserted_msg.id,
                Some(attachment.mime_type.as_str()),
                attachment.path.clone(),
                msg.is_voice_note,
            );
        }

        if msg.is_voice_note {
            // If the attachment is a voice note, and we enabled automatic transcription,
            // trigger the transcription
            if self.settings.get_transcribe_voice_notes() {
                ctx.notify(voice_note_transcription::TranscribeVoiceNote {
                    message_id: inserted_msg.id,
                });
            }
        }

        if let Some(h) = self.message_expiry_notification_handle.as_ref() {
            h.send(()).expect("send message expiry notification");
        }

        ctx.notify(SendMessage(inserted_msg.id));
    }
}

impl Handler<QueueExpiryUpdate> for ClientActor {
    type Result = ();

    fn handle(&mut self, msg: QueueExpiryUpdate, ctx: &mut Self::Context) -> Self::Result {
        let _span = tracing::trace_span!("QueueExpiryUpdate", %msg).entered();
        tracing::trace!(
            "Sending expiry of {:?} seconds to session {}",
            msg.expires_in,
            msg.session_id
        );
        let storage = self.storage.as_mut().unwrap();

        let mut session = storage
            .fetch_session_by_id(msg.session_id)
            .expect("existing session when sending");

        // TODO: #706
        if session.is_group() {
            tracing::error!("Group change messages and group message expiry timer changes are not supported yet. Please upvote bugs #706 and #707");
            return;
        }

        session.expire_timer_version = storage.update_expiration_timer(
            &session,
            msg.expires_in.map(|x| x.as_secs() as u32),
            None,
        );

        let msg = storage.create_message(&crate::store::NewMessage {
            session_id: session.id,
            source_addr: storage.fetch_self_service_address_aci(),
            expires_in: msg.expires_in,
            expire_timer_version: session.expire_timer_version,
            flags: DataMessageFlags::ExpirationTimerUpdate as i32,
            message_type: Some(MessageType::ExpirationTimerUpdate),
            ..crate::store::NewMessage::new_outgoing()
        });

        ctx.notify(SendMessage(msg.id));
    }
}

impl Handler<SendMessage> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    // Equiv of worker/send.go
    fn handle(&mut self, SendMessage(mid): SendMessage, ctx: &mut Self::Context) -> Self::Result {
        let _span = tracing::info_span!("ClientActor::SendMessage", message_id = mid).entered();
        let sender = self.message_sender();
        let storage = self.storage.as_mut().unwrap().clone();
        let msg = storage.fetch_augmented_message(mid).unwrap();
        let session = storage.fetch_session_by_id(msg.session_id).unwrap();
        let session_id = session.id;

        if msg.sent_timestamp.is_some() {
            tracing::warn!("Message already sent, refusing to retransmit.");
            return Box::pin(async {}.into_actor(self).map(|_, _, _| ()));
        }

        tracing::trace!("Sending for session: {}", session);
        tracing::trace!("Sending message: {}", msg.inner);

        let addr = ctx.address();
        // XXX What about PNI? When should we use it?
        let self_addr = self.self_aci.unwrap();
        Box::pin(
            async move {
                let mut sender = sender.await?;
                if let SessionType::GroupV1(_group) = &session.r#type {
                    // FIXME
                    tracing::error!("Cannot send to Group V1 anymore.");
                }
                let group_v2 = session.group_context_v2();
                if session.is_group_v2() {
                    let gv2 = session.unwrap_group_v2();
                    let Some(self_member) = storage.fetch_group_v2_self_member(&gv2.id) else {
                        return Err(anyhow!("Not member of the group '{}', will not send message", gv2.name));
                    };
                    if gv2.announcement_only && self_member.role < (Role::Administrator as i32) {
                        return Err(anyhow!("Only administrators can send messages in group '{}'", gv2.name));
                    }
                }

                let timestamp = naive_chrono_to_millis(msg.server_timestamp);

                let quote = msg
                    .quote_id
                    .and_then(|quote_id| storage.fetch_augmented_message(quote_id))
                    .map(|quoted_message| {
                        if !quoted_message.attachments > 0 {
                            tracing::warn!("Quoting attachments is incomplete.  Here be dragons.");
                        }
                        let quote_sender = quoted_message
                            .sender_recipient_id
                            .and_then(|x| storage.fetch_recipient_by_id(x));

                        Quote {
                            id: Some(naive_chrono_to_millis(quoted_message.server_timestamp)),
                            author_aci: quote_sender.as_ref().and_then(|r| r.uuid.as_ref().map(Uuid::to_string)),
                            text: quoted_message.text.clone(),

                            ..Default::default()
                        }
                    });

                let mut content = DataMessage {
                    // Don't send body in "contol messages"
                    body: match msg.flags {
                        0 => msg.text.clone(),
                        _ => None,
                    },
                    flags: if msg.flags != 0 {
                        Some(msg.flags as _)
                    } else {
                        None
                    },
                    timestamp: Some(timestamp),
                    // XXX: depends on the features in the message!
                    required_protocol_version: Some(0),
                    group_v2,

                    profile_key: storage.fetch_self_recipient_profile_key(),
                    quote,
                    expire_timer: msg.expires_in.map(|x| x as u32),
                    expire_timer_version: Some(msg.expire_timer_version as u32),
                    body_ranges: crate::store::body_ranges::to_vec(msg.message_ranges.as_ref()),
                    ..Default::default()
                };

                let attachments = storage.fetch_attachments_for_message(msg.id);

                for mut attachment in attachments {
                    let attachment_path = attachment
                        .absolute_attachment_path()
                        .expect("attachment path when uploading");
                    let contents =
                        tokio::fs::read(attachment_path.as_ref())
                            .await
                            .context("reading attachment")?;

                    let content_type = match mime_guess::from_path(attachment_path.as_ref()).first() {
                        Some(mime) => mime.essence_str().into(),
                        None => String::from("application/octet-stream"),
                    };

                    let file_name= Path::new(attachment_path.as_ref())
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned());

                    if attachment.visual_hash.is_none() && content_type.starts_with("image/") {
                        tracing::info!("Computing blurhash for attachment {}", attachment.id);
                        match image::load_from_memory(&contents) {
                            Ok(img) => {
                                let (width, height) = img.dimensions();
                                let img = img.to_rgba8();
                                let hash = tokio::task::spawn_blocking(move || {
                                    blurhash::encode(4, 3, width, height, &img).expect("blurhash encodable image")
                                })
                                .await
                                .context("computing blurhash")?;
                                storage.store_attachment_visual_hash(attachment.id, &hash, width, height);
                                attachment.visual_hash = Some(hash);
                                attachment.width = Some(width as i32);
                                attachment.height = Some(height as i32);
                            }
                            Err(e) => {
                                tracing::warn!("Could not load image for blurhash: {}", e);
                            }
                        }
                    }

                    let spec = AttachmentSpec {
                        content_type,
                        length: contents.len(),
                        file_name,
                        preview: None,
                        voice_note: Some(attachment.is_voice_note),
                        borderless: Some(attachment.is_borderless),
                        width: attachment.width.map(|x| x as u32),
                        height: attachment.height.map(|x| x as u32),
                        caption: attachment.caption,
                        blur_hash: attachment.visual_hash,
                    };
                    let ptr = match sender.upload_attachment(spec, contents, &mut rand::rng()).await {
                        Ok(v) => v,
                        Err(e) => {
                            anyhow::bail!("Failed to upload attachment: {}", e);
                        }
                    };
                    storage.store_attachment_pointer(attachment.id, &ptr);
                    content.attachments.push(ptr);
                }

                let res = addr
                    .send(DeliverMessage {
                        content,
                        online: false,
                        timestamp,
                        session_type: session.r#type,
                        for_story: false,
                    })
                    .await?;

                match res {
                    Ok(results) => {
                        let unidentified = results.iter().all(|res| match res {
                            // XXX: We should be able to send unidentified messages to our own devices too.
                            Ok(message) => message.unidentified || (message.recipient == self_addr),
                            _ => false,
                        });

                        // Look for Ok recipients that couldn't deliver on unidentified.
                        for result in results.iter().filter_map(|res| res.as_ref().ok()) {
                            // Look up recipient to check the current state
                            let recipient = storage
                                .fetch_recipient(&result.recipient)
                                .expect("sent recipient in db");
                            let target_state = if result.unidentified {
                                // Unrestricted and success; keep unrestricted
                                if recipient.unidentified_access_mode
                                    == UnidentifiedAccessMode::Unrestricted
                                {
                                    UnidentifiedAccessMode::Unrestricted
                                } else {
                                    // Success; set Enabled
                                    UnidentifiedAccessMode::Enabled
                                }
                            } else {
                                // Failure; set Disabled
                                UnidentifiedAccessMode::Disabled
                            };
                            if recipient.profile_key().is_some()
                                && recipient.unidentified_access_mode != target_state
                            {
                                // Recipient with profile key, but could not send unidentified.
                                // Mark as disabled.
                                tracing::info!(
                                    "Setting unidentified access mode for {:?} from {:?} to {:?}",
                                    recipient.uuid.unwrap(),
                                    recipient.unidentified_access_mode,
                                    target_state
                                );
                                storage.set_recipient_unidentified(&recipient, target_state);
                            }
                        }

                        let successes = results.iter().filter(|res| res.is_ok()).count();
                        let all_ok = successes == results.len();
                        if all_ok {
                            storage.dequeue_message(mid, chrono::Utc::now().naive_utc(), unidentified);

                            Ok((session_id, mid, msg.inner.text))
                        } else {
                            storage.fail_message(mid);
                            let result_count = results.len();
                            for error in results.into_iter().filter_map(Result::err) {
                                tracing::error!("Could not deliver message: {}", error);
                                match error {
                                    MessageSenderError::ProofRequired { token, options } => {
                                        // Note: 'recaptcha' can refer to reCAPTCHA or hCaptcha
                                        let recaptcha = String::from("recaptcha");

                                        if options.contains(&recaptcha) {
                                            addr.send(ProofRequired {
                                                token: token.to_owned(),
                                                kind: recaptcha,
                                            })
                                            .await
                                            .expect("deliver captcha required");
                                        } else {
                                            tracing::warn!("Rate limit proof requested, but type 'recaptcha' wasn't available!");
                                        }
                                    },
                                    MessageSenderError::NotFound { service_id } => {
                                        tracing::warn!("Recipient not found, removing device sessions {:?}", service_id);
                                        // XXX: This is a hack; we always have an ACI here for now.
                                        let num = match ServiceId::from(self_addr).kind() {
                                            ServiceIdKind::Aci =>
                                                storage.aci_storage().delete_all_sessions(&service_id).await?,
                                            ServiceIdKind::Pni =>
                                                storage.pni_storage().delete_all_sessions(&service_id).await?,
                                        };

                                        tracing::trace!("Removed {} device session(s)", num);
                                    },
                                    _ => {
                                        tracing::error!("The above error goes unhandled.");
                                    }
                                };
                            }
                            tracing::error!("Successfully delivered message to {} out of {} recipients", successes, result_count);
                            anyhow::bail!("Could not deliver message.")
                        }
                    }
                    Err(e) => {
                        storage.fail_message(mid);
                        Err(e)
                    }
                }
            }.instrument(tracing::debug_span!("sending message", mid))
            .into_actor(self)
            .map(move |res, act, _ctx| {
                match res {
                    Ok((sid, mid, message)) => {
                        act.inner.pinned().borrow().messageSent(
                            sid,
                            mid,
                            message.unwrap_or_default().into(),
                        );
                    }
                    Err(e) => {
                        tracing::error!("Sending message: {}", e);
                        act.inner.pinned().borrow().messageNotSent(session_id, mid);
                        if let Some(MessageSenderError::NotFound { .. }) = e.downcast_ref() {
                            // Handles session-is-not-a-group ok
                            act.inner
                                .pinned()
                                .borrow()
                                .refresh_group_v2(session_id as _);
                        }
                    }
                };
            }),
        )
    }
}

impl Handler<EndSession> for ClientActor {
    type Result = ();

    fn handle(&mut self, EndSession(id): EndSession, ctx: &mut Self::Context) -> Self::Result {
        let _span =
            tracing::trace_span!("ClientActor::EndSession(recipient_id = {})", id).entered();

        let storage = self.storage.as_mut().unwrap();
        let recipient = storage
            .fetch_recipient_by_id(id)
            .expect("existing recipient id");
        let session = storage.fetch_or_insert_session_by_recipient_id(recipient.id);

        let msg = storage.create_message(&crate::store::NewMessage {
            session_id: session.id,
            source_addr: recipient.to_service_address(),
            timestamp: chrono::Utc::now().naive_utc(),
            flags: DataMessageFlags::EndSession.into(),
            message_type: Some(MessageType::EndSession),
            ..crate::store::NewMessage::new_outgoing()
        });
        ctx.notify(SendMessage(msg.id));
    }
}

impl Handler<SendTypingNotification> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        SendTypingNotification {
            session_id,
            is_start,
        }: SendTypingNotification,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        tracing::info!(
            "ClientActor::SendTypingNotification({}, {})",
            session_id,
            is_start
        );
        let storage = self.storage.as_mut().unwrap();
        let addr = ctx.address();

        let session = storage.fetch_session_by_id(session_id).unwrap();
        assert_eq!(session_id, session.id);

        tracing::trace!("Sending typing notification for session: {}", session);

        // Since we don't want to stress database needlessly,
        // cache the sent TypingMessage timestamps and try to
        // match delivery receipts against it when they arrive.

        self.clear_transient_timstamps();
        let now = Utc::now().timestamp_millis() as u64;
        self.transient_timestamps.insert(now);

        Box::pin(
            async move {
                let group_id = match &session.r#type {
                    SessionType::DirectMessage(_) => None,
                    SessionType::GroupV1(group) => {
                        Some(hex::decode(&group.id).expect("valid hex identifiers in db"))
                    }
                    SessionType::GroupV2(group) => {
                        Some(hex::decode(&group.id).expect("valid hex identifiers in db"))
                    }
                };

                let content = TypingMessage {
                    timestamp: Some(now),
                    action: Some(if is_start {
                        Action::Started
                    } else {
                        Action::Stopped
                    } as _),
                    group_id,
                };

                addr.send(DeliverMessage {
                    content,
                    online: true,
                    timestamp: now,
                    session_type: session.r#type,
                    for_story: false,
                })
                .await?
                .map(|_unidentified| session_id)
            }
            .into_actor(self)
            .map(move |res, _act, _ctx| {
                match res {
                    Ok(sid) => {
                        tracing::trace!(
                            "Successfully sent typing notification for session {}",
                            sid
                        );
                    }
                    Err(e) => {
                        tracing::error!("Delivering typing notification: {}", e);
                    }
                };
            }),
        )
    }
}

impl Handler<SendReaction> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        SendReaction {
            message_id,
            sender_id,
            emoji,
            remove,
        }: SendReaction,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        tracing::info!(
            "ClientActor::SendReaction({}, {}, {}, {:?})",
            message_id,
            sender_id,
            emoji,
            remove
        );

        let storage = self.storage.as_mut().unwrap();
        let own_id = storage.fetch_self_recipient_id();
        let message = storage.fetch_message_by_id(message_id).unwrap();

        // Outgoing messages should not have sender_recipient_id set
        let (sender_id, emoji) = if sender_id > 0 && sender_id != own_id {
            (sender_id, emoji)
        } else {
            if !message.is_outbound {
                panic!("Inbound message {} has no sender recipient ID", message_id);
            }
            if remove {
                let reaction = storage.fetch_reaction(message_id, own_id);
                if let Some(r) = reaction {
                    (own_id, r.emoji)
                } else {
                    // XXX: Don't continue - we should remove the same emoji
                    tracing::error!("Message {} doesn't have our own reaction!", message_id);
                    (own_id, emoji)
                }
            } else {
                (own_id, emoji)
            }
        };

        let session = storage.fetch_session_by_id(message.session_id).unwrap();
        let sender_recipient = storage.fetch_recipient_by_id(sender_id).unwrap();
        assert_eq!(
            sender_id, sender_recipient.id,
            "message sender recipient id mismatch"
        );

        self.clear_transient_timstamps();
        let now = Utc::now();
        self.transient_timestamps
            .insert(now.timestamp_millis() as u64);

        let addr = ctx.address();
        Box::pin(
            async move {
                let group_v2 = session.group_context_v2();

                let expire_timer = if session.is_group() {
                    None
                } else {
                    session.expiring_message_timeout.map(|t| t.as_secs() as _)
                };
                let expire_timer_version = if session.is_group() {
                    None
                } else {
                    Some(session.expire_timer_version as _)
                };

                let content = DataMessage {
                    group_v2,
                    timestamp: Some(now.timestamp_millis() as u64),
                    required_protocol_version: Some(4), // Source: received emoji from Signal Android
                    expire_timer,
                    expire_timer_version,
                    reaction: Some(Reaction {
                        emoji: Some(emoji.clone()),
                        remove: Some(remove),
                        target_author_aci: sender_recipient.uuid.map(|u| u.to_string()),
                        target_sent_timestamp: Some(naive_chrono_to_millis(
                            message.server_timestamp,
                        )),
                    }),
                    ..Default::default()
                };

                addr.send(DeliverMessage {
                    content,
                    online: false,
                    timestamp: now.timestamp_millis() as u64,
                    session_type: session.r#type,
                    for_story: false,
                })
                .await?
                .map(|_| (emoji, now, own_id))
            }
            .into_actor(self)
            .map(move |res, _act, ctx| {
                match res {
                    Ok((emoji, timestamp, sender_id)) => {
                        ctx.notify(ReactionSent {
                            message_id,
                            sender_id,
                            remove,
                            emoji,
                            timestamp: timestamp.naive_utc(),
                        });
                        tracing::trace!("Reaction sent to message {}", message_id);
                    }
                    Err(e) => {
                        tracing::error!("Could not sent Reaction: {}", e);
                    }
                };
            }),
        )
    }
}

impl Handler<ReactionSent> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        ReactionSent {
            message_id,
            sender_id,
            remove,
            emoji,
            timestamp,
        }: ReactionSent,
        _ctx: &mut Self::Context,
    ) {
        let storage = self.storage.as_mut().unwrap();
        if remove {
            let _ = storage.remove_reaction(message_id, sender_id);
        } else {
            let _ = storage.save_reaction(message_id, sender_id, emoji, timestamp);
        }
    }
}

impl<T: Into<ContentBody>> Handler<DeliverMessage<T>> for ClientActor {
    type Result = ResponseFuture<Result<Vec<SendMessageResult>, anyhow::Error>>;

    fn handle(&mut self, msg: DeliverMessage<T>, _ctx: &mut Self::Context) -> Self::Result {
        let DeliverMessage {
            content,
            timestamp,
            online,
            session_type: session,
            for_story,
        } = msg;
        let content = content.into();

        tracing::trace!("Transmitting {:?} with timestamp {}", content, timestamp);

        let storage = self.storage.clone().unwrap();
        let sender = self.message_sender();
        // XXX What about PNI? When should we use it?
        let local_addr = self.self_aci.unwrap();
        let cert_type = if self.settings.get_share_phone_number() {
            CertType::UuidOnly
        } else {
            CertType::Complete
        };

        let certs = self.unidentified_certificates.clone();

        Box::pin(async move {
            let mut sender = sender.await?;

            let results = match &session {
                SessionType::GroupV1(_group) => {
                    // FIXME
                    tracing::error!("Cannot send to Group V1 anymore.");
                    Vec::new()
                }
                SessionType::GroupV2(group) => {
                    let members = storage.fetch_group_members_by_group_v2_id(&group.id);
                    let members = members
                        .iter()
                        .filter_map(|(_member, recipient)| {
                            let member = recipient.to_service_address();

                            if !recipient.is_registered
                                || Some(ServiceId::from(local_addr)) == member
                            {
                                None
                            } else if let Some(member) = member {
                                // XXX change the cert type when we want to introduce E164 privacy.
                                let access = certs.access_for(cert_type, recipient, for_story);
                                Some((member, access, recipient.needs_pni_signature))
                            } else {
                                tracing::warn!(
                                    "No known UUID for {}; will not deliver this message.",
                                    recipient.e164_or_address()
                                );
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    // Clone + async closure means we can use an immutable borrow.
                    sender
                        .send_message_to_group(&members, content, timestamp, online)
                        .await
                }
                SessionType::DirectMessage(recipient) => {
                    let svc = recipient.to_service_address();

                    let access = certs.access_for(cert_type, recipient, for_story);

                    if let Some(svc) = svc {
                        if !recipient.is_registered {
                            anyhow::bail!("Unregistered recipient {}", svc.service_id_string());
                        }

                        vec![
                            sender
                                .send_message(
                                    &svc,
                                    access,
                                    content.clone(),
                                    timestamp,
                                    recipient.needs_pni_signature,
                                    online,
                                )
                                .await,
                        ]
                    } else {
                        anyhow::bail!("Recipient id {} has no UUID", recipient.id);
                    }
                }
            };
            Ok(results)
        })
    }
}

impl Handler<DeliverSyncMessage> for ClientActor {
    type Result = ResponseFuture<Result<(), MessageSenderError>>;

    fn handle(&mut self, sync: DeliverSyncMessage, _ctx: &mut Self::Context) -> Self::Result {
        let sync = sync.0;
        let sender = self.message_sender();

        Box::pin(async move {
            let mut sender = sender
                .await
                .expect("message sender when sending a sync message");
            sender.send_sync_message(sync).await
        })
    }
}

impl Handler<StorageReady> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        StorageReady { storage }: StorageReady,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage.clone());
        let e164 = self
            .config
            .get_tel()
            .expect("phonenumber present after any registration");
        let aci = self.config.get_aci();
        // XXX What if WhoAmI has not yet run?
        let pni = self.config.get_pni();
        let device_id = self.config.get_device_id();

        tracing::info!("E.164: {e164}, ACI: {aci:?}, PNI: {pni:?}, DeviceId: {device_id}");

        storage.mark_pending_messages_failed();

        let credentials = async move {
            ServiceCredentials {
                aci,
                pni,
                phonenumber: e164,
                password: Some(storage.signal_password().await.unwrap()),
                signaling_key: storage.signaling_key().await.unwrap(),
                device_id: Some(device_id.into()),
            }
        }
        .instrument(tracing::span!(
            tracing::Level::INFO,
            "reading password and signaling key"
        ));

        Box::pin(
            credentials
                .into_actor(self)
                .map(move |credentials, act, ctx| {
                    let _span = tracing::trace_span!("whisperfish startup").entered();

                    act.credentials = Some(credentials);
                    let cred = act.credentials.as_ref().unwrap();

                    act.self_aci = cred.aci.map(Aci::from);
                    act.self_pni = cred.pni.map(Pni::from);

                    Self::queue_migrations(ctx);

                    ctx.notify(Restart);
                    ctx.notify(RefreshPreKeys);
                }),
        )
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Restart;

impl Handler<Restart> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: Restart, ctx: &mut Self::Context) -> Self::Result {
        let service = self.authenticated_service();
        let credentials = self.credentials.clone().unwrap();
        let connectable = self.migration_state.connectable();

        if self.message_expiry_notification_handle.is_none() {
            let (message_expiry_notification_handle, message_expiry_notification) =
                tokio::sync::mpsc::unbounded_channel();
            ctx.add_stream(ExpiredMessagesStream::new(
                self.storage.clone().unwrap(),
                message_expiry_notification,
            ));
            self.message_expiry_notification_handle = Some(message_expiry_notification_handle);
        }

        if let Some(handle) = self.message_stream_handle.take() {
            ctx.cancel_future(handle);
        }

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();
        Box::pin(
            async move {
                connectable.await;
                let mut receiver = MessageReceiver::new(service.clone());

                let pipe = receiver.create_message_pipe(credentials, false).await?;
                let ws = pipe.ws();
                Result::<_, ServiceError>::Ok((pipe, ws))
            }
            .instrument(tracing::trace_span!("set up message receiver"))
            .into_actor(self)
            .map(move |pipe, act, ctx| match pipe {
                Ok((pipe, ws)) => {
                    ctx.notify(unidentified::RotateUnidentifiedCertificates);
                    act.message_stream_handle = Some(
                        ctx.add_stream(
                            pipe.stream()
                                .instrument(tracing::info_span!("message receiver")),
                        ),
                    );

                    act.inner.pinned().borrow_mut().queueEmpty = false;
                    act.initial_queue_process_state = QueueProcessState::Starting;
                    act.inner.pinned().borrow().queueEmptyChanged();

                    ctx.set_mailbox_capacity(1);
                    act.inner.pinned().borrow_mut().connected = true;
                    act.ws = Some(ws);
                    act.inner.pinned().borrow().connectedChanged();

                    // If profile stream was running, restart.
                    if let Some(handle) = act.outdated_profile_stream_handle.take() {
                        ctx.cancel_future(handle);
                    }
                    act.outdated_profile_stream_handle = Some(
                        ctx.add_stream(
                            OutdatedProfileStream::new(act.storage.clone().unwrap())
                                .instrument(tracing::info_span!("outdated profile stream")),
                        ),
                    );
                }
                Err(e) => {
                    tracing::error!("Error starting stream: {}", e);
                    tracing::info!("Retrying in 10");
                    let addr = ctx.address();
                    actix::spawn(async move {
                        actix::clock::sleep(Duration::from_secs(10)).await;
                        addr.send(Restart).await.expect("retry restart");
                    });
                }
            }),
        )
    }
}

/// Queue a force-refresh of a profile fetch
#[derive(Message, Debug)]
#[rtype(result = "()")]
pub enum RefreshProfile {
    BySession(i32),
    ByRecipientId(i32),
}

impl Handler<RefreshProfile> for ClientActor {
    type Result = ();

    fn handle(&mut self, profile: RefreshProfile, _ctx: &mut Self::Context) {
        let _span = tracing::trace_span!("ClientActor::RefreshProfile({:?})", ?profile).entered();
        let storage = self.storage.as_ref().unwrap();
        let recipient = match profile {
            RefreshProfile::BySession(session_id) => {
                match storage.fetch_session_by_id(session_id).map(|x| x.r#type) {
                    Some(SessionType::DirectMessage(recipient)) => recipient,
                    None => {
                        tracing::error!("No session with id {}", session_id);
                        return;
                    }
                    _ => {
                        tracing::error!("Can only refresh profiles for DirectMessage sessions.");
                        return;
                    }
                }
            }
            RefreshProfile::ByRecipientId(id) => match storage.fetch_recipient_by_id(id) {
                Some(r) => r,
                None => {
                    tracing::error!("No recipient with id {}", id);
                    return;
                }
            },
        };
        storage.mark_profile_outdated(&recipient);
        // Polling the actor will poll the OutdatedProfileStream, which should immediately fire the
        // necessary events.  This is hacky (XXX), we should in fact wake the stream somehow to ensure
        // correct behaviour.
    }
}

impl StreamHandler<Result<Incoming, ServiceError>> for ClientActor {
    fn handle(&mut self, msg: Result<Incoming, ServiceError>, ctx: &mut Self::Context) {
        let (guid, msg) = match msg {
            Ok(Incoming::Envelope(e)) => {
                let guid = e.server_guid.clone().unwrap();
                (guid, e)
            }
            Ok(Incoming::QueueEmpty) => {
                tracing::info!("Message queue is empty!");
                self.initial_queue_process_state.observe_signal();
                let inner = self.inner.pinned();
                let mut inner = inner.borrow_mut();
                if self.initial_queue_process_state.is_done() && !inner.queueEmpty {
                    inner.queueEmpty = true;
                    inner.queueEmptyChanged();
                }
                return;
            }
            Err(e) => {
                // XXX: we might want to dispatch on this error.
                tracing::error!("MessagePipe pushed an error: {:?}", e);
                return;
            }
        };

        if msg.destination_service_id.is_none() {
            tracing::warn!("Message has no destination service id; ignoring");
            return;
        }
        let incoming_address = msg.destination_address();
        if ![
            self.self_aci.map(ServiceId::from),
            self.self_pni.map(ServiceId::from),
        ]
        .iter()
        .any(|self_dest| self_dest == &Some(incoming_address))
        {
            tracing::warn!(
                "Message destination {:?} doesn't match our ACI or PNI. Dropping.",
                incoming_address
            );
            return;
        }

        let mut cipher = self.cipher(incoming_address.kind());

        let storage = self.storage.clone().expect("initialized storage");

        self.initial_queue_process_state.observe_guid(&guid);

        ctx.spawn(
            async move {
                let mut visited = false;
                let content = loop {
                    match cipher.open_envelope(msg.clone(), &mut rand::rng()).await {
                        Ok(Some(content)) => {
                            storage.mark_recipient_registered(content.metadata.sender, true);
                            break Some(content);
                        }
                        Ok(None) => {
                            tracing::warn!("Empty envelope");
                            break None;
                        }
                        Err(ServiceError::SignalProtocolError(
                            SignalProtocolError::UntrustedIdentity(dest_protocol_address),
                        )) => {
                            // This branch is the only one that loops, and it *should not* loop more than once.
                            if visited {
                                tracing::warn!("ServiceError::SignalProtocolError visited more than once!");
                            }
                            visited = true;

                            let dest_address = ServiceId::parse_from_service_id_string(dest_protocol_address.name()).expect("valid ACI or PNI UUID in ProtocolAddress");
                            tracing::warn!("Untrusted identity for {}; replacing identity and inserting a warning.", dest_protocol_address);
                            let recipient = storage.fetch_or_insert_recipient_by_address(&dest_address);
                            if dest_address.kind() == ServiceIdKind::Pni {
                                storage.mark_recipient_needs_pni_signature(&recipient, true);
                            }
                            let session = storage.fetch_or_insert_session_by_recipient_id(recipient.id);
                            let msg = crate::store::NewMessage {
                                session_id: session.id,
                                source_addr: Some(dest_address),
                                message_type: Some(MessageType::IdentityKeyChange),
                                // XXX: Message timer?
                                ..crate::store::NewMessage::new_incoming()
                            };
                            storage.create_message(&msg);

                            if !recipient.is_registered {
                                tracing::warn!("Recipient was marked as unregistered, marking as registered.");
                                storage.mark_recipient_registered(dest_address, true);
                            }

                            if !storage.delete_identity_key(&dest_address) {
                                tracing::error!("Could not remove identity key for {}.  Please file a bug.", dest_protocol_address);
                                break None;
                            }
                        }
                        Err(e) => {
                            tracing::error!("Error opening envelope: {:?}", e);
                            break None;
                        }
                    }
                };

                if let Some(content) = content.as_ref() {
                    tracing::trace!(sender = content.metadata.sender.service_id_string(), "opened envelope");
                }

                content
            }.instrument(tracing::trace_span!("opening envelope", incoming_address=incoming_address.service_id_string()))
            .into_actor(self)
            .map(move |content, act, ctx| {
                if let Some(content) = content {
                    act.process_envelope(content, ctx);
                }

                act.initial_queue_process_state
                    .processed_guid(&guid);

                let inner = act.inner.pinned();
                let mut inner = inner.borrow_mut();
                if act.initial_queue_process_state.is_done() && !inner.queueEmpty {
                    inner.queueEmpty = true;
                    inner.queueEmptyChanged();
                }
            }),
        );
    }

    /// Called when the WebSocket somehow has disconnected.
    fn finished(&mut self, ctx: &mut Self::Context) {
        tracing::debug!("Attempting reconnect");

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();

        ctx.notify(Restart);
    }
}

#[derive(Message)]
#[rtype(result = "Result<VerificationCodeResponse, anyhow::Error>")]
pub struct Register {
    pub phonenumber: PhoneNumber,
    pub password: String,
    pub transport: VerificationTransport,
    pub captcha: Option<String>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum VerificationCodeResponse {
    Issued,
    CaptchaRequired,
}

impl Handler<Register> for ClientActor {
    type Result = ResponseActFuture<Self, Result<VerificationCodeResponse, anyhow::Error>>;

    fn handle(&mut self, reg: Register, _ctx: &mut Self::Context) -> Self::Result {
        let Register {
            phonenumber,
            password,
            transport,
            captcha,
        } = reg;

        let mut push_service = self.authenticated_service_with_credentials(ServiceCredentials {
            aci: None,
            pni: None,
            phonenumber: phonenumber.clone(),
            password: Some(password.clone()),
            signaling_key: None,
            device_id: None, // !77
        });

        let session = self.registration_session.clone();

        // XXX add profile key when #192 implemneted
        let registration_procedure = async move {
            let mut session = if let Some(session) = session {
                session
            } else {
                let number = phonenumber.to_string();
                let carrier = phonenumber.carrier();
                let (mcc, mnc) = if let Some(carrier) = carrier {
                    (Some(&carrier[0..3]), Some(&carrier[3..]))
                } else {
                    (None, None)
                };
                push_service
                    .create_verification_session(&number, None, mcc, mnc)
                    .await?
            };

            if session.captcha_required() {
                let captcha = captcha
                    .as_deref()
                    .map(|captcha| captcha.trim())
                    .and_then(|captcha| captcha.strip_prefix("signalcaptcha://"));
                session = push_service
                    .patch_verification_session(&session.id, None, None, None, captcha, None)
                    .await?;
            }

            if session.captcha_required() {
                return Ok((session, VerificationCodeResponse::CaptchaRequired));
            }

            if session.push_challenge_required() {
                anyhow::bail!("Push challenge requested after captcha is accepted.");
            }

            if !session.allowed_to_request_code {
                anyhow::bail!(
                    "Not allowed to request verification code, reason unknown: {:?}",
                    session
                );
            }

            session = push_service
                .request_verification_code(&session.id, "whisperfish", transport)
                .await?;
            Ok((session, VerificationCodeResponse::Issued))
        };

        Box::pin(
            registration_procedure
                .into_actor(self)
                .map(|result, act, _ctx| {
                    let (session, result) = result?;
                    act.registration_session = Some(session);
                    Ok(result)
                }),
        )
    }
}

#[derive(Message)]
// XXX Refactor this into a ConfirmRegistrationResult struct
#[rtype(result = "Result<(Storage, VerifyAccountResponse), anyhow::Error>")]
// XXX Maybe we can merge some fields from the linked registration into this struct.
pub struct ConfirmRegistration {
    pub phonenumber: PhoneNumber,
    pub password: String,
    pub storage_password: Option<String>,
    pub confirm_code: String,
}

impl Handler<ConfirmRegistration> for ClientActor {
    // storage, response
    type Result = ResponseActFuture<Self, Result<(Storage, VerifyAccountResponse), anyhow::Error>>;

    fn handle(&mut self, confirm: ConfirmRegistration, _ctx: &mut Self::Context) -> Self::Result {
        use libsignal_service::provisioning::*;

        let ConfirmRegistration {
            phonenumber,
            password,
            storage_password,
            confirm_code,
        } = confirm;

        let registration_id = generate_registration_id(&mut rand::rng());
        let pni_registration_id = generate_registration_id(&mut rand::rng());
        tracing::trace!("registration_id: {}", registration_id);
        tracing::trace!("pni_registration_id: {}", pni_registration_id);

        assert!(
            self.storage.is_none(),
            "Storage already initialized while registering"
        );
        let config = self.config.clone();

        let mut push_service = self.authenticated_service_with_credentials(ServiceCredentials {
            aci: None,
            pni: None,
            phonenumber,
            password: Some(password.clone()),
            signaling_key: None,
            device_id: None, // !77
        });
        let mut session = self
            .registration_session
            .clone()
            .expect("confirm registration after creating registration session");

        let confirmation_procedure = async move {
            let storage_dir = config.get_share_dir().to_owned().into();
            let storage = Storage::new(
                config.clone(),
                &storage_dir,
                storage_password.as_deref(),
                registration_id,
                pni_registration_id,
                &password,
                None,
                None,
            );

            // XXX centralize the place where attributes are generated.
            let account_attrs = AccountAttributes {
                signaling_key: None,
                registration_id,
                voice: false,
                video: false,
                fetches_messages: true,
                pin: None,
                registration_lock: None,
                unidentified_access_key: None,
                unrestricted_unidentified_access: false,
                discoverable_by_phone_number: true,
                capabilities: whisperfish_device_capabilities(),
                name: Some("Whisperfish".into()),
                pni_registration_id,
            };
            session = push_service
                .submit_verification_code(&session.id, &confirm_code)
                .await?;
            if !session.verified {
                anyhow::bail!("Session is not verified");
            }

            // Only now await the Storage,
            // then we know it is not created unless we are 99% sure we'll actually need it.
            let storage = storage.await?;
            let mut aci_store = storage.aci_storage();
            let mut pni_store = storage.pni_storage();

            // XXX: should we already supply a profile key?
            let mut account_manager = AccountManager::new(push_service, None);

            // XXX: We explicitely opt out of skipping device transfer (the false argument). Double
            //      check whether that's what we want!
            let result = account_manager
                .register_account(
                    &mut rand::rng(),
                    RegistrationMethod::SessionId(&session.id),
                    account_attrs,
                    &mut aci_store,
                    &mut pni_store,
                    false,
                )
                .await?;

            Ok((storage, result))
        };

        Box::pin(
            confirmation_procedure
                .into_actor(self)
                .map(move |result, act, _ctx| {
                    let (storage, result) = result?;
                    act.registration_session = None;
                    act.storage = Some(storage.clone());
                    Ok((storage, result))
                }),
        )
    }
}

#[derive(Message)]
#[rtype(result = "Result<RegisterLinkedResponse, anyhow::Error>")]
pub struct RegisterLinked {
    pub device_name: String,
    pub password: String,
    pub storage_password: Option<String>,
    pub tx_uri: futures::channel::oneshot::Sender<String>,
}

pub struct RegisterLinkedResponse {
    pub storage: Storage,

    pub phone_number: PhoneNumber,
    pub aci_regid: u32,
    pub pni_regid: u32,
    pub device_id: protocol::DeviceId,
    pub service_ids: ServiceIds,
    pub aci_identity_key_pair: protocol::IdentityKeyPair,
    pub pni_identity_key_pair: protocol::IdentityKeyPair,
    pub profile_key: [u8; 32],
}

impl Handler<RegisterLinked> for ClientActor {
    type Result = ResponseActFuture<Self, Result<RegisterLinkedResponse, anyhow::Error>>;

    fn handle(&mut self, reg: RegisterLinked, _ctx: &mut Self::Context) -> Self::Result {
        use libsignal_service::provisioning::*;

        let push_service = self.unauthenticated_service();

        let (tx, mut rx) = futures::channel::mpsc::channel(1);

        assert!(
            self.storage.is_none(),
            "Storage already initialized while registering"
        );

        let config = self.config.clone();

        let registration_procedure = async move {
            let share_dir = config.get_share_dir().to_owned().into();
            let storage = Storage::new(
                config.clone(),
                &share_dir,
                reg.storage_password.as_deref(),
                0, // Temporary regids
                0,
                &reg.password,
                None,
                None,
            );
            // XXX This could also be a return value probably.
            let storage = storage.await?;
            let mut tx_uri = Some(reg.tx_uri);
            let mut aci_store = storage.aci_storage();
            let mut pni_store = storage.pni_storage();

            let (fut1, fut2) = future::join(
                link_device(
                    &mut aci_store,
                    &mut pni_store,
                    &mut rand::rng(),
                    push_service,
                    &reg.password,
                    &reg.device_name,
                    tx,
                ),
                async move {
                    let mut res = Result::<RegisterLinkedResponse, anyhow::Error>::Err(
                        anyhow::Error::msg("Registration timed out"),
                    );
                    while let Some(provisioning_step) = rx.next().await {
                        match provisioning_step {
                            SecondaryDeviceProvisioning::Url(url) => {
                                tracing::info!(
                                    %url,
                                    "generating qrcode from provisioning link",
                                );
                                tx_uri
                                    .take()
                                    .expect("that only one URI is emitted by provisioning code")
                                    .send(url.to_string())
                                    .expect("to forward provisioning URL to caller");
                            }
                            SecondaryDeviceProvisioning::NewDeviceRegistration(
                                NewDeviceRegistration {
                                    phone_number,
                                    device_id,
                                    registration_id: aci_regid,
                                    pni_registration_id: pni_regid,
                                    profile_key: ProfileKey { bytes: profile_key },
                                    service_ids,
                                    aci_private_key,
                                    aci_public_key,
                                    pni_private_key,
                                    pni_public_key,
                                },
                            ) => {
                                let aci_identity_key_pair =
                                    protocol::IdentityKeyPair::new(aci_public_key, aci_private_key);
                                let pni_identity_key_pair =
                                    protocol::IdentityKeyPair::new(pni_public_key, pni_private_key);
                                let mut aci_store = storage.aci_storage();
                                let mut pni_store = storage.pni_storage();
                                aci_store.write_local_registration_id(aci_regid).await?;
                                pni_store.write_local_registration_id(pni_regid).await?;
                                aci_store
                                    .write_identity_key_pair(aci_identity_key_pair)
                                    .await?;
                                pni_store
                                    .write_identity_key_pair(pni_identity_key_pair)
                                    .await?;

                                res = Ok(RegisterLinkedResponse {
                                    storage: storage.clone(),
                                    phone_number,
                                    aci_regid,
                                    pni_regid,
                                    device_id,
                                    service_ids,
                                    aci_identity_key_pair,
                                    pni_identity_key_pair,
                                    profile_key,
                                });
                            }
                        }
                    }
                    res
                },
            )
            .await;

            fut1?;
            fut2
        };

        Box::pin(
            registration_procedure
                .into_actor(self)
                .map(move |result, _act, _ctx| {
                    let response = result?;
                    tracing::info!("Registration successful");
                    _act.storage = Some(response.storage.clone());
                    Ok(response)
                }),
        )
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RefreshPreKeys;

/// Java's RefreshPreKeysJob
impl Handler<RefreshPreKeys> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, _: RefreshPreKeys, _ctx: &mut Self::Context) -> Self::Result {
        let service = self.authenticated_service();
        // XXX add profile key when #192 implemneted
        let mut am = AccountManager::new(service, None);
        let storage = self.storage.clone().unwrap();

        let pni_distribution = self.migration_state.pni_distributed();

        let proc = async move {
            let mut aci = storage.aci_storage();
            let mut pni = storage.pni_storage();

            // It's tempting to run those two in parallel,
            // but I'm afraid the pre-key counts are going to be mixed up.
            am.update_pre_key_bundle(&mut aci, ServiceIdKind::Aci, true, &mut rand::rng())
                .await
                .context("refreshing ACI pre keys")?;

            let _pni_distribution = pni_distribution.await;

            am.update_pre_key_bundle(&mut pni, ServiceIdKind::Pni, true, &mut rand::rng())
                .await
                .context("refreshing PNI pre keys")?;
            anyhow::Result::<()>::Ok(())
        }
        .instrument(tracing::trace_span!("RefreshPreKeys"));
        // XXX: store the last refresh time somewhere.

        Box::pin(proc.into_actor(self).map(move |result, _act, _ctx| {
            if let Err(e) = result {
                tracing::error!("refresh pre keys failed: {:#}", e);
            } else {
                tracing::trace!("successfully refreshed prekeys");
            }
        }))
    }
}

// methods called from Qt
impl ClientWorker {
    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn compact_db(&self) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(CompactDb).await {
                tracing::error!("{:?} in compact_db()", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    #[allow(non_snake_case)]
    pub fn fetchAttachment(&self, attachment_id: i32) {
        self.actor
            .as_ref()
            .unwrap()
            .try_send(FetchAttachment { attachment_id })
            .unwrap();
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn reconnect(&self) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(Restart).await {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn delete_file(&self, file_name: String) {
        let result = remove_file(&file_name);
        match result {
            Ok(()) => {
                tracing::trace!("Deleted file {}", file_name);
            }
            Err(e) => {
                tracing::trace!("Could not delete file {}: {:?}", file_name, e);
            }
        };
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn refresh_profile(&self, recipient_id: i32) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor
                .send(RefreshProfile::ByRecipientId(recipient_id))
                .await
            {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn upload_profile(
        &self,
        given_name: String,
        family_name: String,
        about: String,
        emoji: String,
    ) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor
                .send(UpdateProfile {
                    given_name,
                    family_name,
                    about,
                    emoji,
                })
                .await
            {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    pub fn mark_messages_read(&self, mut msg_id_list: QVariantList) {
        let mut message_ids: Vec<i32> = vec![];
        while !msg_id_list.is_empty() {
            let msg_id_qvar = msg_id_list.remove(0);
            // QMetaType::Int = 2
            if msg_id_qvar.user_type() == 2 {
                message_ids.push(msg_id_qvar.to_int().try_into().unwrap());
            }
        }

        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(MarkMessagesRead(message_ids)).await {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn submit_proof_captcha(&self, token: String, response: String) {
        let actor = self.actor.clone().unwrap();
        let schema = "signalcaptcha://";
        let response = if response.starts_with(schema) {
            response.strip_prefix("signalcaptcha://").unwrap().into()
        } else {
            response
        };
        actix::spawn(async move {
            if let Err(e) = actor
                .send(ProofResponse {
                    kind: "recaptcha".into(),
                    token,
                    response,
                })
                .await
            {
                tracing::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn send_typing_notification(&self, session_id: i32, is_start: bool) {
        if session_id < 0 {
            tracing::warn!("Bad session ID {session_id}, ignoring.");
            return;
        };
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(SendTypingNotification {
                    session_id,
                    is_start,
                })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    #[allow(non_snake_case)]
    fn linkRecipient(&self, recipient_id: i32, external_id: String) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(LinkRecipient {
                    recipient_id,
                    external_id: Some(external_id),
                })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    #[allow(non_snake_case)]
    fn unlinkRecipient(&self, recipient_id: i32) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(LinkRecipient {
                    recipient_id,
                    external_id: None,
                })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    #[allow(non_snake_case)]
    fn sendConfiguration(&self) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(SendConfiguration)
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    #[allow(non_snake_case)]
    fn handleMessageRequest(&self, recipient_aci: String, action: String) {
        if let Ok(aci) = Uuid::parse_str(recipient_aci.as_str()) {
            match action.as_str() {
                "accept" => {
                    actix::spawn(
                        self.actor
                            .as_ref()
                            .unwrap()
                            .send(MessageRequestAnswer {
                                thread: ThreadIdentifier::Aci(aci),
                                action: MessageRequestAction::Accept,
                            })
                            .map(Result::unwrap),
                    );
                }
                "block" => {
                    actix::spawn(
                        self.actor
                            .as_ref()
                            .unwrap()
                            .send(MessageRequestAnswer {
                                thread: ThreadIdentifier::Aci(aci),
                                action: MessageRequestAction::Block,
                            })
                            .map(Result::unwrap),
                    );
                }
                _ => tracing::warn!(
                    "Unrecognized recipient message request handle action: {}",
                    action
                ),
            }
        } else {
            tracing::warn!("QML requested unparsable ACI for recipient accept/block");
        }
    }

    #[with_executor]
    #[allow(non_snake_case)]
    fn updateAnnouncementsOnlyMode(&self, group_id: String, enabled: bool) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(UpdateAnnouncementsOnly { group_id, enabled })
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    fn search(&self, search_string: String, session_id: i32) {
        let session = if session_id > -1 {
            Some(session_id)
        } else {
            None
        };
        let text = if !search_string.is_empty() {
            Some(search_string)
        } else {
            None
        };
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(Search(text, session))
                .map(Result::unwrap),
        );
    }

    #[with_executor]
    #[allow(non_snake_case)]
    fn clearSearch(&self) {
        actix::spawn(
            self.actor
                .as_ref()
                .unwrap()
                .send(Search(None, None))
                .map(Result::unwrap),
        );
    }
}

impl Handler<CompactDb> for ClientActor {
    type Result = usize;

    fn handle(&mut self, _: CompactDb, _ctx: &mut Self::Context) -> Self::Result {
        tracing::trace!("handle(CompactDb)");
        let store = self.storage.clone().unwrap();
        store.compact_db()
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct MarkMessagesRead(Vec<i32>);

impl Handler<MarkMessagesRead> for ClientActor {
    type Result = ResponseFuture<()>;

    fn handle(&mut self, read: MarkMessagesRead, ctx: &mut Self::Context) -> Self::Result {
        let read = read.0;
        let storage = self.storage.clone().unwrap();
        let handle = self.message_expiry_notification_handle.clone().unwrap();
        let send_receipts = self.settings.get_enable_read_receipts();
        self.handle_needs_read_receipts(ctx, read.clone(), send_receipts);
        Box::pin(
            async move {
                storage.mark_messages_read_in_ui(read);
                handle.send(()).expect("send messages expiry notification");
            }
            .instrument(tracing::debug_span!("mark messages read")),
        )
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProofRequired {
    token: String,
    kind: String,
}

impl Handler<ProofRequired> for ClientActor {
    type Result = ();

    fn handle(&mut self, proof: ProofRequired, _ctx: &mut Self::Context) -> Self::Result {
        self.inner
            .pinned()
            .borrow()
            .proofRequested(proof.token.into(), proof.kind.into());
    }
}

#[allow(unused)]
#[derive(Message)]
#[rtype(result = "()")]
pub struct ProofResponse {
    kind: String,
    token: String,
    response: String,
}

impl Handler<ProofResponse> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, proof: ProofResponse, ctx: &mut Self::Context) -> Self::Result {
        let span = tracing::trace_span!("handle ProofResponse");

        let storage = self.storage.clone().unwrap();
        let profile_key = storage.fetch_self_recipient_profile_key().map(|bytes| {
            let mut key = [0u8; 32];
            key.copy_from_slice(&bytes);
            ProfileKey::create(key)
        });

        let service = self.authenticated_service();
        let mut am = AccountManager::new(service, profile_key);

        let addr = ctx.address();

        let proc = async move {
            am.submit_recaptcha_challenge(&proof.token, &proof.response)
                .await
        }
        .instrument(span);

        Box::pin(proc.into_actor(self).map(move |result, _act, _ctx| {
            actix::spawn(async move {
                if let Err(e) = result {
                    tracing::error!("Error sending signalcaptcha proof: {}", e);
                    addr.send(ProofAccepted { result: false }).await
                } else {
                    tracing::trace!("Successfully sent signalcaptcha proof");
                    addr.send(ProofAccepted { result: true }).await
                }
            });
        }))
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProofAccepted {
    result: bool,
}

impl Handler<ProofAccepted> for ClientActor {
    type Result = ();

    fn handle(&mut self, accepted: ProofAccepted, _ctx: &mut Self::Context) {
        self.inner
            .pinned()
            .borrow_mut()
            .proofCaptchaResult(accepted.result);
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteMessage(pub i32);

impl Handler<DeleteMessage> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteMessage(id): DeleteMessage,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage.as_mut().unwrap().delete_message(id);
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct DeleteMessageForAll(pub i32);

impl Handler<DeleteMessageForAll> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        DeleteMessageForAll(id): DeleteMessageForAll,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        self.clear_transient_timstamps();

        let storage = self.storage.as_mut().unwrap();
        let profile_key = storage.fetch_self_recipient_profile_key();

        let message = storage
            .fetch_message_by_id(id)
            .expect("message to delete by id");
        let session = storage
            .fetch_session_by_id(message.session_id)
            .expect("session to delete message from by id");

        let now = Utc::now().timestamp_millis() as u64;
        self.transient_timestamps.insert(now);

        let delete_message = DeliverMessage {
            content: DataMessage {
                group_v2: session.group_context_v2(),
                profile_key,
                timestamp: Some(now),
                delete: Some(Delete {
                    target_sent_timestamp: Some(naive_chrono_to_millis(message.server_timestamp)),
                }),
                required_protocol_version: Some(4),
                ..Default::default()
            },
            for_story: false,
            timestamp: now,
            online: false,
            session_type: session.r#type,
        };

        // XXX: We can't get a result back, I think we should?
        ctx.notify(delete_message);
        storage.delete_message(message.id);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ExportAttachment {
    pub attachment_id: i32,
}

impl Handler<ExportAttachment> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        ExportAttachment { attachment_id }: ExportAttachment,
        _ctx: &mut Self::Context,
    ) {
        let storage = self.storage.as_mut().unwrap();

        // 1) Chech the attachment

        let attachment = storage.fetch_attachment(attachment_id);
        if attachment.is_none() {
            tracing::error!(
                "Attachment id {} doesn't exist, can't export it!",
                attachment_id
            );
            return;
        }
        let attachment = attachment.unwrap();
        if attachment.attachment_path.is_none() {
            tracing::error!(
                "Attachment id {} has no path stored, can't export it!",
                attachment_id
            );
            return;
        }

        // 2) Check the source file

        let source = PathBuf::from_str(&attachment.absolute_attachment_path().unwrap()).unwrap();
        if !source.exists() {
            tracing::error!(
                "Attachment {} doesn't exist anymore, not exporting!",
                source.to_str().unwrap()
            );
            return;
        }

        // 3) Check the target dir

        let target_dir = (if attachment.content_type.starts_with("image") {
            dirs::picture_dir()
        } else if attachment.content_type.starts_with("audio") {
            dirs::audio_dir()
        } else if attachment.content_type.starts_with("video") {
            dirs::video_dir()
        } else {
            dirs::download_dir()
        })
        .unwrap()
        .join("Whisperfish");

        if !std::path::Path::exists(&target_dir) && std::fs::create_dir(&target_dir).is_err() {
            tracing::error!(
                "Couldn't create directory {}, can't export attachment!",
                target_dir.to_str().unwrap()
            );
            return;
        }

        // 4) Check free space
        let free_space = fs2::free_space(&target_dir).expect("checking free space");
        let file_size = std::fs::metadata(&source)
            .expect("attachment file size")
            .len();
        if (free_space - file_size) < (100 * 1024 * 1024) {
            // 100 MiB
            tracing::error!("Not enough free space after copying, not exporting the attachment!");
            return;
        };

        // 5) Check the target filename

        let mut target = match attachment.file_name {
            Some(name) => target_dir.join(name),
            None => target_dir.join(source.file_name().unwrap()),
        };

        let basename = target
            .file_stem()
            .expect("attachment filename (before the dot)")
            .to_owned();
        let basename = basename.to_str().unwrap();
        let mut count = 0;
        while target.exists() {
            count += 1;
            if target.extension().is_some() {
                target.set_file_name(format!(
                    "{}-{}.{}",
                    basename,
                    count,
                    target.extension().unwrap().to_str().unwrap()
                ));
            } else {
                target.set_file_name(format!("{}-{}", basename, count));
            }
        }
        let target = target.to_str().unwrap();

        // 6) Copy the file

        match std::fs::copy(source, target) {
            Err(e) => tracing::trace!("Copying attachment failed: {}", e),
            Ok(size) => tracing::trace!(
                "Attachent {} exported to {} ({} bytes)",
                attachment_id,
                target,
                size
            ),
        };
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct LinkRecipient {
    pub recipient_id: i32,
    pub external_id: Option<String>,
}

impl Handler<LinkRecipient> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        LinkRecipient {
            recipient_id,
            external_id,
        }: LinkRecipient,
        _ctx: &mut Self::Context,
    ) {
        let storage = self.storage.as_mut().unwrap();
        storage.set_recipient_external_id(recipient_id, external_id);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct SendConfiguration;

impl Handler<SendConfiguration> for ClientActor {
    type Result = ();

    fn handle(&mut self, _: SendConfiguration, _ctx: &mut Self::Context) {
        if self.config.get_device_id() != *DEFAULT_DEVICE_ID {
            tracing::info!("Not the primary device, ignoring SendConfiguration request");
            return;
        };
        let sender = self.message_sender();
        let configuration = SyncMessage {
            configuration: Some(self.get_configuration()),
            ..SyncMessage::with_padding(&mut rand::rng())
        };

        actix::spawn(async move {
            let mut sender = sender.await.unwrap();

            sender
                .send_sync_message(configuration)
                .await
                .expect("send configuration");
        });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
/// Set or unset group announcements only mode
pub struct UpdateAnnouncementsOnly {
    pub group_id: String,
    pub enabled: bool,
}

impl Handler<UpdateAnnouncementsOnly> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        UpdateAnnouncementsOnly { group_id, enabled }: UpdateAnnouncementsOnly,
        _ctx: &mut Self::Context,
    ) {
        let storage = self.storage.as_mut().unwrap().clone();
        let Some(group_v2) = storage.fetch_group_by_group_v2_id(&group_id) else {
            tracing::error!("No such group: '{}'", group_id);
            return;
        };
        let Some(self_member) = storage.fetch_group_v2_self_member(&group_id) else {
            tracing::error!("You are not a member of the group '{}'", group_v2.name);
            return;
        };
        if self_member.role < libsignal_service::proto::member::Role::Administrator as i32 {
            tracing::error!("You are not admin in the group '{}'", group_v2.name);
            return;
        }
        storage.update_group_v2_announcement_only(&group_v2, enabled);
        storage.observe_update(
            whisperfish_store::schema::group_v2s::dsl::group_v2s,
            group_id,
        );
        // TODO: libsignal-service-rs doesn't support sending group updates
        tracing::warn!("Sending group changes is not yet supported. Changes are local only.");
    }
}

#[derive(Message)]
#[rtype(result = "()")]
/// Set recipient into accepted or blocked state
pub struct MessageRequestAnswer {
    pub thread: ThreadIdentifier,
    pub action: MessageRequestAction,
}

impl Handler<MessageRequestAnswer> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        MessageRequestAnswer { thread, action }: MessageRequestAnswer,
        _ctx: &mut Self::Context,
    ) {
        let storage = self.storage.as_mut().unwrap().clone();
        match thread {
            ThreadIdentifier::Aci(aci) => {
                let address = Aci::from(aci);
                match action {
                    MessageRequestAction::Accept => {
                        storage.mark_recipient_accepted(&address.into());
                    }
                    MessageRequestAction::Block => {
                        storage.mark_recipient_blocked(&address.into());
                    }
                    _ => {
                        tracing::error!("Unimplemented message request action: {:?}", action);
                        return;
                    }
                }
            }
            ThreadIdentifier::Group(_group) => {
                tracing::warn!("Group message request responses are not yet implemented. Please upvote bug #327");
                return;
            }
        }

        let self_addr = Aci::from(self.config.get_aci().expect("valid uuid at this point"));
        let sender = self.message_sender();
        actix::spawn(async move {
            let sender = sender.await;
            if let Err(e) = sender {
                tracing::error!("message sender failed: {}", e);
                return;
            }
            let mut sender = sender.unwrap();

            let result = sender
                .send_message_request_response(&self_addr.into(), &thread, action)
                .await;

            if let Err(e) = result {
                tracing::error!("message request response failed: {}", e);
            }
        });
    }
}

#[derive(Message)]
#[rtype(result = "()")]
/// Search with message contents (or clear results if not given),
/// and from a certain session (or all sessions if not given).
pub struct Search(Option<String>, Option<i32>);

impl Handler<Search> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        Search {
            0: search_text,
            1: session_id,
        }: Search,
        _ctx: &mut Self::Context,
    ) {
        let mut search_results = QVariantList::default();

        let Some(search_text) = search_text else {
            self.inner.pinned().borrow_mut().searchResults = search_results;
            self.inner.pinned().borrow_mut().searchResultsChanged();
            return;
        };

        let storage = self.storage.as_mut().unwrap().clone();
        let messages = storage.search_messages(&search_text, session_id);

        if messages.is_empty() {
            self.inner.pinned().borrow_mut().searchResults = search_results;
            self.inner.pinned().borrow_mut().searchResultsChanged();
            return;
        }

        let sessions = storage.fetch_sessions();
        let mut s_map: HashMap<i32, orm::Session> = HashMap::with_capacity(sessions.len());
        for s in sessions {
            s_map.insert(s.id, s);
        }

        let self_recipient_id = storage.fetch_self_recipient_id();
        let recipients = storage.fetch_recipients();
        let mut r_map: HashMap<i32, orm::Recipient> = HashMap::with_capacity(recipients.len());
        for r in recipients {
            r_map.insert(r.id, r);
        }

        for m in messages {
            let Some(s) = s_map.get(&m.session_id) else {
                tracing::error!(
                    "Session {} for message {} doesn't exist?",
                    m.session_id,
                    m.id
                );
                continue;
            };
            let (chat_name, sender_name) = match &s.r#type {
                SessionType::GroupV1(_) => continue,
                SessionType::GroupV2(g) => {
                    let grp_r = r_map
                        .get(&m.sender_recipient_id.unwrap_or(self_recipient_id))
                        .unwrap();
                    (
                        g.name.to_owned(),
                        grp_r
                            .profile_joined_name
                            .to_owned()
                            .unwrap_or_else(|| grp_r.e164_or_address()),
                    )
                }
                SessionType::DirectMessage(dm_r) => {
                    let a = dm_r
                        .profile_joined_name
                        .to_owned()
                        .unwrap_or_else(|| dm_r.e164_or_address());
                    let b = r_map
                        .get(
                            &(if m.is_outbound {
                                m.sender_recipient_id.unwrap()
                            } else {
                                self_recipient_id
                            }),
                        )
                        .map(|snd_r| {
                            snd_r
                                .profile_joined_name
                                .to_owned()
                                .unwrap_or_else(|| snd_r.e164_or_address())
                        })
                        .unwrap();
                    if m.is_outbound {
                        (a, b)
                    } else {
                        (b, a)
                    }
                }
            };

            let mut result = QVariantMap::default();
            result.insert("messageId".into(), QVariant::from(m.id));
            result.insert("sessionId".into(), QVariant::from(m.session_id));
            result.insert("isOutbound".into(), QVariant::from(m.is_outbound));
            result.insert("chatName".into(), chat_name.to_qvariant());
            result.insert("senderName".into(), sender_name.to_qvariant());
            result.insert("text".into(), m.text.unwrap().to_qvariant());
            result.insert(
                "timestamp".into(),
                m.server_timestamp.to_string().to_qvariant(),
            );
            search_results.push(result.to_qvariant());
        }
        self.inner.pinned().borrow_mut().searchResults = search_results;
        self.inner.pinned().borrow_mut().searchResultsChanged();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_message_without_attachments() {
        let attachments = vec![];
        let q = QueueMessage {
            attachments,
            session_id: 8,
            message: "Lorem ipsum dolor sit amet".into(),
            quote: 12,
            is_voice_note: false,
        };
        assert_eq!(format!("{}", q), "QueueMessage { session_id: 8, message: \"Lorem ips...\", quote: 12, attachments: \"[]\", is_voice_note: false }");
    }

    #[test]
    fn queue_message_with_one_attachment() {
        let attachments = vec![NewAttachment {
            path: "/path/to/pic.jpg".into(),
            mime_type: "image/jpeg".into(),
        }];
        let q = QueueMessage {
            attachments,
            session_id: 8,
            message: "Lorem ipsum dolor sit amet".into(),
            quote: 12,
            is_voice_note: false,
        };
        assert_eq!(format!("{}", q), "QueueMessage { session_id: 8, message: \"Lorem ips...\", quote: 12, attachments: \"[NewAttachment { path: \"/path/to/pic.jpg\", mime_type: \"image/jpeg\" }]\", is_voice_note: false }");
    }

    #[test]
    fn queue_message_with_multiple_attachments() {
        let attachments = vec![
            NewAttachment {
                path: "/path/to/pic.jpg".into(),
                mime_type: "image/jpeg".into(),
            },
            NewAttachment {
                path: "/path/to/audio.mp3".into(),
                mime_type: "audio/mpeg".into(),
            },
        ];
        let q = QueueMessage {
            attachments,
            session_id: 8,
            message: "Lorem ipsum dolor sit amet".into(),
            quote: 12,
            is_voice_note: false,
        };
        assert_eq!(format!("{}", q), "QueueMessage { session_id: 8, message: \"Lorem ips...\", quote: 12, attachments: \"[NewAttachment { path: \"/path/to/pic.jpg\", mime_type: \"image/jpeg\" }, NewAttachment { path: \"/path/to/audio.mp3\", mime_type: \"audio/mpeg\" }]\", is_voice_note: false }");
    }
}
