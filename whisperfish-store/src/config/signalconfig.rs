use anyhow::Context;
use libsignal_service::{protocol::DeviceId, ServiceAddress};
use phonenumber::PhoneNumber;
use uuid::Uuid;

mod phonenumber_serde_e164 {
    use std::{fmt, sync::Mutex};

    use serde::{Deserializer, Serializer};

    use super::*;

    pub fn serialize<S>(p: &Mutex<Option<PhoneNumber>>, s: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match p.lock().expect("mutex alive").as_ref() {
            Some(pn) => s.serialize_str(&pn.to_string()),
            None => s.serialize_str(""),
        }
    }

    pub fn deserialize<'de, D>(d: D) -> Result<Mutex<Option<PhoneNumber>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct PhoneNumberStringVisitor;
        impl<'de> serde::de::Visitor<'de> for PhoneNumberStringVisitor {
            type Value = Option<PhoneNumber>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a string containing a phone number")
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                // unfortunately we lose some typed information
                // from errors deserializing the json string
                if v.is_empty() {
                    Ok(None)
                } else {
                    phonenumber::parse(None, v).map_err(E::custom).map(Some)
                }
            }
        }

        d.deserialize_any(PhoneNumberStringVisitor).map(Mutex::new)
    }
}

/// Global Config
///
/// This struct holds the global configuration of the whisperfish app.
#[derive(serde::Serialize, serde::Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub struct SignalConfig {
    /// Our telephone number. This field is changed in threads and thus has to be Send/Sync but
    /// mutable at the same time.
    #[serde(with = "phonenumber_serde_e164")]
    tel: std::sync::Mutex<Option<PhoneNumber>>,
    /// Our ACI. This field is changed in threads and thus has to be Send/Sync but mutable at the
    /// same time.
    uuid: std::sync::Mutex<Option<Uuid>>,
    /// Our PNI. This field is changed in threads and thus has to be Send/Sync but mutable at the
    /// same time.
    pni: std::sync::Mutex<Option<Uuid>>,
    /// Same goes for the device id
    device_id: std::sync::Mutex<u32>,
    /// Directory for persistent share files
    // XXX share dir is an ugly name, use another one
    // XXX we don't (de-)serialize this field as there is another instance that is accessing the
    // default path in `settings.rs`. As long as `settings.rs` is not accessing this struct, we
    // cannot set this path by a config file.
    #[serde(skip)]
    share_dir: std::path::PathBuf,
    /// Verbosity of the logging messages
    pub verbose: bool,
    /// Enable writing to log file
    pub logfile: bool,
    /// Enable Tokio console debugging and Tracy support
    pub tracing: bool,
    /// Whether whisperfish was automatically started (probably systemd) or by the user. We do not
    /// want to serialize this field to the config file. This config is only set by command line
    /// arguments.
    #[serde(skip)]
    pub autostart: bool,

    #[serde(skip)]
    pub override_captcha: Option<String>,
}

impl Default for SignalConfig {
    fn default() -> Self {
        let path =
            crate::store::default_location().expect("Could not get xdg share directory path");

        Self {
            tel: std::sync::Mutex::new(None),
            uuid: std::sync::Mutex::new(None),
            pni: std::sync::Mutex::new(None),
            device_id: std::sync::Mutex::new(libsignal_service::push_service::DEFAULT_DEVICE_ID),
            share_dir: path.to_path_buf(),
            verbose: false,
            logfile: false,
            tracing: false,
            autostart: false,
            override_captcha: None,
        }
    }
}

impl SignalConfig {
    pub fn migrate_config() -> Result<(), anyhow::Error> {
        let config_dir = dirs::config_dir().expect("No config directory found");

        let old_path = config_dir.join("harbour-whisperfish");

        let new_path = config_dir.join("be.rubdos").join("harbour-whisperfish");

        let old_file = &old_path.join("config.yml");
        let new_file = &new_path.join("config.yml");

        if new_file.exists() {
            return Ok(());
        }

        if !new_path.exists() {
            eprintln!("Creating new config path...");
            std::fs::create_dir_all(&new_path)?;
        }

        if !old_file.exists() {
            if !new_file.exists() {
                eprintln!("Creating empty config file...");
                std::fs::File::create(new_file)?;
                return Ok(());
            }
            eprintln!("Old config doesn't exist, migration not needed");
            return Ok(());
        }

        // Sailjail mounts the old and new paths separately, which makes
        // std::fs::rename fail. That means we have to use copy-and-delete.
        eprintln!("Migrating old config file...");
        std::fs::copy(old_file, new_file)?;
        std::fs::remove_file(old_file)?;
        eprintln!("Config file migrated");
        Ok(())
    }

    pub fn read_from_file() -> Result<Self, anyhow::Error> {
        let path = dirs::config_dir()
            .context("Could not get xdg config directory path")?
            .join("be.rubdos")
            .join("harbour-whisperfish")
            .join("config.yml");

        let fd = std::fs::File::open(&path)
            .with_context(|| format!("Could not open config file: {}", &path.display()))?;
        let ret = serde_yaml::from_reader(fd)
            .with_context(|| format!("Could not read config file: {}", &path.display()))?;

        Ok(ret)
    }

    pub fn write_to_file(&self) -> Result<(), anyhow::Error> {
        let path = dirs::config_dir()
            // XXX use anyhow context here
            .expect("No config directory found")
            .join("be.rubdos")
            .join("harbour-whisperfish");

        // create config directory if it does not exist
        if !path.exists() {
            std::fs::create_dir_all(&path).with_context(|| {
                format!("Could not create config directory: {}", &path.display())
            })?;
        }

        // write to config file
        let path = path.join("config.yml");
        let fd = std::fs::File::create(&path)
            .with_context(|| format!("Could not open config file to write: {}", &path.display()))?;
        serde_yaml::to_writer(fd, &self)
            .with_context(|| format!("Could not write config file: {}", &path.display()))?;

        Ok(())
    }

    pub fn get_share_dir(&self) -> std::path::PathBuf {
        self.share_dir.to_owned()
    }

    pub fn get_avatar_dir(&self) -> std::path::PathBuf {
        self.share_dir.join("storage").join("avatars")
    }

    pub fn attachments_regex(&self) -> regex::Regex {
        regex::Regex::new(r"~/\.local/share(/be.rubdos)?/harbour-whisperfish/storage/(attachments|camera)/([a-zA-Z0-9_-]+\.[a-zA-Z0-9]+)").unwrap()
    }

    pub fn default_attachment_dir(&self) -> std::path::PathBuf {
        self.share_dir.join("storage").join("attachments")
    }

    pub fn default_camera_dir(&self) -> std::path::PathBuf {
        self.share_dir.join("storage").join("camera")
    }

    pub fn default_voice_note_dir(&self) -> std::path::PathBuf {
        self.share_dir.join("storage").join("voice_notes")
    }

    pub fn get_identity_dir(&self) -> std::path::PathBuf {
        self.share_dir
            .join("storage")
            .join("identity")
            .join("identity_key")
    }

    pub fn get_tel(&self) -> Option<PhoneNumber> {
        self.tel.lock().unwrap().clone()
    }

    pub fn get_aci(&self) -> Option<Uuid> {
        *self.uuid.lock().unwrap()
    }

    pub fn get_pni(&self) -> Option<Uuid> {
        *self.pni.lock().unwrap()
    }

    /// Create ServiceAddress from Whisperfish user. Prefers ACI over PNI.
    pub fn get_addr(&self) -> Option<ServiceAddress> {
        self.get_aci()
            .map(ServiceAddress::new_aci)
            .or(self.get_pni().map(ServiceAddress::new_pni))
    }

    pub fn get_device_id(&self) -> DeviceId {
        DeviceId::from(*self.device_id.lock().unwrap())
    }

    pub fn set_tel(&self, tel: PhoneNumber) {
        *self.tel.lock().unwrap() = Some(tel);
    }

    pub fn set_aci(&self, uuid: Uuid) {
        *self.uuid.lock().unwrap() = Some(uuid);
    }

    pub fn set_pni(&self, uuid: Uuid) {
        *self.pni.lock().unwrap() = Some(uuid);
    }

    pub fn set_device_id(&self, id: u32) {
        *self.device_id.lock().unwrap() = id;
    }
}
