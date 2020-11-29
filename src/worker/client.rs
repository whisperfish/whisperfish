use std::io::Write;
use std::path::{Path, PathBuf};

use actix::prelude::*;
use chrono::prelude::*;
use qmetaobject::*;

use crate::gui::StorageReady;
use crate::sfos::SailfishApp;
use crate::store::Storage;

use libsignal_protocol::Context;
use libsignal_service::configuration::SignalServers;
use libsignal_service::content::DataMessageFlags;
use libsignal_service::content::{
    AttachmentPointer, ContentBody, DataMessage, GroupContext, GroupType, SyncMessage,
};
use libsignal_service::prelude::*;
use libsignal_service::push_service::DEFAULT_DEVICE_ID;
use libsignal_service::sender::AttachmentSpec;
use libsignal_service::AccountManager;
use libsignal_service_actix::prelude::*;

pub use libsignal_service::push_service::ConfirmCodeResponse;
use libsignal_service::push_service::{SmsVerificationCodeResponse, VoiceVerificationCodeResponse};

fn millis_to_naive_chrono(ts: u64) -> NaiveDateTime {
    NaiveDateTime::from_timestamp((ts / 1000) as i64, ((ts % 1000) * 1000) as u32)
}

#[derive(Message)]
#[rtype(result = "()")]
/// Enqueue a message on socket by MID
pub struct SendMessage(pub i32);

#[derive(Message)]
#[rtype(result = "()")]
struct AttachmentDownloaded(i32);

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct ClientWorker {
    base: qt_base_class!(trait QObject),
    messageReceived: qt_signal!(sid: i32, mid: i32),
    messageReceipt: qt_signal!(sid: i32, mid: i32),
    notifyMessage: qt_signal!(sid: i32, source: QString, message: QString, isGroup: bool),
    promptResetPeerIdentity: qt_signal!(),
    messageSent: qt_signal!(sid: i32, mid: i32, message: QString),

    connected: qt_property!(bool; NOTIFY connectedChanged),
    connectedChanged: qt_signal!(),

    actor: Option<Addr<ClientActor>>,
}

/// ClientActor keeps track of the connection state.
pub struct ClientActor {
    inner: QObjectBox<ClientWorker>,

    /// Some(Service) when connected, otherwise None
    service: Option<AwcPushService>,
    credentials: Option<Credentials>,
    local_addr: Option<ServiceAddress>,
    storage: Option<Storage>,
    cipher: Option<ServiceCipher>,
    context: Context,

    start_time: chrono::DateTime<chrono::Local>,
    store_context: Option<libsignal_protocol::StoreContext>,
}

impl ClientActor {
    pub fn new(app: &mut SailfishApp) -> Result<Self, failure::Error> {
        let inner = QObjectBox::new(ClientWorker::default());
        app.set_object_property("ClientWorker".into(), inner.pinned());

        let crypto = libsignal_protocol::crypto::DefaultCrypto::default();

        Ok(Self {
            inner,
            credentials: None,
            local_addr: None,
            service: None,
            storage: None,
            cipher: None,
            context: Context::new(crypto)?,

            start_time: chrono::Local::now(),
            store_context: None,
        })
    }

    fn unauthenticated_service(&self) -> AwcPushService {
        let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));
        let service_cfg = self.service_cfg();
        AwcPushService::new(service_cfg, None, &useragent)
    }

    fn authenticated_service(
        &self,
        uuid: Option<String>,
        e164: String,
        password: String,
        signaling_key: Option<[u8; 52]>,
    ) -> (Credentials, AwcPushService) {
        let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));
        let service_cfg = self.service_cfg();
        let credentials = Credentials {
            uuid,
            e164,
            password: Some(password),
            signaling_key,
        };

        let service = AwcPushService::new(service_cfg, Some(credentials.clone()), &useragent);
        (credentials, service)
    }

    fn service_cfg(&self) -> ServiceConfiguration {
        // XXX: read the configuration files!
        SignalServers::Production.into()
    }

    /// Process incoming message from Signal
    ///
    /// This was `MessageHandler` in Go.
    ///
    /// TODO: consider putting this as an actor `Handle<>` implementation instead.
    pub fn handle_message(
        &mut self,
        ctx: &mut <Self as Actor>::Context,
        source: String,
        msg: DataMessage,
        is_sync_sent: bool,
        timestamp: u64,
    ) {
        let settings = crate::settings::Settings::default();

        let storage = self.storage.as_mut().expect("storage");

        if msg.flags() & DataMessageFlags::EndSession as u32 != 0 {
            use libsignal_protocol::stores::SessionStore;
            if let Err(e) = storage.delete_all_sessions(source.as_bytes()) {
                log::error!("End session requested, but could not end session: {:?}", e);
            }
        }

        let mut new_message = crate::store::NewMessage {
            source,
            text: msg.body().into(),
            flags: msg.flags() as i32,
            outgoing: is_sync_sent,
            sent: is_sync_sent,
            timestamp: millis_to_naive_chrono(if is_sync_sent && timestamp > 0 {
                timestamp
            } else {
                msg.timestamp()
            } as u64),
            has_attachment: !msg.attachments.is_empty(),
            mime_type: None,  // Attachments are further handled asynchronously
            received: false,  // This is set true by a receipt handler
            session_id: None, // Canary value checked later
            attachment: None,
        };

        let group = if let Some(group) = msg.group.as_ref() {
            match group.r#type() {
                GroupType::Update => {
                    new_message.text = String::from("Member joined group");
                }
                GroupType::Quit => {
                    new_message.text = String::from("Member left group");
                }
                t => log::warn!("Unhandled group type {:?}", t),
            }

            Some(crate::store::NewGroup {
                id: group.id(),
                name: group.name().to_string(),
                members: group.members_e164.clone(),
            })
        } else {
            None
        };

        let is_unread = !new_message.sent;
        let (message, session) = storage.process_message(new_message, group, is_unread);

        if settings.get_bool("attachment_log") && !msg.attachments.is_empty() {
            log::trace!("Logging message to the attachment log");
            // XXX Sync code, but it's not the only sync code in here...
            let mut log = self.attachment_log();

            writeln!(
                log,
                "[{}] {:?} for message ID {}",
                chrono::Utc::now(),
                msg,
                message.id
            )
            .expect("write to the attachment log");
        }

        if settings.get_bool("save_attachments") && !settings.get_bool("incognito") {
            for attachment in msg.attachments {
                // Go used to always set has_attachment and mime_type, but also
                // in this method, as well as the generated path.
                // We have this function that returns a filesystem path, so we can
                // set it ourselves.
                let dir = settings.get_string("attachment_dir");
                let dest = Path::new(&dir);

                ctx.notify(FetchAttachment {
                    mid: message.id,
                    dest: dest.to_path_buf(),
                    ptr: attachment,
                });
            }
        }

        self.inner
            .pinned()
            .borrow_mut()
            .messageReceived(session.id, message.id);
        self.inner.pinned().borrow_mut().notifyMessage(
            session.id,
            session
                .group_name
                .as_deref()
                .unwrap_or(&session.source)
                .into(),
            message.message.into(),
            session.group_id.is_some(),
        );
    }

    fn process_receipt(&mut self, msg: Envelope) {
        log::info!("Received receipt: {}", msg.timestamp());

        // XXX: figure out edge cases in which these are *not* initialized.
        let storage = self.storage.as_mut().expect("storage initialized");

        // XXX: this should probably not be based on ts alone.
        let ts = msg.timestamp();
        let source = msg.source_e164();
        // XXX should this not be encrypted and authenticated?
        // Signal uses timestamps in milliseconds, chrono has nanoseconds
        let ts = millis_to_naive_chrono(ts);
        log::trace!(
            "Marking message from {} at {} ({}) as received.",
            source,
            ts,
            msg.timestamp()
        );
        if let Some((sess, msg)) = storage.mark_message_received(ts) {
            self.inner
                .pinned()
                .borrow_mut()
                .messageReceipt(sess.id, msg.id)
        }
    }

    fn process_message(&mut self, msg: Envelope, ctx: &mut <Self as Actor>::Context) {
        // XXX: figure out edge cases in which these are *not* initialized.
        let storage = self.storage.as_mut().expect("storage initialized");
        let cipher = self.cipher.as_mut().expect("cipher initialized");

        let Content { body, metadata } = match cipher.open_envelope(msg) {
            Ok(Some(content)) => content,
            Ok(None) => {
                log::info!("Empty envelope");
                return;
            }
            Err(e) => {
                log::error!("Error opening envelope: {:?}", e);
                return;
            }
        };

        log::trace!(
            "Opened envelope Content {{ body: {:?}, metadata: {:?} }}",
            body,
            metadata
        );

        match body {
            ContentBody::DataMessage(message) => self.handle_message(
                ctx,
                metadata
                    .sender
                    .e164
                    .clone()
                    .expect("valid e164 in ServiceAddress. Whisperfish issue #80"),
                message,
                false,
                metadata.timestamp,
            ),
            ContentBody::SynchronizeMessage(message) => {
                if let Some(sent) = message.sent {
                    log::trace!("Sync sent message");
                    // These are messages sent through a paired device.

                    let message = sent.message.expect("sync sent with message");
                    self.handle_message(
                        ctx,
                        // Empty string mainly when groups,
                        // but maybe needs a check. TODO
                        sent.destination_e164.unwrap_or_else(|| String::from("")),
                        message,
                        true,
                        0,
                    );
                } else if let Some(_request) = message.request {
                    log::trace!("Sync request message");
                } else if !message.read.is_empty() {
                    log::trace!("Sync read message");
                    for read in &message.read {
                        // XXX: this should probably not be based on ts alone.
                        let ts = read.timestamp();
                        let source = read.sender_e164();
                        // Signal uses timestamps in milliseconds, chrono has nanoseconds
                        let ts = millis_to_naive_chrono(ts);
                        log::trace!(
                            "Marking message from {} at {} ({}) as received.",
                            source,
                            ts,
                            read.timestamp()
                        );
                        if let Some((sess, msg)) = storage.mark_message_received(ts) {
                            self.inner
                                .pinned()
                                .borrow_mut()
                                .messageReceipt(sess.id, msg.id)
                        } else {
                            log::warn!("Could not mark as received!");
                        }
                    }
                } else {
                    log::warn!("Sync message without known sync type");
                }
            }
            ContentBody::TypingMessage(_typing) => {
                log::info!("{} is typing.", metadata.sender);
            }
            ContentBody::ReceiptMessage(receipt) => {
                log::info!("{} received a message.", metadata.sender);
                for &ts in &receipt.timestamp {
                    // Signal uses timestamps in milliseconds, chrono has nanoseconds
                    if let Some((sess, msg)) =
                        storage.mark_message_received(millis_to_naive_chrono(ts))
                    {
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
                log::info!("{} is calling.", metadata.sender);
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
}

#[derive(Message)]
#[rtype(result = "()")]
struct FetchAttachment {
    mid: i32,
    dest: PathBuf,
    ptr: AttachmentPointer,
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
        let FetchAttachment { mid, dest, ptr } = fetch;

        let client_addr = ctx.address();

        let mut service = self.unauthenticated_service();
        let mut storage = self.storage.clone().unwrap();

        // Sailfish and/or Rust needs "image/jpg" and some others need coaching
        // before taking a wild guess
        let ext = match ptr.content_type() {
            "text/plain" => "txt",
            "image/jpeg" => "jpg",
            "image/jpg" => "jpg",
            other => mime_guess::get_mime_extensions_str(other)
                .expect("Could not find mime")
                .first()
                .unwrap(),
        };

        let ptr2 = ptr.clone();
        Box::pin(
            async move {
                use futures::io::AsyncReadExt;
                use libsignal_service::attachment_cipher::*;

                let mut stream = loop {
                    let r = service.get_attachment(&ptr).await;
                    match r {
                        Ok(stream) => break stream,
                        Err(ServiceError::Timeout{ .. }) => {
                            log::warn!("get_attachment timed out, retrying")
                        },
                        Err(e) => Err(e)?,
                    }
                };
                log::info!("Downloading attachment");

                // We need the whole file for the crypto to check out 😢
                let mut ciphertext = if let Some(size) = ptr.size {
                    Vec::with_capacity(size as usize)
                } else {
                    Vec::new()
                };
                let len = stream.read_to_end(&mut ciphertext).await
                    .expect("streamed attachment");

                // Downloaded attachment length (1781792) is not equal to expected length of 1708516 bytes.
                // Not sure where the difference comes from at this point.
                if len != ptr.size.unwrap() as usize {
                    log::warn!("Downloaded attachment length ({}) is not equal to expected length of {} bytes.", len, ptr.size.unwrap());
                }
                let key_material = ptr.key();
                assert_eq!(
                    key_material.len(),
                    64,
                    "key material for attachments is ought to be 64 bytes"
                );
                let mut key = [0u8; 64];
                key.copy_from_slice(&key_material);

                decrypt_in_place(key, &mut ciphertext).expect("attachment decryption");
                if let Some(size) = ptr.size {
                    log::debug!("Truncating attachment to {}B", size);
                    ciphertext.truncate(size as usize);
                }

                let attachment_path =
                    crate::store::save_attachment(&dest, ext, futures::io::Cursor::new(ciphertext))
                        .await;

                storage.register_attachment(
                    mid,
                    attachment_path.to_str().expect("attachment path utf-8"),
                    ptr.content_type(),
                );
                client_addr.send(AttachmentDownloaded(mid)).await?;
                Ok(())
            }
            .into_actor(self)
            .map(move |r: Result<(), failure::Error>, act, _ctx| {
                // Synchronise on the actor, to log the error to attachment.log
                if let Err(e) = r {
                    let e = format!("Error fetching attachment for message with ID `{}` {:?}: {:?}", mid, ptr2, e);
                    log::error!("{}", e);
                    let mut log = act.attachment_log();
                    if let Err(e) = writeln!(log, "{}", e) {
                        log::error!("Could not write error to error log: {}", e);
                    }
                }
            })
        )
    }
}

impl Handler<SendMessage> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    // Equiv of worker/send.go
    fn handle(&mut self, SendMessage(mid): SendMessage, _ctx: &mut Self::Context) -> Self::Result {
        log::info!("ClientActor::SendMessage({:?})", mid);
        let storage = self.storage.as_mut().unwrap();
        let msg = storage.fetch_message(mid).unwrap();

        let mut sender = MessageSender::new(
            self.service.clone().unwrap(),
            self.cipher.clone().unwrap(),
            DEFAULT_DEVICE_ID,
        );
        let session = storage.fetch_session(msg.sid).unwrap();

        if !msg.queued {
            log::warn!("Message is not queued, refusing to transmit.");
            return Box::pin(async {}.into_actor(self).map(|_, _, _| ()));
        }

        log::trace!("Sending for session: {:?}", session);
        log::trace!("Sending message: {:?}", msg);

        let local_addr = self.local_addr.clone().unwrap();
        let storage = storage.clone();
        Box::pin(
            async move {
                let group = if let Some(group_id) = session.group_id.as_ref() {
                    if group_id != "" {
                        Some(GroupContext {
                            id: Some(hex::decode(group_id).expect("hex encoded group id")),
                            r#type: Some(GroupType::Deliver.into()),

                            ..Default::default()
                        })
                    } else {
                        None
                    }
                } else {
                    None
                };

                // XXX online status goes in that bool
                let online = false;
                let timestamp = msg.timestamp.timestamp_millis() as u64;
                let mut content = DataMessage {
                    body: Some(msg.message.clone()),
                    flags: None,
                    timestamp: Some(timestamp),
                    // XXX: depends on the features in the message!
                    required_protocol_version: Some(0),
                    group,

                    ..Default::default()
                };

                if msg.flags == 1 {
                    log::warn!("End session unimplemented");
                }

                if let Some(attachment) = msg.attachment {
                    use actix_threadpool::BlockingError;
                    let attachment_path = attachment.clone();
                    let contents = match actix_threadpool::run(move || {
                        std::fs::read(&attachment_path)
                    })
                    .await
                    {
                        Err(BlockingError::Canceled) => {
                            failure::bail!("Threadpool Canceled");
                        }
                        Err(BlockingError::Error(e)) => {
                            failure::bail!("Could not read attachment: {}", e);
                        }
                        Ok(contents) => contents,
                    };
                    let spec = AttachmentSpec {
                        content_type: mime_guess::from_path(&attachment)
                            .first()
                            .unwrap()
                            .essence_str()
                            .into(),
                        length: contents.len(),
                        file_name: Path::new(&attachment)
                            .file_name()
                            .map(|f| f.to_string_lossy().into_owned()),
                        preview: None,
                        voice_note: false,
                        borderless: false,
                        width: 0,
                        height: 0,
                        caption: None,
                        blur_hash: None,
                    };
                    let ptr = match sender.upload_attachment(spec, contents).await {
                        Ok(v) => v,
                        Err(e) => {
                            failure::bail!("Failed to upload attachment: {}", e);
                        }
                    };
                    content.attachments.push(ptr);
                }

                log::trace!("Transmitting {:?}", content);

                let mut needs_sync = false;

                if session.is_group {
                    let members = session.group_members.as_ref().unwrap();
                    // I'm gonna be *really* glad when this is strictly typed and handled by the DB.
                    for member in members.split(',') {
                        let recipient = ServiceAddress {
                            e164: Some(member.to_string()),
                            relay: None,
                            uuid: None,
                        };
                        if local_addr.matches(&recipient) {
                            continue;
                        }
                        // Clone + async closure means we can use an immutable borrow.
                        match sender
                            .send_message(recipient, None, content.clone(), timestamp, online)
                            .await
                        {
                            Ok(s) => {
                                if s.needs_sync {
                                    needs_sync = true;
                                }
                            }
                            Err(e) => log::error!("Error sending message: {}", e),
                        }
                    }
                } else {
                    let recipient = ServiceAddress {
                        e164: Some(session.source.clone()),
                        relay: None,
                        uuid: None,
                    };

                    match sender
                        .send_message(recipient, None, content.clone(), timestamp, online)
                        .await
                    {
                        Ok(s) => {
                            if s.needs_sync {
                                needs_sync = true;
                            }
                        }
                        Err(e) => log::error!("Error sending message: {}", e),
                    }
                }

                if needs_sync {
                    // Sync messages for connected devices
                    use libsignal_service::content::sync_message;

                    let container = SyncMessage {
                        sent: Some(sync_message::Sent {
                            destination_e164: if session.is_group {
                                None
                            } else {
                                Some(session.source)
                            },
                            message: Some(content),
                            timestamp: Some(timestamp),

                            ..Default::default()
                        }),

                        ..Default::default()
                    };
                    log::trace!("Transmitting {:?}", container);

                    match sender
                        .send_message(local_addr, None, container, timestamp, online)
                        .await
                    {
                        Ok(s) => {
                            if s.needs_sync {
                                log::warn!("Still got a needs_sync");
                            }
                        }
                        Err(e) => log::error!("Error sending message: {}", e),
                    }
                }

                // Mark as sent
                storage.dequeue_message(mid);

                Ok((session.id, mid, msg.message))
            }
            .into_actor(self)
            .map(move |res, act, _ctx| match res {
                Ok((sid, mid, message)) => {
                    act.inner
                        .pinned()
                        .borrow()
                        .messageSent(sid, mid, message.into())
                }
                Err(e) => log::error!("Sending message: {}", e),
            }),
        )
    }
}

impl Handler<AttachmentDownloaded> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        AttachmentDownloaded(mid): AttachmentDownloaded,
        _ctx: &mut Self::Context,
    ) {
        log::info!("Attachment downloaded for message {:?}", mid);
        // XXX: refresh Qt views
    }
}

impl Handler<StorageReady> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        StorageReady(storage, config): StorageReady,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        self.storage = Some(storage.clone());

        let context = self.context.clone();
        let storage_for_password = storage.clone();
        let request_password = async move {
            // Web socket
            let phonenumber = phonenumber::parse(None, config.tel.expect("phone number")).unwrap();
            let uuid = config.uuid.clone();
            let e164 = phonenumber
                .format()
                .mode(phonenumber::Mode::E164)
                .to_string();
            log::info!("E164: {}", e164);

            let password = storage_for_password.signal_password().await.unwrap();
            let signaling_key = Some(storage_for_password.signaling_key().await.unwrap());

            (uuid, e164, password, signaling_key)
        };
        let service_cfg = self.service_cfg();

        Box::pin(request_password.into_actor(self).map(
            move |(uuid, e164, password, signaling_key), act, ctx| {
                let (credentials, service) =
                    act.authenticated_service(uuid.clone(), e164.clone(), password, signaling_key);
                // end web socket

                // Signal service context
                let store_context = libsignal_protocol::store_context(
                    &context,
                    // Storage is a pointer-to-shared-storage
                    storage.clone(),
                    storage.clone(),
                    storage.clone(),
                    storage.clone(),
                )
                .expect("initialized storage");
                let local_addr = ServiceAddress {
                    uuid,
                    e164: Some(e164),
                    relay: None,
                };
                let cipher = ServiceCipher::from_context(
                    context.clone(),
                    store_context.clone(),
                    local_addr.clone(),
                    service_cfg
                        .credentials_validator(&context)
                        .expect("trust root"),
                );
                // end signal service context
                act.credentials = Some(credentials);
                act.service = Some(service);
                act.cipher = Some(cipher);
                act.local_addr = Some(local_addr);
                act.store_context = Some(store_context);

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

    fn handle(&mut self, _: Restart, _ctx: &mut Self::Context) -> Self::Result {
        let service = self.service.clone().unwrap();
        let credentials = self.credentials.clone().unwrap();

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();
        Box::pin(
            async move {
                let mut receiver = MessageReceiver::new(service.clone());

                receiver.create_message_pipe(credentials).await
            }
            .into_actor(self)
            .map(move |pipe, act, ctx| match pipe {
                Ok(pipe) => {
                    ctx.add_stream(pipe.stream());
                    act.inner.pinned().borrow_mut().connected = true;
                    act.inner.pinned().borrow().connectedChanged();
                }
                Err(e) => {
                    log::error!("Error starting stream: {}", e);
                    log::info!("Retrying in 10");
                    let addr = ctx.address();
                    Arbiter::spawn(async move {
                        actix::clock::delay_for(actix::clock::Duration::from_secs(10)).await;
                        addr.send(Restart).await.expect("retry restart");
                    });
                }
            }),
        )
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

        if msg.is_receipt() {
            self.process_receipt(msg);
        } else if msg.is_prekey_signal_message()
            || msg.is_signal_message()
            || msg.is_unidentified_sender()
        {
            self.process_message(msg, ctx);
        } else {
            log::warn!("Unknown envelope type {:?}", msg.r#type());
        }
    }

    /// Called when the WebSocket somehow has disconnected.
    fn finished(&mut self, ctx: &mut Self::Context) {
        log::debug!("Attempting reconnect");

        self.inner.pinned().borrow_mut().connected = false;
        self.inner.pinned().borrow().connectedChanged();

        ctx.notify(Restart);
    }
}

#[derive(Clone, Eq, PartialEq)]
pub enum RegistrationResponse {
    Ok,
    CaptchaRequired,
}

impl From<VoiceVerificationCodeResponse> for RegistrationResponse {
    fn from(response: VoiceVerificationCodeResponse) -> RegistrationResponse {
        match response {
            VoiceVerificationCodeResponse::CaptchaRequired => RegistrationResponse::CaptchaRequired,
            VoiceVerificationCodeResponse::CallIssued => RegistrationResponse::Ok,
        }
    }
}

impl From<SmsVerificationCodeResponse> for RegistrationResponse {
    fn from(response: SmsVerificationCodeResponse) -> RegistrationResponse {
        match response {
            SmsVerificationCodeResponse::CaptchaRequired => RegistrationResponse::CaptchaRequired,
            SmsVerificationCodeResponse::SmsSent => RegistrationResponse::Ok,
        }
    }
}

#[derive(Message)]
#[rtype(result = "Result<RegistrationResponse, failure::Error>")]
pub struct Register {
    pub e164: String,
    pub password: String,
    pub use_voice: bool,
}

impl Handler<Register> for ClientActor {
    type Result = ResponseActFuture<Self, Result<RegistrationResponse, failure::Error>>;

    fn handle(&mut self, reg: Register, _ctx: &mut Self::Context) -> Self::Result {
        let Register {
            e164,
            password,
            use_voice,
        } = reg;

        let (_credentials, push_service) =
            self.authenticated_service(None, e164.clone(), password, None);
        let mut account_manager = AccountManager::new(self.context.clone(), push_service);
        let registration_procedure = async move {
            if use_voice {
                account_manager
                    .request_voice_verification_code(&e164)
                    .await
                    .map(Into::into)
            } else {
                account_manager
                    .request_sms_verification_code(&e164)
                    .await
                    .map(Into::into)
            }
        };

        Box::pin(
            registration_procedure
                .into_actor(self)
                .map(|result, _act, _ctx| Ok(result?)),
        )
    }
}

#[derive(Message)]
#[rtype(result = "Result<(u32, ConfirmCodeResponse), failure::Error>")]
pub struct ConfirmRegistration {
    pub e164: String,
    pub password: String,
    pub confirm_code: u32,
    pub signaling_key: [u8; 52],
}

impl Handler<ConfirmRegistration> for ClientActor {
    type Result = ResponseActFuture<Self, Result<(u32, ConfirmCodeResponse), failure::Error>>;

    fn handle(&mut self, confirm: ConfirmRegistration, _ctx: &mut Self::Context) -> Self::Result {
        let ConfirmRegistration {
            e164,
            password,
            confirm_code,
            signaling_key,
        } = confirm;

        let registration_id =
            libsignal_protocol::generate_registration_id(&self.context, 0).unwrap();
        log::trace!("registration_id: {}", registration_id);

        let (_credentials, mut push_service) =
            self.authenticated_service(None, e164.clone(), password, None);
        let confirmation_procedure = async move {
            push_service.confirm_verification_code(confirm_code,
                libsignal_service::push_service::ConfirmCodeMessage::new_without_unidentified_access (
                    signaling_key.to_vec(),
                    registration_id,
                )).await
        };

        Box::pin(
            confirmation_procedure
                .into_actor(self)
                .map(move |result, _act, _ctx| Ok((registration_id, result?))),
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

        let service = self.service.clone().unwrap();
        let mut am = AccountManager::new(self.context.clone(), service);

        let (next_signed_pre_key_id, pre_keys_offset_id) =
            self.storage.as_ref().unwrap().next_pre_key_ids();

        let store_context = self.store_context.clone().unwrap();
        let proc = async move {
            am.update_pre_key_bundle(store_context, next_signed_pre_key_id, pre_keys_offset_id)
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
