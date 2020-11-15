use std::rc::Rc;

use failure::*;
use qmetaobject::*;

use crate::gui::WhisperfishApp;
use crate::settings::SignalConfig;
use crate::store::{self, Storage};

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct SetupWorker {
    base: qt_base_class!(trait QObject),

    registrationSuccess: qt_signal!(),
    invalidDatastore: qt_signal!(),
    invalidPhoneNumber: qt_signal!(),
    clientFailed: qt_signal!(),
    setupComplete: qt_signal!(),

    requiresCaptcha: qt_signal!(),

    phoneNumber: qt_property!(QString; NOTIFY setupChanged),
    registered: qt_property!(bool; NOTIFY setupChanged),
    locked: qt_property!(bool; NOTIFY setupChanged),
    encryptedKeystore: qt_property!(bool; NOTIFY setupChanged),
    localId: qt_property!(QString; NOTIFY setupChanged),
    identity: qt_property!(QString; NOTIFY setupChanged),

    useVoice: qt_property!(bool; NOTIFY setupChanged),

    /// Emitted when any of the properties change.
    setupChanged: qt_signal!(),

    pub config: Option<SignalConfig>,

    /// Emitted when captcha has been filed
    captchaFiled: qt_method!(fn(&mut self)),
    captcha_filed_listeners: Vec<futures::channel::oneshot::Sender<()>>,
}

impl SetupWorker {
    #[allow(non_snake_case)]
    fn captchaFiled(&mut self) {
        for listener in self.captcha_filed_listeners.drain(..) {
            if let Err(_) = listener.send(()) {
                log::warn!("Request for captcha fulfilled, but nobody listens.");
            }
        }
    }

    pub async fn run(app: Rc<WhisperfishApp>) {
        log::info!("SetupWorker::run");
        let this = app.setup_worker.pinned();

        let identity_path = crate::store::default_location()
            .unwrap()
            .join("storage")
            .join("identity")
            .join("identity_key");

        // Check registration
        if identity_path.is_file() {
            log::info!("identity_key found, assuming registered");
            this.borrow_mut().registered = true;
        } else {
            log::info!("identity_key not found");
            this.borrow_mut().registered = false;
        }
        this.borrow().setupChanged();

        this.borrow_mut().config = match SetupWorker::read_config(app.clone()).await {
            Ok(config) => Some(config),
            Err(e) => {
                log::error!("Error reading config: {:?}", e);
                this.borrow().clientFailed();
                return;
            }
        };

        let whisperfish_config_file = Self::conf_dir().join("harbour-whisperfish.conf");
        if !whisperfish_config_file.exists() {
            app.settings.pinned().borrow_mut().defaults();
        }

        let config = this.borrow().config.as_ref().unwrap().clone();

        log::debug!("config: {:?}", config);
        // XXX: nice formatting?
        this.borrow_mut().phoneNumber = config.tel.unwrap_or("".into()).into();

        if !this.borrow().registered {
            if let Err(e) = SetupWorker::register(app.clone()).await {
                log::error!("Error in registration: {}", e);
                this.borrow().clientFailed();
                return;
            }
            this.borrow_mut().registered = true;
            this.borrow().setupChanged();
        } else {
            // Open storage
            if let Err(e) = SetupWorker::setup_storage(app.clone()).await {
                log::error!("Error setting up storage: {}", e);
                this.borrow().clientFailed();
                return;
            }
        }

        app.storage_ready().await;

        this.borrow().setupChanged();
    }

    fn conf_dir() -> std::path::PathBuf {
        let conf_dir = dirs::config_dir()
            .ok_or(format_err!("Could not find config directory."))
            .unwrap()
            .join("harbour-whisperfish");

        if !conf_dir.exists() {
            std::fs::create_dir(&conf_dir).unwrap();
        }

        conf_dir
    }

    async fn read_config(app: Rc<WhisperfishApp>) -> Result<SignalConfig, Error> {
        let signal_config_file = Self::conf_dir().join("config.yml");

        if let Ok(file) = std::fs::File::open(&signal_config_file) {
            Ok(serde_yaml::from_reader(file)?)
        } else {
            let contents = SignalConfig {
                tel: None,
                uuid: None,
                // XXX
                server: None,
                root_ca: None,
                proxy_server: None,
                verification_type: "voice".into(),
                storage_dir: "".into(),
                unencrypted_storage: false,
                storage_password: "".into(),
                log_level: "debug".into(),
                user_agent: "Whisperfish".into(),
                always_trust_peer_id: false,
            };
            Self::write_config(app, &contents).await?;
            Ok(contents)
        }
    }

    async fn write_config(_app: Rc<WhisperfishApp>, contents: &SignalConfig) -> Result<(), Error> {
        let signal_config_file = Self::conf_dir().join("config.yml");
        let file = std::fs::File::create(signal_config_file)?;
        serde_yaml::to_writer(file, &contents)?;
        Ok(())
    }

    async fn setup_storage(app: Rc<WhisperfishApp>) -> Result<(), Error> {
        let settings = app.settings.pinned();

        let storage = if settings.borrow().get_bool("encrypt_database") {
            let password: String = app
                .prompt
                .pinned()
                .borrow_mut()
                .ask_password()
                .await
                .ok_or_else(|| format_err!("No password provided"))?
                .into();

            Storage::open_with_password(&store::default_location()?, password).await?
        } else {
            Storage::open(&store::default_location()?)?
        };

        *app.storage.borrow_mut() = Some(storage);

        Ok(())
    }

    async fn register(app: Rc<WhisperfishApp>) -> Result<(), Error> {
        let this = app.setup_worker.pinned();

        let storage_password: String = app
            .prompt
            .pinned()
            .borrow_mut()
            .ask_password()
            .await
            .ok_or(format_err!("No password code provided"))?
            .into();

        let number = loop {
            let number: String = app
                .prompt
                .pinned()
                .borrow_mut()
                .ask_phone_number()
                .await
                .ok_or(format_err!("No phone number provided"))?
                .into();

            match phonenumber::parse(None, number) {
                Ok(number) => break number,
                Err(e) => {
                    log::warn!("Could not parse phone number: {}", e);
                    this.borrow().invalidPhoneNumber();
                }
            }
        };

        let e164 = number.format().mode(phonenumber::Mode::E164).to_string();
        log::info!("E164: {}", e164);
        this.borrow_mut().phoneNumber = e164.clone().into();

        // generate a random 24 bytes password
        use rand::distributions::Alphanumeric;
        use rand::{Rng, RngCore};
        let mut rng = rand::thread_rng();
        let password: String = rng.sample_iter(&Alphanumeric).take(24).collect();

        while app
            .client_actor
            .send(super::client::Register {
                e164: e164.clone(),
                password: password.clone(),
                use_voice: this.borrow().useVoice,
            })
            .await??
            == super::client::RegistrationResponse::CaptchaRequired
        {
            log::info!("Captcha required");
            let (tx, rx) = futures::channel::oneshot::channel();
            this.borrow_mut().captcha_filed_listeners.push(tx);
            this.borrow_mut().requiresCaptcha();
            rx.await?;
        }

        let code: String = app
            .prompt
            .pinned()
            .borrow_mut()
            .ask_verification_code()
            .await
            .ok_or(format_err!("No verification code provided"))?
            .into();
        let code = code.parse()?;

        let mut signaling_key = [0u8; 52];
        rng.fill_bytes(&mut signaling_key);
        let signaling_key = signaling_key;

        let (regid, res) = app
            .client_actor
            .send(super::client::ConfirmRegistration {
                e164: e164.clone(),
                password: password.clone(),
                confirm_code: code,
                signaling_key,
            })
            .await??;

        log::info!("Registration result: {:?}", res);

        let mut this = this.borrow_mut();
        let mut cfg = this.config.as_mut().unwrap();
        cfg.uuid = Some(res.uuid);
        cfg.tel = Some(e164);
        Self::write_config(app.clone(), cfg).await?;

        // Install storage
        let storage = Storage::new_with_password(
            &store::default_location()?,
            &storage_password,
            regid,
            &password,
            signaling_key,
        )
        .await?;
        *app.storage.borrow_mut() = Some(storage);

        Ok(())
    }
}
