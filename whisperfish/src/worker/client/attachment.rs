use super::voice_note_transcription;
use super::ClientActor;
use actix::prelude::*;
use anyhow::Context;
use libsignal_service::{content::AttachmentPointer, prelude::*};
use mime_classifier::{ApacheBugFlag, LoadContext, MimeClassifier, NoSniffFlag};
use std::io::Write;
use std::path::PathBuf;
use tracing_futures::Instrument;

#[derive(Message)]
#[rtype(result = "()")]
pub struct FetchAttachment {
    pub attachment_id: i32,
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
        let _span = tracing::info_span!("handle FetchAttachment", attachment_id).entered();

        let client_addr = ctx.address();

        let mut service = self.unauthenticated_service();
        let storage = self.storage.clone().unwrap();

        let attachment = storage
            .fetch_attachment(attachment_id)
            .expect("existing attachment");
        let message = storage
            .fetch_message_by_id(attachment.message_id)
            .expect("existing message");
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
        let dir = self.settings.get_string("attachment_dir");
        let dest = PathBuf::from(dir);

        // Take the extension of the file_name string, if it exists
        let ptr_ext = ptr
            .file_name
            .as_ref()
            .and_then(|file| file.split('.').last());

        // Sailfish and/or Rust needs "image/jpg" and some others need coaching
        // before taking a wild guess
        let mut ext = match ptr.content_type() {
            "text/plain" => "txt",
            "image/jpeg" => "jpg",
            "image/png" => "png",
            "image/jpg" => "jpg",
            "text/x-signal-plain" => "txt",
            "application/x-signal-view-once" => "bin",
            "audio/x-scpls" => "pls",
            other => mime_guess::get_mime_extensions_str(other)
                .and_then(|x| x.first())
                .copied() // &&str -> &str
                .unwrap_or_else(|| {
                    let ext = ptr_ext.unwrap_or("bin");
                    tracing::warn!("Could not find mime type for {other}; defaulting to .{ext}",);
                    ext
                }),
        }
        .to_string();

        let ptr2 = attachment.clone();
        let attachment_id = attachment.id;
        let session_id = message.session_id;
        let message_id = message.id;
        let transcribe_voice_notes = self.settings.get_transcribe_voice_notes();

        storage
            .update_attachment_progress(attachment_id, 0)
            .expect("update attachment progress");

        Box::pin(
            async move {
                use futures::io::AsyncReadExt;
                use libsignal_service::attachment_cipher::*;

                let mut stream = loop {
                    let r = service.get_attachment(&ptr).await;
                    match r {
                        Ok(stream) => break stream,
                        Err(ServiceError::Timeout { .. }) => {
                            tracing::warn!("get_attachment timed out, retrying")
                        }
                        Err(e) => return Err(e.into()),
                    }
                };

                // We need the whole file for the crypto to check out 😢
                let actual_len = ptr.size.unwrap() as usize;
                let mut ciphertext = Vec::with_capacity(actual_len as usize);

                let mut stream_len = 0;
                let mut buf = vec![0u8; 128 * 1024];
                let mut bytes_since_previous_report = 0;
                loop {
                    let read = stream.read(&mut buf).await?;
                    bytes_since_previous_report += read;
                    stream_len += read;
                    ciphertext.extend_from_slice(&buf[..read]);
                    assert_eq!(stream_len, ciphertext.len());

                    // Report progress if more than 0.5% has been downloaded
                    if bytes_since_previous_report > actual_len / 200 {
                        storage.update_attachment_progress(attachment_id, stream_len)?;
                        bytes_since_previous_report = 0;
                    }

                    if read == 0 {
                        break;
                    }
                }

                let key_material = ptr.key();
                assert_eq!(
                    key_material.len(),
                    64,
                    "key material for attachments is ought to be 64 bytes"
                );
                let mut key = [0u8; 64];
                key.copy_from_slice(key_material);
                let mut ciphertext = tokio::task::spawn_blocking(move || {
                    decrypt_in_place(key, &mut ciphertext).expect("attachment decryption");
                    ciphertext
                })
                .await
                .context("decryption threadpoool")?;

                // Signal puts exponentially increasing padding at the end
                // to prevent some distinguishing attacks, so it has to be truncated.
                if stream_len > actual_len {
                    tracing::info!(
                        "The attachment contains {} bytes of padding",
                        (stream_len - actual_len)
                    );
                    tracing::info!("Truncating from {} to {} bytes", stream_len, actual_len);
                    ciphertext.truncate(actual_len as usize);
                }

                // Signal Desktop sometimes sends a JPEG image with .png extension,
                // so double check the received .png image, and rename it if necessary.
                if ext == "png" {
                    tracing::trace!("Checking for JPEG with .png extension...");
                    let classifier = MimeClassifier::new();
                    let computed_type = classifier.classify(
                        LoadContext::Image,
                        NoSniffFlag::Off,
                        ApacheBugFlag::Off,
                        &None,
                        &ciphertext as &[u8],
                    );
                    if computed_type == mime::IMAGE_JPEG {
                        tracing::info!("Received JPEG file with .png suffix, renaming to .jpg");
                        ext = "jpg".into();
                    }
                }

                let _attachment_path = storage
                    .save_attachment(attachment_id, &dest, &ext, &ciphertext)
                    .await?;

                client_addr
                    .send(AttachmentDownloaded {
                        session_id,
                        message_id,
                    })
                    .await?;

                if attachment.is_voice_note {
                    // If the attachment is a voice note, and we enabled automatic transcription,
                    // trigger the transcription
                    if transcribe_voice_notes {
                        client_addr
                            .send(voice_note_transcription::TranscribeVoiceNote { message_id })
                            .await?;
                    }
                }

                Ok(())
            }
            .instrument(tracing::trace_span!(
                "download attachment",
                attachment_id,
                session_id,
                message_id,
            ))
            .into_actor(self)
            .map(move |r: Result<(), anyhow::Error>, act, _ctx| {
                // Synchronise on the actor, to log the error to attachment.log
                if let Err(e) = r {
                    let e = format!(
                        "Error fetching attachment for message with ID `{}` {:?}: {:?}",
                        message.id, ptr2, e
                    );
                    if let Err(e) = act
                        .storage
                        .as_ref()
                        .unwrap()
                        .reset_attachment_progress(attachment_id)
                    {
                        tracing::error!("Could not reset attachment progress: {}", e);
                    }
                    tracing::error!("{} in handle()", e);
                    let mut log = act.attachment_log();
                    if let Err(e) = writeln!(log, "{}", e) {
                        tracing::error!("Could not write error to error log: {}", e);
                    }
                }
            }),
        )
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct AttachmentDownloaded {
    session_id: i32,
    message_id: i32,
}

impl Handler<AttachmentDownloaded> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        AttachmentDownloaded {
            session_id,
            message_id,
        }: AttachmentDownloaded,
        _ctx: &mut Self::Context,
    ) {
        tracing::info!("Attachment downloaded for message {}", message_id);
        self.inner
            .pinned()
            .borrow()
            .attachmentDownloaded(session_id, message_id);
    }
}
