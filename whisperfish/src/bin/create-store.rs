use clap::Parser;
use libsignal_service::protocol::*;
use std::{path::PathBuf, sync::Arc};
use whisperfish::{config::SignalConfig, store};

/// Initializes a storage, meant for creating storage migration tests.
#[derive(Parser, Debug)]
#[structopt(name = "create-store", author, version, about, long_about = None)]
struct Opts {
    /// Whisperfish storage password
    #[clap(short, long)]
    password: Option<String>,

    /// Path where the storage will be created
    #[clap(parse(from_os_str))]
    path: PathBuf,

    /// Whether to fill the storage with dummy data
    #[clap(short, long)]
    fill_dummy: bool,
}

async fn create_storage(
    config: Arc<SignalConfig>,
    storage_password: Option<&str>,
    path: store::StorageLocation<PathBuf>,
) -> store::Storage {
    use rand::Rng;
    let rng = rand::thread_rng();

    // Signaling password for REST API
    let password: String = rng
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    // Registration ID
    let regid: u32 = 12345;
    let pni_regid: u32 = 12346;

    store::Storage::new(
        config,
        &path,
        storage_password,
        regid,
        pni_regid,
        &password,
        None,
        None,
    )
    .await
    .unwrap()
}

async fn add_dummy_data(storage: &mut store::Storage) {
    use std::str::FromStr;
    let mut rng = rand::thread_rng();

    // Invent two users with devices
    let user_id = uuid::Uuid::from_str("5844fce4-4407-401a-9dbc-fc86c6def4e6").unwrap();
    let device_id = 1;
    let addr_1 = ProtocolAddress::new(user_id.to_string(), DeviceId::from(device_id));

    let user_id = uuid::Uuid::from_str("7bec59e1-140d-4b53-98f1-dc8fd2c011c8").unwrap();
    let device_id = 2;
    let addr_2 = ProtocolAddress::new(user_id.to_string(), DeviceId::from(device_id));

    let device_id = 3;
    let addr_3 = ProtocolAddress::new("+32412345678".into(), DeviceId::from(device_id));

    // Create two identities and two sessions
    let key_1 = IdentityKeyPair::generate(&mut rng);
    let key_2 = IdentityKeyPair::generate(&mut rng);
    let key_3 = IdentityKeyPair::generate(&mut rng);

    storage
        .aci_storage()
        .save_identity(&addr_1, key_1.identity_key())
        .await
        .unwrap();
    storage
        .aci_storage()
        .save_identity(&addr_2, key_2.identity_key())
        .await
        .unwrap();
    storage
        .aci_storage()
        .save_identity(&addr_3, key_3.identity_key())
        .await
        .unwrap();

    let session_1 = SessionRecord::new_fresh();
    let session_2 = SessionRecord::new_fresh();
    let session_3 = SessionRecord::new_fresh();
    storage
        .aci_storage()
        .store_session(&addr_1, &session_1)
        .await
        .unwrap();
    storage
        .aci_storage()
        .store_session(&addr_2, &session_2)
        .await
        .unwrap();
    storage
        .aci_storage()
        .store_session(&addr_3, &session_3)
        .await
        .unwrap();
}

#[actix_rt::main]
async fn main() -> Result<(), anyhow::Error> {
    let opt: Opts = Parser::parse_from(std::env::args_os());

    // TODO: probably source more config flags, see harbour-whisperfish main.rs
    let config = match whisperfish::config::SignalConfig::read_from_file() {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Config file not found: {}", e);
            whisperfish::config::SignalConfig::default()
        }
    };
    let config = Arc::new(config);

    let path = opt.path;
    let mut store = create_storage(config, opt.password.as_deref(), path.into()).await;

    if opt.fill_dummy {
        add_dummy_data(&mut store).await;
    }

    Ok(())
}
