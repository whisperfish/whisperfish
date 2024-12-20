use anyhow::Context;
use clap::Parser;
use dbus::blocking::Connection;
use signal_hook::{consts::SIGINT, iterator::Signals};
use single_instance::SingleInstance;
use std::{os::unix::prelude::OsStrExt, thread, time::Duration};
use tracing_subscriber::{fmt::format, layer::SubscriberExt, util::SubscriberInitExt};
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

    /// Print timestamps in the log. Off by default, because journald records the output as well.
    #[clap(long)]
    ts: bool,

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
    let mut opt: Opts = Parser::parse_from(args);

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

    if opt.verbose {
        opt.ts = true;
        config.verbose = true;
    }
    if opt.prestart {
        config.autostart = true;
    }
    config.override_captcha = opt.captcha;

    let shared_dir = config.get_share_dir();

    let file_appender = tracing_appender::rolling::hourly(&shared_dir, "harbour-whisperfish.log");
    let (stdio, _guard) = tracing_appender::non_blocking(std::io::stdout());
    let (file, _guard) = tracing_appender::non_blocking(file_appender);

    let log_filter = if config.verbose {
        // Enable QML debug output and full backtrace (for Sailjail).
        std::env::set_var("QT_LOGGING_TO_CONSOLE", "1");
        std::env::set_var("RUST_BACKTRACE", "full");
        "whisperfish=trace,libsignal_service=trace,libsignal_service_hyper=trace"
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
    } else if config.logfile {
        let filter = tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_filter));

        tracing_subscriber::registry()
            .with(filter)
            .with(
                tracing_subscriber::fmt::layer()
                    .with_ansi(false)
                    .event_format(format().with_ansi(false))
                    .with_writer(file),
            )
            .with(tracing_subscriber::fmt::layer().with_writer(stdio))
            .init();
    } else if opt.ts {
        tracing_subscriber::fmt::fmt()
            .with_env_filter(log_filter)
            .init();
    } else {
        tracing_subscriber::fmt::fmt()
            .without_time()
            .with_env_filter(log_filter)
            .init();
    }

    qtlog::enable();

    const MAX_LOGFILE_COUNT: usize = 5;
    const LOGFILE_REGEX: &str = r"harbour-whisperfish.\d{8}_\d{6}\.log";
    if config.logfile {
        store::Storage::clear_old_logs(&shared_dir, MAX_LOGFILE_COUNT, LOGFILE_REGEX);
    }

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

    // Push verbose and logfile settings to QSettings...
    settings.set_bool("verbose", config.verbose);
    settings.set_bool("logfile", config.logfile);

    // This will panic here if feature `sailfish` is not enabled
    gui::run(config).unwrap();

    // ...and pull them back after execution.
    match config::SignalConfig::read_from_file() {
        Ok(mut config) => {
            config.verbose = settings.get_verbose();
            config.logfile = settings.get_logfile();
            if let Err(e) = config.write_to_file() {
                tracing::error!("Could not save config.yml: {}", e)
            };
        }
        Err(e) => tracing::error!("Could not open config.yml: {}", e),
    };

    tracing::info!("Shut down.");

    Ok(())
}
