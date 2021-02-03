#![deny(rust_2018_idioms)]

use actix::prelude::*;
use harbour_whisperfish::*;

fn main() -> Result<(), failure::Error> {
    env_logger::init();

    if !handle_arguments()? {
        main_application()?;
    }
    Ok(())
}

fn handle_arguments() -> Result<bool, failure::Error> {
    use dbus::blocking::Connection;
    use std::time::Duration;

    let mut handled = false;

    let c = Connection::new_session()?;
    for arg in std::env::args() {
        log::info!("arg {}", arg);
        if arg.trim().starts_with("signalcaptcha://") {
            handled = true;

            let proxy = c.with_proxy(
                "be.rubdos.whisperfish.captcha",
                "/be/rubdos/whisperfish",
                Duration::from_millis(5000),
            );
            if arg.trim().starts_with("signalcaptcha://") {
                let _: () =
                    proxy.method_call("be.rubdos.whisperfish.captcha", "handleToken", (arg,))?;
            }
        }
    }
    Ok(handled)
}

fn main_application() -> Result<(), failure::Error> {
    let sys = System::new("whisperfish");
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
