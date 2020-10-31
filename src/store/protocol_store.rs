use std::io::{self, Write};
use std::path::Path;

use libsignal_protocol::stores::SerializedSession;
use libsignal_protocol::stores::{IdentityKeyStore, PreKeyStore, SessionStore, SignedPreKeyStore};
use libsignal_protocol::Error as SignalProtocolError;
use libsignal_protocol::InternalError;
use libsignal_protocol::{Address, Buffer};

mod quirk;

use super::*;

pub struct ProtocolStore {
    identity_key: Vec<u8>,
    regid: u32,
}

impl ProtocolStore {
    // This will be here until https://gitlab.com/rubdos/whisperfish/-/issues/40 is resolved,
    // for purposes of tests.
    #[doc(hidden)]
    pub fn invalid() -> Self {
        Self {
            identity_key: vec![],
            regid: 0,
        }
    }

    pub async fn open_with_key(keys: [u8; 16 + 20], path: &Path) -> Result<Self, failure::Error> {
        // Identity
        let identity_path = path.join("storage").join("identity");

        let regid = load_file(keys, identity_path.join("regid")).await?;
        let regid = String::from_utf8(regid)?;
        let regid = regid.parse()?;
        let identity_key = load_file(keys, identity_path.join("identity_key")).await?;

        Ok(Self {
            identity_key,
            regid,
        })
    }
}

impl Storage {
    // XXX: this is made to be Go-compatible: only accept addr's that start with + (phone number).
    // Signal is moving away from this.  Uuid-based paths will work perfectly, but will *not* be
    // backwards compatible with 0.5.
    fn session_path(&self, addr: &Address) -> Option<PathBuf> {
        let addr_str = addr.as_str().unwrap();
        let recipient_id = if addr_str.starts_with('+') {
            // strip the prefix + from e164, as is done in Go (cfr. the `func recID`).
            &addr_str[1..]
        } else {
            return None;
            // addr_str
        };

        Some(self.path.join("storage").join("sessions").join(format!(
            "{}_{}",
            recipient_id,
            addr.device_id()
        )))
    }

    fn identity_path(&self, addr: &Address) -> Option<PathBuf> {
        let addr_str = addr.as_str().unwrap();
        let recipient_id = if addr_str.starts_with('+') {
            // strip the prefix + from e164, as is done in Go (cfr. the `func recID`).
            &addr_str[1..]
        } else {
            return None;
            // addr_str
        };

        Some(
            self.path
                .join("storage")
                .join("identity")
                .join(format!("remote_{}", recipient_id,)),
        )
    }

    fn prekey_path(&self, id: u32) -> PathBuf {
        self.path
            .join("storage")
            .join("prekeys")
            .join(format!("{:09}", id))
    }

    fn signed_prekey_path(&self, id: u32) -> PathBuf {
        self.path
            .join("storage")
            .join("signed_prekeys")
            .join(format!("{:09}", id))
    }
}

impl IdentityKeyStore for Storage {
    fn identity_key_pair(&self) -> Result<(Buffer, Buffer), SignalProtocolError> {
        log::trace!("identity_key_pair");
        let protocol_store = self.protocol_store.lock().expect("mutex");
        // (public, private)
        let mut public = Buffer::new();
        public.append(&[quirk::DJB_TYPE]);
        public.append(&protocol_store.identity_key[..32]);
        Ok((public, Buffer::from(&protocol_store.identity_key[32..])))
    }

    fn local_registration_id(&self) -> Result<u32, SignalProtocolError> {
        Ok(self.protocol_store.lock().expect("mutex").regid)
    }

    fn is_trusted_identity(&self, addr: Address, key: &[u8]) -> Result<bool, SignalProtocolError> {
        if let Some(path) = self.identity_path(&addr) {
            if !path.is_file() {
                // TOFU
                Ok(true)
            } else {
                // check contents with key
                let contents = load_file_sync(self.keys.unwrap(), path).expect("identity");
                Ok(contents == key)
            }
        } else {
            log::warn!("Trying trusted identity with uuid, currently unsupported.");
            Err(InternalError::InvalidArgument)?
        }
    }

    fn save_identity(&self, addr: Address, key: &[u8]) -> Result<(), SignalProtocolError> {
        if let Some(path) = self.identity_path(&addr) {
            write_file_sync(self.keys.unwrap(), path, key).expect("save identity key");
            Ok(())
        } else {
            log::warn!("Trying to save trusted identity with uuid, currently unsupported.");
            Err(InternalError::InvalidArgument)?
        }
    }
}

impl PreKeyStore for Storage {
    fn load(&self, id: u32, writer: &mut dyn Write) -> Result<(), io::Error> {
        log::trace!("Loading prekey {}", id);
        let path = self.prekey_path(id);
        let contents = load_file_sync(self.keys.unwrap(), path).unwrap();
        let contents = quirk::pre_key_from_0_5(&contents).unwrap();
        writer.write(&contents)?;
        Ok(())
    }

    fn store(&self, id: u32, body: &[u8]) -> Result<(), SignalProtocolError> {
        log::trace!("Storing prekey {}", id);
        let path = self.prekey_path(id);
        let contents = quirk::pre_key_to_0_5(body).unwrap();
        write_file_sync(self.keys.unwrap(), path, &contents).expect("written file");
        Ok(())
    }

    fn contains(&self, id: u32) -> bool {
        log::trace!("Checking for prekey {}", id);
        self.prekey_path(id).is_file()
    }

    fn remove(&self, id: u32) -> Result<(), SignalProtocolError> {
        log::trace!("Removing prekey {}", id);
        let path = self.prekey_path(id);
        std::fs::remove_file(path).unwrap();
        Ok(())
    }
}

impl SessionStore for Storage {
    fn load_session(
        &self,
        addr: Address,
    ) -> Result<Option<SerializedSession>, SignalProtocolError> {
        let path = if let Some(path) = self.session_path(&addr) {
            path
        } else {
            return Ok(None);
        };

        log::trace!("Loading session for {:?} from {:?}", addr, path);

        let buf = if let Ok(buf) = load_file_sync(self.keys.unwrap(), path) {
            buf
        } else {
            return Ok(None);
        };

        Ok(Some(SerializedSession {
            session: Buffer::from(&quirk::session_from_0_5(&buf)? as &[u8]),
            extra_data: None,
        }))
    }

    fn get_sub_device_sessions(&self, addr: &[u8]) -> Result<std::vec::Vec<i32>, InternalError> {
        log::trace!(
            "Looking for sub_device sessions for {}",
            String::from_utf8_lossy(addr)
        );
        // Strip `+'
        let addr = &addr[1..];

        let session_dir = crate::store::default_location()
            .unwrap()
            .join("storage")
            .join("sessions");

        let ids = std::fs::read_dir(session_dir)
            .expect("initialized storage")
            .filter_map(|entry| {
                let entry = entry.expect("directory listing");
                if !entry.path().is_file() {
                    log::warn!("Non-file session entry: {:?}. Skipping", entry);
                    return None;
                }

                // XXX: *maybe* Signal could become a cross-platform desktop app.
                use std::os::unix::ffi::OsStrExt;
                let name = entry.file_name();
                let name = name.as_os_str().as_bytes();

                log::trace!("parsing {:?}", entry);

                if name.len() < addr.len() + 2 {
                    log::trace!("filename {:?} not long enough", entry);
                    return None;
                }

                if &name[..addr.len()] == addr {
                    if name[addr.len()] != '_' as u8 {
                        log::warn!("Weird session directory entry: {:?}. Skipping", entry);
                        return None;
                    }
                    // skip underscore
                    let id = std::str::from_utf8(&name[(addr.len() + 1)..]).ok()?;
                    id.parse().ok()
                } else {
                    log::trace!("filename {:?} without prefix match", entry);
                    None
                }
            })
            .filter(|id| *id != libsignal_service::push_service::DEFAULT_DEVICE_ID)
            .collect();

        Ok(ids)
    }

    fn contains_session(&self, addr: Address) -> Result<bool, SignalProtocolError> {
        let path = self.session_path(&addr);
        if let Some(path) = path {
            Ok(path.is_file())
        } else {
            Ok(false)
        }
    }

    fn store_session(
        &self,
        addr: Address,
        session: libsignal_protocol::stores::SerializedSession,
    ) -> Result<(), InternalError> {
        let path = self.session_path(&addr).expect("path for session FIXME");

        log::trace!("Storing session for {:?} at {:?}", addr, path);

        let quirked = quirk::session_to_0_5(&session.session.as_slice())?;
        write_file_sync(self.keys.unwrap(), path, &quirked).unwrap();
        Ok(())
    }

    fn delete_session(&self, addr: Address) -> Result<(), SignalProtocolError> {
        let path = self.session_path(&addr).expect("path for session deletion");
        std::fs::remove_file(path).unwrap();
        Ok(())
    }

    fn delete_all_sessions(&self, addr: &[u8]) -> Result<usize, SignalProtocolError> {
        log::warn!(
            "Deleting all sessions for {}",
            String::from_utf8_lossy(addr)
        );
        // Strip `+'
        assert_eq!(addr[0], b'+', "expecting session with phone number");
        let addr = &addr[1..];

        let session_dir = crate::store::default_location()
            .unwrap()
            .join("storage")
            .join("sessions");

        let entries = std::fs::read_dir(session_dir)
            .expect("initialized storage")
            .filter_map(|entry| {
                let entry = entry.expect("directory listing");
                if !entry.path().is_file() {
                    log::warn!("Non-file session entry: {:?}. Skipping", entry);
                    return None;
                }

                // XXX: *maybe* Signal could become a cross-platform desktop app.
                use std::os::unix::ffi::OsStrExt;
                let name = entry.file_name();
                let name = name.as_os_str().as_bytes();

                log::trace!("parsing {:?}", entry);

                if name.len() < addr.len() + 2 {
                    log::trace!("filename {:?} not long enough", entry);
                    return None;
                }

                if &name[..addr.len()] == addr {
                    if name[addr.len()] != '_' as u8 {
                        log::warn!("Weird session directory entry: {:?}. Skipping", entry);
                        return None;
                    }
                    // skip underscore
                    let id = std::str::from_utf8(&name[(addr.len() + 1)..]).ok()?;
                    let _: u32 = id.parse().ok()?;
                    Some(entry.path())
                } else {
                    log::trace!("filename {:?} without prefix match", entry);
                    None
                }
            });

        let mut count = 0;
        for entry in entries {
            std::fs::remove_file(entry)?;
            count += 1;
        }

        Ok(count)
    }
}

impl SignedPreKeyStore for Storage {
    fn load(&self, id: u32, writer: &mut dyn Write) -> Result<(), io::Error> {
        log::trace!("Loading signed prekey {}", id);
        let path = self.signed_prekey_path(id);

        let contents = load_file_sync(self.keys.unwrap(), path).unwrap();
        let contents = quirk::signed_pre_key_from_0_5(&contents).unwrap();

        writer.write(&contents)?;
        Ok(())
    }

    fn store(&self, id: u32, body: &[u8]) -> Result<(), SignalProtocolError> {
        log::trace!("Storing prekey {}", id);
        let path = self.prekey_path(id);
        let contents = quirk::signed_pre_key_to_0_5(body).unwrap();
        write_file_sync(self.keys.unwrap(), path, &contents).expect("written file");
        Ok(())
    }

    fn contains(&self, id: u32) -> bool {
        log::trace!("Checking for signed prekey {}", id);
        self.signed_prekey_path(id).is_file()
    }

    fn remove(&self, id: u32) -> Result<(), SignalProtocolError> {
        log::trace!("Removing signed prekey {}", id);
        let path = self.signed_prekey_path(id);
        std::fs::remove_file(path).unwrap();
        Ok(())
    }
}
