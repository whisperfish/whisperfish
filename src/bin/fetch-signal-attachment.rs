use std::path::Path;

use failure::{ensure, format_err, Error};
use futures::io::AsyncReadExt;
use harbour_whisperfish::settings::SignalConfig;
use harbour_whisperfish::store::{self, Storage};
use structopt::StructOpt;

use libsignal_service::configuration::SignalServers;
use libsignal_service::prelude::*;
use libsignal_service_actix::prelude::*;

/// Signal attachment downloader for Whisperfish
#[derive(StructOpt, Debug)]
#[structopt(name = "fetch-signal-attachment")]
struct Opt {
    /// Whisperfish storage password
    #[structopt(short, long)]
    password: Option<String>,

    /// CDN number (normally either 0 or 2)
    #[structopt(short, long)]
    cdn_number: u32,

    /// AttachmentPointer CdnKey or CdnId
    #[structopt(short, long, allow_hyphen_values(true))]
    cdn_key: String,

    /// Key of AttachmentPointer
    #[structopt(short, long)]
    key: String,

    /// Message will be found by ID.
    ///
    /// Specify either this or `timestamp`
    #[structopt(short, long)]
    message_id: i32,

    /// Extension for file
    #[structopt(short, long)]
    ext: String,

    /// Mime-type for file
    #[structopt(short, long)]
    mime_type: String,
}

fn read_config() -> Result<SignalConfig, Error> {
    // XXX non-existing file?
    let conf_dir =
        dirs::config_dir().ok_or_else(|| format_err!("Could not find config directory."))?;
    let signal_config_file = conf_dir.join("harbour-whisperfish").join("config.yml");
    let signal_config_file = std::fs::File::open(signal_config_file)?;

    Ok(serde_yaml::from_reader(signal_config_file)?)
}

#[actix_rt::main]
async fn main() -> Result<(), Error> {
    env_logger::init();

    let opt = Opt::from_args();

    let config = read_config()?;
    let settings = harbour_whisperfish::settings::Settings::default();
    let dir = settings.get_string("attachment_dir");
    let dest = Path::new(&dir);

    let mut storage = Storage::open(&store::default_location()?, opt.password).await?;

    let key_material = hex::decode(opt.key)?;
    ensure!(
        key_material.len() == 64,
        "Attachment key should have 64 bytes"
    );

    // Check whether we can find the message that this attachment should be linked to.
    let mid = opt.message_id;
    let msg = storage
        .fetch_message_by_id(mid)
        .expect("find message by mid");
    ensure!(
        msg.id == mid,
        "unreachable: Fetched message ID does not equal supplied mid"
    );

    // Connection details for OWS servers
    // XXX: https://gitlab.com/rubdos/whisperfish/-/issues/80
    let phonenumber = phonenumber::parse(None, config.tel.expect("phone number")).unwrap();
    let uuid = config.uuid.map(|u| u.parse().expect("valid UUID"));
    let e164 = phonenumber
        .format()
        .mode(phonenumber::Mode::E164)
        .to_string();
    log::info!("E164: {}", e164);
    let signaling_key = Some(storage.signaling_key().await.unwrap());
    let credentials = ServiceCredentials {
        uuid,
        phonenumber,
        password: None,
        signaling_key,
        device_id: None, // !77
    };

    let service_cfg = SignalServers::Production.into();

    // Connect to OWS
    let useragent = format!("Whisperfish-{}", env!("CARGO_PKG_VERSION"));
    let mut service = AwcPushService::new(service_cfg, Some(credentials.clone()), &useragent);

    // Download the attachment
    let mut stream = service
        .get_attachment_by_id(&opt.cdn_key, opt.cdn_number)
        .await?;
    log::info!("Downloading attachment");

    // We need the whole file for the crypto to check out 😢
    let mut ciphertext = Vec::new();
    let len = stream
        .read_to_end(&mut ciphertext)
        .await
        .expect("streamed attachment");

    log::info!("Downloaded {} bytes", len);

    let mut key = [0u8; 64];
    key.copy_from_slice(&key_material);
    libsignal_service::attachment_cipher::decrypt_in_place(key, &mut ciphertext)
        .expect("attachment decryption");

    let attachment_path =
        crate::store::save_attachment(&dest, &opt.ext, futures::io::Cursor::new(ciphertext)).await;

    log::info!("Attachment stored at {:?}", attachment_path);

    storage.register_attachment(
        mid,
        attachment_path
            .canonicalize()
            .unwrap()
            .to_str()
            .expect("attachment path utf-8"),
        &opt.mime_type,
    );
    log::info!("Attachment registered with message {:?}", msg);
    Ok(())
}
