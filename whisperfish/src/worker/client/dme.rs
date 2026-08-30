use actix::prelude::*;
use chrono::Utc;
use libsignal_service::protocol::{DeviceId, ServiceId};
use uuid::Uuid;

use super::unidentified::CertType;
use super::{ClientActor, SESSION_RESET_INTERVAL};

/// Ask the client to send a DME (DecryptionErrorMessage / "retry receipt") to
/// a sender whose group message failed to decrypt locally with
/// `NoSenderKeyState`.
///
/// On receipt the sender re-shares its sender key and (best-effort) resends the
/// payload; without it, every subsequent group envelope from that sender is
/// dropped by us forever.
#[derive(Debug, Message)]
#[rtype(result = "()")]
pub struct NoSenderKeyDme {
    /// ServiceId of the failed envelope's sender.
    pub recipient: ServiceId,
    /// Device of the failed envelope's sender.
    pub failed_device: DeviceId,
    pub distribution_id: Uuid,
    /// Client timestamp of the failed envelope; the sender uses it to find its
    /// send-log entry.
    pub failed_timestamp: u64,
}

impl Handler<NoSenderKeyDme> for ClientActor {
    type Result = ();

    #[tracing::instrument(skip(self, _ctx), fields(
        recipient = msg.recipient.service_id_string(),
        distribution_id = %msg.distribution_id,
        failed_timestamp = msg.failed_timestamp,
    ))]
    fn handle(&mut self, msg: NoSenderKeyDme, _ctx: &mut Self::Context) {
        // Prune entries older than the window so the map stays bounded (the
        // send loop that follows can never list more than one entry per
        // (recipient, device, distribution_id) within a SESSION_RESET_INTERVAL
        // window).
        self.last_dme_dispatch.retain(|_, sent_at| {
            Utc::now().signed_duration_since(*sent_at) < SESSION_RESET_INTERVAL
        });

        let key = (msg.recipient, msg.failed_device, msg.distribution_id);
        if let Some(sent_at) = self.last_dme_dispatch.get(&key)
            && Utc::now().signed_duration_since(*sent_at) < SESSION_RESET_INTERVAL
        {
            tracing::trace!("DME rate-limited; skipping");
            return;
        }
        self.last_dme_dispatch.insert(key, Utc::now());

        // Sealed-sender access, resolved exactly as for ordinary sends.  When
        // the recipient isn't in storage we fall back to identified delivery.
        let storage = self.storage.clone();
        let certs = self.unidentified_certificates.clone();
        let cert_type = if self.settings.get_share_phone_number() {
            CertType::UuidOnly
        } else {
            CertType::Complete
        };
        let unidentified_access = storage
            .as_ref()
            .and_then(|storage| storage.fetch_recipient(&msg.recipient))
            .and_then(|recipient| certs.access_for(cert_type, &recipient, false));

        let sender = self.message_sender();
        actix::spawn(async move {
            let mut sender = match sender.await {
                Ok(sender) => sender,
                Err(e) => {
                    tracing::warn!(?e, "could not construct MessageSender for DME; skipping");
                    return;
                }
            };
            match sender
                .send_sender_key_decryption_error_message(
                    &msg.recipient,
                    unidentified_access,
                    msg.failed_timestamp,
                    msg.failed_device,
                )
                .await
            {
                Ok(()) => tracing::info!("sent DME (retry receipt) for NoSenderKeyState"),
                Err(e) => tracing::warn!(?e, "failed to send DME"),
            }
        });
    }
}
