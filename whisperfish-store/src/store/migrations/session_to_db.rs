mod quirk;

use crate::observer::Observable;
use crate::store::orm::{self, Prekey, SessionRecord, SignedPrekey};
use crate::store::Storage;
use libsignal_service::protocol::{
    self, DeviceId, IdentityKey, PreKeyId, ProtocolAddress, SignedPreKeyId,
};
use libsignal_service::push_service::DEFAULT_DEVICE_ID;
use protocol::SignalProtocolError;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};

pub struct SessionStorageMigration<O: Observable>(pub Storage<O>);

impl<O: Observable> Deref for SessionStorageMigration<O> {
    type Target = Storage<O>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl<O: Observable> DerefMut for SessionStorageMigration<O> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

fn addr_to_path_component<'a>(addr: &'a (impl AsRef<[u8]> + ?Sized + 'a)) -> &'a str {
    let addr: &'a [u8] = addr.as_ref();
    let addr = if addr[0] == b'+' { &addr[1..] } else { addr };
    std::str::from_utf8(addr).expect("address in valid UTF8")
}

fn option_warn<T>(o: Option<T>, s: &'static str) -> Option<T> {
    if o.is_none() {
        tracing::warn!("{}", s)
    }
    o
}

fn name_to_protocol_addr(name: &str, id: DeviceId) -> Option<ProtocolAddress> {
    if let Ok(uuid) = uuid::Uuid::parse_str(name) {
        return Some(ProtocolAddress::new(uuid.to_string(), id));
    }

    let phonenumbers = [name, &format!("+{}", name)];
    for pn in &phonenumbers {
        if let Ok(addr) = phonenumber::parse(None, pn) {
            return Some(ProtocolAddress::new(
                addr.format().mode(phonenumber::Mode::E164).to_string(),
                id,
            ));
        }
    }
    None
}

impl<O: Observable> SessionStorageMigration<O> {
    #[tracing::instrument(name = "session_to_db", skip(self))]
    pub async fn execute(&self) {
        let session_dir = self.0.path().join("storage").join("sessions");
        if session_dir.exists() {
            tracing::trace!("calling migrate_sessions");
            self.migrate_sessions().await;

            if let Err(e) = tokio::fs::remove_dir(session_dir).await {
                tracing::warn!("Could not remove alledgedly empty session dir: {}", e);
            }
        }

        if self.0.path().join("storage").join("identity").exists() {
            tracing::trace!("calling migrate_identities");
            self.migrate_identities().await;
        }

        if self.0.path().join("storage").join("prekeys").exists() {
            tracing::trace!("calling migrate_prekeys");
            self.migrate_prekeys().await;
        }

        if self
            .0
            .path()
            .join("storage")
            .join("signed_prekeys")
            .exists()
        {
            tracing::trace!("calling migrate_signed_prekeys");
            self.migrate_signed_prekeys().await;
        }
    }

    fn read_dir_and_filter(&self, dir: impl AsRef<Path>) -> Box<dyn Iterator<Item = String>> {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Potentially in the future also e.kind() == std::io::ErrorKind::NotADirectory
                tracing::info!(
                    "Migrating sessions is not necessary; there's no session directory."
                );
                return Box::new(std::iter::empty());
            }
            Err(e) => {
                panic!("Something went wrong reading the session directory: {}", e);
            }
        };

        Box::new(entries.filter_map(|entry| {
            let entry = entry.expect("directory listing");
            if !entry.path().is_file() {
                tracing::warn!("Non-file directory entry: {:?}. Skipping", entry);
                return None;
            }

            // XXX: *maybe* Signal could become a cross-platform desktop app.
            //      Issue #77
            use std::os::unix::ffi::OsStringExt;
            let name = entry.file_name().into_vec();

            match String::from_utf8(name) {
                Ok(s) => Some(s),
                Err(_e) => {
                    tracing::warn!("non-UTF8 session name; skipping");
                    None
                }
            }
        }))
    }

    #[tracing::instrument(skip(self))]
    async fn migrate_prekeys(&self) {
        let prekey_dir = self.path().join("storage").join("prekeys");
        let prekeys = self.read_dir_and_filter(prekey_dir).filter_map(|name| {
            let id = name
                .parse::<u32>()
                .map_err(|_| tracing::warn!("Unparseable prekey id {}", name))
                .ok()?;

            Some(PreKeyId::from(id))
        });

        for prekey in prekeys {
            let path = self.prekey_path(prekey);

            tracing::trace!("Loading prekey {} for migration", prekey);
            let _lock = self.protocol_store.write().await;
            let buf = match self.read_file(&path).await {
                Ok(buf) => buf,
                Err(e) if !path.exists() => {
                    tracing::trace!(
                        "Skipping prekey because {} does not exist ({})",
                        path.display(),
                        e
                    );

                    continue;
                }
                Err(e) => {
                    tracing::error!("Problem reading prekey at {} ({})", path.display(), e);
                    continue;
                }
            };

            let buf = quirk::pre_key_from_0_5(&buf).unwrap();

            {
                use crate::schema::prekeys::dsl::*;
                use diesel::prelude::*;
                let prekey_record = Prekey {
                    id: u32::from(prekey) as _,
                    record: buf,
                    identity: orm::Identity::Aci,
                };
                let res = diesel::insert_into(prekeys)
                    .values(prekey_record)
                    .execute(&mut *self.0.db());

                use diesel::result::{DatabaseErrorKind, Error};
                match res {
                    Ok(1) => (),
                    Ok(n) => unreachable!(
                        "inserting a single record cannot return {} rows changed.",
                        n
                    ),
                    Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                        tracing::warn!(
                            "Already found prekey {} in the database.",
                            u32::from(prekey)
                        );
                    }
                    Err(e) => panic!("{1}: {:?}", e, "well behaving database"),
                }
            }

            // By now, the session is safely stored in the database, so we can remove the file.
            if let Err(e) = std::fs::remove_file(path) {
                tracing::debug!(
                    "Could not delete prekey {}, assuming non-existing: {}",
                    prekey,
                    e
                );
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn migrate_signed_prekeys(&self) {
        let prekey_dir = self.path().join("storage").join("signed_prekeys");
        let prekeys = self.read_dir_and_filter(prekey_dir).filter_map(|name| {
            let id = name
                .parse::<u32>()
                .map_err(|_| tracing::warn!("Unparseable prekey id {}", name))
                .ok()?;

            Some(SignedPreKeyId::from(id))
        });

        for prekey in prekeys {
            let path = self.signed_prekey_path(prekey);

            tracing::trace!("Loading signed prekey {} for migration", prekey);
            let _lock = self.protocol_store.write().await;
            let buf = match self.read_file(&path).await {
                Ok(buf) => buf,
                Err(e) if !path.exists() => {
                    tracing::trace!(
                        "Skipping signed prekey because {} does not exist ({})",
                        path.display(),
                        e
                    );

                    continue;
                }
                Err(e) => {
                    tracing::error!(
                        "Problem reading signed prekey at {} ({})",
                        path.display(),
                        e
                    );
                    continue;
                }
            };

            let buf = quirk::signed_pre_key_from_0_5(&buf).unwrap();

            {
                use crate::schema::signed_prekeys::dsl::*;
                use diesel::prelude::*;
                let signed_prekey_record = SignedPrekey {
                    id: u32::from(prekey) as _,
                    record: buf,
                    identity: orm::Identity::Aci,
                };
                let res = diesel::insert_into(signed_prekeys)
                    .values(signed_prekey_record)
                    .execute(&mut *self.0.db());

                use diesel::result::{DatabaseErrorKind, Error};
                match res {
                    Ok(1) => (),
                    Ok(n) => unreachable!(
                        "inserting a single record cannot return {} rows changed.",
                        n
                    ),
                    Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                        tracing::warn!(
                            "Already found signed prekey {} in the database.",
                            u32::from(prekey)
                        );
                    }
                    Err(e) => panic!("{1}: {:?}", e, "well behaving database"),
                }
            }

            // By now, the session is safely stored in the database, so we can remove the file.
            if let Err(e) = std::fs::remove_file(path) {
                tracing::debug!(
                    "Could not delete prekey {}, assuming non-existing: {}",
                    prekey,
                    e
                );
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn migrate_sessions(&self) {
        let session_dir = self.path().join("storage").join("sessions");

        let sessions = self
            .read_dir_and_filter(session_dir)
            // Parse the session file names
            .filter_map(|name| {
                if name.len() < 3 {
                    tracing::warn!("Strange session name; skipping ({})", name);
                    return None;
                }

                tracing::info!("Migrating session {}", name);

                // Parse: session file consists of ADDR + _ + ID
                let mut split = name.split('_');
                let name = option_warn(split.next(), "no session name; skipping")?;
                let id = option_warn(split.next(), "no session id; skipping")?;
                let id: u32 = option_warn(id.parse().ok(), "unparseable session id")?;

                let addr = option_warn(
                    name_to_protocol_addr(name, id.try_into().unwrap()),
                    "unparsable file name",
                )?;
                Some(addr)
            });

        // Now read the files, put them in the database, and remove the file
        for addr in sessions {
            let path = self.session_path(&addr);

            tracing::trace!("Loading session for {:?} from {:?}", addr, path);
            let _lock = self.protocol_store.read().await;

            let buf = match self.read_file(&path).await {
                Ok(buf) => match quirk::session_from_0_5(&buf) {
                    Ok(buf) => buf,
                    Err(e) => {
                        tracing::warn!("Corrupt session: {}. Continuing", e);
                        continue;
                    }
                },
                Err(e) if !path.exists() => {
                    tracing::trace!(
                        "Skipping session because session file does not exist ({})",
                        e
                    );
                    continue;
                }
                Err(e) => {
                    tracing::error!(
                        "Problem reading session: {}.  Skipping, but here be dragons.",
                        e
                    );
                    continue;
                }
            };

            {
                use crate::schema::session_records::dsl::*;
                use diesel::prelude::*;
                let session_record = SessionRecord {
                    address: addr.name().to_string(),
                    device_id: u32::from(addr.device_id()) as i32,
                    record: buf,
                    identity: orm::Identity::Aci,
                };
                let res = diesel::insert_into(session_records)
                    .values(session_record)
                    .execute(&mut *self.0.db());

                use diesel::result::{DatabaseErrorKind, Error};
                match res {
                    Ok(1) => (),
                    Ok(n) => unreachable!(
                        "inserting a single record cannot return {} rows changed.",
                        n
                    ),
                    Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                        tracing::warn!("Already found a session for {} in the database. Skipping and deleting the one on storage.", addr);
                    }
                    Err(e) => panic!("{1}: {:?}", e, "well behaving database"),
                }
            }

            // By now, the session is safely stored in the database, so we can remove the file.
            if let Err(e) = std::fs::remove_file(path) {
                tracing::debug!(
                    "Could not delete session {}, assuming non-existing: {}",
                    addr.to_string(),
                    e
                );
            }
        }
    }

    #[tracing::instrument(skip(self))]
    async fn migrate_identities(&self) {
        let identity_dir = self.0.path().join("storage").join("identity");

        let identities = self
            .read_dir_and_filter(identity_dir)
            // Parse the session file names
            .filter_map(|name| {
                if !name.starts_with("remote_") {
                    let allow_list = [
                        "http_password",
                        "http_signaling_key",
                        "identity_key",
                        "pni_identity_key",
                        "regid",
                        "pni_regid",
                    ];
                    if !allow_list.contains(&name.as_str()) {
                        tracing::warn!(
                            "Identity file `{}` does not start with `remote_`; skipping",
                            name
                        );
                    }
                    return None;
                }

                let addr = &name["remote_".len()..];
                let addr = option_warn(
                    name_to_protocol_addr(addr, *DEFAULT_DEVICE_ID),
                    "unparsable file name",
                )?;

                Some(addr)
            });

        for addr in identities {
            tracing::trace!("Migrating identity for {:?} to database", addr);
            let buf = self
                .read_identity_key_file(&addr)
                .await
                .expect("readable identity file");
            let buf = if let Some(buf) = buf {
                buf
            } else {
                // XXX: comply with promises.
                tracing::warn!(
                    "Not migrating {}, since it's an unparsable form of identity. This file will be removed in the future.",
                    addr
                );
                continue;
            };

            use crate::schema::identity_records::dsl::*;
            use diesel::prelude::*;
            let res = diesel::insert_into(identity_records)
                .values((address.eq(addr.name()), record.eq(buf.serialize().to_vec())))
                .execute(&mut *self.0.db());

            use diesel::result::{DatabaseErrorKind, Error};
            match res {
                Ok(1) => (),
                Ok(n) => unreachable!(
                    "inserting a single record cannot return {} rows changed.",
                    n
                ),
                Err(Error::DatabaseError(DatabaseErrorKind::UniqueViolation, _)) => {
                    tracing::warn!("Already found an identity for {} in the database. Skipping and deleting the one on storage.", addr);
                }
                Err(e) => panic!("{1}: {:?}", e, "well behaving database"),
            }

            // By now, the identity is safely stored in the database, so we can remove the file.
            if let Err(e) = std::fs::remove_file(self.identity_path(&addr)) {
                tracing::debug!(
                    "Could not delete identity {}, assuming non-existing: {}",
                    addr.to_string(),
                    e
                );
            }
        }
    }

    fn session_path(&self, addr: &ProtocolAddress) -> PathBuf {
        let recipient_id = addr_to_path_component(addr.name());

        self.0.path().join("storage").join("sessions").join(format!(
            "{}_{}",
            recipient_id,
            addr.device_id()
        ))
    }

    fn identity_path(&self, addr: &ProtocolAddress) -> PathBuf {
        let recipient_id = addr_to_path_component(addr.name());

        self.0
            .path()
            .join("storage")
            .join("identity")
            .join(format!("remote_{}", recipient_id,))
    }

    fn prekey_path(&self, id: PreKeyId) -> PathBuf {
        self.0
            .path()
            .join("storage")
            .join("prekeys")
            .join(format!("{:09}", u32::from(id)))
    }

    fn signed_prekey_path(&self, id: SignedPreKeyId) -> PathBuf {
        self.0
            .path()
            .join("storage")
            .join("signed_prekeys")
            .join(format!("{:09}", u32::from(id)))
    }

    async fn read_identity_key_file(
        &self,
        addr: &ProtocolAddress,
    ) -> Result<Option<IdentityKey>, SignalProtocolError> {
        let path = self.identity_path(addr);
        if path.is_file() {
            let buf = self.read_file(path).await.expect("read identity key");
            match buf.len() {
                // Old format
                32 => Ok(Some(
                    protocol::PublicKey::from_djb_public_key_bytes(&buf)?.into(),
                )),
                // New format
                33 => Ok(Some(IdentityKey::decode(&buf)?)),
                _ => Err(SignalProtocolError::InvalidArgument(format!(
                    "Identity key has length {}, expected 32 or 33",
                    buf.len()
                ))),
            }
        } else {
            Ok(None)
        }
    }
}
