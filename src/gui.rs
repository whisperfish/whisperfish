use std::cell::RefCell;
#[allow(unused_imports)]
use std::rc::Rc;

use crate::store::{Storage, StorageReady};
#[allow(unused_imports)]
use crate::{actor, model, settings::Settings, sfos::SailfishApp, worker};

use actix::prelude::*;
use futures::prelude::*;
use qmetaobject::*;

pub struct WhisperfishApp {
    pub session_actor: Addr<model::SessionActor>,
    pub message_actor: Addr<actor::MessageActor>,
    pub contact_model: QObjectBox<model::ContactModel>,
    pub device_model: QObjectBox<model::DeviceModel>,
    pub prompt: QObjectBox<model::Prompt>,
    pub file_picker: QObjectBox<model::FilePicker>,

    pub client_actor: Addr<worker::ClientActor>,
    pub send_worker: QObjectBox<worker::SendWorker>,
    pub setup_worker: QObjectBox<worker::SetupWorker>,

    pub settings: QObjectBox<Settings>,

    pub storage: RefCell<Option<Storage>>,
}

impl WhisperfishApp {
    pub async fn storage_ready(&self) {
        let storage = self.storage.borrow().as_ref().unwrap().clone();
        let msg = StorageReady(storage);

        let mut sends = futures::stream::FuturesUnordered::<
            Box<dyn Future<Output = Result<(), String>> + std::marker::Unpin>,
        >::new();
        sends.push(Box::new(
            self.session_actor
                .send(msg.clone())
                .map_err(|e| format!("SessionActor {:?}", e)),
        ));
        sends.push(Box::new(
            self.message_actor
                .send(msg.clone())
                .map_err(|e| format!("MessageActor {:?}", e)),
        ));
        sends.push(Box::new(
            self.client_actor
                .send(msg)
                .map_err(|e| format!("ClientActor {:?}", e)),
        ));

        while let Some(res) = sends.next().await {
            if let Err(e) = res {
                log::error!("Error handling StorageReady: {}", e);
            }
        }
    }
}

#[cfg(feature = "sailfish")]
pub async fn run() -> Result<(), failure::Error> {
    let mut app = SailfishApp::application("harbour-whisperfish".into());
    log::info!("SailfishApp::application loaded");
    let version: QString = env!("CARGO_PKG_VERSION").into();
    app.set_title("Whisperfish".into());
    app.set_application_version(version.clone());
    app.install_default_translator().unwrap();

    let message_actor = actor::MessageActor::new(&mut app).start();
    let session_actor = model::SessionActor::new(&mut app).start();
    let client_actor = worker::ClientActor::new(&mut app).start();

    let whisperfish = Rc::new(WhisperfishApp {
        session_actor,
        message_actor,
        client_actor,
        contact_model: QObjectBox::new(model::ContactModel::default()),
        device_model: QObjectBox::new(model::DeviceModel::default()),
        prompt: QObjectBox::new(model::Prompt::default()),
        file_picker: QObjectBox::new(model::FilePicker::default()),

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
    app.set_object_property("SendWorker".into(), whisperfish.send_worker.pinned());

    app.set_source(SailfishApp::path_to("qml/harbour-whisperfish.qml".into()));

    app.show();
    app.exec_async().await;

    Ok(())
}
