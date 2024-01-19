use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use libsignal_service::proto::AttachmentPointer;
use whisperfish_store::config::SignalConfig;
use whisperfish_store::{orm, temp, NewMessage, Storage, StorageLocation};

pub type InMemoryDb = (Storage, StorageLocation<tempfile::TempDir>);

pub fn storage() -> InMemoryDb {
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    rt.block_on(async {
        let cfg = SignalConfig::default();
        let cfg = Arc::new(cfg);
        let temp = temp();
        (
            Storage::new(
                cfg,
                &temp,
                None,
                12345,
                12346,
                "Some Password",
                [0; 52],
                None,
                None,
            )
            .await
            .expect("Failed to initalize storage"),
            temp,
        )
    })
}

fn fetch_augmented_messages(c: &mut Criterion) {
    let mut group = c.benchmark_group("fetch_augmented_messages");
    group.significance_level(0.05).sample_size(20);
    let pn = phonenumber::parse(None, "+32474000000").unwrap();
    for elements in (9..18).map(|x| 1 << x) {
        group.throughput(Throughput::Elements(elements));
        for attachments in 0..3 {
            // for receipts in (0..6) {
            let (mut storage, _loc) = storage();
            // Insert `elements` messages
            let session = storage.fetch_or_insert_session_by_phonenumber(&pn);
            for _ in 0..elements {
                let msg = storage.create_message(&NewMessage {
                    session_id: session.id,
                    source_e164: Some(pn.clone()),
                    source_uuid: None,
                    text: "Foo bar".into(),
                    timestamp: chrono::Utc::now().naive_utc(),
                    sent: false,
                    received: false,
                    is_read: false,
                    flags: 0,
                    attachment: None,
                    mime_type: None,
                    has_attachment: false,
                    outgoing: false,
                    is_unidentified: false,
                    quote_timestamp: None,
                    expires_in: None,
                    server_guid: None,
                    story_type: orm::StoryType::None,
                    body_ranges: None,
                    edit: None,
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
