use rstest::rstest;

use chrono::prelude::*;

use harbour_whisperfish::store::{NewGroup, NewMessage, NewSession};

mod common;
use common::*;

#[rstest]
fn test_fetch_session_none(in_memory_db: InMemoryDb) {
    let in_memory_db = in_memory_db.0;
    setup_db(&in_memory_db);

    let session = in_memory_db.fetch_session(1);
    assert!(session.is_none());
}

#[rstest]
fn test_fetch_group_session(in_memory_db: InMemoryDb) {
    let in_memory_db = in_memory_db.0;
    let session_config = NewSession {
        source: String::from("+358501234567"),
        message: String::from("whisperfish on paras:DDDD ja signal:DDD"),
        timestamp: Utc::now().naive_utc(),
        sent: true,
        received: false,
        unread: false,
        is_group: true,
        group_id: None,
        group_name: Some(String::from("Spurdospärde")),
        group_members: Some(String::from("Joni,Viljami,Make,Spurdoliina")),
        has_attachment: false,
    };

    setup_db(&in_memory_db);
    setup_session(&in_memory_db, &session_config);

    let res = in_memory_db.fetch_session(1);
    assert!(res.is_some());

    let session = res.unwrap();
    assert_eq!(session.id, 1);
    assert_eq!(session.source, String::from("+358501234567"));
    assert_eq!(session.group_name, Some(String::from("Spurdospärde")));
    assert_eq!(
        session.message,
        String::from("whisperfish on paras:DDDD ja signal:DDD")
    );
}

#[rstest]
fn test_fetch_session_with_other(in_memory_db: InMemoryDb) {
    let in_memory_db = in_memory_db.0;
    let session_config_1 = NewSession {
        source: String::from("foo"),
        message: String::from("first"),
        timestamp: Utc::now().naive_utc(),
        sent: true,
        received: false,
        unread: false,
        is_group: false,
        group_id: None,
        group_name: Some(String::from("")),
        group_members: Some(String::from("")),
        has_attachment: false,
    };

    let session_config_2 = NewSession {
        source: String::from("31337"),
        message: String::from("31337"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        unread: false,
        is_group: false,
        group_id: None,
        group_name: Some(String::from("")),
        group_members: Some(String::from("")),
        has_attachment: false,
    };

    setup_db(&in_memory_db);
    setup_session(&in_memory_db, &session_config_1);
    setup_session(&in_memory_db, &session_config_2);

    // Test retrieving the sessions in reverse order

    let res = in_memory_db.fetch_session(2);
    assert!(res.is_some());

    let session = res.unwrap();
    assert_eq!(session.id, 2);
    assert_eq!(session.source, String::from("31337"));
    assert_eq!(session.message, String::from("31337"));
    assert_eq!(session.group_name, Some(String::from("")));

    let res = in_memory_db.fetch_session(1);
    assert!(res.is_some());

    let session = res.unwrap();
    assert_eq!(session.id, 1);
    assert_eq!(session.source, String::from("foo"));
    assert_eq!(session.message, String::from("first"));
    assert_eq!(session.group_name, Some(String::from("")));
}

#[rstest]
fn test_fetch_all_messages_none(in_memory_db: InMemoryDb) {
    let in_memory_db = in_memory_db.0;
    setup_db(&in_memory_db);

    let messages = in_memory_db.fetch_all_messages(1).unwrap();
    assert_eq!(messages.len(), 0);
}

#[rstest]
fn test_receive_messages_no_session(in_memory_db: InMemoryDb) {
    let in_memory_db = in_memory_db.0;
    setup_db(&in_memory_db);

    let mut new_messages: Vec<NewMessage> = Vec::new();

    for i in 1..4 {
        new_messages.push(NewMessage {
            session_id: Some(1),
            source: String::from("a number"),
            text: String::from(format!("MSG {}", i)),
            timestamp: Utc::now().naive_utc(),
            sent: false,
            received: true,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: false,
        });
    }

    let inserted = setup_messages(&in_memory_db, new_messages);
    assert_eq!(inserted, 3);

    // Now testify SQLite sucks
    let res = in_memory_db.fetch_session(1);
    assert!(res.is_none());
}

#[rstest]
fn test_process_message_no_session_source(in_memory_db: InMemoryDb) {
    let mut in_memory_db = in_memory_db.0;
    setup_db(&in_memory_db);

    let res = in_memory_db.fetch_session(1);
    assert!(res.is_none());

    let new_message = NewMessage {
        session_id: Some(1),
        source: String::from("a number"),
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    in_memory_db.process_message(new_message, None, true);

    // Test a session was created
    let res = in_memory_db.fetch_session(1);
    assert!(res.is_some());

    // Test a message was created
    let res = in_memory_db.fetch_latest_message();
    assert!(res.is_some());
}

#[rstest]
fn test_process_message_unresolved_session_source_resolved(in_memory_db: InMemoryDb) {
    let mut in_memory_db = in_memory_db.0;
    setup_db(&in_memory_db);

    let new_message = NewMessage {
        session_id: None,
        source: String::from("a number"),
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    in_memory_db.process_message(new_message, None, true);

    let messages = in_memory_db.fetch_all_messages(1).unwrap();
    assert_eq!(messages.len(), 1);

    let res = in_memory_db.fetch_latest_message();
    assert!(res.is_some());

    let msg = res.unwrap();
    assert_eq!(msg.sid, 1);
}

#[rstest]
fn test_process_message_exists_session_source(in_memory_db: InMemoryDb) {
    let mut in_memory_db = in_memory_db.0;
    let session_config = NewSession {
        source: String::from("+358501234567"),
        message: String::from("whisperfish on paras:DDDD ja signal:DDD"),
        timestamp: Utc.timestamp(0, 0).naive_utc(),
        sent: true,
        received: false,
        unread: false,
        is_group: false,
        group_id: None,
        group_name: None,
        group_members: None,
        has_attachment: false,
    };

    setup_db(&in_memory_db);
    setup_session(&in_memory_db, &session_config);

    let sess1_res = in_memory_db.fetch_session(1);
    assert!(sess1_res.is_some());
    let sess1 = sess1_res.unwrap();
    assert_eq!(sess1.timestamp, Utc.timestamp(0, 0).naive_utc());

    for second in 1..11 {
        let timestamp = Utc.timestamp(second, 0).naive_utc();

        let new_message = NewMessage {
            session_id: Some(1),
            source: String::from("+358501234567"),
            text: String::from("nyt joni ne velat!"),
            timestamp,
            sent: false,
            received: true,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: false,
        };

        in_memory_db.process_message(new_message, None, true);

        // Test no extra session was created
        let latest_sess_res = in_memory_db.fetch_latest_session();
        assert!(latest_sess_res.is_some());
        let latest_sess = latest_sess_res.unwrap();

        assert_eq!(latest_sess.id, sess1.id);
        assert_eq!(latest_sess.timestamp, timestamp);

        // Test a message was created
        let res = in_memory_db.fetch_latest_message();
        assert!(res.is_some());

        let msg = res.unwrap();
        assert_eq!(msg.timestamp, timestamp);
    }
}

/// This tests code that may potentially be removed after release
/// but it's important as long as we receive messages without ACK
#[rstest]
fn test_dev_message_update(in_memory_db: InMemoryDb) {
    let mut in_memory_db = in_memory_db.0;
    let session_config = NewSession {
        source: String::from("+358501234567"),
        message: String::from("whisperfish on paras:DDDD ja signal:DDD"),
        timestamp: Utc::now().naive_utc(),
        sent: true,
        received: false,
        unread: false,
        is_group: false,
        group_id: None,
        group_name: None,
        group_members: None,
        has_attachment: false,
    };

    setup_db(&in_memory_db);
    setup_session(&in_memory_db, &session_config);

    // Receive basic message
    let new_message = NewMessage {
        session_id: Some(1),
        source: String::from("+358501234567"),
        text: String::from("nyt joni ne velat!"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    in_memory_db.process_message(new_message, None, true);

    // Though this is tested in other cases, double-check a message exists
    let db_messages_res = in_memory_db.fetch_all_messages(1);
    let db_messages = db_messages_res.unwrap();
    assert_eq!(db_messages.len(), 1);

    // However, there should have been an attachment
    // which the Go worker would do before `process_message`
    let other_message = NewMessage {
        session_id: Some(1),
        source: String::from("+358501234567"),
        text: String::from("nyt joni ne velat!"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        flags: 0,
        attachment: Some(String::from("uuid-uuid-uuid-uuid")),
        mime_type: Some(String::from("text/plain")),
        has_attachment: true,
        outgoing: false,
    };

    in_memory_db.process_message(other_message, None, true);

    // And all the messages should still be only one message
    let db_messages_res = in_memory_db.fetch_all_messages(1);
    let db_messages = db_messages_res.unwrap();
    assert_eq!(db_messages.len(), 1);
}

#[rstest]
fn test_process_message_with_group(in_memory_db: InMemoryDb) {
    let mut in_memory_db = in_memory_db.0;
    setup_db(&in_memory_db);

    let res = in_memory_db.fetch_session(1);
    assert!(res.is_none());

    let new_message = NewMessage {
        session_id: Some(1),
        source: String::from("a number"),
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
    };

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let group = NewGroup {
        id: &group_id,
        name: String::from("Spurdospärde"),
        members: vec![
            String::from("Joni"),
            String::from("Make"),
            String::from("Spurdoliina"),
        ],
    };

    in_memory_db.process_message(new_message, Some(group), true);

    // Test a session was created
    let session = in_memory_db
        .fetch_session(1)
        .expect("Expected to find session");
    assert!(session.is_group);
    assert_eq!(session.group_name, Some(String::from("Spurdospärde")));
    assert_eq!(session.group_id, Some(String::from("2a7e474b")));
    assert_eq!(session.source, String::from("2a7e474b"));

    // Test a message was created
    let message = in_memory_db
        .fetch_latest_message()
        .expect("Expected to find message");
    assert_eq!(message.source, "a number");
    assert_eq!(message.sid, session.id);
}

// XXX: These tests worked back when Storage had the message_handler implemented.
// This has since been moved to ClientActor, and testing that requires Qt-enabled tests.
// https://gitlab.com/rubdos/whisperfish/-/issues/82

// #[rstest]
// fn test_message_handler_without_group(in_memory_db: InMemoryDb) {
//     setup_db(&in_memory_db);
//
//     let res = in_memory_db.fetch_session(1);
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
//     in_memory_db.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = in_memory_db
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(!session.is_group);
//
//     // Test a message was created
//     let message = in_memory_db
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.sid, session.id);
// }

// #[rstest]
// fn test_message_handler_leave_group(in_memory_db: InMemoryDb) {
//     setup_db(&in_memory_db);
//
//     let res = in_memory_db.fetch_session(1);
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
//     in_memory_db.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = in_memory_db
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(session.is_group);
//
//     // Test a message was created
//     let message = in_memory_db
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.message, "Member left group");
//     assert_eq!(message.sid, session.id);
// }

// #[rstest]
// fn test_message_handler_join_group(in_memory_db: InMemoryDb) {
//     setup_db(&in_memory_db);
//
//     let res = in_memory_db.fetch_session(1);
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
//     in_memory_db.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = in_memory_db
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(session.is_group);
//
//     // Test a message was created
//     let message = in_memory_db
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.message, "Member joined group");
//     assert_eq!(message.sid, session.id);
// }

// #[rstest]
// fn test_message_handler_group_attachment_no_save(in_memory_db: InMemoryDb) {
//     setup_db(&in_memory_db);
//
//     let res = in_memory_db.fetch_session(1);
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
//     in_memory_db.message_handler(msg, false, 0);
//
//     // Test a session was created
//     let session = in_memory_db
//         .fetch_session(1)
//         .expect("Expected to find session");
//     assert!(session.is_group);
//
//     // Test a message was created
//     let message = in_memory_db
//         .fetch_latest_message()
//         .expect("Expected to find message");
//     assert_eq!(message.source, "8483");
//     assert_eq!(message.message, "KIKKI HIIREN KUVA:DDD");
//     assert_eq!(message.sid, session.id);
//
//     // By default, attachments are not saved, so this should not exist
//     assert!(message.attachment.is_none());
// }
