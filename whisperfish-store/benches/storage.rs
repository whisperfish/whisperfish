mod common;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use libsignal_service::{
    proto::AttachmentPointer,
    protocol::{Aci, ServiceId},
};
use std::hint::black_box;
use uuid::Uuid;
use whisperfish_store::config::SignalConfig;
use whisperfish_store::{orm, temp, NewMessage, Storage, StorageLocation};

pub type InMemoryDb = (
    Storage<common::DummyObservatory>,
    StorageLocation<tempfile::TempDir>,
);

pub fn storage() -> InMemoryDb {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        let cfg = SignalConfig::default();
        let cfg = std::sync::Arc::new(cfg);
        let temp = temp();
        (
            Storage::new(cfg, &temp, None, 12345, 12346, "Some Password", None, None)
                .await
                .expect("Failed to initalize storage"),
            temp,
        )
    })
}

fn fetch_augmented_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("fetch_augmented_messages");
    group.significance_level(0.05).sample_size(20);
    let addr = ServiceId::from(Aci::from(
        Uuid::parse_str("92f086c2-9316-4860-94f8-c6878e87a847").unwrap(),
    ));
    for elements in (9..18).map(|x| 1 << x) {
        group.throughput(Throughput::Elements(elements));
        for attachments in 0..3 {
            // for receipts in (0..6) {
            let (mut storage, _loc) = storage();
            // Insert `elements` messages
            let session = storage.fetch_or_insert_session_by_address(&addr);
            for _ in 0..elements {
                let msg = storage.create_message(&NewMessage {
                    session_id: session.id,
                    source_addr: Some(addr),
                    text: "Foo bar".into(),
                    timestamp: chrono::Utc::now().naive_utc(),
                    sent: false,
                    received: false,
                    is_read: false,
                    flags: 0,
                    outgoing: false,
                    is_unidentified: false,
                    quote_timestamp: None,
                    expires_in: None,
                    server_guid: None,
                    story_type: orm::StoryType::None,
                    body_ranges: None,
                    message_type: None,
                    edit: None,
                    expire_timer_version: 1,
                });
                for _attachment in 0..attachments {
                    storage.register_attachment(msg.id, AttachmentPointer::default());
                }
                // for _receipt in 0..receipts {
                //     storage.register_attachment(msg.id, "", "");
                // }
            }
            group.bench_with_input(
                BenchmarkId::from_parameter(format!(
                    "{} messages/{} attachments",
                    elements, attachments
                )),
                &elements,
                move |b, _| {
                    // Now benchmark the retrieve function
                    b.iter(|| black_box(storage.fetch_all_messages_augmented(session.id, true)))
                },
            );
            // }
        }
    }
    group.finish();
}

criterion_group!(benches, fetch_augmented_messages);
criterion_main!(benches);
