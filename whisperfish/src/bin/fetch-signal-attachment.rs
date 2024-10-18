use clap::Parser;
use futures::io::AsyncReadExt;
use libsignal_service::configuration::SignalServers;
use libsignal_service::prelude::*;
use mime_classifier::{ApacheBugFlag, LoadContext, MimeClassifier, NoSniffFlag};
use std::{path::Path, sync::Arc};
use whisperfish::store::{self, Storage};

/// Signal attachment downloader for Whisperfish
#[derive(Parser, Debug)]
#[clap(name = "fetch-signal-attachment", author, version, about, long_about = None)]
struct Opts {
    /// Whisperfish storage password
    #[structopt(short, long)]
    password: Option<String>,

    /// CDN number (normally either 0 or 2)
    #[structopt(short = 'c', long)]
    cdn_number: u32,

    /// AttachmentPointer CdnKey or CdnId
    #[structopt(short = 'C', long, allow_hyphen_values(true))]
    cdn_key: String,

    /// Key of AttachmentPointer
    #[structopt(short, long)]
    key: String,

    /// Message will be found by ID.
    ///
    /// Specify either this or `timestamp`
    #[structopt(short = 'M', long)]
    message_id: i32,

    /// Extension for file
    #[structopt(short, long)]
    ext: String,

    /// Mime-type for file
    #[structopt(short = 'm', long)]
    mime_type: String,
}

#[actix_rt::main]
async fn main() -> Result<(), anyhow::Error> {
    let mut opt: Opts = Parser::parse_from(std::env::args_os());

    let config = whisperfish::config::SignalConfig::read_from_file()?;
    let config = Arc::new(config);
    let settings = whisperfish::config::SettingsBridge::default();
    let dir = settings.get_string("attachment_dir");
    let dest = Path::new(&dir);

    let mut storage =
        Storage::open(config.clone(), &store::default_location()?, opt.password).await?;

    let key_material = hex::decode(opt.key)?;
    anyhow::ensure!(
        key_material.len() == 64,
        "Attachment key should have 64 bytes"
    );

    // Check whether we can find the message that this attachment should be linked to.
    let mid = opt.message_id;
    let msg = storage
        .fetch_message_by_id(mid)
        .expect("find message by mid");
    anyhow::ensure!(
        msg.id == mid,
        "unreachable: Fetched message ID does not equal supplied mid"
    );

    // Connection details for OWS servers
    // XXX: https://gitlab.com/whisperfish/whisperfish/-/issues/80
    let phonenumber = config.get_tel().expect("phone number present");
    let aci = config.get_aci();
    let pni = config.get_pni();
    let device_id = config.get_device_id();
    let e164 = phonenumber
        .format()
        .mode(phonenumber::Mode::E164)
        .to_string();
    tracing::info!("E164: {}", e164);
    let signaling_key = storage.signaling_key().await.unwrap();
    let credentials = ServiceCredentials {
        aci,
        pni,
        phonenumber,
        password: None,
        signaling_key,
        device_id: Some(device_id.into()),
    };

    // Connect to OWS
    let mut service = PushService::new(
        SignalServers::Production,
        Some(credentials.clone()),
        whisperfish::user_agent(),
    );

    // Download the attachment
    let mut stream = service
        .get_attachment_by_id(&opt.cdn_key, opt.cdn_number)
        .await?;
    tracing::info!("Downloading attachment");

    // We need the whole file for the crypto to check out 😢
    let mut ciphertext = Vec::new();
    let len = stream
        .read_to_end(&mut ciphertext)
        .await
        .expect("streamed attachment");

    tracing::info!("Downloaded {} bytes", len);

    let mut key = [0u8; 64];
    key.copy_from_slice(&key_material);
    libsignal_service::attachment_cipher::decrypt_in_place(key, &mut ciphertext)
        .expect("attachment decryption");

    // Signal Desktop sometimes sends a JPEG image with .png extension,
    // so double check the received .png image, and rename it if necessary.
    opt.ext = opt.ext.to_lowercase();
    if opt.ext == "png" {
        tracing::trace!("Checking for JPEG with .png extension...");
        let classifier = MimeClassifier::new();
        let computed_type = classifier.classify(
            LoadContext::Image,
            NoSniffFlag::Off,
            ApacheBugFlag::Off,
            &None,
            &ciphertext as &[u8],
        );
        if computed_type == mime::IMAGE_JPEG {
            tracing::info!("Received JPEG file with .png suffix, fixing suffix");
            opt.ext = String::from("jpg");
        }
    }

    let attachment_id = storage.register_attachment(
        mid,
        // Reconstruct attachment pointer
        AttachmentPointer {
            content_type: Some(opt.mime_type),
            ..Default::default()
        },
    );

    let attachment_path = storage
        .save_attachment(attachment_id, dest, &opt.ext, &ciphertext)
        .await
        .unwrap();

    tracing::info!("Attachment stored at {:?}", attachment_path);
    tracing::info!("Attachment registered with message {}", msg);
    Ok(())
}
