//! These integration tests check whether an old storage implementation can be opened. The old
//! storage implementation is stripped down to write files (e.g., identity keys, sessions,
//! attachments, etc.), and opening the database. We don't test any migrations of messages, etc.
//! here. The created storage is then read with the current functions.
//!
//! Currently the storage implementation in `current_storage` is at git commit
//! e8ef69ba76b5f40fc149bf1c240df99b62f19b60. Be aware that only necessary parts were copied that
//! were changed in later commits.

use libsignal_service::protocol::{DeviceId, IdentityKeyStore, SessionStore};
use rstest::rstest;
use std::{ops::Deref, sync::Arc};
use whisperfish_store as current_storage;
use whisperfish_store::config::SignalConfig;

async fn create_old_storage(
    storage_password: Option<&str>,
    path: &current_storage::StorageLocation<tempfile::TempDir>,
) -> current_storage::Storage {
    use rand::{Rng, RngCore};
    let rng = rand::thread_rng();

    // Signaling password for REST API
    let password: String = rng
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    // Signaling key that decrypts the incoming Signal messages
    let mut rng = rand::thread_rng();
    let mut signaling_key = [0u8; 52];
    rng.fill_bytes(&mut signaling_key);
    let signaling_key = signaling_key;

    // Registration ID
    let regid: u32 = 12345;
    let pni_regid: u32 = 12345;

    current_storage::Storage::new(
        Arc::new(SignalConfig::default()),
        path,
        storage_password,
        regid,
        pni_regid,
        &password,
        signaling_key,
        None,
        None,
    )
    .await
    .unwrap()
}

async fn open_storage(
    storage_password: Option<String>,
    path: &whisperfish_store::StorageLocation<std::path::PathBuf>,
) -> whisperfish_store::Storage {
    whisperfish_store::Storage::open(Arc::new(SignalConfig::default()), path, storage_password)
        .await
        .unwrap()
}

fn create_random_protocol_address() -> libsignal_service::protocol::ProtocolAddress {
    use rand::Rng;
    let mut rng = rand::thread_rng();

    let user_id = uuid::Uuid::new_v4();
    let device_id = rng.gen_range(2..=20);

    libsignal_service::protocol::ProtocolAddress::new(
        user_id.to_string(),
        DeviceId::from(device_id),
    )
}

fn create_random_identity_key() -> libsignal_service::protocol::IdentityKey {
    let mut rng = rand::thread_rng();

    let key_pair = libsignal_service::protocol::IdentityKeyPair::generate(&mut rng);

    *key_pair.identity_key()
}

#[rstest(
    storage_password,
    case(Some(String::from("some password"))),
    case(None)
)]
#[actix_rt::test]
async fn read_own_identity_key(storage_password: Option<String>) {
    let location = current_storage::temp();
    let storage = create_old_storage(storage_password.as_deref(), &location).await;

    // Get own identity key
    let own_identity_key_1 = storage.get_identity_key_pair().await.unwrap();

    // Drop storage
    drop(storage);

    // Open storage with new implementation
    let location: whisperfish_store::StorageLocation<std::path::PathBuf> =
        location.deref().to_path_buf().into();
    let storage = open_storage(storage_password, &location).await;

    // Get own identity key
    let own_identity_key_2 = storage.get_identity_key_pair().await.unwrap();

    // Test equality
    assert_eq!(
        own_identity_key_1.serialize(),
        own_identity_key_2.serialize()
    );
}

#[rstest(
    storage_password,
    case(Some(String::from("some password"))),
    case(None)
)]
#[actix_rt::test]
async fn read_regid(storage_password: Option<String>) {
    let location = current_storage::temp();
    let storage = create_old_storage(storage_password.as_deref(), &location).await;

    // Get own identity key
    let regid_1 = storage.get_local_registration_id().await.unwrap();

    // Drop storage
    drop(storage);

    // Open storage with new implementation
    let location: whisperfish_store::StorageLocation<std::path::PathBuf> =
        location.deref().to_path_buf().into();
    let storage = open_storage(storage_password, &location).await;

    // Get own identity key
    let regid_2 = storage.get_local_registration_id().await.unwrap();

    // Test equality
    assert_eq!(regid_1, regid_2);
}

#[rstest(
    storage_password,
    case(Some(String::from("some password"))),
    case(None)
)]
#[actix_rt::test]
async fn read_signal_password(storage_password: Option<String>) {
    let location = current_storage::temp();
    let storage = create_old_storage(storage_password.as_deref(), &location).await;

    // Get own identity key
    let value_1 = storage.signal_password().await.unwrap();

    // Drop storage
    drop(storage);

    // Open storage with new implementation
    let location: whisperfish_store::StorageLocation<std::path::PathBuf> =
        location.deref().to_path_buf().into();
    let storage = open_storage(storage_password, &location).await;

    // Get own identity key
    let value_2 = storage.signal_password().await.unwrap();

    // Test equality
    assert_eq!(value_1, value_2);
}

#[rstest(
    storage_password,
    case(Some(String::from("some password"))),
    case(None)
)]
#[actix_rt::test]
async fn read_signaling_key(storage_password: Option<String>) {
    let location = current_storage::temp();
    let storage = create_old_storage(storage_password.as_deref(), &location).await;

    // Get own identity key
    let value_1 = storage.signaling_key().await.unwrap();

    // Drop storage
    drop(storage);

    // Open storage with new implementation
    let location: whisperfish_store::StorageLocation<std::path::PathBuf> =
        location.deref().to_path_buf().into();
    let storage = open_storage(storage_password, &location).await;

    // Get own identity key
    let value_2 = storage.signaling_key().await.unwrap();

    // Test equality
    assert_eq!(value_1, value_2);
}

#[rstest(
    storage_password,
    case(Some(String::from("some password"))),
    case(None)
)]
#[actix_rt::test]
async fn read_other_identity_key(storage_password: Option<String>) {
    let location = current_storage::temp();
    let mut storage = create_old_storage(storage_password.as_deref(), &location).await;

    // Create new identity key
    let addr = create_random_protocol_address();
    let key = create_random_identity_key();

    // Store identity key
    storage.save_identity(&addr, &key).await.unwrap();

    // Drop storage
    drop(storage);

    // Open storage with new implementation
    let location: whisperfish_store::StorageLocation<std::path::PathBuf> =
        location.deref().to_path_buf().into();
    let storage = open_storage(storage_password, &location).await;

    // Get saved identity key
    let key_2 = storage.get_identity(&addr).await.unwrap().unwrap();

    // Test equality
    assert_eq!(key, key_2);
}

async fn copy_to_temp(root: std::path::PathBuf) -> tempfile::TempDir {
    let new_root = tempfile::tempdir().unwrap();

    let mut queue = std::collections::VecDeque::new();
    queue.push_back((root, new_root.path().to_owned()));

    while let Some((source_path, target)) = queue.pop_front() {
        if source_path.is_dir() && !target.exists() {
            tokio::fs::create_dir(&target).await.unwrap();
        }

        let mut read_dir = tokio::fs::read_dir(source_path).await.unwrap();
        while let Some(child) = read_dir.next_entry().await.unwrap() {
            let path = child.path();
            if path.is_dir() {
                let new_target = target.join(path.file_name().unwrap());
                queue.push_back((path, new_target));
            } else {
                assert!(path.is_file());

                let target_path = target.join(path.file_name().unwrap());

                tokio::fs::copy(path, target_path).await.unwrap();
            }
        }
    }

    new_root
}

/// These storages were initialized in June 2022, while moving the identity and session store into the SQLite database.
///
/// https://gitlab.com/whisperfish/whisperfish/-/merge_requests/249
#[rstest]
#[case("tests/resources/storage_migration/without-password-2022-06".into(), None)]
#[case("tests/resources/storage_migration/with-password-123456-2022-06".into(), Some("123456".into()))]
#[actix_rt::test]
async fn test_2022_06_migration(
    #[case] path: std::path::PathBuf,
    #[case] storage_password: Option<String>,
) {
    use std::str::FromStr;
    use whisperfish_store::migrations::session_to_db::SessionStorageMigration;

    env_logger::try_init().ok();

    let path = current_storage::StorageLocation::Path(copy_to_temp(path).await);
    let storage = whisperfish_store::Storage::open(
        Arc::new(SignalConfig::default()),
        &path,
        storage_password,
    )
    .await
    .expect("open older storage");
    let migration = SessionStorageMigration(storage);
    println!("Start migration");
    migration.execute().await;
    println!("End migration");
    let SessionStorageMigration(storage) = migration;

    let user_id = uuid::Uuid::from_str("5844fce4-4407-401a-9dbc-fc86c6def4e6").unwrap();
    let device_id = 1;
    let addr_1 = libsignal_service::protocol::ProtocolAddress::new(
        user_id.to_string(),
        DeviceId::from(device_id),
    );

    let user_id = uuid::Uuid::from_str("7bec59e1-140d-4b53-98f1-dc8fd2c011c8").unwrap();
    let device_id = 2;
    let addr_2 = libsignal_service::protocol::ProtocolAddress::new(
        user_id.to_string(),
        DeviceId::from(device_id),
    );

    let identity_key_1 = storage.get_identity(&addr_1).await.unwrap();
    let identity_key_2 = storage.get_identity(&addr_2).await.unwrap();
    assert!(identity_key_1.is_some());
    assert!(identity_key_2.is_some());

    let session_1 = storage.load_session(&addr_1).await.unwrap();
    let session_2 = storage.load_session(&addr_2).await.unwrap();
    assert!(session_1.is_some());
    assert!(session_2.is_some());

    assert!(!path.join("storage").join("sessions").exists());
}
