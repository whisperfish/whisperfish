use std::collections::HashMap;

use crate::store::orm::{self, UnidentifiedAccessMode};

use super::ClientActor;
use actix::prelude::*;
use libsignal_service::{prelude::*, protocol, unidentified_access::UnidentifiedAccess};

#[derive(Hash, Eq, PartialEq, Clone, Copy)]
pub enum CertType {
    Complete,
    UuidOnly,
}

impl CertType {
    fn all() -> impl Iterator<Item = Self> {
        vec![Self::Complete, Self::UuidOnly].into_iter()
    }
}

// XXX In principle, these can be persisted, and don't need to be fetched on every start.
#[derive(Default, Clone)]
pub struct UnidentifiedCertificates {
    certs: HashMap<CertType, protocol::SenderCertificate>,
}

const UNRESTRICTED_ACCESS_KEY: [u8; 16] = [0u8; 16];

impl UnidentifiedCertificates {
    pub fn get(&self, cert: CertType) -> Option<&protocol::SenderCertificate> {
        self.certs.get(&cert)
    }

    /// Look up the correct access key for a certain recipient.
    ///
    /// Java equivalent: UnidentifiedAccessUtil::getTargetUnidentifiedAccessKey
    pub fn access_key_for(&self, recipient: &orm::Recipient, for_story: bool) -> Option<[u8; 16]> {
        match recipient.unidentified_access_mode {
            UnidentifiedAccessMode::Unknown => Some(
                recipient
                    .unidentified_access_key()
                    .unwrap_or(UNRESTRICTED_ACCESS_KEY),
            ),
            UnidentifiedAccessMode::Disabled => None,
            UnidentifiedAccessMode::Enabled => recipient.unidentified_access_key(),
            UnidentifiedAccessMode::Unrestricted => Some(UNRESTRICTED_ACCESS_KEY),
        }
        .or({
            if for_story {
                Some(UNRESTRICTED_ACCESS_KEY)
            } else {
                None
            }
        })
    }

    pub fn access_for(
        &self,
        cert: CertType,
        recipient: &orm::Recipient,
        for_story: bool,
    ) -> Option<UnidentifiedAccess> {
        self.get(cert).and_then(|cert| {
            self.access_key_for(recipient, for_story)
                .map(|key| UnidentifiedAccess {
                    certificate: cert.clone(),
                    key: key.to_vec(),
                })
        })
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct RotateUnidentifiedCertificates;

impl Handler<RotateUnidentifiedCertificates> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        _: RotateUnidentifiedCertificates,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let mut service = self.authenticated_service();
        // Short cut
        let all_certs_available =
            CertType::all().all(|t| self.unidentified_certificates.certs.contains_key(&t));
        Box::pin(
            async move {
                let mut certs = HashMap::<_, protocol::SenderCertificate>::default();
                if !all_certs_available {
                    for cert_type in CertType::all() {
                        let cert = match cert_type {
                            CertType::Complete => service.get_sender_certificate().await?,
                            CertType::UuidOnly => {
                                service.get_uuid_only_sender_certificate().await?
                            }
                        };
                        certs.insert(cert_type, cert);
                    }
                }
                Result::<_, ServiceError>::Ok(certs)
            }
            .into_actor(self)
            .map(move |certs, act, _ctx| {
                if all_certs_available {
                    return;
                }
                match certs {
                    Ok(certs) => {
                        tracing::debug!("Fetched {} sender certificates", certs.len());
                        act.unidentified_certificates.certs = certs;
                    }
                    Err(e) => {
                        tracing::error!("Error fetching sender certificates: {}", e);
                    }
                }
            }),
        )
    }
}
