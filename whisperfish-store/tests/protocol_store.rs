mod common;

#[cfg(test)]
mod tests {
    use libsignal_service::master_key::{MasterKey, MasterKeyStore, StorageServiceKey};
    use libsignal_service::protocol::IdentityChange;
    use libsignal_service::protocol::{GenericSignedPreKey, IdentityKeyPair, SignalProtocolError};
    use libsignal_service::session_store::SessionStoreExt;
    use std::sync::Arc;

    use libsignal_service::protocol::*;
    use rstest::rstest;

    use whisperfish_store::config::SignalConfig;
    use whisperfish_store::{Settings, Storage, StorageLocation};

    use crate::common::{DummyObservatory, SimpleStorage};

    async fn create_example_storage(
        storage_password: Option<&str>,
    ) -> Result<
        (
            Storage<DummyObservatory>,
            StorageLocation<tempfile::TempDir>,
        ),
        anyhow::Error,
    > {
        use rand::distr::Alphanumeric;
        use rand::Rng;

        let location = whisperfish_store::temp();
        let rng = rand::rng();

        // Signaling password for REST API
        let password: String = rng
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();

        // Registration ID
        let regid = 12345;
        let pni_regid = 12345;

        let storage = Storage::new(
            Arc::new(SignalConfig::default()),
            &location,
            storage_password,
            regid,
            pni_regid,
            &password,
            None,
            None,
        )
        .await?;

        Ok((storage, location))
    }

    fn create_random_protocol_address() -> (ServiceId, ProtocolAddress) {
        use rand::Rng;
        let mut rng = rand::rng();

        let user_id = uuid::Uuid::new_v4();
        let device_id = rng.random_range(2..=20);

        let svc = ServiceId::from(Aci::from(user_id));
        let prot = ProtocolAddress::new(user_id.to_string(), DeviceId::new(device_id).unwrap());
        (svc, prot)
    }

    fn create_random_identity_key() -> IdentityKey {
        let mut rng = rand::rng();

        let key_pair = IdentityKeyPair::generate(&mut rng);

        *key_pair.identity_key()
    }

    fn create_random_prekey() -> PreKeyRecord {
        use rand::Rng;
        let mut rng = rand::rng();

        let key_pair = KeyPair::generate(&mut rng);
        let id: u32 = rng.random();

        PreKeyRecord::new(PreKeyId::from(id), &key_pair)
    }

    fn create_random_signed_prekey() -> SignedPreKeyRecord {
        use rand::Rng;
        let mut rng = rand::rng();

        let key_pair = KeyPair::generate(&mut rng);
        let id: u32 = rng.random();
        let timestamp = Timestamp::from_epoch_millis(rng.random::<u64>());
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
        let (svc2, addr2) = create_random_protocol_address();
        let key1 = create_random_identity_key();
        let key2 = create_random_identity_key();

        let mut aci_storage = storage.aci_storage();

        // In the beginning, the storage should be emtpy and return an error
        // XXX Doesn't implement equality *arg*
        assert_eq!(aci_storage.get_identity(&addr1).await.unwrap(), None);
        assert_eq!(aci_storage.get_identity(&addr2).await.unwrap(), None);

        // We store both keys and should get false because there wasn't a key with that address
        // yet
        assert_eq!(
            aci_storage.save_identity(&addr1, &key1).await.unwrap(),
            IdentityChange::NewOrUnchanged
        );
        assert_eq!(
            aci_storage.save_identity(&addr2, &key2).await.unwrap(),
            IdentityChange::NewOrUnchanged
        );

        // Now, we should get both keys
        assert_eq!(aci_storage.get_identity(&addr1).await.unwrap(), Some(key1));
        assert_eq!(aci_storage.get_identity(&addr2).await.unwrap(), Some(key2));

        // After removing key2, it shouldn't be there
        storage.delete_identity_key(&svc2);
        // XXX Doesn't implement equality *arg*
        assert_eq!(aci_storage.get_identity(&addr2).await.unwrap(), None);

        // We can now overwrite key1 with key1 and should get true returned
        assert_eq!(
            aci_storage.save_identity(&addr1, &key1).await.unwrap(),
            IdentityChange::NewOrUnchanged
        );

        // We can now overwrite key1 with key2 and should get false returned
        assert_eq!(
            aci_storage.save_identity(&addr1, &key2).await.unwrap(),
            IdentityChange::ReplacedExisting
        );
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
            DeviceId::new(u8::from(addr3.device_id()) + 1).unwrap(),
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
        assert_eq!(ids[0], std::cmp::min(addr3.device_id(), addr4.device_id()));
        assert_eq!(ids[1], std::cmp::max(addr3.device_id(), addr4.device_id()));

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

    #[tokio::test]
    async fn store_and_load_master_key_and_storage_key() {
        use rand::distr::Alphanumeric;
        use rand::Rng;

        let location = whisperfish_store::temp();
        let mut rng = rand::rng();

        // Signaling password for REST API
        let password: String = (&mut rng)
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();

        // Registration ID
        let regid = 12345;
        let pni_regid = 12345;

        let storage = SimpleStorage::new(
            Arc::new(SignalConfig::default()),
            &location,
            None,
            regid,
            pni_regid,
            &password,
            None,
            None,
        )
        .await;
        assert!(storage.is_ok(), "{}", storage.err().unwrap());
        let storage = storage.unwrap();

        // Part 1: Store and load known master and storage keys

        use base64::prelude::*;
        const MASTER_KEY_BASE64: &str = "9hquLIIZmom8fHF7H8pbUAreawmPLEqli5ceJ94pFkU=";
        const STORAGE_KEY_BASE64: &str = "QMgZ5RGTLFTr4u/J6nypaJX6DKDlSgMw8vmxU6gxnvI=";

        assert!(storage.read_setting(Settings::MASTER_KEY).is_none());
        assert!(storage
            .read_setting(Settings::STORAGE_SERVICE_KEY)
            .is_none());

        let master_key =
            MasterKey::from_slice(&BASE64_STANDARD.decode(MASTER_KEY_BASE64).unwrap()).unwrap();
        let storage_key =
            StorageServiceKey::from_slice(&BASE64_STANDARD.decode(STORAGE_KEY_BASE64).unwrap())
                .unwrap();

        storage.write_setting(Settings::MASTER_KEY, MASTER_KEY_BASE64);
        assert!(storage.read_setting(Settings::MASTER_KEY).is_some());
        assert!(storage
            .read_setting(Settings::STORAGE_SERVICE_KEY)
            .is_none());

        storage.write_setting(Settings::STORAGE_SERVICE_KEY, STORAGE_KEY_BASE64);
        assert!(storage.read_setting(Settings::MASTER_KEY).is_some());
        assert!(storage
            .read_setting(Settings::STORAGE_SERVICE_KEY)
            .is_some());

        let master_key_db = storage.fetch_master_key();
        let storage_key_db = storage.fetch_storage_service_key();

        assert_eq!(Some(master_key), master_key_db);
        assert_eq!(Some(storage_key), storage_key_db);

        // Part 2: Store (overwrite) and load a generated master key and a derived storage key

        let master_key = MasterKey::generate(&mut rng);
        let storage_key = StorageServiceKey::from_master_key(&master_key);

        storage.write_setting(
            Settings::MASTER_KEY,
            &BASE64_STANDARD.encode(master_key.inner),
        );
        storage.write_setting(
            Settings::STORAGE_SERVICE_KEY,
            &BASE64_STANDARD.encode(storage_key.inner),
        );

        let master_key_db = storage.fetch_master_key();
        let storage_key_db = storage.fetch_storage_service_key();

        assert_eq!(Some(master_key), master_key_db);
        assert_eq!(Some(storage_key), storage_key_db);

        // Part 3: Delete the setting and verify that they are gone

        storage.delete_setting(Settings::MASTER_KEY);
        storage.delete_setting(Settings::STORAGE_SERVICE_KEY);

        assert!(storage.read_setting(Settings::MASTER_KEY).is_none());
        assert!(storage
            .read_setting(Settings::STORAGE_SERVICE_KEY)
            .is_none());
    }
}
