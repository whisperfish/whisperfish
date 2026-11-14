use futures::prelude::*;
use rstest::fixture;
use std::future::Future;
use std::sync::Arc;
use uuid::Uuid;
use whisperfish_store::config::SignalConfig;
use whisperfish_store::observer::{Event, Interest, Observatory};
use whisperfish_store::{Storage, StorageLocation, temp};

#[derive(Default, Clone)]
pub struct DummyObservatory;

impl Observatory for DummyObservatory {
    type Subscriber = ();

    fn register(&self, _id: Uuid, _interests: Vec<Interest>, _subscriber: Self::Subscriber) {}

    fn update_interests(&self, _handle: Uuid, _interests: Vec<Interest>) {}

    fn distribute_event(&self, _event: Event) {}
}

pub type SimpleStorage = Storage<DummyObservatory>;

pub type InMemoryDb = (SimpleStorage, StorageLocation<tempfile::TempDir>);

pub type InMemoryDbWithConfig = (
    SimpleStorage,
    Arc<SignalConfig>,
    StorageLocation<tempfile::TempDir>,
);

/// Storage fixture which additionally yields the config
#[fixture]
#[allow(clippy::manual_async_fn)]
pub fn storage_with_config() -> impl Future<Output = InMemoryDbWithConfig> {
    async {
        let config = Arc::new(SignalConfig::default());
        let temp = temp();
        (
            Storage::new(
                config.clone(),
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
            config,
            temp,
        )
    }
}

/// We do not want to test on a live db, use temporary dir
#[fixture]
#[allow(clippy::manual_async_fn)]
pub fn storage(
    storage_with_config: impl Future<Output = InMemoryDbWithConfig>,
) -> impl Future<Output = InMemoryDb> {
    storage_with_config.map(|(storage, _config, temp)| (storage, temp))
}

/// Like [`storage`], but with a self recipient,
/// which outgoing calls require as their ringer.
#[fixture]
#[allow(clippy::manual_async_fn)]
pub fn storage_with_self_recipient(
    storage_with_config: impl Future<Output = InMemoryDbWithConfig>,
) -> impl Future<Output = InMemoryDb> {
    async {
        let (storage, config, temp) = storage_with_config.await;

        let self_e164 = phonenumber::parse(None, "+35850999888777").expect("valid phone number");
        config.set_tel(self_e164);

        // Eagerly create the self recipient.
        storage.fetch_self_recipient_id();
        (storage, temp)
    }
}
