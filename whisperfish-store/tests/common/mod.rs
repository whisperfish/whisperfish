use rstest::fixture;
use std::future::Future;
use std::sync::Arc;
use whisperfish_store::config::SignalConfig;
use whisperfish_store::{temp, Storage, StorageLocation};

pub type InMemoryDb = (Storage, StorageLocation<tempfile::TempDir>);

/// We do not want to test on a live db, use temporary dir
#[fixture]
#[allow(clippy::manual_async_fn)]
pub fn storage() -> impl Future<Output = InMemoryDb> {
    async {
        let temp = temp();
        (
            Storage::new(
                // XXX add tempdir to this cfg
                Arc::new(SignalConfig::default()),
                &temp,
                None,
                12345,
                12346,
                "Some Password",
                None,
                None,
            )
            .await
            .expect("Failed to initalize storage"),
            temp,
        )
    }
}
