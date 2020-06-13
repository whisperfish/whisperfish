use std::rc::Rc;
use std::cell::RefCell;

use crate::{actor, model, sfos::SailfishApp, worker, Settings};
use crate::store::{Storage, StorageReady};

use actix::prelude::*;
use qmetaobject::*;

pub struct WhisperfishApp {
    pub session_actor: Addr<model::SessionActor>,
    pub message_actor: Addr<actor::MessageActor>,
    pub contact_model: QObjectBox<model::ContactModel>,
    pub device_model: QObjectBox<model::DeviceModel>,
    pub prompt: QObjectBox<model::Prompt>,
    pub file_picker: QObjectBox<model::FilePicker>,

    pub client_worker: QObjectBox<worker::ClientWorker>,
    pub send_worker: QObjectBox<worker::SendWorker>,
    pub setup_worker: QObjectBox<worker::SetupWorker>,

    pub settings: QObjectBox<Settings>,

    pub storage: RefCell<Option<Storage>>,
}

impl WhisperfishApp {
    pub async fn storage_ready(&self) {
        let storage = self.storage.borrow().as_ref().unwrap().clone();
        self.session_actor.send(StorageReady(storage)).await
            .expect("Session Actor should not be busy");
        let storage = self.storage.borrow().as_ref().unwrap().clone();
        self.message_actor.send(StorageReady(storage)).await
            .expect("Message Actor should not be busy");
    }
}

#[cfg(feature = "sailfish")]
pub async fn run() -> Result<(), failure::Error> {
    let mut app = SailfishApp::application("harbour-whisperfish".into());
    log::info!("SailfishApp::application loaded");
    let version: QString = "0.6.0".into(); // XXX source from Cargo.toml
    app.set_title("Whisperfish".into());
    app.set_application_version(version.clone());
    app.install_default_translator().unwrap();

    let message_actor = actor::MessageActor::new(&mut app).start();
    let session_actor = model::SessionActor::new(&mut app).start();

    let whisperfish = Rc::new(WhisperfishApp {
        session_actor,
        message_actor,
        contact_model: QObjectBox::new(model::ContactModel::default()),
        device_model: QObjectBox::new(model::DeviceModel::default()),
        prompt: QObjectBox::new(model::Prompt::default()),
        file_picker: QObjectBox::new(model::FilePicker::default()),

        client_worker: QObjectBox::new(worker::ClientWorker::default()),
        send_worker: QObjectBox::new(worker::SendWorker::default()),
        setup_worker: QObjectBox::new(worker::SetupWorker::default()),

        settings: QObjectBox::new(Settings::default()),

        storage: RefCell::new(None),
    });

    Arbiter::spawn(worker::SetupWorker::run(whisperfish.clone()));

    app.set_property("AppVersion".into(), version.into());

    app.set_object_property("Prompt".into(), whisperfish.prompt.pinned());
    app.set_object_property("SettingsBridge".into(), whisperfish.settings.pinned());
    app.set_object_property("FilePicker".into(), whisperfish.file_picker.pinned());
    app.set_object_property("ContactModel".into(), whisperfish.contact_model.pinned());
    app.set_object_property("DeviceModel".into(), whisperfish.device_model.pinned());
    app.set_object_property("SetupWorker".into(), whisperfish.setup_worker.pinned());
    app.set_object_property("ClientWorker".into(), whisperfish.client_worker.pinned());
    app.set_object_property("SendWorker".into(), whisperfish.send_worker.pinned());

    app.set_source(SailfishApp::path_to("qml/harbour-whisperfish.qml".into()));

    app.show();
    app.exec_async().await;

    Ok(())
}
