use crate::gui::WhisperfishApp;
use crate::store::Storage;
use crate::store::TrustLevel;
use anyhow::Context;
use libsignal_service::prelude::MasterKey;
use libsignal_service::prelude::StorageServiceKey;
use libsignal_service::protocol::{self, Aci};
use libsignal_service::push_service::{ServiceIds, VerificationTransport, DEFAULT_DEVICE_ID};
use phonenumber::PhoneNumber;
use qmetaobject::prelude::*;
use std::rc::Rc;
use std::sync::Arc;
use uuid::Uuid;

pub struct RegistrationResult {
    storage: Storage,

    phonenumber: PhoneNumber,
    service_ids: ServiceIds,
    device_id: protocol::DeviceId,
    profile_key: Option<[u8; 32]>,
}

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct SetupWorker {
    base: qt_base_class!(trait QObject),

    registrationSuccess: qt_signal!(),
    invalidDatastore: qt_signal!(),
    invalidPhoneNumber: qt_signal!(),
    clientFailed: qt_signal!(),
    setupComplete: qt_signal!(),

    phoneNumberInner: Option<PhoneNumber>,
    phoneNumber: qt_property!(QString; READ get_phone_number NOTIFY setupChanged),
    uuidInner: Option<Uuid>,
    uuid: qt_property!(QString; READ get_uuid NOTIFY setupChanged ALIAS uuid),
    pniInner: Option<Uuid>,
    pni: qt_property!(QString; READ get_pni NOTIFY setupChanged ALIAS pni),
    deviceId: qt_property!(u32; NOTIFY setupChanged),

    registered: qt_property!(bool; NOTIFY setupChanged),
    locked: qt_property!(bool; NOTIFY setupChanged),
    localId: qt_property!(QString; NOTIFY setupChanged),

    useVoice: qt_property!(bool; NOTIFY setupChanged),

    callingSupported: qt_property!(bool; READ calling_supported NOTIFY setupChanged),

    /// Emitted when any of the properties change.
    setupChanged: qt_signal!(),
}

impl SetupWorker {
    const MAX_PASSWORD_ENTER_ATTEMPTS: i8 = 3;

    #[tracing::instrument(skip(app, config))]
    pub async fn run(app: Rc<WhisperfishApp>, config: std::sync::Arc<crate::config::SignalConfig>) {
        let this = app.setup_worker.pinned();

        // Check registration
        if config.get_identity_dir().is_file() {
            tracing::info!("identity_key found, assuming registered");
            this.borrow_mut().registered = true;
        } else {
            tracing::info!("identity_key not found");
            this.borrow_mut().registered = false;
        }
        this.borrow().setupChanged();

        // Defaults does not override unset settings
        app.settings_bridge.pinned().borrow_mut().defaults();

        // XXX: nice formatting?
        this.borrow_mut().phoneNumberInner = config.get_tel();
        this.borrow_mut().uuidInner = config.get_aci();
        this.borrow_mut().pniInner = config.get_pni();
        this.borrow_mut().deviceId = config.get_device_id().into();

        if !this.borrow().registered {
            if let Err(e) = SetupWorker::register(app.clone(), config.clone()).await {
                tracing::error!("Error in registration: {}", e);
                this.borrow().clientFailed();
                return;
            }
            this.borrow_mut().registered = true;
            this.borrow().setupChanged();

            if let Err(e) = config.write_to_file() {
                tracing::error!("Error writing config file: {}", e);
                this.borrow().clientFailed();
                return;
            }
        } else if let Err(e) = SetupWorker::setup_storage(app.clone(), config.clone()).await {
            tracing::error!("Error setting up storage: {}", e);
            this.borrow().clientFailed();
            return;
        }

        if config.get_aci().is_none() {
            tracing::error!("ACI UUID not set after registration or opening storage!");
            this.borrow().clientFailed();
            return;
        }

        app.storage_ready().await;
        app.app_state.pinned().borrow_mut().setMayExit(
            app.settings_bridge
                .pinned()
                .borrow()
                .get_bool("quit_on_ui_close"),
        );

        this.borrow().setupChanged();
        this.borrow().setupComplete();
    }

    async fn open_storage(
        app: Rc<WhisperfishApp>,
        config: Arc<crate::config::SignalConfig>,
    ) -> Result<Storage, anyhow::Error> {
        let res = Storage::open(
            config.clone(),
            &config.get_share_dir().to_owned().into(),
            None,
        )
        .await;
        if res.is_ok() {
            return res;
        }

        app.app_state
            .pinned()
            .borrow_mut()
            .activate_hidden_window(true);

        for i in 1..=SetupWorker::MAX_PASSWORD_ENTER_ATTEMPTS {
            let password: String = app
                .prompt
                .pinned()
                .borrow_mut()
                .ask_password()
                .await
                .context("No password provided")?
                .into();

            match Storage::open(
                config.clone(),
                &config.get_share_dir().to_owned().into(),
                Some(password),
            )
            .await
            {
                Ok(storage) => return Ok(storage),
                Err(error) => tracing::error!(
                    "Attempt {} of opening encrypted storage failed: {:?}",
                    i,
                    error
                ),
            }
        }

        tracing::error!("Error setting up storage: too many bad password attempts");
        res
    }

    async fn setup_storage(
        app: Rc<WhisperfishApp>,
        config: Arc<crate::config::SignalConfig>,
    ) -> Result<(), anyhow::Error> {
        let storage = SetupWorker::open_storage(app.clone(), config).await?;
        storage.reset_all_attachment_progress();

        app.app_state.pinned().borrow().set_storage(storage);

        Ok(())
    }

    async fn register(
        app: Rc<WhisperfishApp>,
        config: Arc<crate::config::SignalConfig>,
    ) -> Result<(), anyhow::Error> {
        let this = app.setup_worker.pinned();

        app.app_state
            .pinned()
            .borrow_mut()
            .activate_hidden_window(true);

        let storage_password: String = app
            .prompt
            .pinned()
            .borrow_mut()
            .ask_password()
            .await
            .context("No password code provided")?
            .into();

        let storage_password = if storage_password.is_empty() {
            None
        } else {
            Some(storage_password)
        };

        let is_primary: bool = app
            .prompt
            .pinned()
            .borrow_mut()
            .ask_registration_type()
            .await
            .context("No registration type chosen")?;

        // generate a random 24 bytes password
        use rand::distributions::Alphanumeric;
        use rand::Rng;
        let rng = rand::thread_rng();
        let password: String = rng
            .sample_iter(&Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();

        let reg = if is_primary {
            let result =
                SetupWorker::register_as_primary(app.clone(), &config, password, storage_password)
                    .await?;

            use libsignal_service::master_key::MasterKeyStore;

            // FIXME: tracing::info doesn't seem to work here - why?
            let master_key = MasterKey::generate(&mut rand::thread_rng());
            let storage_key = StorageServiceKey::from_master_key(&master_key);
            result.storage.store_master_key(Some(&master_key));
            result.storage.store_storage_service_key(Some(&storage_key));

            result
        } else {
            SetupWorker::register_as_secondary(app.clone(), password, storage_password).await?

            // XXX Trigger Keys sync request to primary device
            // XXX Trigger Config sync request to primary device
        };

        let storage = reg.storage;

        let mut this = this.borrow_mut();

        this.phoneNumberInner = Some(reg.phonenumber.clone());
        this.uuidInner = Some(reg.service_ids.aci);
        this.pniInner = Some(reg.service_ids.pni);
        this.deviceId = reg.device_id.into();

        config.set_tel(reg.phonenumber.clone());
        config.set_aci(reg.service_ids.aci);
        config.set_pni(reg.service_ids.pni);
        config.set_device_id(reg.device_id.into());

        if let Some(profile_key) = reg.profile_key {
            storage.update_profile_key(
                Some(reg.phonenumber),
                Some(Aci::from(reg.service_ids.aci).into()),
                &profile_key,
                TrustLevel::Certain,
            );
        }

        app.app_state.pinned().borrow().set_storage(storage);

        Ok(())
    }

    async fn register_as_primary(
        app: Rc<WhisperfishApp>,
        config: &crate::config::SignalConfig,
        password: String,
        storage_password: Option<String>,
    ) -> Result<RegistrationResult, anyhow::Error> {
        let this = app.setup_worker.pinned();

        let number = loop {
            let number: String = app
                .prompt
                .pinned()
                .borrow_mut()
                .ask_phone_number()
                .await
                .context("No phone number provided")?
                .into();

            match phonenumber::parse(None, number) {
                Ok(number) => break number,
                Err(e) => {
                    tracing::warn!("Could not parse phone number: {}", e);
                    this.borrow().invalidPhoneNumber();
                }
            }
        };

        if let Some(captcha) = &config.override_captcha {
            tracing::info!("Using override captcha {}", captcha);
        }
        let transport = if this.borrow().useVoice {
            VerificationTransport::Voice
        } else {
            VerificationTransport::Sms
        };
        let mut res = app
            .client_actor
            .send(super::client::Register {
                phonenumber: number.clone(),
                password: password.to_string(),
                transport,
                captcha: config.override_captcha.clone(),
            })
            .await??;

        while res == super::client::VerificationCodeResponse::CaptchaRequired {
            let captcha: String = app
                .prompt
                .pinned()
                .borrow_mut()
                .ask_captcha()
                .await
                .context("No captcha result provided")?
                .into();
            res = app
                .client_actor
                .send(super::client::Register {
                    phonenumber: number.clone(),
                    password: password.to_string(),
                    transport,
                    captcha: Some(captcha),
                })
                .await??;
        }

        let code: String = app
            .prompt
            .pinned()
            .borrow_mut()
            .ask_verification_code()
            .await
            .context("No verification code provided")?
            .into();
        let code = code.parse()?;

        let (storage, res) = app
            .client_actor
            .send(super::client::ConfirmRegistration {
                phonenumber: number.clone(),
                password,
                storage_password,
                confirm_code: code,
            })
            .await??;

        tracing::info!("Registration result: {:?}", res);

        Ok(RegistrationResult {
            storage,
            phonenumber: number,
            service_ids: ServiceIds {
                aci: res.aci,
                pni: res.pni,
            },
            device_id: protocol::DeviceId::from(DEFAULT_DEVICE_ID),
            profile_key: None,
        })
    }

    async fn register_as_secondary(
        app: Rc<WhisperfishApp>,
        password: String,
        storage_password: Option<String>,
    ) -> Result<RegistrationResult, anyhow::Error> {
        use futures::FutureExt;

        let (tx_uri, rx_uri) = futures::channel::oneshot::channel();

        let res_fut = app.client_actor.send(super::client::RegisterLinked {
            device_name: String::from("Whisperfish"),
            password,
            storage_password,
            tx_uri,
        });

        let res_fut = res_fut.fuse();
        let rx_uri = rx_uri.fuse();

        futures::pin_mut!(res_fut, rx_uri);

        loop {
            futures::select! {
                uri_result = rx_uri => {
                    app.prompt
                        .pinned()
                        .borrow_mut()
                        .show_link_qr(uri_result?);
                }
                res = res_fut => {
                    let res = res??;
                    return Ok(RegistrationResult {
                        storage: res.storage,
                        phonenumber: res.phone_number,
                        service_ids: res.service_ids,
                        device_id: res.device_id,
                        profile_key: Some(res.profile_key),
                    });
                }
                complete => return Err(anyhow::Error::msg("Linking to device completed without any result")),
            }
        }
    }

    fn calling_supported(&self) -> bool {
        cfg!(feature = "calling")
    }

    fn get_uuid(&self) -> QString {
        self.uuidInner
            .as_ref()
            .map(Uuid::to_string)
            .unwrap_or_default()
            .into()
    }

    fn get_pni(&self) -> QString {
        self.pniInner
            .as_ref()
            .map(Uuid::to_string)
            .unwrap_or_default()
            .into()
    }

    fn get_phone_number(&self) -> QString {
        self.phoneNumberInner
            .as_ref()
            .map(PhoneNumber::to_string)
            .unwrap_or_default()
            .into()
    }
}
