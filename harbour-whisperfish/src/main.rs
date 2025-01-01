use anyhow::Context;
use clap::Parser;
use dbus::blocking::Connection;
use signal_hook::{consts::SIGINT, iterator::Signals};
use single_instance::SingleInstance;
use std::{os::unix::prelude::OsStrExt, thread, time::Duration};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use whisperfish::*;

/// Unofficial but advanced Signal client for Sailfish OS
#[derive(Parser, Debug)]
#[clap(name = "harbour-whisperfish", author, version, about, long_about = None)]
struct Opts {
    /// Captcha override
    ///
    /// By opening <https://signalcaptchas.org/registration/generate.html> in a browser,
    /// and intercepting the redirect (by using the console),
    /// it is possible to inject a signalcaptcha URL.
    ///
    /// This is as a work around for <https://gitlab.com/whisperfish/whisperfish/-/issues/378>
    #[clap(short = 'c', long)]
    captcha: Option<String>,

    /// Verbosity.
    ///
    /// Equivalent with setting
    /// `QT_LOGGING_TO_CONSOLE=1 RUST_LOG=libsignal_service=trace,libsignal_service_actix=trace,whisperfish=trace`.
    /// Implies '--ts'
    #[clap(short = 'v', long)]
    verbose: bool,

    /// Whether whisperfish was launched from autostart. Also accepts '-prestart'
    #[clap(long)]
    prestart: bool,

    /// Send a signal to shutdown Whisperfish
    #[clap(long)]
    quit: bool,
}

fn main() {
    // Ctrl-C --> graceful shutdown
    if let Ok(mut signals) = Signals::new([SIGINT].iter()) {
        thread::spawn(move || {
            let mut terminate = false;
            for _ in signals.forever() {
                if !terminate {
                    tracing::info!("[SIGINT] Trying to exit gracefully...");
                    terminate = true;
                    dbus_quit_app().ok();
                } else {
                    tracing::info!("[SIGINT] Exiting forcefully...");
                    std::process::exit(1);
                }
            }
        });
    }

    // Sailjail only accepts -prestart on the command line as optional argument,
    // clap however only supports --prestart.
    // See: https://github.com/clap-rs/clap/issues/2468
    // and https://github.com/sailfishos/sailjail/commit/8a239de9451685a82a2ee17fef0c1d33a089c28c
    // XXX: Get rid of this when the situation changes
    let args = std::env::args_os().map(|arg| {
        if arg == std::ffi::OsStr::from_bytes(b"-prestart") {
            "--prestart".into()
        } else {
            arg
        }
    });

    // Then, handle command line arguments and overwrite settings from config file if necessary
    let opt: Opts = Parser::parse_from(args);

    if opt.quit {
        if let Err(e) = dbus_quit_app() {
            eprintln!("{}", e);
        }
        return;
    }

    // Migrate the config file from
    // ~/.config/harbour-whisperfish/config.yml to
    // ~/.config/be.rubdos/harbour-whisperfish/config.yml
    match config::SignalConfig::migrate_config() {
        Ok(()) => (),
        Err(e) => {
            eprintln!("Could not migrate config file: {}", e);
        }
    };

    // Migrate the QSettings file from
    // ~/.config/harbour-whisperfish/harbour-whisperfish.conf to
    // ~/.config/be.rubdos/harbour-whisperfish/harbour-whisperfish.conf
    match config::SettingsBridge::migrate_qsettings() {
        Ok(()) => (),
        Err(e) => {
            eprintln!("Could not migrate QSettings file: {}", e);
        }
    };

    // Read config file or get a default config
    let mut config = match config::SignalConfig::read_from_file() {
        Ok(x) => x,
        Err(e) => {
            eprintln!("Config file not found: {}", e);
            config::SignalConfig::default()
        }
    };

    // Migrate the db and storage folders from
    // ~/.local/share/harbour-whisperfish/[...] to
    // ~/.local/share/rubdos.be/harbour-whisperfish/[...]
    match store::Storage::migrate_storage() {
        Ok(()) => (),
        Err(e) => {
            eprintln!("Could not migrate db and storage: {}", e);
            std::process::exit(1);
        }
    };

    // Write config to initialize a default config
    if let Err(e) = config.write_to_file() {
        eprintln!("{}", e);
        std::process::exit(1);
    }

    if opt.prestart {
        config.autostart = true;
    }
    config.override_captcha = opt.captcha;

    let log_filter = if config.verbose || opt.verbose {
        // Enable QML debug output and full backtrace (for Sailjail).
        std::env::set_var("QT_LOGGING_TO_CONSOLE", "1");
        std::env::set_var("RUST_BACKTRACE", "full");
        "whisperfish=trace,libsignal_service=trace"
    } else {
        "whisperfish=info,warn"
    };

    #[cfg(feature = "flame")]
    let mut _guard = None;

    if config.tracing {
        #[cfg(not(feature = "coz"))]
        {
            use tracing_subscriber::prelude::*;
            let registry = tracing_subscriber::registry().with(tracing_subscriber::fmt::layer());
            #[cfg(feature = "console-subscriber")]
            let registry = registry.with(console_subscriber::spawn());

            #[cfg(feature = "tracy")]
            let registry = registry.with(tracing_tracy::TracyLayer::new());

            #[cfg(feature = "flame")]
            let registry = {
                eprintln!("Enabling flamegraph tracing");
                let (layer, guard) =
                    tracing_flame::FlameLayer::with_file("./tracing.folded").unwrap();
                _guard = Some(guard);
                registry.with(layer)
            };

            registry.init();
        }

        #[cfg(feature = "coz")]
        tracing::subscriber::set_global_default(tracing_coz::TracingCozBridge::new()).unwrap();
    } else {
        if std::env::var("RUST_LOG").is_err() {
            std::env::set_var("RUST_LOG", log_filter);
        }
        let env_filter = EnvFilter::from_default_env();

        let journald = tracing_journald::layer()
            .expect("open journald socket")
            .with_syslog_identifier("harbour-whisperfish".into());

        // If verbose, print to terminal (with timestamps and tracing).
        // Otherwise, send to journald (without tracing).
        if opt.verbose {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(tracing_subscriber::fmt::layer())
                .init();
        } else {
            tracing_subscriber::registry()
                .with(env_filter)
                .with(journald)
                .init();
        }
    }

    qtlog::enable();

    let instance_lock = SingleInstance::new("whisperfish").unwrap();
    if !instance_lock.is_single() {
        if let Err(e) = dbus_show_app() {
            tracing::error!("{}", e);
        }
        return;
    }

    if let Err(e) = run_main_app(config) {
        tracing::error!("Fatal error: {}", e);
        std::process::exit(1);
    }
}

fn dbus_show_app() -> Result<(), dbus::Error> {
    tracing::info!("Calling app.show() on DBus.");

    let c = Connection::new_session()?;
    let proxy = c.with_proxy(
        "be.rubdos.whisperfish",
        "/be/rubdos/whisperfish/app",
        Duration::from_millis(20000),
    );

    proxy.method_call("be.rubdos.whisperfish.app", "show", ())
}

fn dbus_quit_app() -> Result<(), dbus::Error> {
    tracing::info!("Calling app.quit() on DBus.");

    let c = Connection::new_session()?;
    let proxy = c.with_proxy(
        "be.rubdos.whisperfish",
        "/be/rubdos/whisperfish/app",
        Duration::from_millis(1000),
    );

    proxy.method_call("be.rubdos.whisperfish.app", "quit", ())
}

fn run_main_app(config: config::SignalConfig) -> Result<(), anyhow::Error> {
    tracing::info!("Start main app (with autostart = {})", config.autostart);

    // Initialise storage here
    // Right now, we only create the attachment (and storage) directory if necessary
    // With more refactoring there should be probably more initialization here
    // Not creating the storage/attachment directory is fatal and we return here.
    let mut settings = crate::config::SettingsBridge::default();
    settings.migrate_qsettings_paths();

    for dir in &[
        settings.get_string("attachment_dir"),
        settings.get_string("camera_dir"),
        settings.get_string("avatar_dir"),
    ] {
        let path = std::path::Path::new(dir.trim());
        if !path.exists() {
            std::fs::create_dir_all(path)
                .with_context(|| format!("Could not create dir: {}", path.display()))?;
        }
    }

    // This will panic here if feature `sailfish` is not enabled
    gui::run(config).unwrap();

    match config::SignalConfig::read_from_file() {
        Ok(mut config) => {
            config.verbose = settings.get_verbose();
            if let Err(e) = config.write_to_file() {
                tracing::error!("Could not save config.yml: {}", e)
            };
        }
        Err(e) => tracing::error!("Could not open config.yml: {}", e),
    };

    tracing::info!("Shut down.");

    Ok(())
}
