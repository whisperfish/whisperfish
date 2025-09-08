use anyhow::Context;

use libsignal_protocol::DeviceId;
use libsignal_service::push_service::DEFAULT_DEVICE_ID;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use qttypes::QSettings;

#[derive(QObject)]
#[allow(non_snake_case, dead_code)]
pub struct SettingsBridge {
    base: qt_base_class!(trait QObject),

    // XXX
    // stringListSet: qt_method!(fn (&self, key: String, value: String)),
    // stringListValue: qt_method!(fn (&self, key: String, value: String)),
    avatarExists: qt_method!(fn(&self, key: String) -> bool),
    isPrimaryDevice: qt_method!(fn(&self) -> bool),

    inner: QSettings,

    debug_mode: qt_property!(bool; READ get_debug_mode WRITE set_debug_mode NOTIFY debug_mode_changed),
    enable_typing_indicators: qt_property!(bool; READ get_enable_typing_indicators WRITE set_enable_typing_indicators NOTIFY enable_typing_indicators_changed),
    enable_read_receipts: qt_property!(bool; READ get_enable_read_receipts WRITE set_enable_read_receipts NOTIFY enable_read_receipts_changed),
    enable_link_previews: qt_property!(bool; READ get_enable_link_previews WRITE set_enable_link_previews NOTIFY enable_link_previews_changed),
    notification_privacy: qt_property!(String; READ get_notification_privacy WRITE set_notification_privacy NOTIFY notification_privacy_changed),
    prefer_device_contacts: qt_property!(bool; READ get_prefer_device_contacts WRITE set_prefer_device_contacts NOTIFY prefer_device_contacts_changed),
    minimise_notify: qt_property!(bool; READ get_minimise_notify WRITE set_minimise_notify NOTIFY minimise_notify_changed),
    save_attachments: qt_property!(bool; READ get_save_attachments WRITE set_save_attachments NOTIFY save_attachments_changed),
    share_contacts: qt_property!(bool; READ get_share_contacts WRITE set_share_contacts NOTIFY share_contacts_changed),
    enable_enter_send: qt_property!(bool; READ get_enable_enter_send WRITE set_enable_enter_send NOTIFY enable_enter_send_changed),
    scale_image_attachments: qt_property!(bool; READ get_scale_image_attachments WRITE set_scale_image_attachments NOTIFY scale_image_attachments_changed),
    attachment_log: qt_property!(bool; READ get_attachment_log WRITE set_attachment_log NOTIFY attachment_log_changed),
    quit_on_ui_close: qt_property!(bool; READ get_quit_on_ui_close WRITE set_quit_on_ui_close NOTIFY quit_on_ui_close_changed),
    show_phone_number: qt_property!(bool; READ get_show_phone_number WRITE set_show_phone_number NOTIFY show_phone_number_changed),
    share_phone_number: qt_property!(bool; READ get_share_phone_number WRITE set_share_phone_number NOTIFY share_phone_number_changed),
    transcribe_voice_notes: qt_property!(bool; READ get_transcribe_voice_notes WRITE set_transcribe_voice_notes NOTIFY transcribe_voice_notes_changed),

    // These will be mirrored to `config.yml` at Whisperfish exit
    verbose: qt_property!(bool; READ get_verbose WRITE set_verbose NOTIFY verbose_changed),

    country_code: qt_property!(String; READ get_country_code WRITE set_country_code NOTIFY country_code_changed),
    avatar_dir: qt_property!(String; READ get_avatar_dir WRITE set_avatar_dir NOTIFY avatar_dir_changed),
    attachment_dir: qt_property!(String; READ get_attachment_dir WRITE set_attachment_dir NOTIFY attachment_dir_changed),
    camera_dir: qt_property!(String; READ get_camera_dir WRITE set_camera_dir NOTIFY camera_dir_changed),
    voice_note_dir: qt_property!(String; READ get_voice_note_dir WRITE set_voice_note_dir NOTIFY voice_note_dir_changed),
    plaintext_password: qt_property!(String; READ get_plaintext_password WRITE set_plaintext_password NOTIFY plaintext_password_changed),

    // Update notification values
    last_version: qt_property!(String; READ get_last_version WRITE set_last_version NOTIFY last_version_changed),
    next_update_time: qt_property!(String; READ get_next_update_time WRITE set_next_update_time NOTIFY next_update_time_changed),

    debug_mode_changed: qt_signal!(value: bool),
    enable_typing_indicators_changed: qt_signal!(value: bool),
    enable_read_receipts_changed: qt_signal!(value: bool),
    enable_link_previews_changed: qt_signal!(value: bool),
    notification_privacy_changed: qt_signal!(value: String),
    prefer_device_contacts_changed: qt_signal!(value: bool),
    minimise_notify_changed: qt_signal!(value: bool),
    save_attachments_changed: qt_signal!(value: bool),
    share_contacts_changed: qt_signal!(value: bool),
    enable_enter_send_changed: qt_signal!(value: bool),
    scale_image_attachments_changed: qt_signal!(value: bool),
    attachment_log_changed: qt_signal!(value: bool),
    quit_on_ui_close_changed: qt_signal!(value: bool),
    show_phone_number_changed: qt_signal!(value: bool),
    share_phone_number_changed: qt_signal!(value: bool),

    verbose_changed: qt_signal!(value: bool),

    country_code_changed: qt_signal!(value: String),
    avatar_dir_changed: qt_signal!(value: String),
    attachment_dir_changed: qt_signal!(value: String),
    camera_dir_changed: qt_signal!(value: String),
    voice_note_dir_changed: qt_signal!(value: String),
    plaintext_password_changed: qt_signal!(value: String),
    transcribe_voice_notes_changed: qt_signal!(value: bool),
    last_version_changed: qt_signal!(value: String),
    next_update_time_changed: qt_signal!(value: String),
}

impl Default for SettingsBridge {
    fn default() -> Self {
        Self {
            base: Default::default(),

            avatarExists: Default::default(),
            isPrimaryDevice: Default::default(),

            inner: QSettings::from_path(
                dirs::config_dir()
                    .context("Could not get xdg config directory path")
                    .unwrap()
                    .join("be.rubdos")
                    .join("harbour-whisperfish")
                    .join("harbour-whisperfish.conf")
                    .to_str()
                    .unwrap(),
            ),

            debug_mode: false,
            enable_typing_indicators: false,
            enable_read_receipts: false,
            enable_link_previews: false,
            notification_privacy: "complete".into(),
            prefer_device_contacts: false,
            minimise_notify: false,
            save_attachments: true,
            share_contacts: true,
            enable_enter_send: false,
            scale_image_attachments: false,
            attachment_log: false,
            quit_on_ui_close: true,
            show_phone_number: true,
            share_phone_number: false,
            transcribe_voice_notes: false,

            verbose: false,

            country_code: Default::default(),
            avatar_dir: Default::default(),
            attachment_dir: Default::default(),
            camera_dir: Default::default(),
            voice_note_dir: Default::default(),
            plaintext_password: Default::default(),

            last_version: Default::default(),
            next_update_time: Default::default(),

            debug_mode_changed: Default::default(),
            enable_typing_indicators_changed: Default::default(),
            enable_read_receipts_changed: Default::default(),
            enable_link_previews_changed: Default::default(),
            notification_privacy_changed: Default::default(),
            prefer_device_contacts_changed: Default::default(),
            minimise_notify_changed: Default::default(),
            save_attachments_changed: Default::default(),
            share_contacts_changed: Default::default(),
            enable_enter_send_changed: Default::default(),
            scale_image_attachments_changed: Default::default(),
            attachment_log_changed: Default::default(),
            quit_on_ui_close_changed: Default::default(),
            country_code_changed: Default::default(),
            avatar_dir_changed: Default::default(),
            attachment_dir_changed: Default::default(),
            camera_dir_changed: Default::default(),
            voice_note_dir_changed: Default::default(),
            plaintext_password_changed: Default::default(),
            show_phone_number_changed: Default::default(),
            share_phone_number_changed: Default::default(),
            transcribe_voice_notes_changed: Default::default(),

            verbose_changed: Default::default(),

            last_version_changed: Default::default(),
            next_update_time_changed: Default::default(),
        }
    }
}

impl Drop for SettingsBridge {
    fn drop(&mut self) {
        {
            // XXX Do we need to sync manually here?
            // Maybe this is implemented in qtypes?
            self.inner.sync();
        }
    }
}

impl SettingsBridge {
    fn contains(&self, key: &str) -> bool {
        self.inner.contains(key)
    }

    fn value_bool(&self, key: &str) -> bool {
        self.inner.value_bool(key)
    }

    pub fn set_bool(&mut self, key: &str, value: bool) {
        self.inner.set_bool(key, value);
    }

    pub fn set_bool_if_unset(&mut self, key: &str, value: bool) {
        if !self.contains(key) {
            self.set_bool(key, value);
        }
    }

    fn value_string(&self, key: &str) -> String {
        self.inner.value_string(key)
    }

    pub fn set_string(&mut self, key: &str, value: &str) {
        self.inner.set_string(key, value);
    }

    pub fn set_string_if_unset(&mut self, key: &str, value: &str) {
        if !self.contains(key) {
            self.set_string(key, value);
        }
    }

    pub fn get_notification_privacy(&self) -> String {
        let np = self.get_string("notification_privacy");
        match np.as_ref() {
            "" => {
                tracing::debug!("Fallback notification_privacy setting onto legacy options");
                let enable = self.get_bool("enable_notify");
                let show_message = self.get_bool("show_notify_message");
                match (enable, show_message) {
                    (false, _) => "off".into(),
                    (true, false) => "sender-only".into(),
                    (true, true) => "complete".into(),
                }
            }
            "off" | "minimal" | "sender-only" | "complete" => np,
            _ => {
                tracing::warn!(
                    "Unrecognised notification privacy setting {}, reverting to off",
                    np
                );
                "off".into()
            }
        }
    }

    pub fn get_debug_mode(&self) -> bool {
        self.get_bool("debug_mode")
    }

    pub fn get_enable_typing_indicators(&self) -> bool {
        self.get_bool("enable_typing_indicators")
    }

    pub fn get_enable_read_receipts(&self) -> bool {
        self.get_bool("enable_read_receipts")
    }

    pub fn get_enable_link_previews(&self) -> bool {
        self.get_bool("enable_link_previews")
    }

    pub fn get_prefer_device_contacts(&self) -> bool {
        self.get_bool("prefer_device_contacts")
    }

    pub fn get_minimise_notify(&self) -> bool {
        self.get_bool("minimise_notify")
    }

    pub fn get_save_attachments(&self) -> bool {
        self.get_bool("save_attachments")
    }

    pub fn get_share_contacts(&self) -> bool {
        self.get_bool("share_contacts")
    }

    pub fn get_enable_enter_send(&self) -> bool {
        self.get_bool("enable_enter_send")
    }

    pub fn get_scale_image_attachments(&self) -> bool {
        self.get_bool("scale_image_attachments")
    }

    pub fn get_attachment_log(&self) -> bool {
        self.get_bool("attachment_log")
    }

    pub fn get_quit_on_ui_close(&self) -> bool {
        self.get_bool("quit_on_ui_close")
    }

    pub fn get_show_phone_number(&self) -> bool {
        self.get_bool("show_phone_number")
    }

    pub fn get_share_phone_number(&self) -> bool {
        self.get_bool("share_phone_number")
    }

    pub fn get_transcribe_voice_notes(&self) -> bool {
        self.get_bool("transcribe_voice_notes")
    }

    pub fn get_verbose(&self) -> bool {
        self.get_bool("verbose")
    }

    pub fn get_avatar_dir(&self) -> String {
        self.get_string("avatar_dir")
    }

    pub fn get_country_code(&self) -> String {
        self.get_string("country_code")
    }

    pub fn get_attachment_dir(&self) -> String {
        self.get_string("attachment_dir")
    }

    pub fn get_camera_dir(&self) -> String {
        self.get_string("camera_dir")
    }

    pub fn get_voice_note_dir(&self) -> String {
        self.get_string("voice_note_dir")
    }

    pub fn get_plaintext_password(&self) -> String {
        self.get_string("plaintext_password")
    }

    pub fn get_last_version(&self) -> String {
        self.get_string("last_version")
    }

    pub fn get_next_update_time(&self) -> String {
        self.get_string("next_update_time")
    }

    pub fn set_notification_privacy(&mut self, value: String) {
        self.set_string("notification_privacy", &value);
        self.notification_privacy_changed(value);
    }

    pub fn set_debug_mode(&mut self, value: bool) {
        self.set_bool("debug_mode", value);
        self.debug_mode_changed(value);
    }

    pub fn set_enable_typing_indicators(&mut self, value: bool) {
        self.set_bool("enable_typing_indicators", value);
        self.enable_typing_indicators_changed(value);
    }

    pub fn set_enable_read_receipts(&mut self, value: bool) {
        self.set_bool("enable_read_receipts", value);
        self.enable_read_receipts_changed(value);
    }

    pub fn set_enable_link_previews(&mut self, value: bool) {
        self.set_bool("enable_link_previews", value);
        self.enable_link_previews_changed(value);
    }

    pub fn set_prefer_device_contacts(&mut self, value: bool) {
        self.set_bool("prefer_device_contacts", value);
        self.prefer_device_contacts_changed(value);
    }

    pub fn set_minimise_notify(&mut self, value: bool) {
        self.set_bool("minimise_notify", value);
        self.minimise_notify_changed(value);
    }

    pub fn set_save_attachments(&mut self, value: bool) {
        self.set_bool("save_attachments", value);
        self.save_attachments_changed(value);
    }

    pub fn set_share_contacts(&mut self, value: bool) {
        self.set_bool("share_contacts", value);
        self.share_contacts_changed(value);
    }

    pub fn set_enable_enter_send(&mut self, value: bool) {
        self.set_bool("enable_enter_send", value);
        self.enable_enter_send_changed(value);
    }

    pub fn set_scale_image_attachments(&mut self, value: bool) {
        self.set_bool("scale_image_attachments", value);
        self.scale_image_attachments_changed(value);
    }

    pub fn set_attachment_log(&mut self, value: bool) {
        self.set_bool("attachment_log", value);
        self.attachment_log_changed(value);
    }

    pub fn set_quit_on_ui_close(&mut self, value: bool) {
        self.set_bool("quit_on_ui_close", value);
        self.quit_on_ui_close_changed(value);
    }

    pub fn set_show_phone_number(&mut self, value: bool) {
        self.set_bool("show_phone_number", value);
        self.show_phone_number_changed(value);
    }

    pub fn set_share_phone_number(&mut self, value: bool) {
        self.set_bool("share_phone_number", value);
        self.share_phone_number_changed(value);
    }

    pub fn set_transcribe_voice_notes(&mut self, value: bool) {
        self.set_bool("transcribe_voice_notes", value);
        self.transcribe_voice_notes_changed(value);
    }

    pub fn set_verbose(&mut self, value: bool) {
        self.set_bool("verbose", value);
        self.verbose_changed(value);
    }

    pub fn set_country_code(&mut self, value: String) {
        self.set_string("country_code", &value);
        self.country_code_changed(value);
    }

    pub fn set_avatar_dir(&mut self, value: String) {
        self.set_string("avatar_dir", &value);
        self.avatar_dir_changed(value);
    }

    pub fn set_attachment_dir(&mut self, value: String) {
        self.set_string("attachment_dir", &value);
        self.attachment_dir_changed(value);
    }

    pub fn set_camera_dir(&mut self, value: String) {
        self.set_string("camera_dir", &value);
        self.camera_dir_changed(value);
    }

    pub fn set_voice_note_dir(&mut self, value: String) {
        self.set_string("voice_note_dir", &value);
        self.voice_note_dir_changed(value);
    }

    pub fn set_plaintext_password(&mut self, value: String) {
        self.set_string("plaintext_password", &value);
        self.plaintext_password_changed(value);
    }

    pub fn set_last_version(&mut self, value: String) {
        self.set_string("last_version", &value);
        self.last_version_changed(value);
    }

    pub fn set_next_update_time(&mut self, value: String) {
        self.set_string("next_update_time", &value);
        self.next_update_time_changed(value);
    }

    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn avatarExists(&mut self, uuid: String) -> bool {
        self.avatar_exists(uuid)
    }

    #[allow(non_snake_case)]
    fn isPrimaryDevice(&mut self) -> bool {
        // XXX There must be easier way to access current device id...
        let config = whisperfish_store::config::SignalConfig::read_from_file();
        if config.is_ok() {
            let config = config.unwrap();
            config.get_device_id() == DeviceId::from(DEFAULT_DEVICE_ID)
        } else {
            false
        }
    }

    pub fn defaults(&mut self) {
        tracing::info!("Setting default settings.");

        self.set_bool_if_unset("debug_mode", false);
        self.set_bool_if_unset("enable_notify", true);
        self.set_bool_if_unset("enable_typing_indicators", false);
        self.set_bool_if_unset("enable_read_receipts", false);
        self.set_bool_if_unset("enable_link_previews", false);
        self.set_bool_if_unset("show_notify_message", false);
        self.set_bool_if_unset("prefer_device_contacts", false);
        self.set_bool_if_unset("minimise_notify", false);
        self.set_bool_if_unset("save_attachments", true);
        self.set_bool_if_unset("share_contacts", true);
        self.set_bool_if_unset("enable_enter_send", false);
        self.set_bool_if_unset("scale_image_attachments", false);
        self.set_bool_if_unset("attachment_log", false);
        self.set_bool_if_unset("quit_on_ui_close", true);
        self.set_bool_if_unset("transcribe_voice_notes", false);
        self.set_string_if_unset("country_code", "");
        self.set_string_if_unset(
            "avatar_dir",
            // XXX this has to be adapted to current config struct
            &crate::config::SignalConfig::default()
                .get_avatar_dir()
                .to_string_lossy(),
        );
        self.set_string_if_unset(
            "attachment_dir",
            // XXX this has to be adapted to current config struct
            &crate::config::SignalConfig::default()
                .default_attachment_dir()
                .to_string_lossy(),
        );
        self.set_string_if_unset(
            "camera_dir",
            // XXX this has to be adapted to current config struct
            &crate::config::SignalConfig::default()
                .default_camera_dir()
                .to_string_lossy(),
        );
        self.set_string_if_unset(
            "voice_note_dir",
            // XXX this has to be adapted to current config struct
            &crate::config::SignalConfig::default()
                .default_voice_note_dir()
                .to_string_lossy(),
        );
    }

    pub fn get_string(&self, key: impl AsRef<str>) -> String {
        self.value_string(key.as_ref())
    }

    pub fn get_bool(&self, key: impl AsRef<str>) -> bool {
        self.value_bool(key.as_ref())
    }

    pub fn avatar_exists(&self, uuid: impl AsRef<str>) -> bool {
        crate::config::SignalConfig::default()
            .get_avatar_dir()
            .join(uuid.as_ref())
            .exists()
    }

    pub fn migrate_qsettings_paths(&mut self) {
        let old_path = ".local/share/harbour-whisperfish";
        let new_path = ".local/share/be.rubdos/harbour-whisperfish";
        let keys = ["attachment_dir", "camera_dir", "voice_note_dir"];
        for key in keys.iter() {
            if self.inner.contains(key) {
                self.inner.set_string(
                    key,
                    self.inner
                        .value_string(key)
                        .to_string()
                        .replace(old_path, new_path)
                        .as_str(),
                );
            }
        }
    }

    pub fn migrate_qsettings() -> Result<(), anyhow::Error> {
        let config_dir = dirs::config_dir().expect("No config directory found");

        let old_path = config_dir.join("harbour-whisperfish");

        let new_path = config_dir.join("be.rubdos").join("harbour-whisperfish");

        let old_file = &old_path.join("harbour-whisperfish.conf");
        let new_file = &new_path.join("harbour-whisperfish.conf");

        if new_file.exists() {
            return Ok(());
        }

        if !new_path.exists() {
            eprintln!("Creating new config path...");
            std::fs::create_dir_all(&new_path)?;
        }

        if !old_file.exists() {
            if !new_file.exists() {
                eprintln!("Creating empty QSettings file...");
                std::fs::File::create(new_file)?;
                return Ok(());
            }
            eprintln!("Old QSettings file doesn't exist, migration not needed");
            return Ok(());
        }

        // Sailjail mounts the old and new paths separately, which makes
        // std::fs::rename fail. That means we have to use copy-and-delete.
        eprintln!("Migrating old QSettings file...");
        std::fs::copy(old_file, new_file)?;
        std::fs::remove_file(old_file)?;
        eprintln!("QSettings file migrated");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::{Read, Write};

    #[test]
    fn settings_integration_smoke_tests() {
        qmeta_async::run(|| {
            let temp_dir = tempfile::tempdir().unwrap();
            let settings_pathbuf = temp_dir.path().join("qsettings.conf");
            let settings_file = settings_pathbuf.to_str().unwrap();

            let mut file = File::create(settings_file).unwrap();
            file.write_all(b"[General]\n").unwrap();
            file.write_all(b"test_bool=true\n").unwrap();
            file.write_all(b"test_string=Hello world\n").unwrap();
            drop(file);

            // Can't use ..Default::default() because not everything is Copy
            let mut settings = SettingsBridge::default();
            settings.inner = QSettings::from_path(settings_file);

            assert!(settings.get_bool("test_bool"));
            assert_eq!(
                settings.get_string("test_string"),
                "Hello world".to_string()
            );

            settings.set_bool("test_bool", false);
            settings.set_string("test_string", "Hello Qt");
            drop(settings);

            let mut file = File::open(settings_file).unwrap();
            let mut content = String::new();
            file.read_to_string(&mut content).unwrap();
            assert!(content.contains("test_bool=false"));
            assert!(content.contains("test_string=Hello Qt"));

            drop(temp_dir);
            assert!(!settings_pathbuf.as_path().exists());
        })
        .unwrap();
    }

    #[test]
    fn qsettings_path_migration() {
        qmeta_async::run(|| {
            let temp_dir = tempfile::tempdir().unwrap();
            let settings_pathbuf = temp_dir.path().join("qsettings.conf");
            let settings_file = settings_pathbuf.to_str().unwrap();

            let mut file = File::create(settings_file).unwrap();
            file.write_all(b"[General]\n").unwrap();
            file.write_all(b"attachment_dir=/x/.local/share/harbour-whisperfish/a\n")
                .unwrap();
            file.write_all(b"camera_dir=/x/.local/share/harbour-whisperfish/c\n")
                .unwrap();
            drop(file);

            // Can't use ..Default::default() because not everything is Copy
            let mut settings = SettingsBridge::default();
            settings.inner = QSettings::from_path(settings_file);

            assert_eq!(
                settings.get_string("attachment_dir"),
                "/x/.local/share/harbour-whisperfish/a"
            );
            assert_eq!(
                settings.get_string("camera_dir"),
                "/x/.local/share/harbour-whisperfish/c"
            );

            settings.migrate_qsettings_paths();

            assert_eq!(
                settings.get_string("attachment_dir"),
                "/x/.local/share/be.rubdos/harbour-whisperfish/a"
            );
            assert_eq!(
                settings.get_string("camera_dir"),
                "/x/.local/share/be.rubdos/harbour-whisperfish/c"
            );

            // Triggers QSettings::sync()
            drop(settings);

            let mut file = File::open(settings_file).unwrap();
            let mut content = String::new();
            file.read_to_string(&mut content).unwrap();
            assert!(
                content.contains("attachment_dir=/x/.local/share/be.rubdos/harbour-whisperfish/a")
            );
            assert!(content.contains("camera_dir=/x/.local/share/be.rubdos/harbour-whisperfish/c"));
            drop(file);

            drop(temp_dir);
            assert!(!settings_pathbuf.exists());
        })
        .unwrap();
    }
}
