use crate::platform::{MayExit, QmlApp, is_harbour};
use crate::store::Storage;
use crate::{config::SettingsBridge, methods, model, worker};
use actix::prelude::*;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Weak;

#[derive(actix::Message, Clone)]
#[rtype(result = "()")]
pub struct StorageReady {
    pub storage: Storage,
}

#[derive(QObject, Default)]
#[allow(non_snake_case)]
pub struct AppState {
    base: qt_base_class!(trait QObject),

    closed: bool,
    setActive: qt_method!(fn(&self)),
    setClosed: qt_method!(fn(&self)),
    isClosed: qt_method!(fn(&self) -> bool),
    activate: qt_signal!(),

    gstreamer_version: qt_property!(QString; READ gstreamer_version ALIAS gstreamerVersion CONST),
    gstreamer_version_major: qt_property!(u32; READ gstreamer_version_major ALIAS gstreamerVersionMajor CONST),
    gstreamer_version_minor: qt_property!(u32; READ gstreamer_version_minor ALIAS gstreamerVersionMinor CONST),

    may_exit: MayExit,
    setMayExit: qt_method!(fn(&self, value: bool)),
    mayExit: qt_method!(fn(&self) -> bool),

    isHarbour: qt_method!(fn(&self) -> bool),
    isEncrypted: qt_method!(fn(&self) -> bool),

    messageCount: qt_method!(fn(&self) -> i32),
    sessionCount: qt_method!(fn(&self) -> i32),
    recipientCount: qt_method!(fn(&self) -> i32),
    unsentCount: qt_method!(fn(&self) -> i32),

    prekeyCounts: qt_method!(fn(&self) -> QString),
    kyberPrekeyCounts: qt_method!(fn(&self) -> QString),

    pub storage: RefCell<Option<Storage>>,
    /// Address of the [`crate::worker::username::UsernameResolverActor`], set
    /// once at startup in [`run`]. Reachable from QML-constructed observing
    /// models (e.g. `CreateConversation`) via the auto-injected `app`
    /// property, hence living here rather than on `WhisperfishApp`.
    pub username_resolver: RefCell<Option<actix::Addr<crate::worker::username::UsernameResolver>>>,
    // XXX Is this really thread safe?
    pub rustlegraphs: Rc<RefCell<HashMap<String, Weak<rustlegraph::Vizualizer>>>>,

    #[allow(clippy::type_complexity)]
    pub on_storage_ready: RefCell<Vec<Box<dyn FnOnce(Storage)>>>,
}

#[cfg(feature = "_gstreamer")]
impl AppState {
    fn gstreamer_version(&self) -> QString {
        let (major, minor, micro, nano) = gstreamer::version();
        QString::from(format!("{}.{}.{}.{}", major, minor, micro, nano))
    }

    fn gstreamer_version_major(&self) -> u32 {
        gstreamer::version().0
    }

    fn gstreamer_version_minor(&self) -> u32 {
        gstreamer::version().1
    }
}

#[cfg(not(feature = "_gstreamer"))]
impl AppState {
    fn gstreamer_version(&self) -> QString {
        QString::default()
    }

    fn gstreamer_version_major(&self) -> u32 {
        0
    }

    fn gstreamer_version_minor(&self) -> u32 {
        0
    }
}

impl AppState {
    #[allow(non_snake_case)]
    #[with_executor]
    fn setActive(&mut self) {
        self.closed = false;
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn isClosed(&self) -> bool {
        self.closed
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn setClosed(&mut self) {
        self.closed = true;
    }

    #[with_executor]
    pub fn activate_hidden_window(&mut self, may_exit: bool) {
        if self.closed {
            self.activate();
            self.closed = false;
            self.may_exit.set_may_exit(may_exit);
        }
    }

    #[allow(non_snake_case)]
    #[with_executor]
    pub fn setMayExit(&mut self, value: bool) {
        self.may_exit.set_may_exit(value);
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn mayExit(&mut self) -> bool {
        self.may_exit.may_exit()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn isHarbour(&mut self) -> bool {
        is_harbour()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn isEncrypted(&mut self) -> bool {
        self.storage.borrow().as_ref().unwrap().is_encrypted()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn messageCount(&mut self) -> i32 {
        self.storage.borrow().as_ref().unwrap().message_count()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn sessionCount(&mut self) -> i32 {
        self.storage.borrow().as_ref().unwrap().session_count()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn recipientCount(&mut self) -> i32 {
        self.storage.borrow().as_ref().unwrap().recipient_count()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn unsentCount(&mut self) -> i32 {
        self.storage.borrow().as_ref().unwrap().unsent_count()
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn prekeyCounts(&self) -> QString {
        if let Some(storage) = self.storage.borrow_mut().as_mut() {
            let (aci_count, pni_count) = storage.get_prekey_counts();
            format!("{} / {}", aci_count, pni_count).into()
        } else {
            "? / ?".into()
        }
    }

    #[allow(non_snake_case)]
    #[with_executor]
    fn kyberPrekeyCounts(&self) -> QString {
        if let Some(storage) = self.storage.borrow_mut().as_mut() {
            let (aci_count, pni_count) = storage.get_kyber_prekey_counts();
            format!("{} / {}", aci_count, pni_count).into()
        } else {
            "? / ?".into()
        }
    }

    pub fn set_storage(&self, storage: Storage) {
        for cb in self.on_storage_ready.take() {
            let storage = storage.clone();
            // Reason for actix::spawn, see below
            actix::spawn(async move { cb(storage) });
        }
        *self.storage.borrow_mut() = Some(storage);
    }

    pub fn deferred_with_storage(&self, cb: impl FnOnce(Storage) + 'static) {
        // Calling the callback from within AppState means we have the RefCells that AppState is
        // encapsulated in potentially panic.
        // Spawn the closure on actix, this ensures the borrow of self will have ended.
        if let Some(storage) = self.storage.borrow_mut().as_mut() {
            let storage = storage.clone();
            actix::spawn(async move { cb(storage) });
        } else {
            self.on_storage_ready.borrow_mut().push(Box::new(cb));
        }
    }

    #[with_executor]
    fn new() -> Self {
        Self {
            gstreamer_version: Default::default(),
            gstreamer_version_major: Default::default(),
            gstreamer_version_minor: Default::default(),

            base: Default::default(),
            closed: false,
            may_exit: MayExit::default(),
            setActive: Default::default(),
            isClosed: Default::default(),
            setClosed: Default::default(),
            isHarbour: Default::default(),
            activate: Default::default(),
            setMayExit: Default::default(),
            mayExit: Default::default(),

            storage: RefCell::default(),
            username_resolver: RefCell::default(),
            rustlegraphs: Rc::new(RefCell::new(HashMap::new())),
            isEncrypted: Default::default(),

            messageCount: Default::default(),
            sessionCount: Default::default(),
            recipientCount: Default::default(),
            unsentCount: Default::default(),

            // XXX: Remove these two when #803 gets fixed
            prekeyCounts: Default::default(),
            kyberPrekeyCounts: Default::default(),

            on_storage_ready: Default::default(),
        }
    }
}

pub struct WhisperfishApp {
    pub app_state: QObjectBox<AppState>,
    pub session_methods: QObjectBox<methods::SessionMethods>,
    pub message_methods: QObjectBox<methods::MessageMethods>,
    pub contact_model: QObjectBox<model::ContactModel>,
    pub prompt: QObjectBox<model::Prompt>,

    pub client_actor: Addr<worker::ClientActor>,
    pub setup_worker: QObjectBox<worker::SetupWorker>,

    pub settings_bridge: QObjectBox<SettingsBridge>,
}

impl WhisperfishApp {
    pub async fn storage_ready(&self) {
        let storage = self
            .app_state
            .pinned()
            .borrow()
            .storage
            .borrow()
            .as_ref()
            .unwrap()
            .clone();
        // SessionMethods is not an actor: set its storage directly. Set
        // exactly once; a duplicate dispatch indicates a logic error
        // upstream.
        if self
            .session_methods
            .pinned()
            .borrow()
            .storage
            .set(storage.clone())
            .is_err()
        {
            tracing::error!("StorageReady dispatched twice; ignoring duplicate for SessionMethods");
        } else {
            tracing::trace!("SessionMethods has a registered storage");
        }

        let msg = StorageReady { storage };

        if let Err(e) = self.client_actor.send(msg.clone()).await {
            tracing::error!("Error handling StorageReady: {}", e);
        }

        // Username resolver subactor registers its storage the same way; lookups
        // can't proceed (and be emitted on the observer bus) before this lands.
        let resolver = self
            .app_state
            .pinned()
            .borrow()
            .username_resolver
            .borrow()
            .clone();
        if let Some(resolver) = resolver {
            if let Err(e) = resolver.send(msg).await {
                tracing::error!("Error handling StorageReady (resolver): {}", e);
            }
        } else {
            tracing::error!("UsernameResolverActor not spawned when StorageReady dispatched");
        }
    }
}

// Allow if-same-cond, because CI_COMMIT_TAG and GIT_VERSION might have the same content.
#[allow(clippy::ifs_same_cond)]
fn long_version() -> String {
    let pkg = env!("CARGO_PKG_VERSION");

    // If it's tagged, use the tag as-is
    // If it's in CI, use the cargo version with the ref-name and job id appended
    // else, we use whatever git thinks is the version,
    // finally, we fall back on Cargo's version as-is
    if let Some(tag) = option_env!("CI_COMMIT_TAG") {
        // Tags are mainly used for specific versions
        tag.into()
    } else if let (Some(ref_name), Some(job_id)) =
        (option_env!("CI_COMMIT_REF_NAME"), option_env!("CI_JOB_ID"))
    {
        // This is always the fall-back in CI
        format!("v{}-{}-{}", pkg, ref_name, job_id)
    } else if let Some(git_version) = option_env!("GIT_VERSION") {
        // This is normally possible with any build
        git_version.into()
    } else {
        // But if git is not available, we fall back on cargo
        format!("v{}", env!("CARGO_PKG_VERSION"))
    }
}

macro_rules! cstr {
    ($s:expr) => {
        &std::ffi::CString::new($s).unwrap() as &std::ffi::CStr
    };
}

pub fn run(config: crate::config::SignalConfig) -> Result<(), anyhow::Error> {
    qmeta_async::run(|| {
        // For audio recording
        #[cfg(feature = "_gstreamer")]
        gstreamer::init().expect("gstreamer initialization");

        // XXX this arc thing should be removed in the future and refactored
        let config = std::sync::Arc::new(config);

        // Register types
        {
            let uri = cstr!("be.rubdos.whisperfish");
            qml_register_type::<model::RustleGraph>(uri, 1, 0, cstr!("RustleGraph"));
            #[cfg(feature = "voice-note-recording")]
            qml_register_type::<model::VoiceNoteRecorder>(uri, 1, 0, cstr!("VoiceNoteRecorder"));

            qml_register_type::<model::Sessions>(uri, 1, 0, cstr!("Sessions"));
            qml_register_type::<model::Session>(uri, 1, 0, cstr!("Session"));
            qml_register_type::<model::CreateConversation>(uri, 1, 0, cstr!("CreateConversation"));
            qml_register_type::<model::Message>(uri, 1, 0, cstr!("Message"));
            qml_register_type::<model::Recipient>(uri, 1, 0, cstr!("Recipient"));
            qml_register_type::<model::Group>(uri, 1, 0, cstr!("Group"));
            qml_register_type::<model::Attachment>(uri, 1, 0, cstr!("Attachment"));
            qml_register_type::<model::Reactions>(uri, 1, 0, cstr!("Reactions"));
            qml_register_type::<model::GroupedReactions>(uri, 1, 0, cstr!("GroupedReactions"));
            qml_register_type::<model::Receipts>(uri, 1, 0, cstr!("Receipts"));
            qml_register_type::<model::TypingModel>(uri, 1, 0, cstr!("TypingModel"));
        }

        let mut app = QmlApp::application("harbour-whisperfish".into());
        let long_version: QString = long_version().into();
        tracing::info!("QmlApp::application loaded - version {}", long_version);
        let version: QString = env!("CARGO_PKG_VERSION").into();
        app.set_title("Whisperfish".into());
        app.set_application_version(version.clone());
        app.install_default_translator().unwrap();

        let app_state = AppState::new();
        crate::qblurhashimageprovider::install(app.engine());
        crate::qrustlegraphimageprovider::install(app.engine(), app_state.rustlegraphs.clone());

        // XXX Spaghetti
        let session_methods = QObjectBox::new(methods::SessionMethods::default());
        app.set_object_property("SessionModel".into(), session_methods.pinned());
        let client_actor =
            worker::ClientActor::new(&mut app, std::sync::Arc::clone(&config))?.start();
        // Username resolver is a standalone subactor (no ClientActor
        // dependency): username/link lookups use the unidentified websocket,
        // which needs no credentials. It receives `StorageReady` alongside
        // the client actor below; lookups before that point are dropped.
        let username_resolver =
            worker::username::UsernameResolver::new(config.get_signal_server()).start();
        app_state
            .username_resolver
            .borrow_mut()
            .get_or_insert(username_resolver);
        let message_methods = QObjectBox::new(methods::MessageMethods::default());
        message_methods.pinned().borrow_mut().client_actor = Some(client_actor.clone());
        app.set_object_property("MessageModel".into(), message_methods.pinned());

        let whisperfish = Rc::new(WhisperfishApp {
            app_state: QObjectBox::new(app_state),
            session_methods,
            message_methods,
            client_actor,
            contact_model: QObjectBox::new(model::ContactModel::default()),
            prompt: QObjectBox::new(model::Prompt::default()),

            setup_worker: QObjectBox::new(worker::SetupWorker::default()),

            settings_bridge: QObjectBox::new(SettingsBridge::default()),
        });

        app.set_property("AppVersion".into(), version.into());
        app.set_property("LongAppVersion".into(), long_version.into());
        let ci_job_url: Option<QString> = option_env!("CI_JOB_URL").map(Into::into);
        let ci_job_url = ci_job_url.map(Into::into).unwrap_or_else(|| false.into());
        app.set_property("CiJobUrl".into(), ci_job_url);

        app.set_object_property("Prompt".into(), whisperfish.prompt.pinned());
        app.set_object_property(
            "SettingsBridge".into(),
            whisperfish.settings_bridge.pinned(),
        );
        app.set_object_property("ContactModel".into(), whisperfish.contact_model.pinned());
        app.set_object_property("SetupWorker".into(), whisperfish.setup_worker.pinned());
        app.set_object_property("AppState".into(), whisperfish.app_state.pinned());

        // We need to decied when to close the app based on the current setup state and
        // background service configuration. We do that in QML in the lastWindowClosed signal
        // emitted from the main QtGuiApplication object, since the corresponding app object in
        // rust is occupied running the main loop.
        // XXX: find a way to set quit_on_last_window_closed from SetupWorker and Settings at
        // runtime to get rid of the QML part here.
        app.set_quit_on_last_window_closed(false);
        app.promote_gui_app_to_qml_context("RootApp".into());

        // We need harbour-whisperfish.qml for the QML-only signalcaptcha application
        // so we have to use another filename for the main QML file for Whisperfish.
        app.set_source(QmlApp::path_to("qml/harbour-whisperfish-main.qml".into()));

        if config.autostart
            && !whisperfish
                .settings_bridge
                .pinned()
                .borrow()
                .get_bool("quit_on_ui_close")
            && !is_harbour()
        {
            // keep the ui closed until needed on auto-start
            whisperfish
                .app_state
                .pinned()
                .borrow_mut()
                .setMayExit(false);
            whisperfish.app_state.pinned().borrow_mut().setClosed();
        } else {
            app.show_full_screen();
        }

        actix::spawn(worker::SetupWorker::run(
            whisperfish.clone(),
            std::sync::Arc::clone(&config),
        ));

        app.exec();
        Ok(())
    })
    .flatten()
}
