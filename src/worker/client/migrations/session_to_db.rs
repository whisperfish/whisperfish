use actix::prelude::*;
use libsignal_service::prelude::protocol::ProtocolAddress;

use libsignal_service::prelude::protocol;
use protocol::SignalProtocolError;

use crate::store::orm::SessionRecord;

use super::*;

mod quirk;

#[derive(Message)]
#[rtype(result = "()")]
pub struct MoveSessionsToDatabase;

struct SessionStorageMigration(Storage);
impl std::ops::Deref for SessionStorageMigration {
    type Target = Storage;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}
impl std::ops::DerefMut for SessionStorageMigration {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Handler<MoveSessionsToDatabase> for ClientActor {
    type Result = ResponseFuture<()>;
    fn handle(&mut self, _: MoveSessionsToDatabase, _ctx: &mut Self::Context) -> Self::Result {
        let storage = self.storage.clone().expect("initialized storage");

        let proc = async move {
            let migration = SessionStorageMigration(storage.clone());

            if storage.path().join("storage").join("sessions").exists() {
                migration.migrate_sessions().await;
            }

            if storage.path().join("storage").join("identity").exists() {
                migration.migrate_identities().await;
            }
        };

        std::pin::Pin::from(Box::new(proc))
    }
}

fn addr_to_path_component<'a>(addr: &'a (impl AsRef<[u8]> + ?Sized + 'a)) -> &'a str {
    let addr: &'a [u8] = addr.as_ref();
    let addr = if addr[0] == b'+' { &addr[1..] } else { addr };
    std::str::from_utf8(addr).expect("address in valid UTF8")
}

fn option_warn<T>(o: Option<T>, s: &'static str) -> Option<T> {
    if o.is_none() {
        log::warn!("{}", s)
    }
    o
}

impl SessionStorageMigration {
    async fn migrate_sessions(&self) {
        let session_dir = self.path().join("storage").join("sessions");

        let sessions = std::fs::read_dir(session_dir)
            // XXX: actually, storage will stop initializing this.
            .expect("initialized storage")
            // Parse the session file names
            .filter_map(|entry| {
                let entry = entry.expect("directory listing");
                if !entry.path().is_file() {
                    log::warn!("Non-file session entry: {:?}. Skipping", entry);
                    return None;
                }

                // XXX: *maybe* Signal could become a cross-platform desktop app.
                //      Issue #77
                use std::os::unix::ffi::OsStrExt;
                let name = entry.file_name();
                let name = name.as_os_str().as_bytes();

                if name.len() < 3 {
                    log::warn!(
                        "Strange session name; skipping ({})",
                        String::from_utf8_lossy(name)
                    );
                    return None;
                }
                let name = option_warn(
                    std::str::from_utf8(name).ok(),
                    "non-UTF8 session name; skipping",
                )?;

                log::info!("Migrating session {}", name);

                // Parse: session file consists of ADDR + _ + ID
                let mut split = name.split('_');
                let name = option_warn(split.next(), "no session name; skipping")?;
                let id = option_warn(split.next(), "no session id; skipping")?;
                let id: u32 = option_warn(id.parse().ok(), "unparseable session id")?;
                Some(ProtocolAddress::new(name.to_string(), id))
            });

        // Now read the files, put them in the database, and remove the file
        for addr in sessions {
            let path = self.session_path(&addr);

            log::trace!("Loading session for {:?} from {:?}", addr, path);
            let _lock = self.protocol_store.read().await;

            let buf = match self.read_file(&path).await {
                Ok(buf) => match quirk::session_from_0_5(&buf) {
                    Ok(buf) => buf,
                    Err(e) => {
                        log::warn!("Corrupt session: {}. Continuing", e);
                        continue;
                    }
                },
                Err(e) if !path.exists() => {
                    log::trace!(
                        "Skipping session because session file does not exist ({})",
                        e
                    );
                    continue;
                }
                Err(e) => {
                    log::error!(
                        "Problem reading session: {}.  Skipping, but here be dragons.",
                        e
                    );
                    continue;
                }
            };

            // XXX Phone number possibly needs a + prefix or something like that.
            //     Maybe pull it through phonenumber for normalisation.
            let recipient = self.0.fetch_recipient(Some(addr.name()), Some(addr.name()));
            let recipient = if let Some(recipient) = recipient {
                recipient
            } else {
                // FIXME, we can create this recipient at this point
                log::warn!("No recipient for this session; leaving alone.");
                continue;
            };
            {
                use crate::schema::session_records::dsl::*;
                use diesel::prelude::*;
                let session_record = SessionRecord {
                    recipient_id: recipient.id,
                    device_id: addr.device_id() as i32,
                    record: buf,
                };
                let db = self.0.db.lock();
                diesel::insert_into(session_records)
                    .values(session_record)
                    .execute(&*db)
                    // XXX we should catch duplicate primary keys here.
                    .expect("inserting record into db");
            }

            // By now, the session is safely stored in the database, so we can remove the file.
            if let Err(e) = std::fs::remove_file(path) {
                log::debug!(
                    "Could not delete session {}, assuming non-existing: {}",
                    addr.to_string(),
                    e
                );
            }
        }
    }

    async fn migrate_identities(&self) {
        let identity_dir = self.0.path().join("storage").join("identity");

        let identities = std::fs::read_dir(identity_dir)
            // XXX: actually, storage will stop initializing this.
            .expect("initialized storage")
            // Parse the session file names
            .filter_map(|entry| {
                let entry = entry.expect("directory listing");
                if !entry.path().is_file() {
                    log::warn!("Non-file identity entry: {:?}. Skipping", entry);
                    return None;
                }

                // XXX: *maybe* Signal could become a cross-platform desktop app.
                //      Issue #77
                use std::os::unix::ffi::OsStrExt;
                let name = entry.file_name();
                let name = name.as_os_str().as_bytes();
                let name = option_warn(
                    std::str::from_utf8(name).ok(),
                    "non-UTF8 identity name; skipping",
                )?;

                if !name.starts_with("remote_") {
                    log::warn!("Identity file does not start with `remote_`; skipping");
                }

                let mut split = name.split('_');
                assert_eq!(split.next(), Some("remote"));
                let addr = option_warn(split.next(), "no addr component for identity")?;

                Some(ProtocolAddress::new(addr.to_string(), DEFAULT_DEVICE_ID))
            });

        for addr in identities {
            log::trace!("Migrating identity for {:?} to database", addr);
            let buf = self
                .read_identity_key_file(&addr)
                .await
                .expect("readable identity file")
                .expect("existing identity file");

            // uuid's have 36 characters.
            // phone numbers don't, with a bit of luck.
            let addr_is_uuid = addr.name().len() == 36;

            // XXX Phone number possibly needs a + prefix or something like that.
            //     Maybe pull it through phonenumber for normalisation.
            let recipient = self.0.fetch_recipient(Some(addr.name()), Some(addr.name()));
            let recipient = if let Some(recipient) = recipient {
                recipient
            } else {
                // FIXME, we can create this recipient at this point
                log::warn!("No recipient for this identity; leaving alone.");
                continue;
            };

            let should_update = match (addr_is_uuid, recipient.identity.is_some()) {
                (true, true) => {
                    log::error!("Found an existing identity for {}, overwriting via UUID from file storage, sorry!", recipient.to_service_address());
                    true
                }
                (true, false) => {
                    log::debug!(
                        "Found no existing identity for {}",
                        recipient.to_service_address()
                    );
                    true
                }
                (false, true) => {
                    log::warn!("Found an existing identity for {}, not overwriting via E164 from file storage, sorry!", recipient.to_service_address());
                    false
                }
                (false, false) => {
                    log::warn!("Found no existing identity for {}, but E164-based identity in storage. Inserting.", recipient.to_service_address());
                    true
                }
            };
            if should_update {
                use crate::schema::recipients::dsl::*;
                use diesel::prelude::*;
                let db = self.0.db.lock();
                diesel::update(&recipient)
                    .set(identity.eq(buf.serialize().to_vec()))
                    .execute(&*db)
                    .expect("updating recipient into db");

                // By now, the identity is safely stored in the database, so we can remove the file.
                if let Err(e) = std::fs::remove_file(self.identity_path(&addr)) {
                    log::debug!(
                        "Could not delete identity {}, assuming non-existing: {}",
                        addr.to_string(),
                        e
                    );
                }
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
