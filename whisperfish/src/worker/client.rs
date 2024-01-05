// XXX maybe the session-to-db migration should move into the store module.
pub mod migrations;

mod groupv2;
mod linked_devices;
mod message_expiry;
mod profile;
mod profile_upload;
mod unidentified;

pub use self::groupv2::*;
pub use self::linked_devices::*;
use self::migrations::MigrationCondVar;
pub use self::profile_upload::*;
use self::unidentified::UnidentifiedCertificates;
use libsignal_service::proto::data_message::{Delete, Quote};
use libsignal_service::proto::sync_message::Sent;
use libsignal_service::push_service::RegistrationMethod;
use libsignal_service::sender::SendMessageResult;
use libsignal_service::sender::SentMessage;
use uuid::Uuid;
use whisperfish_store::TrustLevel;
use zkgroup::profiles::ProfileKey;

use super::message_expiry::ExpiredMessagesStream;
use super::profile_refresh::OutdatedProfileStream;
use crate::actor::SendReaction;
use crate::actor::SessionActor;
use crate::gui::StorageReady;
use crate::model::DeviceModel;
use crate::platform::QmlApp;
use crate::store::orm::UnidentifiedAccessMode;
use crate::store::{millis_to_naive_chrono, orm, Storage};
use crate::worker::client::orm::shorten;
use crate::worker::client::unidentified::CertType;
use actix::prelude::*;
use anyhow::Context;
use chrono::prelude::*;
use futures::prelude::*;
use libsignal_service::configuration::SignalServers;
use libsignal_service::content::sync_message::Request as SyncRequest;
use libsignal_service::content::DataMessageFlags;
use libsignal_service::content::{
    sync_message, AttachmentPointer, ContentBody, DataMessage, GroupContextV2, Metadata, Reaction,
    TypingMessage,
};
use libsignal_service::prelude::*;
use libsignal_service::proto::typing_message::Action;
use libsignal_service::proto::{receipt_message, ReceiptMessage};
use libsignal_service::protocol::*;
use libsignal_service::push_service::{
    AccountAttributes, DeviceCapabilities, DeviceId, RegistrationSessionMetadataResponse,
    ServiceIds, VerificationTransport, VerifyAccountResponse,
};
use libsignal_service::sender::AttachmentSpec;
use libsignal_service::websocket::SignalWebSocket;
use libsignal_service::AccountManager;
use libsignal_service_actix::prelude::*;
use mime_classifier::{ApacheBugFlag, LoadContext, MimeClassifier, NoSniffFlag};
use phonenumber::PhoneNumber;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use std::borrow::Cow;
use std::collections::HashSet;
use std::fmt::{Display, Error, Formatter};
use std::fs::remove_file;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Duration;

// Maximum theoretical TypingMessage send rate,
// plus some change for Reaction messages etc.
const TM_MAX_RATE: f32 = 30.0; // messages per minute
const TM_CACHE_CAPACITY: f32 = 5.0; // 5 min
const TM_CACHE_TRESHOLD: f32 = 4.5; // 4 min 30 sec

#[derive(actix::Message, Debug)]
#[rtype(result = "()")]
pub struct QueueMessage {
    pub session_id: i32,
    pub message: String,
    pub attachment: String,
    pub quote: i32,
}

impl Display for QueueMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result<(), Error> {
        write!(
            f,
            "QueueMessage {{ session_id: {}, message: \"{}\", quote: {}, attachment: \"{}\" }}",
            &self.session_id,
            shorten(&self.message, 9),
            &self.quote,
            &self.attachment,
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
    session: orm::Session,
}

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
#[rtype(result = "()")]
struct AttachmentDownloaded {
    session_id: i32,
    message_id: i32,
}

#[derive(Message)]
#[rtype(result = "usize")]
pub struct CompactDb(usize);

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
    messageReceipt: qt_signal!(sid: i32, mid: i32),
    notifyMessage: qt_signal!(
        sid: i32,
        mid: i32,
        sessionName: QString,
        senderName: QString,
        senderIdentifier: QString,
        senderUuid: QString,
        message: QString,
        isGroup: bool
    ),
    promptResetPeerIdentity: qt_signal!(),
    messageSent: qt_signal!(sid: i32, mid: i32, message: QString),
    messageNotSent: qt_signal!(sid: i32, mid: i32),
    // FIXME: Rust "r#type" to Qt "type" doesn't work
    proofRequested: qt_signal!(token: QString, r#type: QString),
    proofCaptchaResult: qt_signal!(success: bool),

    send_typing_notification: qt_method!(fn(&self, id: i32, is_start: bool)),
    submit_proof_captcha: qt_method!(fn(&self, token: String, response: String)),

    connected: qt_property!(bool; NOTIFY connectedChanged),
    connectedChanged: qt_signal!(),

    actor: Option<Addr<ClientActor>>,
    session_actor: Option<Addr<SessionActor>>,
    device_model: Option<QObjectBox<DeviceModel>>,

    // Linked device management
    link_device: qt_method!(fn(&self, tsurl: String)),
    unlink_device: qt_method!(fn(&self, id: i64)),
    reload_linked_devices: qt_method!(fn(&self)),
    compact_db: qt_method!(fn(&self)),

    refresh_group_v2: qt_method!(fn(&self, session_id: usize)),

    delete_file: qt_method!(fn(&self, file_name: String)),

    refresh_profile: qt_method!(fn(&self, recipient_id: i32)),
    upload_profile: qt_method!(
        fn(&self, given_name: String, family_name: String, about: String, emoji: String)
    ),
}

/// ClientActor keeps track of the connection state.
pub struct ClientActor {
    inner: QObjectBox<ClientWorker>,

    migration_state: MigrationCondVar,

    unidentified_certificates: unidentified::UnidentifiedCertificates,
    credentials: Option<ServiceCredentials>,
    local_addr: Option<ServiceAddress>,
    storage: Option<Storage>,
    ws: Option<SignalWebSocket>,
    // XXX The cipher should be behind a Mutex.
    // By considering the session that needs to be accessed,
    // we could lock only a single session to enforce serialized access.
    // That's a lot of code though, and it should probably happen *inside* the ServiceCipher
    // instead.
    // Having ServiceCipher implement `Clone` is imo. a problem, now that everything is `async`.
    // Putting in behind a Mutex is a lot of work now though,
    // especially considering this should be *internal* to ServiceCipher.
    cipher: Option<ServiceCipher<crate::store::Storage, rand::rngs::ThreadRng>>,
    config: std::sync::Arc<crate::config::SignalConfig>,

    transient_timestamps: HashSet<u64>,

    start_time: DateTime<Local>,

    outdated_profile_stream_handle: Option<SpawnHandle>,
    message_expiry_notification_handle: Option<tokio::sync::mpsc::UnboundedSender<()>>,

    registration_session: Option<RegistrationSessionMetadataResponse>,
}

fn whisperfish_device_capabilities() -> DeviceCapabilities {
    DeviceCapabilities {
        announcement_group: false,
        gv2: true,
        storage: false,
        gv1_migration: true,
        sender_key: true,
        change_number: false,
        gift_badges: false,
        stories: false,
    }
}

impl ClientActor {
    pub fn new(
        app: &mut QmlApp,
        session_actor: Addr<SessionActor>,
        config: std::sync::Arc<crate::config::SignalConfig>,
    ) -> Result<Self, anyhow::Error> {
        let inner = QObjectBox::new(ClientWorker::default());
        let device_model = QObjectBox::new(DeviceModel::default());
        app.set_object_property("ClientWorker".into(), inner.pinned());
        app.set_object_property("DeviceModel".into(), device_model.pinned());

        inner.pinned().borrow_mut().session_actor = Some(session_actor);
        inner.pinned().borrow_mut().device_model = Some(device_model);

        let transient_timestamps: HashSet<u64> =
            HashSet::with_capacity((TM_CACHE_CAPACITY * TM_MAX_RATE) as _);

        Ok(Self {
            inner,
            migration_state: MigrationCondVar::new(),
            unidentified_certificates: UnidentifiedCertificates::default(),
            credentials: None,
            local_addr: None,
            storage: None,
            cipher: None,
            ws: None,
            config,

            transient_timestamps,

            start_time: Local::now(),

            outdated_profile_stream_handle: None,
            message_expiry_notification_handle: None,

            registration_session: None,
        })
    }

    fn service_ids(&self) -> Option<ServiceIds> {
        Some(ServiceIds {
            aci: self.config.get_uuid()?,
            pni: self.config.get_pni()?,
        })
    }

    fn user_agent(&self) -> String {
        crate::user_agent()
    }

    fn unauthenticated_service(&self) -> AwcPushService {
        let service_cfg = self.service_cfg();
        AwcPushService::new(service_cfg, None, self.user_agent())
    }

    fn authenticated_service_with_credentials(
        &self,
        credentials: ServiceCredentials,
    ) -> AwcPushService {
        let service_cfg = self.service_cfg();

        AwcPushService::new(service_cfg, Some(credentials), self.user_agent())
    }

    /// Panics if no authentication credentials are set.
    fn authenticated_service(&self) -> AwcPushService {
        self.authenticated_service_with_credentials(self.credentials.clone().unwrap())
    }

    fn message_sender(
        &self,
    ) -> impl Future<
        Output = Result<
            MessageSender<AwcPushService, crate::store::Storage, rand::rngs::ThreadRng>,
            ServiceError,
        >,
    > {
        let storage = self.storage.clone().unwrap();
        let service = self.authenticated_service();
        let mut u_service = self.unauthenticated_service();

        let ws = self.ws.clone().unwrap();
        let cipher = self.cipher.clone().unwrap();
        let local_addr = self.local_addr.unwrap();
        let device_id = self.config.get_device_id();
        async move {
            let u_ws = u_service.ws("/v1/websocket/", None, false).await?;
            Ok(MessageSender::new(
                ws,
                u_ws,
                service,
                cipher,
                rand::thread_rng(),
                storage,
                local_addr,
                device_id,
            ))
        }
    }

    fn service_cfg(&self) -> ServiceConfiguration {
        // XXX: read the configuration files!
        SignalServers::Production.into()
    }

    pub fn clear_transient_timstamps(&mut self) {
        if self.transient_timestamps.len() > (TM_CACHE_CAPACITY * TM_MAX_RATE) as _ {
            // slots / slots_per_minute = minutes
            const DURATION: u64 = (TM_CACHE_TRESHOLD * 60.0 * 1000.0) as _;
            let limit = (Utc::now().timestamp_millis() as u64) - DURATION;

            let len_before = self.transient_timestamps.len();
            self.transient_timestamps.retain(|t| *t > limit);
            log::trace!(
                "Removed {}/{} cached transient timestamps",
                len_before - self.transient_timestamps.len(),
                self.transient_timestamps.len()
            );
        }
    }

    pub fn handle_needs_delivery_receipt(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        message: &DataMessage,
        metadata: &Metadata,
    ) -> Option<()> {
        let uuid = metadata.sender.uuid;
        let storage = self.storage.as_mut().expect("storage");
        let recipient = storage.fetch_recipient(None, Some(uuid))?;
        let session = storage.fetch_or_insert_session_by_recipient_id(recipient.id);

        let content = ReceiptMessage {
            r#type: Some(receipt_message::Type::Delivery as _),
            timestamp: vec![message.timestamp?],
        };

        ctx.notify(DeliverMessage {
            content,
            timestamp: Utc::now().timestamp_millis() as u64,
            // XXX Session ID is artificial here.
            session,
            online: false,
            for_story: false,
        });

        Some(())
    }

    /// Process incoming message from Signal
    ///
    /// This was `MessageHandler` in Go.
    ///
    /// TODO: consider putting this as an actor `Handle<>` implementation instead.
    pub fn handle_message(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        // XXX: remove this argument
        source_phonenumber: Option<PhoneNumber>,
        source_uuid: Option<Uuid>,
        msg: &DataMessage,
        sync_sent: Option<Sent>,
        metadata: &Metadata,
    ) -> Option<i32> {
        let timestamp = metadata.timestamp;
        let settings = crate::config::SettingsBridge::default();
        let is_sync_sent = sync_sent.is_some();

        let mut storage = self.storage.clone().expect("storage");
        let sender_recipient = if source_phonenumber.is_some() || source_uuid.is_some() {
            Some(storage.merge_and_fetch_recipient(
                source_phonenumber.clone(),
                source_uuid,
                None,
                crate::store::TrustLevel::Certain,
            ))
        } else {
            None
        };

        if msg.flags() & DataMessageFlags::EndSession as u32 != 0 {
            let storage = storage.clone();
            if let Some(svc) = sender_recipient
                .as_ref()
                .and_then(|r| r.to_service_address())
            {
                actix::spawn(async move {
                    if let Err(e) = storage.delete_all_sessions(&svc).await {
                        log::error!("End session requested, but could not end session: {:?}", e);
                    }
                });
            } else {
                log::error!("Requested session reset but no service address associated");
            }
        }

        if (source_phonenumber.is_some() || source_uuid.is_some()) && !is_sync_sent {
            if let Some(key) = msg.profile_key.as_deref() {
                let (recipient, was_updated) = storage.update_profile_key(
                    source_phonenumber.clone(),
                    source_uuid,
                    None,
                    key,
                    crate::store::TrustLevel::Certain,
                );
                if was_updated {
                    ctx.notify(RefreshProfile::ByRecipientId(recipient.id));
                }
            }
        }

        if msg.flags() & DataMessageFlags::ProfileKeyUpdate as u32 != 0 {
            log::info!("Message was ProfileKeyUpdate; not inserting.");
        }

        let expiration_timer_update =
            msg.flags() & DataMessageFlags::ExpirationTimerUpdate as u32 != 0;
        let alt_body = if let Some(reaction) = &msg.reaction {
            if let Some((message, session)) = storage.process_reaction(
                &sender_recipient
                    .clone()
                    .or_else(|| storage.fetch_self_recipient())
                    .expect("sender or self-sent"),
                msg,
                reaction,
            ) {
                log::info!("Reaction saved for message {}/{}", session.id, message.id);
                self.inner
                    .pinned()
                    .borrow_mut()
                    .messageReactionReceived(session.id, message.id);
            } else {
                log::error!("Could not find a message for this reaction. Dropping.");
                log::warn!(
                    "This probably indicates out-of-order receipt delivery. Please upvote issue #260"
                );
            }
            None
        } else if expiration_timer_update {
            Some(format!("Expiration timer has been changed ({:?} seconds).  This is only partially implemented in Whisperfish.", msg.expire_timer))
        } else if let Some(GroupContextV2 {
            group_change: Some(ref _group_change),
            ..
        }) = msg.group_v2
        {
            Some(format!(
                "Group changed by {}",
                source_phonenumber
                    .as_ref()
                    .map(PhoneNumber::to_string)
                    .or(source_uuid.as_ref().map(Uuid::to_string))
                    .as_deref()
                    .unwrap_or("nobody")
            ))
        } else if !msg.attachments.is_empty() {
            log::trace!("Received an attachment without body, replacing with empty text.");
            Some("".into())
        } else if msg.sticker.is_some() {
            log::warn!("Received a sticker, but inserting empty message.");
            Some("This is a sticker, but stickers are currently unsupported.".into())
        } else if msg.payment.is_some()
            || msg.group_call_update.is_some()
            || !msg.contact.is_empty()
        {
            Some("Unimplemented message type".into())
        } else {
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
                let own_id = storage
                    .fetch_self_recipient()
                    .expect("self recipient in db")
                    .id;
                // Missing sender_recipient_id => we are the sender
                let sender_id = db_message.sender_recipient_id.unwrap_or(own_id);
                if sender_id != sender_recipient.as_ref().unwrap().id {
                    log::warn!("Received a delete message from a different user, ignoring it.");
                } else {
                    storage.delete_message(db_message.id);
                }
            } else {
                log::warn!(
                    "Message {} not found for deletion!",
                    target_sent_timestamp.timestamp_millis()
                );
            }
        }

        let body = msg.body.clone().or(alt_body);
        let text = if let Some(body) = body {
            body
        } else {
            log::debug!("Message without (alt) body, not inserting");
            return None;
        };

        let is_unidentified = if let Some(sent) = &sync_sent {
            sent.unidentified_status.iter().any(|x| {
                Some(x.destination_uuid()) == source_uuid.as_ref().map(Uuid::to_string).as_deref()
                    && x.unidentified()
            })
        } else {
            metadata.unidentified_sender
        };

        let group = if let Some(group) = msg.group_v2.as_ref() {
            let mut key_stack = [0u8; zkgroup::GROUP_MASTER_KEY_LEN];
            key_stack.clone_from_slice(group.master_key.as_ref().expect("group message with key"));
            let key = GroupMasterKey::new(key_stack);
            let secret = GroupSecretParams::derive_from_master_key(key);

            let store_v2 = crate::store::GroupV2 {
                secret,
                revision: group.revision(),
            };

            // XXX handle group.group_change like a real client
            if let Some(_change) = group.group_change.as_ref() {
                log::warn!("We're not handling raw group changes yet. Let's trigger a group refresh for now.");
                ctx.notify(RequestGroupV2Info(store_v2.clone(), key_stack));
            } else if !storage.group_v2_exists(&store_v2) {
                log::info!(
                    "We don't know this group. We'll request it's structure from the server."
                );
                ctx.notify(RequestGroupV2Info(store_v2.clone(), key_stack));
            }

            Some(storage.fetch_or_insert_session_by_group_v2(&store_v2))
        } else {
            None
        };

        let session = group.unwrap_or_else(|| {
            let recipient = storage.merge_and_fetch_recipient(
                source_phonenumber.clone(),
                source_uuid,
                None,
                TrustLevel::Certain,
            );
            storage.fetch_or_insert_session_by_recipient_id(recipient.id)
        });

        if msg.flags() & DataMessageFlags::ExpirationTimerUpdate as u32 != 0 {
            storage.update_expiration_timer(session.id, msg.expire_timer);
        }

        let new_message = crate::store::NewMessage {
            source_e164: source_phonenumber,
            source_uuid,
            text,
            flags: msg.flags() as i32,
            outgoing: is_sync_sent,
            is_unidentified,
            sent: is_sync_sent,
            timestamp: millis_to_naive_chrono(if is_sync_sent && timestamp > 0 {
                timestamp
            } else {
                msg.timestamp()
            }),
            has_attachment: !msg.attachments.is_empty(),
            mime_type: None, // Attachments are further handled asynchronously
            received: false, // This is set true by a receipt handler
            session_id: session.id,
            attachment: None,
            is_read: is_sync_sent,
            quote_timestamp: msg.quote.as_ref().and_then(|x| x.id),
            // Don't auto-destroy the update message
            expires_in: if !expiration_timer_update {
                session.expiring_message_timeout
            } else {
                None
            },
        };

        let message = storage.create_message(&new_message);

        if settings.get_bool("attachment_log") && !msg.attachments.is_empty() {
            log::trace!("Logging message to the attachment log");
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
            let attachment = storage.register_attachment(message.id, attachment.clone());

            if settings.get_bool("save_attachments") {
                ctx.notify(FetchAttachment {
                    attachment_id: attachment.id,
                });
            }
        }

        self.inner
            .pinned()
            .borrow_mut()
            .messageReceived(session.id, message.id);

        // XXX If from ourselves, skip
        if !is_sync_sent && !session.is_muted {
            let session_name: Cow<'_, str> = match &session.r#type {
                orm::SessionType::GroupV1(group) => Cow::from(&group.name),
                orm::SessionType::GroupV2(group) => Cow::from(&group.name),
                orm::SessionType::DirectMessage(recipient) => recipient.name(),
            };

            self.inner.pinned().borrow_mut().notifyMessage(
                session.id,
                message.id,
                session_name.as_ref().into(),
                sender_recipient
                    .as_ref()
                    .map(|x| x.name().as_ref().into())
                    .unwrap_or_else(|| "".into()),
                sender_recipient
                    .as_ref()
                    .map(|x| x.e164_or_uuid().into())
                    .unwrap_or_else(|| "".into()),
                sender_recipient
                    .map(|x| x.uuid().into())
                    .unwrap_or_else(|| "".into()),
                message.text.as_deref().unwrap_or("").into(),
                session.is_group(),
            );
        }
        Some(message.id)
    }

    fn handle_sync_request(&mut self, meta: Metadata, req: SyncRequest) {
        use sync_message::request::Type;
        log::trace!("Processing sync request {:?}", req.r#type());

        let local_addr = self.local_addr.unwrap();
        let storage = self.storage.clone().unwrap();
        let sender = self.message_sender();

        actix::spawn(async move {
            let mut sender = sender.await?;
            match req.r#type() {
                Type::Unknown => {
                    log::warn!("Unknown sync request from {:?}:{}. Please upgrade Whisperfish or file an issue.", meta.sender, meta.sender_device);
                    return Ok(());
                }
                Type::Contacts => {
                    use libsignal_service::sender::ContactDetails;
                    // In fact, we should query for registered contacts instead of sessions here.
                    // https://gitlab.com/whisperfish/whisperfish/-/issues/133
                    let recipients: Vec<orm::Recipient> = {
                        use crate::store::schema::recipients::dsl::*;
                        use diesel::prelude::*;
                        let mut db = storage.db();
                        recipients.load(&mut *db)?
                    };

                    let contacts = recipients.into_iter().map(|recipient| {
                            ContactDetails {
                                // XXX: expire timer from dm session
                                number: recipient.e164.as_ref().map(PhoneNumber::to_string),
                                uuid: recipient.uuid.as_ref().map(Uuid::to_string),
                                name: recipient.profile_joined_name.clone(),
                                profile_key: recipient.profile_key,
                                // XXX other profile stuff
                                ..Default::default()
                            }
                    });

                    sender.send_contact_details(&local_addr, None, contacts, false, true).await?;
                },
                Type::Groups => {
                    use libsignal_service::sender::GroupDetails;
                    let sessions = storage.fetch_group_sessions();

                    let groups = sessions.into_iter().map(|session| {
                        let group = session.unwrap_group_v1();
                        let members = storage.fetch_group_members_by_group_v1_id(&group.id);
                        GroupDetails {
                            name: Some(group.name.clone()),
                            members_e164: members.iter().filter_map(|(_member, recipient)| recipient.e164.as_ref().map(PhoneNumber::to_string)).collect(),
                            // XXX: update proto file and add more.
                            // members: members.iter().filter_map(|(_member, recipient)| Member {e164: recipient.e164}).collect(),
                            // avatar, active?, color, ..., many cool things to add here!
                            // Tagging issue #204
                            ..Default::default()
                        }
                    });

                    sender.send_groups_details(&local_addr, None, groups, false).await?;
                }
                Type::Blocked => {
                    anyhow::bail!("Unimplemented {:?}", req.r#type());
                }
                Type::Configuration => {
                    anyhow::bail!("Unimplemented {:?}", req.r#type());
                }
                Type::Keys => {
                    anyhow::bail!("Unimplemented {:?}", req.r#type());
                }
                Type::PniIdentity => {
                    anyhow::bail!("Unimplemented {:?}", req.r#type());
                },
            };

            Ok::<_, anyhow::Error>(())
        }.map(|v| if let Err(e) = v {log::error!("{:?} in handle_sync_request()", e)}));
    }

    fn process_receipt(&mut self, msg: &Envelope) {
        let millis = msg.timestamp();

        // If the receipt timestamp matches a transient timestamp,
        // such as a TypingMessage or a sent/updated/removed Reaction,
        // stop processing, since there's no such message in database.
        if self.transient_timestamps.contains(&millis) {
            log::info!("Transient receipt: {}", millis);
            return;
        }

        log::info!("Received receipt: {}", millis);

        let storage = self.storage.as_mut().expect("storage initialized");
        let source = msg.source_address();

        let timestamp = millis_to_naive_chrono(millis);
        log::trace!(
            "Marking message from {:?} at {} ({}) as received.",
            source,
            timestamp,
            millis
        );
        if let Some((sess, msg)) = storage.mark_message_received(source.uuid, timestamp, None) {
            self.inner
                .pinned()
                .borrow_mut()
                .messageReceipt(sess.id, msg.id)
        }
    }

    fn process_envelope(
        &mut self,
        Content { body, metadata }: Content,
        ctx: &mut <Self as Actor>::Context,
    ) {
        let storage = self.storage.clone().expect("storage initialized");

        match body {
            ContentBody::NullMessage(_message) => {
                log::trace!("Ignoring NullMessage");
            }
            ContentBody::DataMessage(message) => {
                let uuid = metadata.sender.uuid;
                let message_id =
                    self.handle_message(ctx, None, Some(uuid), &message, None, &metadata);
                if metadata.needs_receipt {
                    if let Some(_message_id) = message_id {
                        self.handle_needs_delivery_receipt(ctx, &message, &metadata);
                    }
                }
                if !metadata.unidentified_sender && message_id.is_some() {
                    // TODO: if the contact should have our profile key already, send it again.
                    //       if the contact should not yet have our profile key, this is ok, and we
                    //       should offer the user a message request.
                    //       Cfr. MessageContentProcessor, grep for handleNeedsDeliveryReceipt.
                    log::warn!("Received an unsealed message from {:?}. Assert that they have our profile key.", metadata.sender);
                }
            }
            ContentBody::SynchronizeMessage(message) => {
                let mut handled = false;
                if let Some(sent) = message.sent {
                    handled = true;
                    log::trace!("Sync sent message");
                    // These are messages sent through a paired device.

                    if let Some(message) = &sent.message {
                        let uuid = sent
                            .destination_uuid
                            .as_deref()
                            .map(Uuid::parse_str)
                            .transpose()
                            .map_err(|_| log::warn!("Unparsable UUID {}", sent.destination_uuid()))
                            .ok()
                            .flatten();
                        let phonenumber = sent
                            .destination_e164
                            .as_deref()
                            .map(|s| phonenumber::parse(None, s))
                            .transpose()
                            .map_err(|_| {
                                log::warn!("Unparsable phonenumber {}", sent.destination_e164())
                            })
                            .ok()
                            .flatten();
                        self.handle_message(
                            ctx,
                            // Empty string mainly when groups,
                            // but maybe needs a check. TODO
                            phonenumber,
                            uuid,
                            message,
                            Some(sent.clone()),
                            &metadata,
                        );
                    } else {
                        log::warn!(
                            "Dropping sync-sent without message; probably Stories related: {:?}",
                            sent
                        );
                    }
                }
                if let Some(request) = message.request {
                    handled = true;
                    log::trace!("Sync request message");
                    self.handle_sync_request(metadata, request);
                }
                if !message.read.is_empty() {
                    handled = true;
                    log::trace!("Sync read message");
                    for read in &message.read {
                        // XXX: this should probably not be based on ts alone.
                        let ts = read.timestamp();
                        let source = read.sender_uuid();
                        // Signal uses timestamps in milliseconds, chrono has nanoseconds
                        let ts = millis_to_naive_chrono(ts);
                        log::trace!(
                            "Marking message from {} at {} ({}) as read.",
                            source,
                            ts,
                            read.timestamp()
                        );
                        if let Some((sess, msg)) = storage.mark_message_read(ts) {
                            self.inner
                                .pinned()
                                .borrow_mut()
                                .messageReceipt(sess.id, msg.id)
                        } else {
                            log::warn!("Could not mark as received!");
                        }
                    }
                }
                if let Some(fetch) = message.fetch_latest {
                    handled = true;
                    match fetch.r#type() {
                        sync_message::fetch_latest::Type::Unknown => {
                            log::warn!("Sync FetchLatest with unknown type")
                        }
                        sync_message::fetch_latest::Type::LocalProfile => {
                            log::trace!("Scheduling local profile refresh");
                            ctx.notify(RefreshOwnProfile { force: true });
                        }
                        sync_message::fetch_latest::Type::StorageManifest => {
                            // XXX
                            log::warn!("Unimplemented: synchronize fetch request StorageManifest")
                        }
                        sync_message::fetch_latest::Type::SubscriptionStatus => {
                            log::warn!(
                                "Unimplemented: synchronize fetch request SubscriptionStatus"
                            )
                        }
                    }
                }
                if !handled {
                    log::warn!("Sync message without known sync type");
                }
            }
            ContentBody::TypingMessage(typing) => {
                log::info!("{:?} is typing.", metadata.sender);
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
                    log::error!("Could not send typing notification to SessionActor: {}", e);
                }
            }
            ContentBody::ReceiptMessage(receipt) => {
                log::info!("{:?} received a message.", metadata.sender);
                // XXX dispatch on receipt.type
                for &ts in &receipt.timestamp {
                    // Signal uses timestamps in milliseconds, chrono has nanoseconds
                    if let Some((sess, msg)) = storage.mark_message_received(
                        metadata.sender.uuid,
                        millis_to_naive_chrono(ts),
                        None,
                    ) {
                        self.inner
                            .pinned()
                            .borrow_mut()
                            .messageReceipt(sess.id, msg.id)
                    } else {
                        log::warn!("Could not mark {} as received!", ts);
                    }
                }
            }
            ContentBody::CallMessage(_call) => {
                log::info!("{:?} is calling.", metadata.sender);
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
}

impl Actor for ClientActor {
    type Context = actix::Context<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        self.inner.pinned().borrow_mut().actor = Some(ctx.address());

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct FetchAttachment {
    attachment_id: i32,
}

impl Handler<FetchAttachment> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    /// Downloads the attachment in the background and registers it in the database.
    /// Saves the given attachment into a random-generated path. Saves the path in the database.
    ///
    /// This was a Message method in Go
    fn handle(
        &mut self,
        fetch: FetchAttachment,
        ctx: &mut <Self as Actor>::Context,
    ) -> Self::Result {
        let FetchAttachment { attachment_id } = fetch;

        let client_addr = ctx.address();

        let mut service = self.unauthenticated_service();
        let storage = self.storage.clone().unwrap();

        let attachment = storage
            .fetch_attachment(attachment_id)
            .expect("existing attachment");
        let message = storage
            .fetch_message_by_id(attachment.message_id)
            .expect("existing message");
        // XXX We probably don't need the session object itself anymore in this part of the code.
        let session = storage
            .fetch_session_by_id(message.session_id)
            .expect("existing session");
        // XXX We may want some graceful error handling here
        let ptr = AttachmentPointer::decode(
            attachment
                .pointer
                .as_deref()
                .expect("fetch attachment on attachments with associated pointer"),
        )
        .expect("valid attachment pointer");

        // Go used to always set has_attachment and mime_type, but also
        // in this method, as well as the generated path.
        // We have this function that returns a filesystem path, so we can
        // set it ourselves.
        let settings = crate::config::SettingsBridge::default();
        let dir = settings.get_string("attachment_dir");
        let dest = PathBuf::from(dir);

        // Sailfish and/or Rust needs "image/jpg" and some others need coaching
        // before taking a wild guess
        let mut ext = match ptr.content_type() {
            "text/plain" => "txt",
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/jpg" => "jpg",
            "text/x-signal-plain" => "txt",
            "application/x-signal-view-once" => "bin",
            other => mime_guess::get_mime_extensions_str(other)
                .and_then(|x| x.first())
                .copied() // &&str -> &str
                .unwrap_or_else(|| {
                    log::warn!("Could not find mime type; defaulting to .bin");
                    "bin"
                }),
        };

        let ptr2 = attachment.clone();
        let attachment_id = attachment.id;
        let session_id = session.id;
        let message_id = message.id;
        Box::pin(
            async move {
                use futures::io::AsyncReadExt;
                use libsignal_service::attachment_cipher::*;

                let mut stream = loop {
                    let r = service.get_attachment(&ptr).await;
                    match r {
                        Ok(stream) => break stream,
                        Err(ServiceError::Timeout { .. }) => {
                            log::warn!("get_attachment timed out, retrying")
                        }
                        Err(e) => return Err(e.into()),
                    }
                };
                log::info!("Downloading attachment");

                // We need the whole file for the crypto to check out 😢
                let actual_len = ptr.size.unwrap();
                let mut ciphertext = Vec::with_capacity(actual_len as usize);
                let stream_len = stream
                    .read_to_end(&mut ciphertext)
                    .await
                    .expect("streamed attachment") as u32;

                let key_material = ptr.key();
                assert_eq!(
                    key_material.len(),
                    64,
                    "key material for attachments is ought to be 64 bytes"
                );
                let mut key = [0u8; 64];
                key.copy_from_slice(key_material);
                decrypt_in_place(key, &mut ciphertext).expect("attachment decryption");

                // Signal puts exponentially increasing padding at the end
                // to prevent some distinguishing attacks, so it has to be truncated.
                if stream_len > actual_len {
                    log::info!(
                        "The attachment contains {} bytes of padding",
                        (stream_len - actual_len)
                    );
                    log::info!("Truncating from {} to {} bytes", stream_len, actual_len);
                    ciphertext.truncate(actual_len as usize);
                }

                // Signal Desktop sometimes sends a JPEG image with .png extension,
                // so double check the received .png image, and rename it if necessary.
                if ext == "png" {
                    log::trace!("Checking for JPEG with .png extension...");
                    let classifier = MimeClassifier::new();
                    let computed_type = classifier.classify(
                        LoadContext::Image,
                        NoSniffFlag::Off,
                        ApacheBugFlag::Off,
                        &None,
                        &ciphertext as &[u8],
                    );
                    if computed_type == mime::IMAGE_JPEG {
                        log::info!("Received JPEG file with .png suffix, renaming to .jpg");
                        ext = "jpg";
                    }
                }

                let _attachment_path = storage
                    .save_attachment(attachment_id, &dest, ext, &ciphertext)
                    .await?;

                client_addr
                    .send(AttachmentDownloaded {
                        session_id,
                        message_id,
                    })
                    .await?;
                Ok(())
            }
            .into_actor(self)
            .map(move |r: Result<(), anyhow::Error>, act, _ctx| {
                // Synchronise on the actor, to log the error to attachment.log
                if let Err(e) = r {
                    let e = format!(
                        "Error fetching attachment for message with ID `{}` {:?}: {:?}",
                        message.id, ptr2, e
                    );
                    log::error!("{} in handle()", e);
                    let mut log = act.attachment_log();
                    if let Err(e) = writeln!(log, "{}", e) {
                        log::error!("Could not write error to error log: {}", e);
                    }
                }
            }),
        )
    }
}

impl Handler<QueueMessage> for ClientActor {
    type Result = ();

    fn handle(&mut self, msg: QueueMessage, ctx: &mut Self::Context) -> Self::Result {
        log::trace!("MessageActor::handle({})", msg);
        let storage = self.storage.as_mut().unwrap();

        let has_attachment = !msg.attachment.is_empty();
        let self_recipient = storage
            .fetch_self_recipient()
            .expect("self recipient set when sending");
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

        let msg = storage.create_message(&crate::store::NewMessage {
            session_id: msg.session_id,
            source_e164: self_recipient.e164,
            source_uuid: self_recipient.uuid,
            text: msg.message,
            timestamp: chrono::Utc::now().naive_utc(),
            has_attachment,
            mime_type: if has_attachment {
                Some(
                    mime_guess::from_path(&msg.attachment)
                        .first_or_octet_stream()
                        .essence_str()
                        .into(),
                )
            } else {
                None
            },
            attachment: if has_attachment {
                Some(msg.attachment)
            } else {
                None
            },
            flags: 0,
            outgoing: true,
            received: false,
            sent: false,
            is_read: true,
            is_unidentified: false,
            quote_timestamp: quote.map(|msg| msg.server_timestamp.timestamp_millis() as u64),
            expires_in: session.expiring_message_timeout,
        });

        ctx.notify(SendMessage(msg.id));
    }
}

impl Handler<SendMessage> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    // Equiv of worker/send.go
    fn handle(&mut self, SendMessage(mid): SendMessage, ctx: &mut Self::Context) -> Self::Result {
        log::info!("ClientActor::SendMessage({:?})", mid);
        let sender = self.message_sender();
        let storage = self.storage.as_mut().unwrap();
        let msg = storage.fetch_augmented_message(mid).unwrap();
        let session = storage.fetch_session_by_id(msg.session_id).unwrap();
        let session_id = session.id;

        if msg.sent_timestamp.is_some() {
            log::warn!("Message already sent, refusing to retransmit.");
            return Box::pin(async {}.into_actor(self).map(|_, _, _| ()));
        }

        let self_recipient = storage.fetch_self_recipient();
        log::trace!("Sending for session: {}", session);
        log::trace!("Sending message: {}", msg.inner);

        let storage = storage.clone();
        let addr = ctx.address();
        Box::pin(
            async move {
                let mut sender = sender.await?;
                if let orm::SessionType::GroupV1(_group) = &session.r#type {
                    // FIXME
                    log::error!("Cannot send to Group V1 anymore.");
                }
                let group_v2 = session.group_context_v2();

                let timestamp = msg.server_timestamp.timestamp_millis() as u64;

                let quote = msg
                    .quote_id
                    .and_then(|quote_id| storage.fetch_augmented_message(quote_id))
                    .map(|quoted_message| {
                        if !quoted_message.attachments > 0 {
                            log::warn!("Quoting attachments is incomplete.  Here be dragons.");
                        }
                        let quote_sender = quoted_message
                            .sender_recipient_id
                            .and_then(|x| storage.fetch_recipient_by_id(x));

                        Quote {
                            id: Some(quoted_message.server_timestamp.timestamp_millis() as u64),
                            author_uuid: quote_sender.as_ref().and_then(|r| r.uuid.as_ref().map(Uuid::to_string)),
                            text: quoted_message.text.clone(),

                            ..Default::default()
                        }
                    });

                let mut content = DataMessage {
                    body: msg.text.clone(),
                    flags: if msg.flags != 0 {
                        Some(msg.flags as _)
                    } else {
                        None
                    },
                    timestamp: Some(timestamp),
                    // XXX: depends on the features in the message!
                    required_protocol_version: Some(0),
                    group_v2,

                    profile_key: self_recipient.and_then(|r| r.profile_key),
                    quote,
                    expire_timer: msg.expires_in.map(|x| x as u32),
                    ..Default::default()
                };

                let attachments = storage.fetch_attachments_for_message(msg.id);

                for attachment in &attachments {
                    let attachment_path = attachment
                        .attachment_path
                        .clone() // Clone for the spawn_blocking below
                        .expect("attachment path when uploading");
                    let contents =
                        tokio::task::spawn_blocking(move || std::fs::read(attachment_path))
                            .await
                            .context("threadpool")?
                            .context("reading attachment")?;
                    let attachment_path = attachment.attachment_path.as_deref().unwrap();
                    let spec = AttachmentSpec {
                        content_type: match mime_guess::from_path(attachment_path).first() {
                            Some(mime) => mime.essence_str().into(),
                            None => String::from("application/octet-stream"),
                        },
                        length: contents.len(),
                        file_name: Path::new(attachment_path)
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned()),
                        preview: None,
                        voice_note: Some(attachment.is_voice_note),
                        borderless: Some(attachment.is_borderless),
                        width: attachment.width.map(|x| x as u32),
                        height: attachment.height.map(|x| x as u32),
                        caption: None,
                        blur_hash: None,
                    };
                    let ptr = match sender.upload_attachment(spec, contents).await {
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
                        session,
                        for_story: false,
                    })
                    .await?;

                match res {
                    Ok(results) => {
                        let unidentified = results.iter().all(|res| match res {
                            Ok(SentMessage { unidentified, .. }) => *unidentified,
                            _ => false,
                        });

                        // Look for Ok recipients that couldn't deliver on unidentified.
                        for result in results.iter().filter_map(|res| res.as_ref().ok()) {
                            // Look up recipient to check the current state
                            let recipient = storage
                                .fetch_recipient_by_uuid(result.recipient.uuid)
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
                                log::info!(
                                    "Setting unidentified access mode for {:?} as {:?}",
                                    recipient,
                                    target_state
                                );
                                storage.set_recipient_unidentified(recipient.id, target_state);
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
                                log::error!("Could not deliver message: {}", error);
                                match error {
                                    MessageSenderError::ProofRequired { token, options } => {
                                        // Note: 'recaptcha' can refer to reCAPTCHA or hCaptcha
                                        let recaptcha = String::from("recaptcha");

                                        if options.contains(&recaptcha) {
                                            addr.send(ProofRequired {
                                                token: token.to_owned(),
                                                r#type: recaptcha,
                                            })
                                            .await
                                            .expect("deliver captcha required");
                                        } else {
                                            log::warn!("Rate limit proof requested, but type 'recaptcha' wasn't available!");
                                        }
                                    },
                                    MessageSenderError::NotFound { uuid } => {
                                        log::warn!("Recipient not found, removing device sessions {}", uuid);
                                        let mut num = storage.delete_all_sessions(&ServiceAddress { uuid }).await?;
                                        log::trace!("Removed {} device session(s)", num);
                                        num = storage.mark_recipient_registered(uuid, false);
                                        log::trace!("Marked {} recipient(s) as unregistered", num);
                                    },
                                    _ => {
                                        log::error!("The above error goes unhandled.");
                                    }
                                };
                            }
                            log::error!("Successfully delivered message to {} out of {} recipients", successes, result_count);
                            anyhow::bail!("Could not deliver message.")
                        }
                    }
                    Err(e) => {
                        storage.fail_message(mid);
                        Err(e)
                    }
                }
            }
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
                        log::error!("Sending message: {}", e);
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
        log::trace!("ClientActor::EndSession(recipient_id = {})", id);

        let storage = self.storage.as_mut().unwrap();
        let recipient = storage
            .fetch_recipient_by_id(id)
            .expect("existing recipient id");
        let session = storage.fetch_or_insert_session_by_recipient_id(recipient.id);

        let msg = storage.create_message(&crate::store::NewMessage {
            session_id: session.id,
            source_e164: recipient.e164,
            source_uuid: recipient.uuid,
            text: "[Whisperfish] Reset secure session".into(),
            timestamp: chrono::Utc::now().naive_utc(),
            has_attachment: false,
            mime_type: None,
            attachment: None,
            flags: DataMessageFlags::EndSession.into(),
            outgoing: true,
            received: false,
            sent: false,
            is_read: true,
            is_unidentified: false,
            quote_timestamp: None,
            expires_in: session.expiring_message_timeout,
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
        log::info!(
            "ClientActor::SendTypingNotification({}, {})",
            session_id,
            is_start
        );
        let storage = self.storage.as_mut().unwrap();
        let addr = ctx.address();

        let session = storage.fetch_session_by_id(session_id).unwrap();
        assert_eq!(session_id, session.id);

        log::trace!("Sending typing notification for session: {}", session);

        // Since we don't want to stress database needlessly,
        // cache the sent TypingMessage timestamps and try to
        // match delivery receipts against it when they arrive.

        self.clear_transient_timstamps();
        let now = Utc::now().timestamp_millis() as u64;
        self.transient_timestamps.insert(now);

        Box::pin(
            async move {
                let group_id = match &session.r#type {
                    orm::SessionType::DirectMessage(_) => None,
                    orm::SessionType::GroupV1(group) => {
                        Some(hex::decode(&group.id).expect("valid hex identifiers in db"))
                    }
                    orm::SessionType::GroupV2(group) => {
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
                    session,
                    for_story: false,
                })
                .await?
                .map(|_unidentified| session_id)
            }
            .into_actor(self)
            .map(move |res, _act, _ctx| {
                match res {
                    Ok(sid) => {
                        log::trace!("Successfully sent typing notification for session {}", sid);
                    }
                    Err(e) => {
                        log::error!("Delivering typing notification: {}", e);
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
        log::info!(
            "ClientActor::SendReaction({}, {}, {}, {:?})",
            message_id,
            sender_id,
            emoji,
            remove
        );

        let storage = self.storage.as_mut().unwrap();
        let self_recipient = storage.fetch_self_recipient().unwrap();
        let message = storage.fetch_message_by_id(message_id).unwrap();

        // Outgoing messages should not have sender_recipient_id set
        let (sender_id, emoji) = if sender_id > 0 && sender_id != self_recipient.id {
            (sender_id, emoji)
        } else {
            if !message.is_outbound {
                panic!("Inbound message {} has no sender recipient ID", message_id);
            }
            if remove {
                let reaction = storage.fetch_reaction(message_id, self_recipient.id);
                if let Some(r) = reaction {
                    (self_recipient.id, r.emoji)
                } else {
                    // XXX: Don't continue - we should remove the same emoji
                    log::error!("Message {} doesn't have our own reaction!", message_id);
                    (self_recipient.id, emoji)
                }
            } else {
                (self_recipient.id, emoji)
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

                let content = DataMessage {
                    group_v2,
                    timestamp: Some(now.timestamp_millis() as u64),
                    required_protocol_version: Some(4), // Source: received emoji from Signal Android
                    reaction: Some(Reaction {
                        emoji: Some(emoji.clone()),
                        remove: Some(remove),
                        target_author_uuid: sender_recipient.uuid.map(|u| u.to_string()),
                        target_sent_timestamp: Some(
                            message.server_timestamp.timestamp_millis() as u64
                        ),
                    }),
                    ..Default::default()
                };

                addr.send(DeliverMessage {
                    content,
                    online: false,
                    timestamp: now.timestamp_millis() as u64,
                    session,
                    for_story: false,
                })
                .await?
                .map(|_| (emoji, now, self_recipient.id))
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
                        log::trace!("Reaction sent to message {}", message_id);
                    }
                    Err(e) => {
                        log::error!("Could not sent Reaction: {}", e);
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
            storage.remove_reaction(message_id, sender_id);
        } else {
            storage.save_reaction(message_id, sender_id, emoji, timestamp);
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
            session,
            for_story,
        } = msg;
        let content = content.into();

        log::trace!("Transmitting {:?} with timestamp {}", content, timestamp);

        let storage = self.storage.clone().unwrap();
        let sender = self.message_sender();
        let local_addr = self.local_addr.unwrap();

        let certs = self.unidentified_certificates.clone();

        Box::pin(async move {
            let mut sender = sender.await?;
            let results = match &session.r#type {
                orm::SessionType::GroupV1(_group) => {
                    // FIXME
                    log::error!("Cannot send to Group V1 anymore.");
                    Vec::new()
                }
                orm::SessionType::GroupV2(group) => {
                    let members = storage.fetch_group_members_by_group_v2_id(&group.id);
                    let members = members
                        .iter()
                        .filter_map(|(_member, recipient)| {
                            let member = recipient.to_service_address();

                            if !recipient.is_registered || Some(local_addr) == member {
                                None
                            } else if let Some(member) = member {
                                // XXX change the cert type when we want to introduce E164 privacy.
                                let access =
                                    certs.access_for(CertType::Complete, recipient, for_story);
                                Some((member, access))
                            } else {
                                log::warn!(
                                    "No known UUID for {}; will not deliver this message.",
                                    recipient.e164_or_uuid()
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
                orm::SessionType::DirectMessage(recipient) => {
                    let svc = recipient.to_service_address();

                    let access = certs.access_for(CertType::Complete, recipient, for_story);

                    if let Some(svc) = svc {
                        if !recipient.is_registered {
                            anyhow::bail!("Unregistered recipient {}", svc.uuid.to_string());
                        }

                        vec![
                            sender
                                .send_message(&svc, access, content.clone(), timestamp, online)
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

impl Handler<AttachmentDownloaded> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        AttachmentDownloaded {
            session_id: sid,
            message_id: mid,
        }: AttachmentDownloaded,
        _ctx: &mut Self::Context,
    ) {
        log::info!("Attachment downloaded for message {}", mid);
        self.inner.pinned().borrow().attachmentDownloaded(sid, mid);
    }
}

impl Handler<StorageReady> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, storageready: StorageReady, _ctx: &mut Self::Context) -> Self::Result {
        self.storage = Some(storageready.storage.clone());
        let phonenumber = self
            .config
            .get_tel()
            .expect("phonenumber present after any registration");
        let uuid = self.config.get_uuid();
        let device_id = self.config.get_device_id();

        storageready.storage.mark_pending_messages_failed();

        let storage_for_password = storageready.storage;
        let request_password = async move {
            log::info!("Phone number: {:?}", phonenumber);
            log::info!("UUID: {:?}", uuid);
            log::info!("DeviceId: {}", device_id);

            let password = storage_for_password.signal_password().await.unwrap();
            let signaling_key = Some(storage_for_password.signaling_key().await.unwrap());

            (uuid, phonenumber, device_id, password, signaling_key)
        };
        let service_cfg = self.service_cfg();

        Box::pin(request_password.into_actor(self).map(
            move |(uuid, phonenumber, device_id, password, signaling_key), act, ctx| {
                // Store credentials
                let credentials = ServiceCredentials {
                    uuid,
                    phonenumber,
                    password: Some(password),
                    signaling_key,
                    device_id: Some(device_id.into()),
                };
                act.credentials = Some(credentials);
                // end store credentials

                // Signal service context
                let storage = act.storage.clone().unwrap();
                // XXX What about the whoami migration?
                let uuid = uuid.expect("local uuid to initialize service cipher");
                let cipher = ServiceCipher::new(
                    storage,
                    rand::thread_rng(),
                    service_cfg.unidentified_sender_trust_root,
                    uuid,
                    device_id.into(),
                );
                // end signal service context
                act.cipher = Some(cipher);
                act.local_addr = Some(ServiceAddress { uuid });

                Self::queue_migrations(ctx);

                ctx.notify(Restart);

                ctx.notify(RefreshPreKeys);
            },
        ))
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
        let migrations_ready = self.migration_state.ready();

        if self.message_expiry_notification_handle.is_none() {
            let (message_expiry_notification_handle, message_expiry_notification) =
                tokio::sync::mpsc::unbounded_channel();
            ctx.add_stream(ExpiredMessagesStream::new(
                self.storage.clone().unwrap(),
                message_expiry_notification,
            ));
            self.message_expiry_notification_handle = Some(message_expiry_notification_handle);
        }

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();
        Box::pin(
            async move {
                migrations_ready.await;
                let mut receiver = MessageReceiver::new(service.clone());

                let pipe = receiver.create_message_pipe(credentials).await?;
                let ws = pipe.ws();
                Result::<_, ServiceError>::Ok((pipe, ws))
            }
            .into_actor(self)
            .map(move |pipe, act, ctx| match pipe {
                Ok((pipe, ws)) => {
                    ctx.notify(unidentified::RotateUnidentifiedCertificates);
                    ctx.add_stream(pipe.stream());

                    ctx.set_mailbox_capacity(1);
                    act.inner.pinned().borrow_mut().connected = true;
                    act.ws = Some(ws);
                    act.inner.pinned().borrow().connectedChanged();

                    // If profile stream was running, restart.
                    if let Some(handle) = act.outdated_profile_stream_handle.take() {
                        ctx.cancel_future(handle);
                    }
                    act.outdated_profile_stream_handle = Some(
                        ctx.add_stream(OutdatedProfileStream::new(act.storage.clone().unwrap())),
                    );
                }
                Err(e) => {
                    log::error!("Error starting stream: {}", e);
                    log::info!("Retrying in 10");
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
#[derive(Message)]
#[rtype(result = "()")]
pub enum RefreshProfile {
    BySession(i32),
    ByRecipientId(i32),
}

impl Handler<RefreshProfile> for ClientActor {
    type Result = ();

    fn handle(&mut self, profile: RefreshProfile, _ctx: &mut Self::Context) {
        let storage = self.storage.as_ref().unwrap();
        let recipient = match profile {
            RefreshProfile::BySession(session_id) => {
                match storage.fetch_session_by_id(session_id).map(|x| x.r#type) {
                    Some(orm::SessionType::DirectMessage(recipient)) => recipient,
                    None => {
                        log::error!("No session with id {}", session_id);
                        return;
                    }
                    _ => {
                        log::error!("Can only refresh profiles for DirectMessage sessions.");
                        return;
                    }
                }
            }
            RefreshProfile::ByRecipientId(id) => match storage.fetch_recipient_by_id(id) {
                Some(r) => r,
                None => {
                    log::error!("No recipient with id {}", id);
                    return;
                }
            },
        };
        if let Some(uuid) = recipient.uuid {
            storage.mark_profile_outdated(uuid);
        } else {
            log::error!(
                "Recipient without uuid; not refreshing profile: {:?}",
                recipient
            );
        }
        // Polling the actor will poll the OutdatedProfileStream, which should immediately fire the
        // necessary events.  This is hacky (XXX), we should in fact wake the stream somehow to ensure
        // correct behaviour.
    }
}

impl StreamHandler<Result<Envelope, ServiceError>> for ClientActor {
    fn handle(&mut self, msg: Result<Envelope, ServiceError>, ctx: &mut Self::Context) {
        let msg = match msg {
            Ok(msg) => msg,
            Err(e) => {
                // XXX: we might want to dispatch on this error.
                log::error!("MessagePipe pushed an error: {:?}", e);
                return;
            }
        };

        let mut cipher = self.cipher.clone().expect("cipher initialized");

        if msg.is_receipt() {
            self.process_receipt(&msg);
        }

        if !(msg.is_prekey_signal_message()
            || msg.is_signal_message()
            || msg.is_unidentified_sender()
            || msg.is_receipt())
        {
            log::warn!("Unknown envelope type {:?}", msg.r#type());
        }

        let storage = self.storage.clone().expect("initialized storage");

        ctx.spawn(
            async move {
                let content = loop {
                    match cipher.open_envelope(msg.clone()).await {
                        Ok(Some(content)) => break content,
                        Ok(None) => {
                            log::warn!("Empty envelope");
                            return None;
                        }
                        Err(ServiceError::SignalProtocolError(
                            SignalProtocolError::UntrustedIdentity(addr),
                        )) => {
                            // This branch is the only one that loops, and it *should not* loop more than once.
                            let source_uuid = Uuid::parse_str(addr.name()).expect("only uuid-based identities accessible in the database");
                            log::warn!("Untrusted identity for {}; replacing identity and inserting a warning.", addr);
                            let recipient = storage.fetch_or_insert_recipient_by_uuid(source_uuid);
                            let session = storage.fetch_or_insert_session_by_recipient_id(recipient.id);
                            let msg = crate::store::NewMessage {
                                session_id: session.id,
                                source_e164: None,
                                source_uuid: Some(source_uuid),
                                text: "[Whisperfish] The identity key for this contact has changed. Please verify your safety number.".into(), // XXX Translate
                                timestamp: chrono::Utc::now().naive_utc(),
                                sent: false,
                                received: true,
                                is_read: false,
                                flags: 0,
                                attachment: None,
                                mime_type: None,
                                has_attachment: false,
                                outgoing: false,
                                is_unidentified: false,
                                quote_timestamp: None,
                                expires_in: session.expiring_message_timeout,
                            };
                            storage.create_message(&msg);

                            if !recipient.is_registered {
                                log::warn!("Recipient was marked as unregistered, marking as registered.");
                                storage.mark_recipient_registered(source_uuid, true);
                            }

                            let removed = storage.delete_identity_key(&addr);
                            if ! removed {
                                log::error!("Could not remove identity key for {}.  Please file a bug.", addr);
                                return None;
                            }
                        }
                        Err(e) => {
                            log::error!("Error opening envelope: {:?}", e);
                            return None;
                        }
                    }
                };

                log::trace!("Opened envelope: {:?}", content);

                Some(content)
            }
            .into_actor(self)
            .map(|content, act, ctx| {
                if let Some(content) = content {
                    act.process_envelope(content, ctx);
                }
            }),
        );
    }

    /// Called when the WebSocket somehow has disconnected.
    fn finished(&mut self, ctx: &mut Self::Context) {
        log::debug!("Attempting reconnect");

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
            uuid: None,
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
#[rtype(result = "Result<(u32, u32, VerifyAccountResponse), anyhow::Error>")]
pub struct ConfirmRegistration {
    pub phonenumber: PhoneNumber,
    pub password: String,
    pub confirm_code: String,
    pub signaling_key: [u8; 52],
}

impl Handler<ConfirmRegistration> for ClientActor {
    // regid, pni_regid, response
    type Result = ResponseActFuture<Self, Result<(u32, u32, VerifyAccountResponse), anyhow::Error>>;

    fn handle(&mut self, confirm: ConfirmRegistration, _ctx: &mut Self::Context) -> Self::Result {
        use libsignal_service::provisioning::*;

        let ConfirmRegistration {
            phonenumber,
            password,
            confirm_code,
            signaling_key,
        } = confirm;

        let registration_id = generate_registration_id(&mut rand::thread_rng());
        let pni_registration_id = generate_registration_id(&mut rand::thread_rng());
        log::trace!("registration_id: {}", registration_id);
        log::trace!("pni_registration_id: {}", pni_registration_id);

        let mut push_service = self.authenticated_service_with_credentials(ServiceCredentials {
            uuid: None,
            phonenumber,
            password: Some(password),
            signaling_key: None,
            device_id: None, // !77
        });
        let mut session = self
            .registration_session
            .clone()
            .expect("confirm registration after creating registration session");
        let confirmation_procedure = async move {
            // XXX centralize the place where attributes are generated.
            let account_attrs = AccountAttributes {
                // XXX probably we should remove the signaling key.
                signaling_key: Some(signaling_key.to_vec()),
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
            // XXX: We explicitely opt out of skipping device transfer (the false argument). Double
            //      check whether that's what we want!
            let result = push_service
                .submit_registration_request(
                    RegistrationMethod::SessionId(&session.id),
                    account_attrs,
                    false,
                )
                .await?;

            Ok(result)
        };

        Box::pin(
            confirmation_procedure
                .into_actor(self)
                .map(move |result, act, _ctx| {
                    if result.is_ok() {
                        act.registration_session = None;
                    }
                    Ok((registration_id, pni_registration_id, result?))
                }),
        )
    }
}

#[derive(Message)]
#[rtype(result = "Result<RegisterLinkedResponse, anyhow::Error>")]
pub struct RegisterLinked {
    pub device_name: String,
    pub password: String,
    pub signaling_key: [u8; 52],
    pub tx_uri: futures::channel::oneshot::Sender<String>,
}

pub struct RegisterLinkedResponse {
    pub phone_number: PhoneNumber,
    pub registration_id: u32,
    pub pni_registration_id: u32,
    pub device_id: DeviceId,
    pub service_ids: ServiceIds,
    pub aci_identity_key_pair: libsignal_protocol::IdentityKeyPair,
    pub pni_identity_key_pair: libsignal_protocol::IdentityKeyPair,
    pub profile_key: Vec<u8>,
}

impl Handler<RegisterLinked> for ClientActor {
    type Result = ResponseActFuture<Self, Result<RegisterLinkedResponse, anyhow::Error>>;

    fn handle(&mut self, reg: RegisterLinked, _ctx: &mut Self::Context) -> Self::Result {
        use libsignal_service::provisioning::*;

        let push_service = self.unauthenticated_service();

        let mut provision_manager: LinkingManager<AwcPushService> =
            LinkingManager::new(push_service, reg.password.clone());

        let (tx, mut rx) = futures::channel::mpsc::channel(1);

        let mut tx_uri = Some(reg.tx_uri);
        let signaling_key = reg.signaling_key;

        let registration_procedure = async move {
            let (fut1, fut2) = future::join(
                provision_manager.provision_secondary_device(
                    &mut rand::thread_rng(),
                    signaling_key,
                    tx,
                ),
                async move {
                    let mut res = Result::<RegisterLinkedResponse, anyhow::Error>::Err(
                        anyhow::Error::msg("Registration timed out"),
                    );
                    while let Some(provisioning_step) = rx.next().await {
                        match provisioning_step {
                            SecondaryDeviceProvisioning::Url(url) => {
                                log::info!("generating qrcode from provisioning link: {}", &url);
                                tx_uri
                                    .take()
                                    .expect("that only one URI is emitted by provisioning code")
                                    .send(url.to_string())
                                    .expect("to forward provisioning URL to caller");
                            }
                            SecondaryDeviceProvisioning::NewDeviceRegistration {
                                phone_number,
                                device_id,
                                registration_id,
                                pni_registration_id,
                                profile_key,
                                service_ids,
                                aci_private_key,
                                aci_public_key,
                                pni_private_key,
                                pni_public_key,
                            } => {
                                let aci_identity_key_pair =
                                    libsignal_protocol::IdentityKeyPair::new(
                                        libsignal_protocol::IdentityKey::new(aci_public_key),
                                        aci_private_key,
                                    );
                                let pni_identity_key_pair =
                                    libsignal_protocol::IdentityKeyPair::new(
                                        libsignal_protocol::IdentityKey::new(pni_public_key),
                                        pni_private_key,
                                    );

                                res = Result::<RegisterLinkedResponse, anyhow::Error>::Ok(
                                    RegisterLinkedResponse {
                                        phone_number,
                                        registration_id,
                                        pni_registration_id,
                                        device_id,
                                        service_ids,
                                        aci_identity_key_pair,
                                        pni_identity_key_pair,
                                        profile_key,
                                    },
                                );
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
                .map(move |result, _act, _ctx| result),
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
        log::trace!("handle(RefreshPreKeys)");

        let service = self.authenticated_service();
        // XXX add profile key when #192 implemneted
        let mut am = AccountManager::new(service, None);
        let storage = self.storage.clone().unwrap();

        let proc = async move {
            let (next_signed_pre_key_id, next_kyber_pre_key_id, pre_keys_offset_id) =
                storage.next_pre_key_ids().await;

            am.update_pre_key_bundle(
                &mut storage.clone(),
                &mut rand::thread_rng(),
                next_signed_pre_key_id,
                next_kyber_pre_key_id,
                pre_keys_offset_id,
                false,
            )
            .await
        };
        // XXX: store the last refresh time somewhere.

        Box::pin(proc.into_actor(self).map(move |result, _act, _ctx| {
            if let Err(e) = result {
                log::error!("Refresh pre keys failed: {}", e);
            } else {
                log::trace!("Successfully refreshed prekeys");
            }
        }))
    }
}

// methods called from Qt
impl ClientWorker {
    #[with_executor]
    pub fn compact_db(&self) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor.send(CompactDb(0)).await {
                log::error!("{:?} in compact_db()", e);
            }
        });
    }

    #[with_executor]
    pub fn delete_file(&self, file_name: String) {
        let result = remove_file(&file_name);
        match result {
            Ok(()) => {
                log::trace!("Deleted file {}", file_name);
            }
            Err(e) => {
                log::trace!("Could not delete file {}: {:?}", file_name, e);
            }
        };
    }

    #[with_executor]
    pub fn refresh_profile(&self, recipient_id: i32) {
        let actor = self.actor.clone().unwrap();
        actix::spawn(async move {
            if let Err(e) = actor
                .send(RefreshProfile::ByRecipientId(recipient_id))
                .await
            {
                log::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
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
                log::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
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
                    _type: "recaptcha".into(),
                    token,
                    response,
                })
                .await
            {
                log::error!("{:?}", e);
            }
        });
    }

    #[with_executor]
    fn send_typing_notification(&self, session_id: i32, is_start: bool) {
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
}

impl Handler<CompactDb> for ClientActor {
    type Result = usize;

    fn handle(&mut self, _: CompactDb, _ctx: &mut Self::Context) -> Self::Result {
        log::trace!("handle(CompactDb)");
        let store = self.storage.clone().unwrap();
        store.compact_db()
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProofRequired {
    token: String,
    r#type: String,
}

impl Handler<ProofRequired> for ClientActor {
    type Result = ();

    fn handle(&mut self, proof: ProofRequired, _ctx: &mut Self::Context) -> Self::Result {
        self.inner
            .pinned()
            .borrow()
            .proofRequested(proof.token.into(), proof.r#type.into());
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct ProofResponse {
    _type: String,
    token: String,
    response: String,
}

impl Handler<ProofResponse> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(&mut self, proof: ProofResponse, ctx: &mut Self::Context) -> Self::Result {
        log::trace!("handle(ProofResponse)");

        let storage = self.storage.clone().unwrap();
        let self_recipient = storage
            .fetch_self_recipient()
            .expect("self recipient in handle(ProofResponse)");
        let profile_key = self_recipient.profile_key.map(|bytes| {
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
        };

        Box::pin(proc.into_actor(self).map(move |result, _act, _ctx| {
            actix::spawn(async move {
                if let Err(e) = result {
                    log::error!("Error sending signalcaptcha proof: {}", e);
                    addr.send(ProofAccepted { result: false }).await
                } else {
                    log::trace!("Successfully sent signalcaptcha proof");
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
        let self_recipient = storage.fetch_self_recipient().expect("self recipient");

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
                profile_key: self_recipient.profile_key,
                timestamp: Some(now),
                delete: Some(Delete {
                    target_sent_timestamp: Some(message.server_timestamp.timestamp_millis() as u64),
                }),
                required_protocol_version: Some(4),
                ..Default::default()
            },
            for_story: false,
            timestamp: now,
            online: false,
            session,
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
            log::error!(
                "Attachment id {} doesn't exist, can't export it!",
                attachment_id
            );
            return;
        }
        let attachment = attachment.unwrap();
        if attachment.attachment_path.is_none() {
            log::error!(
                "Attachment id {} has no path stored, can't export it!",
                attachment_id
            );
            return;
        }

        // 2) Check the source file

        let source = PathBuf::from_str(&attachment.attachment_path.unwrap()).unwrap();
        if !source.exists() {
            log::error!(
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
            log::error!(
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
            log::error!("Not enough free space after copying, not exporting the attachment!");
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
            Err(e) => log::trace!("Copying attachment failed: {}", e),
            Ok(size) => log::trace!(
                "Attachent {} exported to {} ({} bytes)",
                attachment_id,
                target,
                size
            ),
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn queue_message() {
        let q = QueueMessage {
            attachment: "Attachment!".into(),
            session_id: 8,
            message: "Lorem ipsum dolor sit amet".into(),
            quote: 12,
        };
        assert_eq!(format!("{}", q), "QueueMessage { session_id: 8, message: \"Lorem ips...\", quote: 12, attachment: \"Attachment!\" }");
    }
}
