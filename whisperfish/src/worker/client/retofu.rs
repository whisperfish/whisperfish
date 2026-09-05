//! Reset of trust-on-first-use (TOFU) identity state.

use std::future::Future;

use libsignal_protocol::{ServiceId, ServiceIdKind, SignalProtocolError};
use libsignal_service::content::ServiceError;
use libsignal_service::sender::MessageSenderError;

use crate::store::Storage;

/// Extract the recipient [`ServiceId`] whose stored identity key mismatched,
/// if this error represents an untrusted-identity failure.
///
/// Implemented for both the receive path ([`ServiceError`], where the mismatch
/// surfaces as [`SignalProtocolError::UntrustedIdentity`]) and the send path
/// ([`MessageSenderError::UntrustedIdentity`]), so that
/// [`maybe_reset_identity`] can recover either the same way.
pub trait UntrustedIdentityAddress {
    fn untrusted_identity(&self) -> Option<ServiceId>;
}

impl UntrustedIdentityAddress for ServiceError {
    fn untrusted_identity(&self) -> Option<ServiceId> {
        match self {
            ServiceError::SignalProtocolError(SignalProtocolError::UntrustedIdentity(address)) => {
                ServiceId::parse_from_service_id_string(address.name())
            }
            _ => None,
        }
    }
}

impl UntrustedIdentityAddress for MessageSenderError {
    fn untrusted_identity(&self) -> Option<ServiceId> {
        match self {
            MessageSenderError::UntrustedIdentity { address } => Some(*address),
            _ => None,
        }
    }
}

/// Run `f`; on an untrusted-identity error drop the stale key and retry `f`
/// once. `local_identity_kind` selects the identity store (ACI or PNI) holding
/// the recipient's key. Other results are returned verbatim.
pub async fn maybe_reset_identity<T, E, Fut, F>(
    storage: &Storage,
    local_identity_kind: ServiceIdKind,
    mut f: F,
) -> Result<T, E>
where
    E: UntrustedIdentityAddress,
    Fut: Future<Output = Result<T, E>>,
    F: FnMut() -> Fut,
{
    match f().await {
        Ok(value) => Ok(value),
        Err(e) => {
            let Some(untrusted_service_id) = e.untrusted_identity() else {
                return Err(e);
            };

            let _span = tracing::info_span!("recovering untrusted identity", ?untrusted_service_id)
                .entered();

            if !storage
                .recover_untrusted_identity(&untrusted_service_id, local_identity_kind)
                .await
            {
                tracing::warn!("identity recovery failed");
            }

            f().await
        }
    }
}
