use std::io::{self, Write};
use std::path::Path;

use libsignal_protocol::keys::IdentityKeyPair;
use libsignal_protocol::stores::SerializedSession;
use libsignal_protocol::stores::{IdentityKeyStore, PreKeyStore, SessionStore, SignedPreKeyStore};
use libsignal_protocol::Error as SignalProtocolError;
use libsignal_protocol::InternalError;
use libsignal_protocol::{Address, Buffer};

mod quirk;

use super::*;

pub struct ProtocolStore {
    pub(crate) identity_key: Vec<u8>,
    pub(crate) regid: u32,
}

fn addr_to_path_component<'a>(addr: &'a (impl AsRef<[u8]> + ?Sized + 'a)) -> &'a str {
    let addr: &'a [u8] = addr.as_ref();
    let addr = if addr[0] == b'+' { &addr[1..] } else { addr };
    std::str::from_utf8(addr).expect("address in valid UTF8")
}

impl ProtocolStore {
    pub async fn store_with_key(
        keys: Option<[u8; 16 + 20]>,
        path: &Path,
        regid: u32,
        identity_key_pair: IdentityKeyPair,
    ) -> Result<Self, failure::Error> {
        // Identity
        let identity_path = path.join("storage").join("identity");

        let mut identity_key = Vec::new();
        let public = identity_key_pair.public().to_bytes()?;
        let public = public.as_slice();
        assert_eq!(public.len(), 32 + 1);
        assert_eq!(public[0], quirk::DJB_TYPE);
        identity_key.extend(&public[1..]);

        let private = identity_key_pair.private().to_bytes()?;
        let private = private.as_slice();
        assert_eq!(private.len(), 32);
        identity_key.extend(private);

        write_file(
            keys,
            identity_path.join("regid"),
            format!("{}", regid).into_bytes(),
        )
        .await?;
        write_file(
            keys,
            identity_path.join("identity_key"),
            identity_key.clone(),
        )
        .await?;

        Ok(Self {
            identity_key,
            regid,
        })
    }

    pub async fn open_with_key(
        keys: Option<[u8; 16 + 20]>,
        path: &Path,
    ) -> Result<Self, failure::Error> {
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
    fn session_path(&self, addr: &Address) -> PathBuf {
        let addr_str = addr.as_str().unwrap();
        let recipient_id = addr_to_path_component(addr_str);

        self.path.join("storage").join("sessions").join(format!(
            "{}_{}",
            recipient_id,
            addr.device_id()
        ))
    }

    fn identity_path(&self, addr: &Address) -> PathBuf {
        let addr_str = addr.as_str().unwrap();
        let recipient_id = addr_to_path_component(addr_str);

        self.path
            .join("storage")
            .join("identity")
            .join(format!("remote_{}", recipient_id,))
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

    /// Returns a tuple of the next free signed pre-key ID and the next free pre-key ID
    pub fn next_pre_key_ids(&self) -> (u32, u32) {
        let mut pre_key_ids: Vec<u32> =
            std::fs::read_dir(self.path.join("storage").join("prekeys"))
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
                    let id = std::str::from_utf8(&name).ok()?;
                    id.parse().ok()
                })
                .collect();
        pre_key_ids.sort_unstable();

        let mut signed_pre_key_ids: Vec<u32> =
            std::fs::read_dir(self.path.join("storage").join("signed_prekeys"))
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
                    let id = std::str::from_utf8(&name).ok()?;
                    id.parse().ok()
                })
                .collect();
        signed_pre_key_ids.sort_unstable();

        let next_pre_key_id = if pre_key_ids.is_empty() {
            0
        } else {
            pre_key_ids[pre_key_ids.len() - 1] + 1
        };
        let next_signed_pre_key_id = if signed_pre_key_ids.is_empty() {
            0
        } else {
            signed_pre_key_ids[signed_pre_key_ids.len() - 1] + 1
        };
        (next_signed_pre_key_id, next_pre_key_id)
    }

    pub fn delete_identity(&self, addr: Address) -> Result<(), SignalProtocolError> {
        let path = self.identity_path(&addr);
        std::fs::remove_file(path).unwrap();
        Ok(())
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
        let path = self.identity_path(&addr);
        if !path.is_file() {
            // TOFU
            Ok(true)
        } else {
            // check contents with key
            let contents = load_file_sync(self.keys, path).expect("identity");
            Ok(contents == key)
        }
    }

    fn save_identity(&self, addr: Address, key: &[u8]) -> Result<(), SignalProtocolError> {
        let path = self.identity_path(&addr);
        write_file_sync(self.keys, path, key).expect("save identity key");
        Ok(())
    }

    fn get_identity(&self, addr: Address) -> Result<Option<Buffer>, SignalProtocolError> {
        let path = self.identity_path(&addr);
        if path.is_file() {
            let buf = load_file_sync(self.keys, path).expect("read identity key");
            Ok(Some(Buffer::from(buf)))
        } else {
            Ok(None)
        }
    }
}

impl PreKeyStore for Storage {
    fn load(&self, id: u32, writer: &mut dyn Write) -> Result<(), io::Error> {
        log::trace!("Loading prekey {}", id);
        let path = self.prekey_path(id);
        let contents = load_file_sync(self.keys, path).unwrap();
        let contents = quirk::pre_key_from_0_5(&contents).unwrap();
        writer.write_all(&contents)?;
        Ok(())
    }

    fn store(&self, id: u32, body: &[u8]) -> Result<(), SignalProtocolError> {
        log::trace!("Storing prekey {}", id);
        let path = self.prekey_path(id);
        let contents = quirk::pre_key_to_0_5(body).unwrap();
        write_file_sync(self.keys, path, &contents).expect("written file");
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
        let path = self.session_path(&addr);

        log::trace!("Loading session for {:?} from {:?}", addr, path);

        let buf = if let Ok(buf) = load_file_sync(self.keys, path) {
            buf
        } else {
            return Ok(None);
        };

        Ok(Some(SerializedSession {
            session: Buffer::from(&quirk::session_from_0_5(&buf)? as &[u8]),
            extra_data: None,
        }))
    }

    fn get_sub_device_sessions(&self, addr: &[u8]) -> Result<Vec<i32>, InternalError> {
        log::trace!(
            "Looking for sub_device sessions for {}",
            String::from_utf8_lossy(addr)
        );
        let addr = addr_to_path_component(addr).as_bytes();

        let session_dir = self.path.join("storage").join("sessions");

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

                if name.len() < addr.len() + 2 {
                    return None;
                }

                if &name[..addr.len()] == addr {
                    if name[addr.len()] != b'_' {
                        log::warn!("Weird session directory entry: {:?}. Skipping", entry);
                        return None;
                    }
                    // skip underscore
                    let id = std::str::from_utf8(&name[(addr.len() + 1)..]).ok()?;
                    id.parse().ok()
                } else {
                    None
                }
            })
            .filter(|id| *id != libsignal_service::push_service::DEFAULT_DEVICE_ID)
            .collect();

        Ok(ids)
    }

    fn contains_session(&self, addr: Address) -> Result<bool, SignalProtocolError> {
        let path = self.session_path(&addr);
        Ok(path.is_file())
    }

    fn store_session(
        &self,
        addr: Address,
        session: libsignal_protocol::stores::SerializedSession,
    ) -> Result<(), InternalError> {
        let path = self.session_path(&addr);

        log::trace!("Storing session for {:?} at {:?}", addr, path);

        let quirked = quirk::session_to_0_5(&session.session.as_slice())?;
        write_file_sync(self.keys, path, &quirked).unwrap();
        Ok(())
    }

    fn delete_session(&self, addr: Address) -> Result<(), SignalProtocolError> {
        let path = self.session_path(&addr);
        std::fs::remove_file(path).unwrap();
        Ok(())
    }

    fn delete_all_sessions(&self, addr: &[u8]) -> Result<usize, SignalProtocolError> {
        log::warn!(
            "Deleting all sessions for {}",
            String::from_utf8_lossy(addr)
        );
        let addr = addr_to_path_component(addr).as_bytes();

        let session_dir = self.path.join("storage").join("sessions");

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
                    if name[addr.len()] != b'_' {
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

        let contents = load_file_sync(self.keys, path).unwrap();
        let contents = quirk::signed_pre_key_from_0_5(&contents).unwrap();

        writer.write_all(&contents)?;
        Ok(())
    }

    fn store(&self, id: u32, body: &[u8]) -> Result<(), SignalProtocolError> {
        log::trace!("Storing prekey {}", id);
        let path = self.signed_prekey_path(id);
        let contents = quirk::signed_pre_key_to_0_5(body).unwrap();
        write_file_sync(self.keys, path, &contents).expect("written file");
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
