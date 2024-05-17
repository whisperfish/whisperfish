#![recursion_limit = "512"]

pub mod actor;
pub mod config;
pub mod gui;
pub mod model;
pub mod platform;
pub mod qblurhashimageprovider;
pub mod qrustlegraphimageprovider;
pub mod qtlog;
pub mod store;
pub mod worker;

pub fn user_agent() -> String {
    format!("Whisperfish/{}", env!("CARGO_PKG_VERSION"))
}

pub fn conf_dir() -> std::path::PathBuf {
    let conf_dir = dirs::config_dir()
        .expect("config directory")
        .join("be.rubdos")
        .join("harbour-whisperfish");

    if !conf_dir.exists() {
        std::fs::create_dir(&conf_dir).unwrap();
    }

    conf_dir
}
