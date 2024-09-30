use super::*;
use libsignal_service::pre_keys::{KyberPreKeyStoreExt, PreKeysStore};
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

impl<O: Observable> Storage<O> {
    pub fn pni_storage(&self) -> PniStorage<O> {
        PniStorage::new(self.clone())
    }

    pub fn aci_storage(&self) -> AciStorage<O> {
        AciStorage::new(self.clone())
    }

    pub fn aci_or_pni(&self, service_id: ServiceIdType) -> AciOrPniStorage<O> {
        IdentityStorage(self.clone(), AciOrPni(service_id))
    }
}

#[derive(Clone)]
pub struct IdentityStorage<T, O: Observable>(Storage<O>, T);

impl<T: Default, O: Observable + Clone> IdentityStorage<T, O> {
    pub fn new(storage: Storage<O>) -> Self {
        Self(storage, Default::default())
    }
}
#[derive(Default, Clone)]
pub struct Aci;
pub type AciStorage<O> = IdentityStorage<Aci, O>;
#[derive(Default, Clone)]
pub struct Pni;
pub type PniStorage<O> = IdentityStorage<Pni, O>;
// Dynamic dispatch between Aci and Pni
#[derive(Clone)]
pub struct AciOrPni(ServiceIdType);
pub type AciOrPniStorage<O> = IdentityStorage<AciOrPni, O>;
pub trait Identity<O: Observable> {
    fn identity(&self) -> orm::Identity;
    fn identity_key_filename(&self) -> &'static str;
    fn regid_filename(&self) -> &'static str;
    fn identity_key_pair_cached(
        &self,
        storage: &Storage<O>,
    ) -> impl std::future::Future<Output = impl std::ops::Deref<Target = Option<IdentityKeyPair>>>;
    fn identity_key_pair_cached_mut(
        &self,
        storage: &Storage<O>,
    ) -> impl std::future::Future<Output = impl std::ops::DerefMut<Target = Option<IdentityKeyPair>>>;
}
impl<O: Observable> Identity<O> for Aci {
    fn identity(&self) -> orm::Identity {
        orm::Identity::Aci
    }
    fn identity_key_filename(&self) -> &'static str {
        "identity_key"
    }
    fn regid_filename(&self) -> &'static str {
        "regid"
    }
    async fn identity_key_pair_cached(
        &self,
        storage: &Storage<O>,
    ) -> impl std::ops::Deref<Target = Option<IdentityKeyPair>> {
        storage.aci_identity_key_pair.read().await
    }
    async fn identity_key_pair_cached_mut(
        &self,
        storage: &Storage<O>,
    ) -> impl std::ops::DerefMut<Target = Option<IdentityKeyPair>> {
        storage.aci_identity_key_pair.write().await
    }
}
impl<O: Observable> Identity<O> for Pni {
    fn identity(&self) -> orm::Identity {
        orm::Identity::Pni
    }
    fn identity_key_filename(&self) -> &'static str {
        "pni_identity_key"
    }
    fn regid_filename(&self) -> &'static str {
        "pni_regid"
    }
    async fn identity_key_pair_cached(
        &self,
        storage: &Storage<O>,
    ) -> impl std::ops::Deref<Target = Option<IdentityKeyPair>> {
        storage.pni_identity_key_pair.read().await
    }
    async fn identity_key_pair_cached_mut(
        &self,
        storage: &Storage<O>,
    ) -> impl std::ops::DerefMut<Target = Option<IdentityKeyPair>> {
        storage.pni_identity_key_pair.write().await
    }
}
impl<O: Observable> Identity<O> for AciOrPni {
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
    async fn identity_key_pair_cached(
        &self,
        storage: &Storage<O>,
    ) -> impl std::ops::Deref<Target = Option<IdentityKeyPair>> {
        match self.0 {
            ServiceIdType::AccountIdentity => &storage.aci_identity_key_pair,
            ServiceIdType::PhoneNumberIdentity => &storage.pni_identity_key_pair,
        }
        .read()
        .await
    }
    async fn identity_key_pair_cached_mut(
        &self,
        storage: &Storage<O>,
    ) -> impl std::ops::DerefMut<Target = Option<IdentityKeyPair>> {
        match self.0 {
            ServiceIdType::AccountIdentity => &storage.aci_identity_key_pair,
            ServiceIdType::PhoneNumberIdentity => &storage.pni_identity_key_pair,
        }
        .write()
        .await
    }
}

#[async_trait::async_trait(?Send)]
impl<O: Observable> protocol::ProtocolStore for IdentityStorage<AciOrPni, O> {}
#[async_trait::async_trait(?Send)]
impl<O: Observable> protocol::ProtocolStore for IdentityStorage<Aci, O> {}
#[async_trait::async_trait(?Send)]
impl<O: Observable> protocol::ProtocolStore for IdentityStorage<Pni, O> {}

impl<T: Identity<O>, O: Observable> IdentityStorage<T, O> {
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
        *self.1.identity_key_pair_cached_mut(&self.0).await = Some(key_pair);
        self.0
            .write_file(path, ProtocolStore::serialize_identity_key(key_pair))
            .await
            .map_err(|e| {
                SignalProtocolError::InvalidArgument(format!("Cannot write own identity key {}", e))
            })?;
        Ok(())
    }

    #[tracing::instrument(level = "warn", skip(self))]
    // Mutability of self is artificial
    pub async fn remove_identity_key_pair(&mut self) -> Result<(), SignalProtocolError> {
        let _lock = self.0.protocol_store.write().await;

        let path = self
            .0
            .path
            .join("storage")
            .join("identity")
            .join(self.1.identity_key_filename());
        tracing::warn!("removing own identity key pair at {}", path.display());
        *self.1.identity_key_pair_cached_mut(&self.0).await = None;
        tokio::fs::remove_file(path).await.map_err(|e| {
            SignalProtocolError::InvalidArgument(format!("Cannot remove own identity key {}", e))
        })?;
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity<O>, O: Observable> protocol::IdentityKeyStore for IdentityStorage<T, O> {
    #[tracing::instrument(level = "trace", skip(self))]
    async fn get_identity_key_pair(&self) -> Result<IdentityKeyPair, SignalProtocolError> {
        let _lock = self.0.protocol_store.read().await;

        if let Some(ikp) = *self.1.identity_key_pair_cached(&self.0).await {
            return Ok(ikp);
        }

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
        drop(_lock);
        let _lock = self.0.protocol_store.write().await;
        *self.1.identity_key_pair_cached_mut(&self.0).await = Some(key_pair);
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
        Ok(identity_records
            .filter(address.eq(addr.name()).and(identity.eq(self.1.identity())))
            .first(&mut *self.0.db())
            .optional()
            .expect("db")
            .map(|found: orm::IdentityRecord| {
                IdentityKey::decode(&found.record).expect("only valid identity keys in db")
            }))
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity<O>, O: Observable> protocol::SessionStore for IdentityStorage<T, O> {
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
impl<T: Identity<O>, O: Observable> KyberPreKeyStoreExt for IdentityStorage<T, O> {
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
impl<T: Identity<O>, O: Observable> PreKeysStore for IdentityStorage<T, O> {
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

    async fn signed_pre_keys_count(&self) -> Result<usize, SignalProtocolError> {
        use diesel::prelude::*;

        let signed_prekey_count: i64 = {
            use crate::schema::signed_prekeys::dsl::*;

            signed_prekeys
                .select(diesel::dsl::count_star())
                .filter(identity.eq(self.1.identity()))
                .first(&mut *self.0.db())
                .expect("db")
        };

        Ok(signed_prekey_count as usize)
    }

    async fn kyber_pre_keys_count(&self, _last_resort: bool) -> Result<usize, SignalProtocolError> {
        use diesel::prelude::*;

        let kyber_prekey_count: i64 = {
            use crate::schema::kyber_prekeys::dsl::*;

            kyber_prekeys
                .select(diesel::dsl::count_star())
                .filter(identity.eq(self.1.identity()))
                .first(&mut *self.0.db())
                .expect("db")
        };

        Ok(kyber_prekey_count as usize)
    }
}

impl<T: Identity<O>, O: Observable> IdentityStorage<T, O> {
    /// Whether to force a pre key refresh.
    ///
    /// Check whether we have:
    /// - 1 signed EC pre key
    /// - 1 Kyber last resort key
    pub async fn needs_pre_key_refresh(&self) -> bool {
        let signed_count = {
            use crate::schema::signed_prekeys::dsl::*;
            use diesel::prelude::*;

            let prekey_records: i64 = signed_prekeys
                .select(diesel::dsl::count_star())
                .filter(identity.eq(self.1.identity()))
                .first(&mut *self.0.db())
                .expect("db");
            prekey_records
        };

        if signed_count == 0 {
            return true;
        }

        let kyber_count = {
            use crate::schema::kyber_prekeys::dsl::*;
            use diesel::prelude::*;

            let prekey_records: i64 = kyber_prekeys
                .select(diesel::dsl::count_star())
                .filter(identity.eq(self.1.identity()).and(is_last_resort.eq(true)))
                .first(&mut *self.0.db())
                .expect("db");
            prekey_records
        };

        if kyber_count == 0 {
            return true;
        }

        false
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity<O>, O: Observable> protocol::PreKeyStore for IdentityStorage<T, O> {
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
impl<T: Identity<O>, O: Observable> protocol::KyberPreKeyStore for IdentityStorage<T, O> {
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

impl<T: Identity<O>, O: Observable> IdentityStorage<T, O> {
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
impl<O: Observable> Storage<O> {
    /// Removes the identity matching ServiceAddress (ACI or PNI) from the database.
    ///
    /// Does not lock the protocol storage.
    #[tracing::instrument(level = "warn", skip(self, addr), fields(addr = %addr))]
    pub fn delete_identity_key(&self, addr: &ServiceAddress) -> bool {
        use crate::schema::identity_records::dsl::*;
        let removed = diesel::delete(identity_records)
            .filter(
                address
                    .eq(addr.to_service_id())
                    .and(identity.eq(orm::Identity::from(addr.identity.to_string().as_str()))),
            )
            .execute(&mut *self.db())
            .expect("db")
            >= 1;

        if removed {
            tracing::trace!("Identity removed: {:?}", addr)
        } else {
            tracing::trace!("Identity not found: {:?}", addr)
        };

        removed
    }
}
// END identity key

#[async_trait::async_trait(?Send)]
impl<T: Identity<O>, O: Observable> SessionStoreExt for IdentityStorage<T, O> {
    #[tracing::instrument(level = "trace", skip(self, addr), fields(addr = %addr))]
    async fn get_sub_device_sessions(
        &self,
        addr: &ServiceAddress,
    ) -> Result<Vec<u32>, SignalProtocolError> {
        use crate::schema::session_records::dsl::*;

        let records: Vec<i32> = session_records
            .select(device_id)
            .filter(
                address
                    .eq(addr.to_service_id())
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

    #[tracing::instrument(level = "trace", skip(self, addr), fields(addr = %addr))]
    async fn delete_all_sessions(
        &self,
        addr: &ServiceAddress,
    ) -> Result<usize, SignalProtocolError> {
        use crate::schema::session_records::dsl::*;

        let num = diesel::delete(session_records)
            .filter(
                address
                    .eq(addr.to_service_id())
                    .and(identity.eq(self.1.identity())),
            )
            .execute(&mut *self.0.db())
            .expect("db");

        Ok(num)
    }
}

#[async_trait::async_trait(?Send)]
impl<T: Identity<O>, O: Observable> protocol::SignedPreKeyStore for IdentityStorage<T, O> {
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
impl<T: Identity<O>, O: Observable> SenderKeyStore for IdentityStorage<T, O> {
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
