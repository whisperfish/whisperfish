use super::*;
use libsignal_service::pre_keys::{PreKeysStore, ServiceKyberPreKeyStore};
use libsignal_service::protocol::{
    self, GenericSignedPreKey, IdentityKeyPair, SignalProtocolError,
};
use libsignal_service::push_service::ServiceIdType;
use libsignal_service::session_store::SessionStoreExt;
use std::path::Path;

pub struct ProtocolStore;

pub const DJB_TYPE: u8 = 0x05;

impl ProtocolStore {
    pub fn serialize_identity_key(identity_key_pair: IdentityKeyPair) -> Vec<u8> {
        // XXX move to quirk
        let mut identity_key = Vec::new();
        let public = identity_key_pair.public_key().serialize();
        assert_eq!(public.len(), 32 + 1);
        assert_eq!(public[0], DJB_TYPE);
        identity_key.extend(&public[1..]);

        let private = identity_key_pair.private_key().serialize();
        assert_eq!(private.len(), 32);
        identity_key.extend(private);

        identity_key
    }

    pub async fn new(
        store_enc: Option<&encryption::StorageEncryption>,
        path: &Path,
        regid: u32,
        pni_regid: u32,
        aci_identity_key_pair: IdentityKeyPair,
        pni_identity_key_pair: IdentityKeyPair,
    ) -> Result<Self, anyhow::Error> {
        // Identity
        let identity_path = path.join("storage").join("identity");

        let aci_identity_key = Self::serialize_identity_key(aci_identity_key_pair);
        let pni_identity_key = Self::serialize_identity_key(pni_identity_key_pair);

        // Encrypt regid if necessary and write to file
        utils::write_file_async_encrypted(
            identity_path.join("regid"),
            format!("{}", regid).into_bytes(),
            store_enc,
        )
        .await?;
        // Encrypt pni regid if necessary and write to file
        utils::write_file_async_encrypted(
            identity_path.join("pni_regid"),
            format!("{}", pni_regid).into_bytes(),
            store_enc,
        )
        .await?;

        // Encrypt identity key if necessary and write to file
        utils::write_file_async_encrypted(
            identity_path.join("identity_key"),
            aci_identity_key,
            store_enc,
        )
        .await?;

        // Encrypt PNI if necessary and write to file
        utils::write_file_async_encrypted(
            identity_path.join("pni_identity_key"),
            pni_identity_key,
            store_enc,
        )
        .await?;

        Ok(Self)
    }

    pub async fn open() -> Self {
        Self
    }
}

impl Storage {
    pub async fn delete_identity(&self, addr: &ProtocolAddress) -> Result<(), SignalProtocolError> {
        self.delete_identity_key(addr);
        Ok(())
    }
}

impl Storage {
    pub fn pni_storage(&self) -> PniStorage {
        PniStorage::new(self.clone())
    }

    pub fn aci_storage(&self) -> AciStorage {
        AciStorage::new(self.clone())
    }

    pub fn aci_or_pni(&self, service_id: ServiceIdType) -> AciOrPniStorage {
        IdentityStorage(self.clone(), AciOrPni(service_id))
    }
}

#[derive(Clone)]
pub struct IdentityStorage<T>(Storage, T);
impl<T: Default> IdentityStorage<T> {
    pub fn new(storage: Storage) -> Self {
        Self(storage, Default::default())
    }
}
#[derive(Default, Clone)]
pub struct Aci;
pub type AciStorage = IdentityStorage<Aci>;
#[derive(Default, Clone)]
pub struct Pni;
pub type PniStorage = IdentityStorage<Pni>;
// Dynamic dispatch between Aci and Pni
#[derive(Clone)]
pub struct AciOrPni(ServiceIdType);
pub type AciOrPniStorage = IdentityStorage<AciOrPni>;
pub trait Identity {
    fn identity(&self) -> orm::Identity;
    fn identity_key_filename(&self) -> &'static str;
    fn regid_filename(&self) -> &'static str;
}
impl Identity for Aci {
    fn identity(&self) -> orm::Identity {
        orm::Identity::Aci
    }
    fn identity_key_filename(&self) -> &'static str {
        "identity_key"
    }
    fn regid_filename(&self) -> &'static str {
        "regid"
    }
}
impl Identity for Pni {
    fn identity(&self) -> orm::Identity {
        orm::Identity::Pni
    }
    fn identity_key_filename(&self) -> &'static str {
        "pni_identity_key"
    }
    fn regid_filename(&self) -> &'static str {
        "pni_regid"
    }
}
impl Identity for AciOrPni {
    fn identity(&self) -> orm::Identity {
        match self.0 {
            ServiceIdType::AccountIdentity => orm::Identity::Aci,
            ServiceIdType::PhoneNumberIdentity => orm::Identity::Pni,
        }
    }
    fn identity_key_filename(&self) -> &'static str {
        match self.0 {
            ServiceIdType::AccountIdentity => "identity_key",
            ServiceIdType::PhoneNumberIdentity => "pni_identity_key",
        }
    }
    fn regid_filename(&self) -> &'static str {
        match self.0 {
            ServiceIdType::AccountIdentity => "regid",
            ServiceIdType::PhoneNumberIdentity => "pni_regid",
        }
    }
}

#[async_trait::async_trait(?Send)]
impl protocol::ProtocolStore for IdentityStorage<AciOrPni> {}
#[async_trait::async_trait(?Send)]
impl protocol::ProtocolStore for IdentityStorage<Aci> {}
#[async_trait::async_trait(?Send)]
impl protocol::ProtocolStore for IdentityStorage<Pni> {}

impl<T: Identity> IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self, regid))]
    // Mutability of self is artificial
    pub async fn write_local_registration_id(
        &mut self,
        regid: u32,
    ) -> Result<(), SignalProtocolError> {
        let _lock = self.0.protocol_store.write().await;

        let path = self
            .0
            .path
            .join("storage")
            .join("identity")
            .join(self.1.regid_filename());
        self.0
            .write_file(path, format!("{}", regid).into_bytes())
            .await
            .map_err(|e| {
                SignalProtocolError::InvalidArgument(format!("Cannot write regid {}", e))
            })?;

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self, key_pair))]
    // Mutability of self is artificial
    pub async fn write_identity_key_pair(
        &mut self,
        key_pair: IdentityKeyPair,
    ) -> Result<(), SignalProtocolError> {
        let _lock = self.0.protocol_store.write().await;

        let path = self
            .0
            .path
            .join("storage")
            .join("identity")
            .join(self.1.identity_key_filename());
        tracing::trace!("writing own identity key pair at {}", path.display());
        self.0
            .write_file(path, ProtocolStore::serialize_identity_key(key_pair))
            .await
            .map_err(|e| {
                SignalProtocolError::InvalidArgument(format!("Cannot write own identity key {}", e))
            })?;
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> protocol::IdentityKeyStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_identity_key_pair(&self) -> Result<IdentityKeyPair, SignalProtocolError> {
        let _lock = self.0.protocol_store.read().await;

        let path = self
            .0
            .path
            .join("storage")
            .join("identity")
            .join(self.1.identity_key_filename());
        tracing::trace!("reading own identity key pair at {}", path.display());
        let key_pair = {
            use std::convert::TryFrom;
            let mut buf = self.0.read_file(path).await.map_err(|e| {
                SignalProtocolError::InvalidArgument(format!("Cannot read own identity key {}", e))
            })?;
            buf.insert(0, DJB_TYPE);
            let public = IdentityKey::decode(&buf[0..33])?;
            let private = PrivateKey::try_from(&buf[33..])?;
            IdentityKeyPair::new(public, private)
        };
        Ok(key_pair)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_local_registration_id(&self) -> Result<u32, SignalProtocolError> {
        let _lock = self.0.protocol_store.read().await;

        let path = self
            .0
            .path
            .join("storage")
            .join("identity")
            .join(self.1.regid_filename());
        let regid = self.0.read_file(path).await.map_err(|e| {
            SignalProtocolError::InvalidArgument(format!("Cannot read regid {}", e))
        })?;
        let regid = String::from_utf8(regid).map_err(|e| {
            SignalProtocolError::InvalidArgument(format!(
                "Convert regid from bytes to string {}",
                e
            ))
        })?;
        let regid = regid.parse().map_err(|e| {
            SignalProtocolError::InvalidArgument(format!(
                "Convert regid from string to number {}",
                e
            ))
        })?;

        Ok(regid)
    }

    #[tracing::instrument(level = "trace", skip(self, addr), fields(addr = %addr))]
    async fn is_trusted_identity(
        &self,
        addr: &ProtocolAddress,
        key: &IdentityKey,
        // XXX
        _direction: Direction,
    ) -> Result<bool, SignalProtocolError> {
        if let Some(trusted_key) = self.get_identity(addr).await? {
            Ok(trusted_key == *key)
        } else {
            // Trust on first use
            Ok(true)
        }
    }

    /// Should return true when the older key, if present, is different from the new one.
    /// False otherwise.
    #[tracing::instrument(level = "trace", skip(self, addr), fields(addr = %addr))]
    async fn save_identity(
        &mut self,
        addr: &ProtocolAddress,
        key: &IdentityKey,
    ) -> Result<bool, SignalProtocolError> {
        use crate::schema::identity_records::dsl::*;
        let previous = self.get_identity(addr).await?;

        let ret = previous.as_ref() == Some(key);

        if previous.is_some() {
            diesel::update(identity_records)
                .filter(address.eq(addr.name()).and(identity.eq(self.1.identity())))
                .set(record.eq(key.serialize().to_vec()))
                .execute(&mut *self.0.db())
                .expect("db");
        } else {
            diesel::insert_into(identity_records)
                .values((
                    address.eq(addr.name()),
                    record.eq(key.serialize().to_vec()),
                    identity.eq(self.1.identity()),
                ))
                .execute(&mut *self.0.db())
                .expect("db");
        }

        Ok(ret)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_identity(
        &self,
        addr: &ProtocolAddress,
    ) -> Result<Option<IdentityKey>, SignalProtocolError> {
        use crate::schema::identity_records::dsl::*;
        let addr = addr.name();
        Ok(identity_records
            .filter(address.eq(addr).and(identity.eq(self.1.identity())))
            .first(&mut *self.0.db())
            .optional()
            .expect("db")
            .map(|found: orm::IdentityRecord| {
                IdentityKey::decode(&found.record).expect("only valid identity keys in db")
            }))
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> protocol::SessionStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn load_session(
        &self,
        addr: &ProtocolAddress,
    ) -> Result<Option<SessionRecord>, SignalProtocolError> {
        use crate::schema::session_records::dsl::*;
        use diesel::prelude::*;

        let session_record: Option<orm::SessionRecord> = session_records
            .filter(
                address
                    .eq(addr.name())
                    .and(device_id.eq(u32::from(addr.device_id()) as i32))
                    .and(identity.eq(self.1.identity())),
            )
            .first(&mut *self.0.db())
            .optional()
            .expect("db");
        if let Some(session_record) = session_record {
            Ok(Some(SessionRecord::deserialize(&session_record.record)?))
        } else {
            Ok(None)
        }
    }

    #[tracing::instrument(level = "trace", skip(self, session), fields(addr = %addr))]
    async fn store_session(
        &mut self,
        addr: &ProtocolAddress,
        session: &protocol::SessionRecord,
    ) -> Result<(), SignalProtocolError> {
        use crate::schema::session_records::dsl::*;
        use diesel::prelude::*;

        if self.contains_session(addr).await? {
            diesel::update(session_records)
                .filter(
                    address
                        .eq(addr.name())
                        .and(device_id.eq(u32::from(addr.device_id()) as i32))
                        .and(identity.eq(self.1.identity())),
                )
                .set(record.eq(session.serialize()?))
                .execute(&mut *self.0.db())
                .expect("updated session");
        } else {
            diesel::insert_into(session_records)
                .values((
                    address.eq(addr.name()),
                    device_id.eq(u32::from(addr.device_id()) as i32),
                    record.eq(session.serialize()?),
                    identity.eq(self.1.identity()),
                ))
                .execute(&mut *self.0.db())
                .expect("updated session");
        }

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> ServiceKyberPreKeyStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self, body))]
    async fn store_last_resort_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        body: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        use crate::schema::kyber_prekeys::dsl::*;
        use diesel::prelude::*;

        // Insert or replace?
        diesel::insert_into(kyber_prekeys)
            .values(orm::KyberPrekey {
                id: u32::from(kyber_prekey_id) as _,
                record: body.serialize()?,
                identity: self.1.identity(),
                is_last_resort: true,
            })
            .execute(&mut *self.0.db())
            .expect("db");

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn load_last_resort_kyber_pre_keys(
        &self,
    ) -> Result<Vec<KyberPreKeyRecord>, SignalProtocolError> {
        use crate::schema::kyber_prekeys::dsl::*;
        use diesel::prelude::*;

        // XXX Do we need to ensure these are marked as unused?
        let prekey_records: Vec<orm::KyberPrekey> = kyber_prekeys
            .filter(is_last_resort.eq(true).and(identity.eq(self.1.identity())))
            .load(&mut *self.0.db())
            .expect("db");

        Ok(prekey_records
            .into_iter()
            .map(|pkr| KyberPreKeyRecord::deserialize(&pkr.record))
            .collect::<Result<_, _>>()?)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn remove_kyber_pre_key(
        &mut self,
        _kyber_prekey_id: KyberPreKeyId,
    ) -> Result<(), SignalProtocolError> {
        // Mark as used should be used instead
        unimplemented!("unexpected in this flow")
    }

    /// Analogous to markAllOneTimeKyberPreKeysStaleIfNecessary
    #[tracing::instrument(level = "trace", skip(self))]
    async fn mark_all_one_time_kyber_pre_keys_stale_if_necessary(
        &mut self,
        _stale_time: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), SignalProtocolError> {
        unimplemented!("unexpected in this flow")
    }

    /// Analogue of deleteAllStaleOneTimeKyberPreKeys
    #[tracing::instrument(level = "trace", skip(self))]
    async fn delete_all_stale_one_time_kyber_pre_keys(
        &mut self,
        _threshold: chrono::DateTime<chrono::Utc>,
        _min_count: usize,
    ) -> Result<(), SignalProtocolError> {
        unimplemented!("unexpected in this flow")
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> PreKeysStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn next_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        use diesel::dsl::*;
        use diesel::prelude::*;

        let prekey_max: Option<i32> = {
            use crate::schema::prekeys::dsl::*;

            prekeys
                .select(max(id))
                // Don't filter by identity, as we want to know the max for all identities
                .first(&mut *self.0.db())
                .expect("db")
        };
        Ok((prekey_max.unwrap_or(-1) + 1) as u32)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn next_signed_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        use diesel::dsl::*;
        use diesel::prelude::*;

        let signed_prekey_max: Option<i32> = {
            use crate::schema::signed_prekeys::dsl::*;

            signed_prekeys
                .select(max(id))
                // Don't filter by identity, as we want to know the max for all identities
                .first(&mut *self.0.db())
                .expect("db")
        };
        Ok((signed_prekey_max.unwrap_or(-1) + 1) as u32)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn next_pq_pre_key_id(&self) -> Result<u32, SignalProtocolError> {
        use diesel::dsl::*;
        use diesel::prelude::*;

        let kyber_max: Option<i32> = {
            use crate::schema::kyber_prekeys::dsl::*;

            kyber_prekeys
                .select(max(id))
                // Don't filter by identity, as we want to know the max for all identities
                .first(&mut *self.0.db())
                .expect("db")
        };
        Ok((kyber_max.unwrap_or(-1) + 1) as u32)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn set_next_pre_key_id(&mut self, id: u32) -> Result<(), SignalProtocolError> {
        assert_eq!(self.next_pre_key_id().await?, id);
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn set_next_signed_pre_key_id(&mut self, id: u32) -> Result<(), SignalProtocolError> {
        assert_eq!(self.next_signed_pre_key_id().await?, id);
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn set_next_pq_pre_key_id(&mut self, id: u32) -> Result<(), SignalProtocolError> {
        assert_eq!(self.next_pq_pre_key_id().await?, id);
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> protocol::PreKeyStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_pre_key(&self, prekey_id: PreKeyId) -> Result<PreKeyRecord, SignalProtocolError> {
        use crate::schema::prekeys::dsl::*;
        use diesel::prelude::*;

        let prekey_record: Option<orm::Prekey> = prekeys
            .filter(
                id.eq(u32::from(prekey_id) as i32)
                    .and(identity.eq(self.1.identity())),
            )
            .first(&mut *self.0.db())
            .optional()
            .expect("db");
        if let Some(pkr) = prekey_record {
            Ok(PreKeyRecord::deserialize(&pkr.record)?)
        } else {
            Err(SignalProtocolError::InvalidPreKeyId)
        }
    }

    #[tracing::instrument(level = "trace", skip(self, body))]
    async fn save_pre_key(
        &mut self,
        prekey_id: PreKeyId,
        body: &PreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        use crate::schema::prekeys::dsl::*;
        use diesel::prelude::*;

        diesel::insert_into(prekeys)
            .values(orm::Prekey {
                id: u32::from(prekey_id) as _,
                record: body.serialize()?,
                identity: self.1.identity(),
            })
            .execute(&mut *self.0.db())
            .expect("db");

        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn remove_pre_key(&mut self, prekey_id: PreKeyId) -> Result<(), SignalProtocolError> {
        use crate::schema::prekeys::dsl::*;
        use diesel::prelude::*;

        diesel::delete(prekeys)
            .filter(
                id.eq(u32::from(prekey_id) as i32)
                    .and(identity.eq(self.1.identity())),
            )
            .execute(&mut *self.0.db())
            .expect("db");
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> protocol::KyberPreKeyStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn mark_kyber_pre_key_used(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<(), SignalProtocolError> {
        use crate::schema::kyber_prekeys::dsl::*;
        use diesel::prelude::*;

        diesel::delete(kyber_prekeys)
            .filter(
                id.eq((u32::from(kyber_prekey_id)) as i32)
                    .and(identity.eq(self.1.identity()))
                    .and(is_last_resort.eq(false)),
            )
            .execute(&mut *self.0.db())
            .expect("db");
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_kyber_pre_key(
        &self,
        kyber_prekey_id: KyberPreKeyId,
    ) -> Result<KyberPreKeyRecord, SignalProtocolError> {
        use crate::schema::kyber_prekeys::dsl::*;
        use diesel::prelude::*;

        // XXX Do we need to ensure this is *not* a last resort key?
        let prekey_record: Option<orm::KyberPrekey> = kyber_prekeys
            .filter(
                id.eq(u32::from(kyber_prekey_id) as i32)
                    .and(identity.eq(self.1.identity())),
            )
            .first(&mut *self.0.db())
            .optional()
            .expect("db");
        if let Some(pkr) = prekey_record {
            Ok(KyberPreKeyRecord::deserialize(&pkr.record)?)
        } else {
            Err(SignalProtocolError::InvalidKyberPreKeyId)
        }
    }

    #[tracing::instrument(level = "trace", skip(self, body))]
    async fn save_kyber_pre_key(
        &mut self,
        kyber_prekey_id: KyberPreKeyId,
        body: &KyberPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        use crate::schema::kyber_prekeys::dsl::*;
        use diesel::prelude::*;

        // Insert or replace?
        diesel::insert_into(kyber_prekeys)
            .values(orm::KyberPrekey {
                id: u32::from(kyber_prekey_id) as _,
                record: body.serialize()?,
                identity: self.1.identity(),
                is_last_resort: false,
            })
            .execute(&mut *self.0.db())
            .expect("db");

        Ok(())
    }
}

impl<T: Identity> IdentityStorage<T> {
    /// Check whether session exists.
    ///
    /// This does *not* lock the protocol store.  If a transactional check is required, use the
    /// lock from outside.
    #[tracing::instrument(level = "trace", skip(self))]
    async fn contains_session(&self, addr: &ProtocolAddress) -> Result<bool, SignalProtocolError> {
        use crate::schema::session_records::dsl::*;
        use diesel::dsl::*;
        use diesel::prelude::*;

        let count: i64 = session_records
            .select(count_star())
            .filter(
                address
                    .eq(addr.name())
                    .and(device_id.eq(u32::from(addr.device_id()) as i32))
                    .and(identity.eq(self.1.identity())),
            )
            .first(&mut *self.0.db())
            .expect("db");
        Ok(count != 0)
    }
}

// BEGIN identity key block
impl Storage {
    /// Removes the identity matching `addr` from the database, independent of PNI or ACI.
    ///
    /// Does not lock the protocol storage.
    #[tracing::instrument(level = "warn", skip(self))]
    pub fn delete_identity_key(&self, addr: &ProtocolAddress) -> bool {
        use crate::schema::identity_records::dsl::*;
        let addr = addr.name();
        let amount = diesel::delete(identity_records)
            .filter(address.eq(addr))
            .execute(&mut *self.db())
            .expect("db");

        amount == 1
    }
}
// END identity key

#[async_trait::async_trait(?Send)]
impl<T: Identity> SessionStoreExt for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_sub_device_sessions(
        &self,
        addr: &ServiceAddress,
    ) -> Result<Vec<u32>, SignalProtocolError> {
        use crate::schema::session_records::dsl::*;

        let records: Vec<i32> = session_records
            .select(device_id)
            .filter(
                address
                    .eq(addr.uuid.to_string())
                    .and(device_id.ne(libsignal_service::push_service::DEFAULT_DEVICE_ID as i32))
                    .and(identity.eq(self.1.identity())),
            )
            .load(&mut *self.0.db())
            .expect("db");
        Ok(records.into_iter().map(|x| x as u32).collect())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn delete_session(&self, addr: &ProtocolAddress) -> Result<(), SignalProtocolError> {
        use crate::schema::session_records::dsl::*;

        let num = diesel::delete(session_records)
            .filter(
                address
                    .eq(addr.name())
                    .and(device_id.eq(u32::from(addr.device_id()) as i32))
                    .and(identity.eq(self.1.identity())),
            )
            .execute(&mut *self.0.db())
            .expect("db");

        if num != 1 {
            tracing::debug!(
                "Could not delete session {}, assuming non-existing.",
                addr.to_string(),
            );
            Err(SignalProtocolError::SessionNotFound(addr.clone()))
        } else {
            Ok(())
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn delete_all_sessions(
        &self,
        addr: &ServiceAddress,
    ) -> Result<usize, SignalProtocolError> {
        use crate::schema::session_records::dsl::*;

        let num = diesel::delete(session_records)
            .filter(
                address
                    .eq(addr.uuid.to_string())
                    .and(identity.eq(self.1.identity())),
            )
            .execute(&mut *self.0.db())
            .expect("db");

        Ok(num)
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> protocol::SignedPreKeyStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_signed_pre_key(
        &self,
        signed_prekey_id: SignedPreKeyId,
    ) -> Result<SignedPreKeyRecord, SignalProtocolError> {
        use crate::schema::signed_prekeys::dsl::*;
        use diesel::prelude::*;

        let prekey_record: Option<orm::SignedPrekey> = signed_prekeys
            .filter(
                id.eq(u32::from(signed_prekey_id) as i32)
                    .and(identity.eq(self.1.identity())),
            )
            .first(&mut *self.0.db())
            .optional()
            .expect("db");
        if let Some(pkr) = prekey_record {
            Ok(SignedPreKeyRecord::deserialize(&pkr.record)?)
        } else {
            let prekey_record: Option<orm::SignedPrekey> = signed_prekeys
                .filter(id.eq(u32::from(signed_prekey_id) as i32))
                .first(&mut *self.0.db())
                .optional()
                .expect("db");
            if prekey_record.is_some() {
                tracing::warn!(
                    "Signed pre key with ID {signed_prekey_id} found on a separate identity!"
                );
            } else {
                tracing::warn!(
                    "Signed pre key with ID {signed_prekey_id} not found; returning invalid."
                );
            }
            Err(SignalProtocolError::InvalidSignedPreKeyId)
        }
    }

    #[tracing::instrument(level = "trace", skip(self, body))]
    async fn save_signed_pre_key(
        &mut self,
        signed_prekey_id: SignedPreKeyId,
        body: &SignedPreKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        use crate::schema::signed_prekeys::dsl::*;
        use diesel::prelude::*;

        // Insert or replace?
        diesel::insert_into(signed_prekeys)
            .values(orm::SignedPrekey {
                id: u32::from(signed_prekey_id) as _,
                record: body.serialize()?,
                identity: self.1.identity(),
            })
            .execute(&mut *self.0.db())
            .expect("db");

        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity> SenderKeyStore for IdentityStorage<T> {
    #[tracing::instrument(level = "trace", skip(self, record))]
    async fn store_sender_key(
        &mut self,
        addr: &ProtocolAddress,
        distr_id: Uuid,
        record: &SenderKeyRecord,
    ) -> Result<(), SignalProtocolError> {
        let to_insert = orm::SenderKeyRecord {
            address: addr.name().to_owned(),
            device: u32::from(addr.device_id()) as i32,
            distribution_id: distr_id.to_string(),
            record: record.serialize()?,
            created_at: Utc::now().naive_utc(),
            identity: self.1.identity(),
        };

        {
            use crate::schema::sender_key_records::dsl::*;
            diesel::insert_into(sender_key_records)
                .values(to_insert)
                .execute(&mut *self.0.db())
                .expect("db");
        }
        Ok(())
    }

    #[tracing::instrument(level = "trace", skip(self))]
    async fn load_sender_key(
        &mut self,
        addr: &ProtocolAddress,
        distr_id: Uuid,
    ) -> Result<Option<SenderKeyRecord>, SignalProtocolError> {
        let found: Option<orm::SenderKeyRecord> = {
            use crate::schema::sender_key_records::dsl::*;
            sender_key_records
                .filter(
                    address
                        .eq(addr.name())
                        .and(device.eq(u32::from(addr.device_id()) as i32))
                        .and(distribution_id.eq(distr_id.to_string()))
                        .and(identity.eq(self.1.identity())),
                )
                .first(&mut *self.0.db())
                .optional()
                .expect("db")
        };

        match found {
            Some(x) => Ok(Some(SenderKeyRecord::deserialize(&x.record)?)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use libsignal_service::session_store::SessionStoreExt;
    use libsignal_service::{protocol::*, ServiceAddress};
    use rstest::rstest;

    use crate::config::SignalConfig;

    async fn create_example_storage(
        storage_password: Option<&str>,
    ) -> Result<(super::Storage, super::StorageLocation<tempfile::TempDir>), anyhow::Error> {
        use rand::distributions::Alphanumeric;
        use rand::{Rng, RngCore};

        let location = super::temp();
        let rng = rand::thread_rng();

        // Signaling password for REST API
        let password: String = rng
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();

        // Signaling key that decrypts the incoming Signal messages
        let mut rng = rand::thread_rng();
        let mut signaling_key = [0u8; 52];
        rng.fill_bytes(&mut signaling_key);
        let signaling_key = signaling_key;

        // Registration ID
        let regid = 12345;
        let pni_regid = 12345;

        let storage = super::Storage::new(
            Arc::new(SignalConfig::default()),
            &location,
            storage_password,
            regid,
            pni_regid,
            &password,
            signaling_key,
            None,
            None,
        )
        .await?;

        Ok((storage, location))
    }

    fn create_random_protocol_address() -> (ServiceAddress, ProtocolAddress) {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let user_id = uuid::Uuid::new_v4();
        let device_id = rng.gen_range(2..=20);

        let svc = ServiceAddress::from(user_id);
        let prot = ProtocolAddress::new(user_id.to_string(), DeviceId::from(device_id));
        (svc, prot)
    }

    fn create_random_identity_key() -> IdentityKey {
        let mut rng = rand::thread_rng();

        let key_pair = IdentityKeyPair::generate(&mut rng);

        *key_pair.identity_key()
    }

    fn create_random_prekey() -> PreKeyRecord {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let key_pair = KeyPair::generate(&mut rng);
        let id: u32 = rng.gen();

        PreKeyRecord::new(PreKeyId::from(id), &key_pair)
    }

    fn create_random_signed_prekey() -> SignedPreKeyRecord {
        use rand::Rng;
        let mut rng = rand::thread_rng();

        let key_pair = KeyPair::generate(&mut rng);
        let id: u32 = rng.gen();
        let timestamp: u64 = rng.gen();
        let signature = vec![0; 3];

        SignedPreKeyRecord::new(SignedPreKeyId::from(id), timestamp, &key_pair, &signature)
    }

    /// XXX Right now, this functions seems a bit unnecessary, but we will change the creation of a
    /// storage and it might be necessary to check the own identity_key_pair in the protocol store.
    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn own_identity_key_pair(password: Option<&str>) {
        // create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // Copy the identity key pair
        let id_key1 = storage.aci_storage().get_identity_key_pair().await.unwrap();

        // Get access to the protocol store
        // XXX IdentityKeyPair does not implement the std::fmt::Debug trait *arg*
        //assert_eq!(id_key1.unwrap(), store.get_identity_key_pair().await.unwrap());
        assert_eq!(
            id_key1.serialize(),
            storage
                .aci_storage()
                .get_identity_key_pair()
                .await
                .unwrap()
                .serialize()
        );
    }

    /// XXX Right now, this functions seems a bit unnecessary, but we will change the creation of a
    /// storage and it might be necessary to check the regid in the protocol store.
    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn own_regid(password: Option<&str>) {
        // create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // Copy the regid
        let regid_1 = storage
            .aci_storage()
            .get_local_registration_id()
            .await
            .unwrap();

        // Get access to the protocol store
        assert_eq!(
            regid_1,
            storage
                .aci_storage()
                .get_local_registration_id()
                .await
                .unwrap()
        );
    }

    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn save_retrieve_identity_key(password: Option<&str>) {
        // Create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // We need two identity keys and two addresses
        let (_svc1, addr1) = create_random_protocol_address();
        let (_svc2, addr2) = create_random_protocol_address();
        let key1 = create_random_identity_key();
        let key2 = create_random_identity_key();

        let mut storage = storage.aci_storage();

        // In the beginning, the storage should be emtpy and return an error
        // XXX Doesn't implement equality *arg*
        assert_eq!(storage.get_identity(&addr1).await.unwrap(), None);
        assert_eq!(storage.get_identity(&addr2).await.unwrap(), None);

        // We store both keys and should get false because there wasn't a key with that address
        // yet
        assert!(!storage.save_identity(&addr1, &key1).await.unwrap());
        assert!(!storage.save_identity(&addr2, &key2).await.unwrap());

        // Now, we should get both keys
        assert_eq!(storage.get_identity(&addr1).await.unwrap(), Some(key1));
        assert_eq!(storage.get_identity(&addr2).await.unwrap(), Some(key2));

        // After removing key2, it shouldn't be there
        storage.0.delete_identity(&addr2).await.unwrap();
        // XXX Doesn't implement equality *arg*
        assert_eq!(storage.get_identity(&addr2).await.unwrap(), None);

        // We can now overwrite key1 with key1 and should get true returned
        assert!(storage.save_identity(&addr1, &key1).await.unwrap());

        // We can now overwrite key1 with key2 and should get false returned
        assert!(!storage.save_identity(&addr1, &key2).await.unwrap());
    }

    // Direction does not matter yet
    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn is_trusted_identity(password: Option<&str>) {
        // Create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // We need two identity keys and two addresses
        let (_, addr1) = create_random_protocol_address();
        let key1 = create_random_identity_key();
        let key2 = create_random_identity_key();

        let mut storage = storage.aci_storage();

        // Test trust on first use
        assert!(storage
            .is_trusted_identity(&addr1, &key1, Direction::Receiving)
            .await
            .unwrap());

        // Test inserted key
        storage.save_identity(&addr1, &key1).await.unwrap();
        assert!(storage
            .is_trusted_identity(&addr1, &key1, Direction::Receiving)
            .await
            .unwrap());

        // Test wrong key
        assert!(!storage
            .is_trusted_identity(&addr1, &key2, Direction::Receiving)
            .await
            .unwrap());
    }

    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn save_retrieve_prekey(password: Option<&str>) {
        // Create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // We need two identity keys and two addresses
        let id1 = 0u32;
        let id2 = 1u32;
        let key1 = create_random_prekey();
        let key2 = create_random_prekey();

        let mut storage = storage.aci_storage();

        // In the beginning, the storage should be emtpy and return an error
        // XXX Doesn't implement equality *arg*
        assert_eq!(
            storage
                .get_pre_key(PreKeyId::from(id1))
                .await
                .unwrap_err()
                .to_string(),
            SignalProtocolError::InvalidPreKeyId.to_string()
        );

        // Storing both keys and testing retrieval
        storage
            .save_pre_key(PreKeyId::from(id1), &key1)
            .await
            .unwrap();
        storage
            .save_pre_key(PreKeyId::from(id2), &key2)
            .await
            .unwrap();

        // Now, we should get both keys
        assert_eq!(
            storage
                .get_pre_key(PreKeyId::from(id1))
                .await
                .unwrap()
                .serialize()
                .unwrap(),
            key1.serialize().unwrap()
        );
        assert_eq!(
            storage
                .get_pre_key(PreKeyId::from(id2))
                .await
                .unwrap()
                .serialize()
                .unwrap(),
            key2.serialize().unwrap()
        );

        // After removing key2, it shouldn't be there
        storage.remove_pre_key(PreKeyId::from(id2)).await.unwrap();
        // XXX Doesn't implement equality *arg*
        assert_eq!(
            storage
                .get_pre_key(PreKeyId::from(id2))
                .await
                .unwrap_err()
                .to_string(),
            SignalProtocolError::InvalidPreKeyId.to_string()
        );

        // Let's check whether we can overwrite a key
        storage
            .save_pre_key(PreKeyId::from(id1), &key2)
            .await
            .unwrap();
    }

    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn save_retrieve_signed_prekey(password: Option<&str>) {
        // Create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // We need two identity keys and two addresses
        let id1 = 0u32;
        let id2 = 1u32;
        let key1 = create_random_signed_prekey();
        let key2 = create_random_signed_prekey();

        let mut storage = storage.aci_storage();

        // In the beginning, the storage should be emtpy and return an error
        // XXX Doesn't implement equality *arg*
        assert_eq!(
            storage
                .get_signed_pre_key(SignedPreKeyId::from(id1))
                .await
                .unwrap_err()
                .to_string(),
            SignalProtocolError::InvalidSignedPreKeyId.to_string()
        );

        // Storing both keys and testing retrieval
        storage
            .save_signed_pre_key(SignedPreKeyId::from(id1), &key1)
            .await
            .unwrap();
        storage
            .save_signed_pre_key(SignedPreKeyId::from(id2), &key2)
            .await
            .unwrap();

        // Now, we should get both keys
        assert_eq!(
            storage
                .get_signed_pre_key(SignedPreKeyId::from(id1))
                .await
                .unwrap()
                .serialize()
                .unwrap(),
            key1.serialize().unwrap()
        );
        assert_eq!(
            storage
                .get_signed_pre_key(SignedPreKeyId::from(id2))
                .await
                .unwrap()
                .serialize()
                .unwrap(),
            key2.serialize().unwrap()
        );

        // Let's check whether we can overwrite a key
        storage
            .save_signed_pre_key(SignedPreKeyId::from(id1), &key2)
            .await
            .unwrap();
    }

    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn save_retrieve_session(password: Option<&str>) {
        // Create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // Collection of some addresses and sessions
        let (_svc1, addr1) = create_random_protocol_address();
        let (_svc2, addr2) = create_random_protocol_address();
        let (svc3, addr3) = create_random_protocol_address();
        let addr4 = ProtocolAddress::new(
            addr3.name().to_string(),
            DeviceId::from(u32::from(addr3.device_id()) + 1),
        );
        let session1 = SessionRecord::new_fresh();
        let session2 = SessionRecord::new_fresh();
        let session3 = SessionRecord::new_fresh();
        let session4 = SessionRecord::new_fresh();

        let mut storage = storage.aci_storage();

        // In the beginning, the storage should be emtpy and return an error
        assert!(storage.load_session(&addr1).await.unwrap().is_none());
        assert!(storage.load_session(&addr2).await.unwrap().is_none());

        // Store all four sessions: three different names, one name with two different device ids.
        storage.store_session(&addr1, &session1).await.unwrap();
        storage.store_session(&addr2, &session2).await.unwrap();
        storage.store_session(&addr3, &session3).await.unwrap();
        storage.store_session(&addr4, &session4).await.unwrap();

        // Now, we should get the sessions to the first two addresses
        assert_eq!(
            storage
                .load_session(&addr1)
                .await
                .unwrap()
                .unwrap()
                .serialize()
                .unwrap(),
            session1.serialize().unwrap()
        );
        assert_eq!(
            storage
                .load_session(&addr2)
                .await
                .unwrap()
                .unwrap()
                .serialize()
                .unwrap(),
            session2.serialize().unwrap()
        );

        // Let's check whether we can overwrite a key
        storage
            .store_session(&addr1, &session2)
            .await
            .expect("Overwrite session");

        // Get all device ids for the same address
        let mut ids = storage.get_sub_device_sessions(&svc3).await.unwrap();
        ids.sort_unstable();
        assert_eq!(
            DeviceId::from(ids[0]),
            std::cmp::min(addr3.device_id(), addr4.device_id())
        );
        assert_eq!(
            DeviceId::from(ids[1]),
            std::cmp::max(addr3.device_id(), addr4.device_id())
        );

        // If we call delete all sessions, all sessions of one person/address should be removed
        assert_eq!(storage.delete_all_sessions(&svc3).await.unwrap(), 2);
        assert!(storage.load_session(&addr3).await.unwrap().is_none());
        assert!(storage.load_session(&addr4).await.unwrap().is_none());

        // If we delete the first two sessions, they shouldn't be in the store anymore
        SessionStoreExt::delete_session(&storage, &addr1)
            .await
            .unwrap();
        SessionStoreExt::delete_session(&storage, &addr2)
            .await
            .unwrap();
        assert!(storage.load_session(&addr1).await.unwrap().is_none());
        assert!(storage.load_session(&addr2).await.unwrap().is_none());
    }

    #[rstest(password, case(Some("some password")), case(None))]
    #[tokio::test]
    async fn get_next_pre_key_ids(password: Option<&str>) {
        use libsignal_service::pre_keys::PreKeysStore;
        // Create a new storage
        let (storage, _tempdir) = create_example_storage(password).await.unwrap();

        // Create two pre keys and one signed pre key
        let key1 = create_random_prekey();
        let key2 = create_random_prekey();
        let key3 = create_random_signed_prekey();

        let mut storage = storage.aci_storage();

        // In the beginning zero should be returned
        assert_eq!(storage.next_pre_key_id().await.unwrap(), 0);
        assert_eq!(storage.next_pq_pre_key_id().await.unwrap(), 0);
        assert_eq!(storage.next_signed_pre_key_id().await.unwrap(), 0);

        // Now, we add our keys
        storage
            .save_pre_key(PreKeyId::from(0), &key1)
            .await
            .unwrap();
        storage
            .save_pre_key(PreKeyId::from(1), &key2)
            .await
            .unwrap();
        storage
            .save_signed_pre_key(SignedPreKeyId::from(0), &key3)
            .await
            .unwrap();

        // Adapt to keys in the storage
        assert_eq!(storage.next_pre_key_id().await.unwrap(), 2);
        assert_eq!(storage.next_pq_pre_key_id().await.unwrap(), 0);
        assert_eq!(storage.next_signed_pre_key_id().await.unwrap(), 1);
    }
}
