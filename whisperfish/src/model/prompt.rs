use image::codecs::png::PngEncoder;
use image::ImageEncoder;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use std::future::Future;
use std::process::Command;

// XXX code duplication.

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct Prompt {
    base: qt_base_class!(trait QObject),
    promptRegistrationType: qt_signal!(),
    promptPhoneNumber: qt_signal!(),
    promptVerificationCode: qt_signal!(),
    promptPassword: qt_signal!(),
    promptCaptcha: qt_signal!(),
    showLinkQR: qt_signal!(),

    linkingQR: qt_property!(QString; NOTIFY qrChanged),
    qrChanged: qt_signal!(),

    registerAsPrimary: qt_method!(fn(&self, isPrimary: bool)),

    phoneNumber: qt_method!(fn(&self, phoneNumber: QString)),
    verificationCode: qt_method!(fn(&self, code: QString)),
    password: qt_method!(fn(&self, password: QString)),
    captcha: qt_method!(fn(&self, captcha: QString)),
    resetPeerIdentity: qt_method!(fn(&self, confirm: QString)),

    startCaptcha: qt_method!(fn(&self)),

    password_listeners: Vec<futures::channel::oneshot::Sender<QString>>,
    registration_type_listeners: Vec<futures::channel::oneshot::Sender<bool>>,
    code_listeners: Vec<futures::channel::oneshot::Sender<QString>>,
    phone_number_listeners: Vec<futures::channel::oneshot::Sender<QString>>,
    captcha_listeners: Vec<futures::channel::oneshot::Sender<QString>>,
}

impl Prompt {
    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn phoneNumber(&mut self, phone_number: QString) {
        for listener in self.phone_number_listeners.drain(..) {
            if listener.send(phone_number.clone()).is_err() {
                tracing::warn!("Request for phone number fulfilled, but nobody listens.");
            }
        }
    }

    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn verificationCode(&mut self, code: QString) {
        for listener in self.code_listeners.drain(..) {
            if listener.send(code.clone()).is_err() {
                tracing::warn!("Request for verification code fulfilled, but nobody listens.");
            }
        }
    }

    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn password(&mut self, password: QString) {
        for listener in self.password_listeners.drain(..) {
            if listener.send(password.clone()).is_err() {
                tracing::warn!("Request for password fulfilled, but nobody listens.");
            }
        }
    }

    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn captcha(&mut self, captcha: QString) {
        for listener in self.captcha_listeners.drain(..) {
            if listener.send(captcha.clone()).is_err() {
                tracing::warn!("Request for captcha fulfilled, but nobody listens.");
            }
        }
    }

    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn resetPeerIdentity(&self, _confirm: QString) {}

    #[allow(non_snake_case)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn registerAsPrimary(&mut self, isPrimary: bool) {
        for listener in self.registration_type_listeners.drain(..) {
            if listener.send(isPrimary).is_err() {
                tracing::warn!("Request for registration type fulfilled, but nobody listens.");
            }
        }
    }

    pub fn ask_password(&mut self) -> impl Future<Output = Option<QString>> {
        self.promptPassword();

        let (sender, receiver) = futures::channel::oneshot::channel();

        self.password_listeners.push(sender);

        async {
            match receiver.await {
                Ok(pwd) => Some(pwd),
                Err(_e) => {
                    tracing::error!("Password prompt was canceled");
                    None
                }
            }
        }
    }

    pub fn ask_registration_type(&mut self) -> impl Future<Output = Option<bool>> {
        self.promptRegistrationType();

        let (sender, receiver) = futures::channel::oneshot::channel();

        self.registration_type_listeners.push(sender);

        async {
            match receiver.await {
                Ok(pwd) => Some(pwd),
                Err(_e) => {
                    tracing::error!("Registration type prompt was canceled");
                    None
                }
            }
        }
    }

    pub fn show_link_qr(&mut self, url: String) {
        let code = qrencode::QrCode::new(url.as_str()).expect("to generate qrcode for linking URI");
        let image_buf = code.render::<image::Luma<u8>>().build();

        // Export generate QR code pixmap data into a PNG data:-URI string
        let mut image_uri = String::from("data:image/png;base64,");
        {
            use base64::engine::general_purpose as engine;
            let mut image_b64enc = base64::write::EncoderStringWriter::from_consumer(
                &mut image_uri,
                &engine::STANDARD,
            );
            PngEncoder::new(&mut image_b64enc)
                .write_image(
                    &image_buf,
                    image_buf.width(),
                    image_buf.height(),
                    image::ColorType::L8,
                )
                .expect("to write QR code image to data:-URI");
        }

        self.linkingQR = QString::from(image_uri);
        self.qrChanged();
        self.showLinkQR();
    }

    pub fn ask_phone_number(&mut self) -> impl Future<Output = Option<QString>> {
        self.promptPhoneNumber();

        let (sender, receiver) = futures::channel::oneshot::channel();

        self.phone_number_listeners.push(sender);

        async {
            match receiver.await {
                Ok(pwd) => Some(pwd),
                Err(_e) => {
                    tracing::error!("Phone number prompt was canceled");
                    None
                }
            }
        }
    }

    pub fn ask_verification_code(&mut self) -> impl Future<Output = Option<QString>> {
        self.promptVerificationCode();

        let (sender, receiver) = futures::channel::oneshot::channel();

        self.code_listeners.push(sender);

        async {
            match receiver.await {
                Ok(pwd) => Some(pwd),
                Err(_e) => {
                    tracing::error!("Code prompt was canceled");
                    None
                }
            }
        }
    }

    pub fn ask_captcha(&mut self) -> impl Future<Output = Option<QString>> {
        self.promptCaptcha();

        let (sender, receiver) = futures::channel::oneshot::channel();

        self.captcha_listeners.push(sender);

        async {
            match receiver.await {
                Ok(pwd) => Some(pwd),
                Err(_e) => {
                    tracing::error!("Captcha prompt was canceled");
                    None
                }
            }
        }
    }

    #[allow(non_snake_case, clippy::zombie_processes)]
    #[with_executor]
    #[tracing::instrument(skip(self))]
    fn startCaptcha(&mut self) {
        Command::new("/usr/bin/sailfish-qml")
            .args(["harbour-whisperfish"])
            .spawn()
            .expect("/usr/bin/sailfish-qml not found, libsailfishapp-launcher not installed?");
    }
}
