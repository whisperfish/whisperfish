#![recursion_limit = "512"]

#[macro_use]
extern crate cpp;

#[macro_use]
extern crate diesel;

use actix::prelude::*;

mod qrc;

mod sfos;

mod model;
mod actor;

mod worker;

mod schema;

mod settings;
use settings::Settings;

mod gui;
mod store;

fn main() -> Result<(), failure::Error> {
    let mut sys = System::new("whisperfish");
    env_logger::init();
    qrc::load();

    sfos::TokioQEventDispatcher::install();

    sys.block_on(async {
        // Currently not possible, default QmlEngine does not run asynchronous.
        // Soft-blocked on https://github.com/woboq/qmetaobject-rs/issues/102

        #[cfg(feature = "sailfish")]
        gui::run().await.unwrap();
    });

    log::info!("Shut down.");

    Ok(())
}
