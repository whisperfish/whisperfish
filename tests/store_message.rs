use std::future::Future;

use rstest::rstest;

use chrono::prelude::*;

use harbour_whisperfish::store::GroupV1;
use harbour_whisperfish::store::NewMessage;

mod common;
use common::*;

#[rstest]
#[actix_rt::test]
async fn fetch_session_none(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let session = storage.fetch_session_by_id(1);
    assert!(session.is_none());
}

#[rstest]
#[actix_rt::test]
async fn insert_and_fetch_dm(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let e164 = "+358501234567";

    let inserted = storage.fetch_or_insert_session_by_e164(e164);
    assert_eq!(inserted.id, 1);

    let session = storage.fetch_session_by_id(inserted.id).unwrap();
    let recipient = session.unwrap_dm();

    assert_eq!(session.id, inserted.id);
    assert_eq!(recipient.e164, Some(e164.into()));
}

#[rstest]
#[actix_rt::test]
async fn insert_and_fetch_group_session(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let group_id_hex = "1213dc10";
    let group_id = hex::decode(group_id_hex).unwrap();

    let group = GroupV1 {
        id: group_id,
        name: "Spurdospärde".into(),
        members: vec!["+32474".into(), "+32475".into()],
    };

    let inserted = storage.fetch_or_insert_session_by_group_v1(&group);

    let session = storage.fetch_session_by_id(inserted.id).unwrap();
    let fetched_group = session.unwrap_group_v1();

    assert_eq!(session.id, 1);
    assert_eq!(fetched_group.id, group_id_hex);
    assert_eq!(fetched_group.name, group.name);

    let mut members = storage.fetch_group_members_by_group_v1_id(&fetched_group.id);
    members.sort_by_key(|(_member, recipient)| recipient.e164.clone());

    assert_eq!(members.len(), group.members.len());
    assert_eq!(
        members[0].1.e164.as_deref(),
        Some(&group.members[0] as &str)
    );
    assert_eq!(
        members[1].1.e164.as_deref(),
        Some(&group.members[1] as &str)
    );
}

#[rstest]
#[actix_rt::test]
async fn fetch_two_distinct_session(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let session_1_inserted = storage.fetch_or_insert_session_by_e164("+32474");
    let session_2_inserted = storage.fetch_or_insert_session_by_e164("+32475");

    assert_ne!(session_1_inserted.id, session_2_inserted.id);

    // Test retrieving the sessions in reverse order
    let session = storage.fetch_session_by_id(session_2_inserted.id).unwrap();
    let recipient = session.unwrap_dm();
    assert_eq!(session.id, 2);
    assert_eq!(recipient.e164.as_deref(), Some("+32475"));

    let session = storage.fetch_session_by_id(session_1_inserted.id).unwrap();
    let recipient = session.unwrap_dm();
    assert_eq!(session.id, 1);
    assert_eq!(recipient.e164.as_deref(), Some("+32474"));
}

#[rstest]
#[actix_rt::test]
async fn fetch_messages_without_session(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let messages = storage.fetch_all_messages(1);
    assert_eq!(messages.len(), 0);
}

#[rstest]
#[actix_rt::test]
async fn process_message_no_session_source(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    // First assert that session 1 does not exist.
    let session = storage.fetch_session_by_id(1);
    assert!(session.is_none());

    // Now try to add a message.
    let new_message = NewMessage {
        session_id: Some(1),
        source_e164: Some(String::from("a number")),
        source_uuid: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: false,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    let (msg_inserted, session_inserted) = storage.process_message(new_message.clone(), None);

    // Test a session was created
    let session_fetch = storage
        .fetch_session_by_id(session_inserted.id)
        .expect("session has been created");
    assert_eq!(session_fetch.id, session_inserted.id);

    assert_eq!(
        msg_inserted.text.as_deref(),
        Some(&new_message.text as &str)
    );
}

#[rstest]
#[actix_rt::test]
async fn process_message_unresolved_session_source_resolved(
    storage: impl Future<Output = InMemoryDb>,
) {
    let (storage, _temp_dir) = storage.await;

    let new_message = NewMessage {
        session_id: None,
        source_e164: Some(String::from("a number")),
        source_uuid: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    let (_msg, session) = storage.process_message(new_message, None);

    let messages = storage.fetch_all_messages(session.id);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].0.session_id, session.id);
}

#[rstest]
#[actix_rt::test]
async fn process_message_exists_session_source(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let sess1 = storage.fetch_or_insert_session_by_e164("+358501234567");

    for second in 1..11 {
        let timestamp = Utc.timestamp(second, 0).naive_utc();

        let new_message = NewMessage {
            session_id: Some(1),
            source_e164: Some(String::from("+358501234567")),
            source_uuid: None,
            text: String::from("nyt joni ne velat!"),
            timestamp,
            sent: false,
            received: true,
            is_read: true,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: false,
        };

        let (msg, session) = storage.process_message(new_message, None);
        assert_eq!(session.id, sess1.id);

        // Test no extra session was created
        let sessions = storage.fetch_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, sess1.id);

        assert_eq!(msg.server_timestamp, timestamp);
    }
}

/// This tests code that may potentially be removed after release
/// but it's important as long as we receive messages without ACK
#[rstest]
#[ignore]
#[actix_rt::test]
async fn dev_message_update(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let session = storage.fetch_or_insert_session_by_e164("+358501234567");

    let timestamp = Utc::now().naive_utc();
    // Receive basic message
    let new_message = NewMessage {
        session_id: Some(session.id),
        source_e164: Some(String::from("+358501234567")),
        source_uuid: None,
        text: String::from("nyt joni ne velat!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    storage.process_message(new_message, None);

    // Though this is tested in other cases, double-check a message exists
    let db_messages = storage.fetch_all_messages(1);
    assert_eq!(db_messages.len(), 1);

    // However, there should have been an attachment
    // which the Go worker would do before `process_message`
    let other_message = NewMessage {
        session_id: Some(session.id),
        source_e164: Some(String::from("+358501234567")),
        source_uuid: None,
        text: String::from("nyt joni ne velat!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        attachment: Some(String::from("uuid-uuid-uuid-uuid")),
        mime_type: Some(String::from("text/plain")),
        has_attachment: true,
        outgoing: false,
    };

    storage.process_message(other_message, None);

    // And all the messages should still be only one message
    let db_messages = storage.fetch_all_messages(1);
    assert_eq!(db_messages.len(), 1);
}

#[rstest]
#[actix_rt::test]
#[should_panic]
async fn process_inbound_group_message_without_sender(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let new_message = NewMessage {
        session_id: None,
        source_e164: None,
        source_uuid: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let group = GroupV1 {
        id: group_id,
        name: String::from("Spurdospärde"),
        members: vec![
            String::from("Joni"),
            String::from("Make"),
            String::from("Spurdoliina"),
        ],
    };

    let (message_inserted, session_inserted) =
        storage.process_message(new_message, Some(&group.into()));

    // Test a session was created
    let session = storage
        .fetch_session_by_id(session_inserted.id)
        .expect("created session");
    let group = session.unwrap_group_v1();
    assert_eq!(&group.name, ("Spurdospärde"));
    assert_eq!(&group.id, ("2a7e474b"));

    assert_eq!(message_inserted.session_id, session.id);
}

#[rstest]
#[actix_rt::test]
async fn process_outbound_group_message_without_sender(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let new_message = NewMessage {
        session_id: Some(1),
        source_e164: None,
        source_uuid: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: true,
    };

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let group = GroupV1 {
        id: group_id,
        name: String::from("Spurdospärde"),
        members: vec![
            String::from("Joni"),
            String::from("Make"),
            String::from("Spurdoliina"),
        ],
    };

    let (message_inserted, session_inserted) =
        storage.process_message(new_message, Some(&group.into()));

    // Test a session was created
    let session = storage
        .fetch_session_by_id(session_inserted.id)
        .expect("created session");
    let group = session.unwrap_group_v1();
    assert_eq!(&group.name, ("Spurdospärde"));
    assert_eq!(&group.id, ("2a7e474b"));

    assert_eq!(message_inserted.session_id, session.id);
}

#[rstest]
#[actix_rt::test]
async fn process_message_with_group(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let new_message = NewMessage {
        session_id: Some(1),
        source_e164: Some(String::from("Joni")),
        source_uuid: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let group = GroupV1 {
        id: group_id,
        name: String::from("Spurdospärde"),
        members: vec![
            String::from("Joni"),
            String::from("Make"),
            String::from("Spurdoliina"),
        ],
    };

    let (message_inserted, session_inserted) =
        storage.process_message(new_message, Some(&group.into()));

    // Test a session was created
    let session = storage
        .fetch_session_by_id(session_inserted.id)
        .expect("created session");
    let group = session.unwrap_group_v1();
    assert_eq!(&group.name, ("Spurdospärde"));
    assert_eq!(&group.id, ("2a7e474b"));

    assert_eq!(message_inserted.session_id, session.id);
}

// XXX: These tests worked back when Storage had the message_handler implemented.
// This has since been moved to ClientActor, and testing that requires Qt-enabled tests.
// https://gitlab.com/rubdos/whisperfish/-/issues/82

// #[rstest]
// fn message_handler_without_group(storage: InMemoryDb) {
//     setup_db(&storage);
//
//     let res = storage.fetch_session(1);
//     assert!(res.is_none());
//
//     let msg = svcmodels::Message {
//         source: String::from("8483"),
//         message: String::from("sup"),
//         attachments: Vec::new(),
//         group: None,
//         timestamp: 0u64,
//         flags: 0u32,
//     };
//
//     storage.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = storage
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(!session.is_group);
//
//     // Test a message was created
//     let message = storage
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.sid, session.id);
// }

// #[rstest]
// fn message_handler_leave_group(storage: InMemoryDb) {
//     setup_db(&storage);
//
//     let res = storage.fetch_session(1);
//     assert!(res.is_none());
//
//     let group_id = vec![42u8, 126u8, 71u8, 75u8];
//     let group = svcmodels::Group {
//         id: group_id.clone(),
//         hex_id: hex::encode(group_id.clone()),
//         flags: GROUP_LEAVE_FLAG,
//         name: String::from("Spurdospärde"),
//         members: vec![
//             String::from("Joni"),
//             String::from("Make"),
//             String::from("Spurdoliina"),
//         ],
//         avatar: None,
//     };
//
//     let msg = svcmodels::Message {
//         source: String::from("8483"),
//         message: String::from("Spurdoliina went away or something"),
//         attachments: Vec::new(),
//         group: Some(group),
//         timestamp: 0u64,
//         flags: 0u32,
//     };
//
//     storage.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = storage
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(session.is_group);
//
//     // Test a message was created
//     let message = storage
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.message, "Member left group");
//     assert_eq!(message.sid, session.id);
// }

// #[rstest]
// fn message_handler_join_group(storage: InMemoryDb) {
//     setup_db(&storage);
//
//     let res = storage.fetch_session(1);
//     assert!(res.is_none());
//
//     let group_id = vec![42u8, 126u8, 71u8, 75u8];
//     let group = svcmodels::Group {
//         id: group_id.clone(),
//         hex_id: hex::encode(group_id.clone()),
//         flags: GROUP_UPDATE_FLAG,
//         name: String::from("Spurdospärde"),
//         members: vec![String::from("Joni"), String::from("Make")],
//         avatar: None,
//     };
//
//     let msg = svcmodels::Message {
//         source: String::from("8483"),
//         message: String::from("Spurdoliina came back or something"),
//         attachments: Vec::new(),
//         group: Some(group),
//         timestamp: 0u64,
//         flags: 0u32,
//     };
//
//     storage.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = storage
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(session.is_group);
//
//     // Test a message was created
//     let message = storage
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.message, "Member joined group");
//     assert_eq!(message.sid, session.id);
// }

// #[rstest]
// fn message_handler_group_attachment_no_save(storage: InMemoryDb) {
//     setup_db(&storage);
//
//     let res = storage.fetch_session(1);
//     assert!(res.is_none());
//
//     let group_id = vec![42u8, 126u8, 71u8, 75u8];
//     let group = svcmodels::Group {
//         id: group_id.clone(),
//         hex_id: hex::encode(group_id.clone()),
//         flags: 0,
//         name: String::from("Spurdospärde"),
//         members: vec![
//             String::from("Joni"),
//             String::from("Make"),
//             String::from("Spurdoliina"),
//         ],
//         avatar: None,
//     };
//
//     let attachment = svcmodels::Attachment::<u8> {
//         reader: 0u8,
//         mime_type: String::from("image/jpg"),
//     };
//
//     let msg = svcmodels::Message {
//         source: String::from("8483"),
//         message: String::from("KIKKI HIIREN KUVA:DDD"),
//         attachments: vec![attachment],
//         group: Some(group),
//         timestamp: 0u64,
//         flags: 0u32,
//     };
//
//     storage.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = storage
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(session.is_group);
//
//     // Test a message was created
//     let message = storage
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.message, "KIKKI HIIREN KUVA:DDD");
//     assert_eq!(message.sid, session.id);
//
//     // By default, attachments are not saved, so this should not exist
//     assert!(message.attachment.is_none());
// }
