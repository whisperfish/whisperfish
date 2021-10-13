use std::io;
use std::path::Path;

use libsignal_service::prelude::protocol::{self, Context};
use protocol::IdentityKeyPair;
use protocol::SignalProtocolError;

#[path = "./../../src/store/protocol_store/quirk.rs"]
mod quirk;

use super::*;

pub struct ProtocolStore {
    pub(crate) identity_key_pair: IdentityKeyPair,
    pub(crate) regid: u32,
}

fn convert_io_error(e: io::Error) -> SignalProtocolError {
    // XXX can probably be better, but currently this is only used in session_delete and
    // identity_delete
    SignalProtocolError::SessionNotFound(e.to_string())
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
    ) -> Result<Self, anyhow::Error> {
        // Identity
        let identity_path = path.join("storage").join("identity");

        // XXX move to quirk
        let mut identity_key = Vec::new();
        let public = identity_key_pair.public_key().serialize();
        assert_eq!(public.len(), 32 + 1);
        assert_eq!(public[0], quirk::DJB_TYPE);
        identity_key.extend(&public[1..]);

        let private = identity_key_pair.private_key().serialize();
        assert_eq!(private.len(), 32);
        identity_key.extend(private);

        write_file(
            keys,
            identity_path.join("regid"),
            format!("{}", regid).into_bytes(),
        )
        .await?;
        write_file(keys, identity_path.join("identity_key"), identity_key).await?;

        Ok(Self {
            identity_key_pair,
            regid,
        })
    }
}

impl Storage {
    fn session_path(&self, addr: &ProtocolAddress) -> PathBuf {
        let recipient_id = addr_to_path_component(addr.name());

        self.path.join("storage").join("sessions").join(format!(
            "{}_{}",
            recipient_id,
            addr.device_id()
        ))
    }

    fn identity_path(&self, addr: &ProtocolAddress) -> PathBuf {
        let recipient_id = addr_to_path_component(addr.name());

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
}

#[async_trait::async_trait(?Send)]
impl protocol::IdentityKeyStore for Storage {
    async fn get_identity_key_pair(
        &self,
        _: Context,
    ) -> Result<IdentityKeyPair, SignalProtocolError> {
        log::trace!("identity_key_pair");
        let protocol_store = self.protocol_store.read().await;
        Ok(protocol_store.identity_key_pair)
    }

    async fn get_local_registration_id(&self, _: Context) -> Result<u32, SignalProtocolError> {
        Ok(self.protocol_store.read().await.regid)
    }

    async fn is_trusted_identity(
        &self,
        addr: &ProtocolAddress,
        key: &IdentityKey,
        // XXX
        _direction: Direction,
        _ctx: Context,
    ) -> Result<bool, SignalProtocolError> {
        let _lock = self.protocol_store.read().await;

        if let Some(trusted_key) = self.read_identity_key_file(addr).await? {
            Ok(trusted_key == *key)
        } else {
            // Trust on first use
            Ok(true)
        }
    }

    /// Should return true when the older key, if present, is different from the new one.
    /// False otherwise.
    async fn save_identity(
        &mut self,
        addr: &ProtocolAddress,
        key: &IdentityKey,
        _: Context,
    ) -> Result<bool, SignalProtocolError> {
        let _lock = self.protocol_store.write().await;

        // Save return value
        let mut ret = false;

        // Get old key if present and compare to the new one. If they are the same, we set `ret` to
        // true.
        if let Some(key_old) = self.read_identity_key_file(addr).await? {
            ret = key_old == *key;
        };

        // Save new key only if ret is false. If it is `true` the old and the new key are identical
        // and saving is not necessary
        if !ret {
            // Write key
            let path = self.identity_path(addr);
            write_file(self.keys, path, key.serialize().into())
                .await
                .expect("save identity key");
        }

        Ok(ret)
    }

    async fn get_identity(
        &self,
        addr: &ProtocolAddress,
        _: Context,
    ) -> Result<Option<IdentityKey>, SignalProtocolError> {
        let _lock = self.protocol_store.read().await;

        self.read_identity_key_file(addr).await
    }
}

#[async_trait::async_trait(?Send)]
impl protocol::PreKeyStore for Storage {
    async fn get_pre_key(&self, id: u32, _: Context) -> Result<PreKeyRecord, SignalProtocolError> {
        log::trace!("Loading prekey {}", id);
        let _lock = self.protocol_store.read().await;

        let path = self.prekey_path(id);
        let contents = if let Ok(x) = load_file(self.keys, path).await {
            x
        } else {
            return Err(SignalProtocolError::InvalidPreKeyId);
        };
        let contents = quirk::pre_key_from_0_5(&contents).unwrap();
        Ok(PreKeyRecord::deserialize(&contents)?)
    }

    async fn save_pre_key(
        &mut self,
        id: u32,
        body: &PreKeyRecord,
        _: Context,
    ) -> Result<(), SignalProtocolError> {
        log::trace!("Storing prekey {}", id);
        let _lock = self.protocol_store.write().await;

        let path = self.prekey_path(id);
        let contents = quirk::pre_key_to_0_5(&body.serialize()?).unwrap();
        write_file(self.keys, path, contents)
            .await
            .expect("written file");
        Ok(())
    }

    async fn remove_pre_key(&mut self, id: u32, _: Context) -> Result<(), SignalProtocolError> {
        log::trace!("Removing prekey {}", id);
        let _lock = self.protocol_store.write().await;

        let path = self.prekey_path(id);
        std::fs::remove_file(path).unwrap();
        Ok(())
    }
}

impl Storage {
    // XXX Rewrite in terms of get_pre_key
    #[allow(dead_code)]
    async fn contains_pre_key(&self, id: u32) -> bool {
        log::trace!("Checking for prekey {}", id);
        let _lock = self.protocol_store.read().await;

        self.prekey_path(id).is_file()
    }
}

#[async_trait::async_trait(?Send)]
impl protocol::SessionStore for Storage {
    async fn load_session(
        &self,
        addr: &ProtocolAddress,
        _: Context,
    ) -> Result<Option<SessionRecord>, SignalProtocolError> {
        let path = self.session_path(addr);

        log::trace!("Loading session for {:?} from {:?}", addr, path);
        let _lock = self.protocol_store.read().await;

        let buf = if let Ok(buf) = load_file(self.keys, path).await {
            quirk::session_from_0_5(&buf)?
        } else {
            return Ok(None);
        };

        Ok(Some(SessionRecord::deserialize(&buf)?))
    }

    async fn store_session(
        &mut self,
        addr: &ProtocolAddress,
        session: &protocol::SessionRecord,
        _: Context,
    ) -> Result<(), SignalProtocolError> {
        let path = self.session_path(addr);

        log::trace!("Storing session for {:?} at {:?}", addr, path);
        let _lock = self.protocol_store.write().await;

        let quirked = quirk::session_to_0_5(&session.serialize()?)?;
        write_file(self.keys, path, quirked).await.unwrap();
        Ok(())
    }
}

impl Storage {
    #[allow(dead_code)]
    async fn contains_session(
        &self,
        addr: &ProtocolAddress,
        _: Context,
    ) -> Result<bool, SignalProtocolError> {
        let _lock = self.protocol_store.read().await;

        let path = self.session_path(addr);
        Ok(path.is_file())
    }
}

#[async_trait::async_trait(?Send)]
impl protocol::SessionStoreExt for Storage {
    async fn get_sub_device_sessions(&self, addr: &str) -> Result<Vec<u32>, SignalProtocolError> {
        log::trace!("Looking for sub_device sessions for {}", addr);
        let _lock = self.protocol_store.read().await;

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

    async fn delete_session(&self, addr: &ProtocolAddress) -> Result<(), SignalProtocolError> {
        let _lock = self.protocol_store.write().await;

        let path = self.session_path(addr);
        std::fs::remove_file(path).map_err(|e| {
            log::debug!(
                "Could not delete session {}, assuming non-existing: {}",
                addr.to_string(),
                e
            );
            SignalProtocolError::SessionNotFound(addr.to_string())
        })?;
        Ok(())
    }

    async fn delete_all_sessions(&self, addr: &str) -> Result<usize, SignalProtocolError> {
        log::warn!("Deleting all sessions for {}", addr);
        let _lock = self.protocol_store.write().await;

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
            std::fs::remove_file(entry).map_err(convert_io_error)?;
            count += 1;
        }

        Ok(count)
    }
}

#[async_trait::async_trait(?Send)]
impl protocol::SignedPreKeyStore for Storage {
    async fn get_signed_pre_key(
        &self,
        id: u32,
        _: Context,
    ) -> Result<SignedPreKeyRecord, SignalProtocolError> {
        log::trace!("Loading signed prekey {}", id);
        let _lock = self.protocol_store.read().await;

        let path = self.signed_prekey_path(id);

        let contents = if let Ok(x) = load_file(self.keys, path).await {
            x
        } else {
            return Err(SignalProtocolError::InvalidSignedPreKeyId);
        };
        let contents = quirk::signed_pre_key_from_0_5(&contents).unwrap();

        Ok(SignedPreKeyRecord::deserialize(&contents)?)
    }

    async fn save_signed_pre_key(
        &mut self,
        id: u32,
        body: &SignedPreKeyRecord,
        _: Context,
    ) -> Result<(), SignalProtocolError> {
        log::trace!("Storing prekey {}", id);
        let _lock = self.protocol_store.write().await;

        let path = self.signed_prekey_path(id);
        let contents = quirk::signed_pre_key_to_0_5(&body.serialize()?).unwrap();
        write_file(self.keys, path, contents)
            .await
            .expect("written file");
        Ok(())
    }
}

impl Storage {
    #[allow(dead_code)]
    async fn remove_signed_pre_key(&self, id: u32) -> Result<(), SignalProtocolError> {
        log::trace!("Removing signed prekey {}", id);
        let _lock = self.protocol_store.write().await;

        let path = self.signed_prekey_path(id);
        std::fs::remove_file(path).unwrap();
        Ok(())
    }

    // XXX rewrite in terms of get_signed_pre_key
    #[allow(dead_code)]
    async fn contains_signed_pre_key(&self, id: u32) -> bool {
        log::trace!("Checking for signed prekey {}", id);
        let _lock = self.protocol_store.read().await;

        self.signed_prekey_path(id).is_file()
    }

    async fn read_identity_key_file(
        &self,
        addr: &ProtocolAddress,
    ) -> Result<Option<IdentityKey>, SignalProtocolError> {
        let path = self.identity_path(addr);
        if path.is_file() {
            let buf = load_file(self.keys, path).await.expect("read identity key");
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
