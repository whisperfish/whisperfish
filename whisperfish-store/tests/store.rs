mod common;

use self::common::*;
use chrono::prelude::*;
use libsignal_service::content::Reaction;
use libsignal_service::proto::DataMessage;
use libsignal_service::protocol::{Aci, ServiceId};
use phonenumber::PhoneNumber;
use rstest::rstest;
use std::future::Future;
use std::sync::Arc;
use whisperfish_store::config::SignalConfig;
use whisperfish_store::orm::{Receipt, Recipient, StoryType, UnidentifiedAccessMode};
use whisperfish_store::{naive_chrono_to_millis, GroupV1, NewMessage};

#[rstest]
#[tokio::test]
async fn fetch_session_none(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let session = storage.fetch_session_by_id(1);
    assert!(session.is_none());
}

#[rstest]
#[tokio::test]
async fn insert_and_fetch_dm(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let phonenumber = phonenumber::parse(None, "+358501234567").unwrap();

    let inserted = storage.fetch_or_insert_session_by_phonenumber(&phonenumber);
    assert_eq!(inserted.id, 1);

    let session = storage.fetch_session_by_id(inserted.id).unwrap();
    let recipient = session.unwrap_dm();

    assert_eq!(session.id, inserted.id);
    assert_eq!(recipient.e164, Some(phonenumber));
}

#[rstest]
#[tokio::test]
async fn insert_and_fetch_group_session(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let group_id_hex = "1213dc10";
    let group_id = hex::decode(group_id_hex).unwrap();

    let group = GroupV1 {
        id: group_id,
        name: "Spurdospärde".into(),
        members: vec![
            phonenumber::parse(None, "+32474000000").unwrap(),
            phonenumber::parse(None, "+32475000000").unwrap(),
        ],
    };

    let inserted = storage.fetch_or_insert_session_by_group_v1(&group);

    let session = storage.fetch_session_by_id(inserted.id).unwrap();
    let fetched_group = session.unwrap_group_v1();

    assert_eq!(session.id, 1);
    assert_eq!(fetched_group.id, group_id_hex);
    assert_eq!(fetched_group.name, group.name);

    let mut members = storage.fetch_group_members_by_group_v1_id(&fetched_group.id);
    members.sort_by_key(|(_member, recipient)| recipient.e164.as_ref().map(PhoneNumber::to_string));

    assert_eq!(members.len(), group.members.len());
    assert_eq!(members[0].1.e164.as_ref(), Some(&group.members[0]));
    assert_eq!(members[1].1.e164.as_ref(), Some(&group.members[1]));
}

#[rstest]
#[tokio::test]
async fn fetch_two_distinct_session(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let pn1 = phonenumber::parse(None, "+32474000000").unwrap();
    let pn2 = phonenumber::parse(None, "+32475000000").unwrap();

    let session_1_inserted = storage.fetch_or_insert_session_by_phonenumber(&pn1);
    let session_2_inserted = storage.fetch_or_insert_session_by_phonenumber(&pn2);

    assert_ne!(session_1_inserted.id, session_2_inserted.id);

    // Test retrieving the sessions in reverse order
    let session = storage.fetch_session_by_id(session_2_inserted.id).unwrap();
    let recipient = session.unwrap_dm();
    assert_eq!(session.id, 2);
    assert_eq!(recipient.e164.as_ref(), Some(&pn2));

    let session = storage.fetch_session_by_id(session_1_inserted.id).unwrap();
    let recipient = session.unwrap_dm();
    assert_eq!(session.id, 1);
    assert_eq!(recipient.e164.as_ref(), Some(&pn1));
}

#[rstest]
#[tokio::test]
async fn fetch_messages_without_session(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let messages = storage.fetch_all_messages(1, true);
    assert_eq!(messages.len(), 0);
}

#[rstest]
#[tokio::test]
async fn process_message_exists_session_source(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let addr1 = ServiceId::from(Aci::from(uuid::Uuid::new_v4()));
    let sess1 = storage.fetch_or_insert_session_by_address(&addr1);

    for second in 1..11 {
        let timestamp = Utc.timestamp_opt(second, 0).unwrap().naive_utc();

        let new_message = NewMessage {
            session_id: sess1.id,
            source_addr: Some(addr1),
            text: String::from("nyt joni ne velat!"),
            timestamp,
            sent: false,
            received: true,
            is_read: true,
            flags: 0,
            outgoing: false,
            is_unidentified: false,
            quote_timestamp: None,
            expires_in: None,
            expire_timer_version: sess1.expire_timer_version,
            server_guid: None,
            story_type: StoryType::None,
            body_ranges: None,
            message_type: None,

            edit: None,
        };

        let msg = storage.create_message(&new_message);

        // Test no extra session was created
        let sessions = storage.fetch_sessions();
        assert_eq!(sessions.len(), 1);
        assert_eq!(sessions[0].id, sess1.id);

        assert_eq!(msg.server_timestamp, timestamp);
    }
}

#[rstest]
#[tokio::test]
async fn message_searching(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let addr1 = ServiceId::from(Aci::from(uuid::Uuid::new_v4()));
    let sess1 = storage.fetch_or_insert_session_by_address(&addr1);

    let messages = vec!["test", "100% test", "trust me bro it's fine"];
    for (second, message) in messages.into_iter().enumerate() {
        let timestamp = Utc.timestamp_opt(second as i64, 0).unwrap().naive_utc();

        let new_message = NewMessage {
            session_id: sess1.id,
            source_addr: Some(addr1),
            text: String::from(message),
            timestamp,
            expire_timer_version: sess1.expire_timer_version,
            ..NewMessage::new_incoming()
        };

        let _ = storage.create_message(&new_message);
    }

    let mut res;
    let mut iter;
    // case match
    res = storage.search_messages(&String::from("test"), None);
    iter = res.iter();
    assert_eq!(2, res.len());
    assert_eq!(
        &String::from("100% test"),
        iter.next().and_then(|m| m.text.as_ref()).unwrap()
    );
    assert_eq!(
        &String::from("test"),
        iter.next().and_then(|m| m.text.as_ref()).unwrap()
    );

    // case insensitive
    res = storage.search_messages(&String::from("TEST"), None);
    iter = res.iter();
    assert_eq!(2, res.len());
    assert_eq!(
        &String::from("100% test"),
        iter.next().and_then(|m| m.text.as_ref()).unwrap()
    );
    assert_eq!(
        &String::from("test"),
        iter.next().and_then(|m| m.text.as_ref()).unwrap()
    );

    // no matches
    res = storage.search_messages(&String::from("noting matches"), None);
    assert_eq!(0, res.len());

    // use wildcard character %
    res = storage.search_messages(&String::from("100%"), None);
    iter = res.iter();
    assert_eq!(1, res.len());
    assert_eq!(
        &String::from("100% test"),
        iter.next().and_then(|m| m.text.as_ref()).unwrap()
    );

    // use escape character '
    res = storage.search_messages(&String::from("it's"), None);
    iter = res.iter();
    assert_eq!(1, res.len());
    assert_eq!(
        &String::from("trust me bro it's fine"),
        iter.next().and_then(|m| m.text.as_ref()).unwrap()
    );

    // bad actor
    res = storage.search_messages(&String::from("'; DROP TABLE messages;\n --"), None);
    assert_eq!(0, res.len());
    res = storage.search_messages(&String::from("fine"), None);
    assert_eq!(1, res.len());

    // same session
    res = storage.search_messages(&String::from("bro"), Some(sess1.id));
    assert_eq!(1, res.len());

    // wrong session
    res = storage.search_messages(&String::from("trust"), Some(sess1.id + 1));
    assert_eq!(0, res.len());

    // too short string
    res = storage.search_messages(&String::from("t"), Some(sess1.id));
    assert_eq!(0, res.len());
}

#[rstest]
#[tokio::test]
async fn test_two_edits(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let addr1 = ServiceId::from(Aci::from(uuid::Uuid::new_v4()));
    let sess1 = storage.fetch_or_insert_session_by_address(&addr1);

    let timestamp = Utc.timestamp_opt(1, 0).unwrap().naive_utc();

    let new_message = NewMessage {
        session_id: sess1.id,
        source_addr: Some(addr1),
        text: String::from("nyt joni ne velat! Woops this is a typo!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: sess1.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    let msg = storage.create_message(&new_message);

    // Test no extra session was created
    let sessions = storage.fetch_sessions();
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0].id, sess1.id);

    assert_eq!(msg.server_timestamp, timestamp);
    assert_eq!(msg.original_message_id, None);

    let timestamp = Utc.timestamp_opt(2, 0).unwrap().naive_utc();
    let newer_message = NewMessage {
        session_id: sess1.id,
        source_addr: Some(addr1),
        text: String::from("nyt joni ne velat!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: sess1.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: Some(&msg),
    };

    let newer_msg = storage.create_message(&newer_message);
    assert_eq!(newer_msg.original_message_id, Some(msg.id));

    let timestamp = Utc.timestamp_opt(3, 0).unwrap().naive_utc();
    let newerer_message = NewMessage {
        session_id: sess1.id,
        source_addr: Some(addr1),
        text: String::from("nyt joni ne velat!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: sess1.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: Some(&msg),
    };

    let newerer_msg = storage.create_message(&newerer_message);
    assert_eq!(newerer_msg.original_message_id, Some(msg.id));

    let fetch_all = storage.fetch_all_messages(sess1.id, true);
    assert_eq!(fetch_all.len(), 1);
    assert_eq!(
        fetch_all[0].id, newerer_msg.id,
        "fetch_all_messages should only return the most recent edit"
    );

    let fetch_all_history = storage.fetch_all_messages(sess1.id, false);
    assert_eq!(fetch_all_history.len(), 3);

    //
}

/// This tests code that may potentially be removed after release
/// but it's important as long as we receive messages without ACK
#[rstest]
#[ignore]
#[tokio::test]
async fn dev_message_update(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let addr1 = ServiceId::from(Aci::from(uuid::Uuid::new_v4()));
    let session = storage.fetch_or_insert_session_by_address(&addr1);

    let timestamp = Utc::now().naive_utc();
    // Receive basic message
    let new_message = NewMessage {
        session_id: session.id,
        source_addr: Some(addr1),
        text: String::from("nyt joni ne velat!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: session.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    storage.create_message(&new_message);

    // Though this is tested in other cases, double-check a message exists
    let db_messages = storage.fetch_all_messages(session.id, true);
    assert_eq!(db_messages.len(), 1);

    // However, there should have been an attachment
    // which the Go worker would do before `process_message`
    let other_message = NewMessage {
        session_id: session.id,
        source_addr: Some(addr1),
        text: String::from("nyt joni ne velat!"),
        timestamp,
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: session.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    storage.create_message(&other_message);

    // And all the messages should still be only one message
    let db_messages = storage.fetch_all_messages(session.id, true);
    assert_eq!(db_messages.len(), 1);
}

#[rstest]
#[tokio::test]
#[should_panic]
async fn process_inbound_group_message_without_sender(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let pn1 = phonenumber::parse(None, "+358501234567").unwrap();
    let pn2 = phonenumber::parse(None, "+358501234568").unwrap();
    let pn3 = phonenumber::parse(None, "+358501234569").unwrap();
    let group = GroupV1 {
        id: group_id,
        name: String::from("Spurdospärde"),
        members: vec![pn1, pn2, pn3],
    };

    let session = storage.fetch_or_insert_session_by_group_v1(&group);

    // Test a session was created
    let group = session.unwrap_group_v1();
    assert_eq!(&group.name, ("Spurdospärde"));
    assert_eq!(&group.id, ("2a7e474b"));

    let new_message = NewMessage {
        session_id: session.id,
        source_addr: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: session.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    let message_inserted = storage.create_message(&new_message);

    assert_eq!(message_inserted.session_id, session.id);
}

#[rstest]
#[tokio::test]
async fn process_outbound_group_message_without_sender(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let pn1 = phonenumber::parse(None, "+358501234567").unwrap();
    let pn2 = phonenumber::parse(None, "+358501234568").unwrap();
    let pn3 = phonenumber::parse(None, "+358501234569").unwrap();
    let group = GroupV1 {
        id: group_id,
        name: String::from("Spurdospärde"),
        members: vec![pn1, pn2, pn3],
    };

    let session = storage.fetch_or_insert_session_by_group_v1(&group);

    // Test a session was created
    let group = session.unwrap_group_v1();
    assert_eq!(&group.name, ("Spurdospärde"));
    assert_eq!(&group.id, ("2a7e474b"));

    let new_message = NewMessage {
        session_id: session.id,
        source_addr: None,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: true,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: session.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    let message_inserted = storage.create_message(&new_message);

    assert_eq!(message_inserted.session_id, session.id);
}

#[rstest]
#[tokio::test]
async fn process_message_with_group(storage: impl Future<Output = InMemoryDb>) {
    let (storage, _temp_dir) = storage.await;

    let pn1 = phonenumber::parse(None, "+358501234567").unwrap();
    let pn2 = phonenumber::parse(None, "+358501234568").unwrap();
    let pn3 = phonenumber::parse(None, "+358501234569").unwrap();

    // Give the recipients ACI
    for pn in [&pn1, &pn2, &pn3] {
        let uuid1 = uuid::Uuid::new_v4();
        let r1 = storage.fetch_or_insert_recipient_by_phonenumber(pn);
        let r2 = storage.merge_and_fetch_recipient(
            Some(pn.clone()),
            Some(Aci::from(uuid1)),
            None,
            whisperfish_store::TrustLevel::Certain,
        );
        assert!(r1.id == r2.id);
    }

    // Here the client worker will have resolved a group exists
    let group_id = vec![42u8, 126u8, 71u8, 75u8];
    let group = GroupV1 {
        id: group_id,
        name: String::from("Spurdospärde"),
        members: vec![pn1.clone(), pn2, pn3],
    };

    let session = storage.fetch_or_insert_session_by_group_v1(&group);

    let addr1 = storage
        .fetch_recipient_by_e164(&pn1)
        .unwrap()
        .to_service_address();
    assert!(addr1.is_some());

    // Test a session was created
    let group = session.unwrap_group_v1();
    assert_eq!(&group.name, ("Spurdospärde"));
    assert_eq!(&group.id, ("2a7e474b"));

    let new_message = NewMessage {
        session_id: session.id,
        source_addr: addr1,
        text: String::from("MSG 1"),
        timestamp: Utc::now().naive_utc(),
        sent: false,
        received: true,
        is_read: true,
        flags: 0,
        outgoing: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: session.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    let message_inserted = storage.create_message(&new_message);

    assert_eq!(message_inserted.session_id, session.id);
}

// XXX: The method under test was previously without any database relation, and could be tested
// without context.
// #[rstest(ext, case("mp4"), case("jpg"), case("jpg"), case("png"), case("txt"))]
// #[tokio::test]
// async fn test_save_attachment(ext: &str) {
//     use rand::distributions::Alphanumeric;
//     use rand::{Rng, RngCore};
//
//     let location = whisperfish_store::temp();
//     let rng = rand::thread_rng();
//
//     // Signaling password for REST API
//     let password: String = rng.sample_iter(&Alphanumeric).take(24).collect();
//
//     // Signaling key that decrypts the incoming Signal messages
//     let mut rng = rand::thread_rng();
//     let mut signaling_key = [0u8; 52];
//     rng.fill_bytes(&mut signaling_key);
//     let signaling_key = signaling_key;
//
//     // Registration ID
//     let regid = 12345;
//     let pni_regid = 12345;
//
//     let storage = SimpleStorage::new(
//         Arc::new(SignalConfig::default()),
//         &location,
//         None,
//         regid,
//         pni_regid,
//         &password,
//         signaling_key,
//         None,
//         None,
//     )
//     .await
//     .unwrap();
//
//     // Create content for attachment and write to file
//     let content = [1u8; 10];
//     let fname = storage
//         .save_attachment(
//             &storage.path().join("storage").join("attachments"),
//             ext,
//             &content,
//         )
//         .await
//         .unwrap();
//
//     // Check existence of attachment
//     let exists = std::path::Path::new(&fname).exists();
//
//     println!("Looking for {}", fname.to_str().unwrap());
//     assert!(exists);
//
//     assert_eq!(
//         fname.extension().unwrap(),
//         ext,
//         "{} <> {}",
//         fname.to_str().unwrap(),
//         ext
//     );
// }

#[rstest(
    storage_password,
    case(Some(String::from("some password"))),
    case(None)
)]
#[tokio::test]
async fn test_create_and_open_storage(
    storage_password: Option<String>,
) -> Result<(), anyhow::Error> {
    use libsignal_service::pre_keys::PreKeysStore;
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    let location = whisperfish_store::temp();
    let rng = rand::thread_rng();

    // Signaling password for REST API
    let password: String = rng
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    // Registration ID
    let regid = 12345;
    let pni_regid = 12345;

    let storage = SimpleStorage::new(
        Arc::new(SignalConfig::default()),
        &location,
        storage_password.as_deref(),
        regid,
        pni_regid,
        &password,
        None,
        None,
    )
    .await;
    assert!(storage.is_ok(), "{}", storage.err().unwrap());
    let storage = storage.unwrap();
    assert_eq!(storage.is_encrypted(), storage_password.is_some());

    macro_rules! tests {
        ($storage:ident) => {{
            use libsignal_service::protocol::IdentityKeyStore;
            // TODO: assert that tables exist
            assert_eq!(password, $storage.signal_password().await?);
            assert_eq!(None, $storage.signaling_key().await?);
            assert_eq!(
                regid,
                $storage.aci_storage().get_local_registration_id().await?
            );
            assert_eq!(
                pni_regid,
                $storage.pni_storage().get_local_registration_id().await?
            );

            let unsigned = $storage.aci_storage().next_pre_key_id().await?;
            let kyber = $storage.aci_storage().next_pq_pre_key_id().await?;
            let signed = $storage.aci_storage().next_signed_pre_key_id().await?;
            // Unstarted client will have no pre-keys.
            assert_eq!(0, signed);
            assert_eq!(0, kyber);
            assert_eq!(0, unsigned);

            Result::<_, anyhow::Error>::Ok(())
        }};
    }

    tests!(storage)?;
    drop(storage);

    if storage_password.is_some() {
        assert!(
            SimpleStorage::open(Arc::new(SignalConfig::default()), &location, None)
                .await
                .is_err(),
            "Storage was not encrypted"
        );
    }

    let storage = SimpleStorage::open(
        Arc::new(SignalConfig::default()),
        &location,
        storage_password,
    )
    .await;
    assert!(storage.is_ok(), "{}", storage.err().unwrap());
    let storage = storage.unwrap();

    tests!(storage)?;

    Ok(())
}

#[tokio::test]
async fn test_recipient_actions() {
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    let location = whisperfish_store::temp();
    let rng = rand::thread_rng();

    // Signaling password for REST API
    let password: String = rng
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    // Registration ID
    let regid = 12345;
    let pni_regid = 12346;

    let storage = SimpleStorage::new(
        Arc::new(SignalConfig::default()),
        &location,
        None,
        regid,
        pni_regid,
        &password,
        None,
        None,
    )
    .await;
    assert!(storage.is_ok(), "{}", storage.err().unwrap());
    let mut storage = storage.unwrap();

    // It seems this function is completely unused!
    let tmp_write_dir = tempfile::tempdir().unwrap();
    let tmp_write_file = tmp_write_dir.path().join("tmp.file");
    storage
        .write_file(tmp_write_file, "something")
        .await
        .unwrap();

    let uuid1 = uuid::Uuid::new_v4();
    let addr1 = ServiceId::from(Aci::from(uuid1));

    let recip = storage.fetch_or_insert_recipient_by_address(&addr1);

    assert_eq!(
        recip.unidentified_access_mode,
        UnidentifiedAccessMode::Unknown
    );
    storage.set_recipient_unidentified(&recip, UnidentifiedAccessMode::Disabled);
    let recip = storage.fetch_or_insert_recipient_by_address(&addr1);
    assert_eq!(
        recip.unidentified_access_mode,
        UnidentifiedAccessMode::Disabled
    );
    storage.set_recipient_unidentified(&recip, UnidentifiedAccessMode::Enabled);
    let recip = storage.fetch_or_insert_recipient_by_address(&addr1);
    assert_eq!(
        recip.unidentified_access_mode,
        UnidentifiedAccessMode::Enabled
    );
    storage.set_recipient_unidentified(&recip, UnidentifiedAccessMode::Unrestricted);
    let recip = storage.fetch_or_insert_recipient_by_address(&addr1);
    assert_eq!(
        recip.unidentified_access_mode,
        UnidentifiedAccessMode::Unrestricted
    );

    let session = storage.fetch_or_insert_session_by_recipient_id(recip.id);
    let ts = NaiveDateTime::parse_from_str("2023-04-01 07:01:32", "%Y-%m-%d %H:%M:%S").unwrap();
    let msg = NewMessage {
        session_id: session.id,
        timestamp: ts,
        sent: true,
        received: true,
        flags: 0,
        outgoing: true,
        source_addr: Some(addr1),
        text: "Hi!".into(),
        is_read: false,
        is_unidentified: false,
        quote_timestamp: None,
        expires_in: None,
        expire_timer_version: session.expire_timer_version,
        server_guid: None,
        story_type: StoryType::None,
        body_ranges: None,
        message_type: None,

        edit: None,
    };

    let msg = storage.create_message(&msg);
    storage.dequeue_message(msg.id, ts, false);

    assert!(!msg.is_read);
    let pointer = storage.mark_message_read(msg.server_timestamp).unwrap();
    let msg = storage.fetch_message_by_id(pointer.message_id).unwrap();
    assert!(msg.is_read);

    // START DELIVERY AND READ RECEIPTS

    let now = chrono::Utc::now().naive_utc();
    let delivered_1 = now.with_minute(1).unwrap();
    let delivered_2 = now.with_minute(2).unwrap();
    let read_1 = now.with_minute(3).unwrap();
    let read_2 = now.with_minute(4).unwrap();

    // First delivery receipt

    assert!(storage.fetch_message_receipts(msg.id).is_empty());
    let num_delivered = storage
        .mark_messages_delivered(
            ServiceId::from(Aci::from(uuid1)),
            vec![msg.server_timestamp],
            delivered_1,
        )
        .len();
    assert_eq!(num_delivered, 1);
    let receipts = storage.fetch_message_receipts(msg.id);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].0.delivered, Some(delivered_1));

    // Second, later, delivery receipt doesn't update the previous one

    let num_delivered = storage
        .mark_messages_delivered(
            ServiceId::from(Aci::from(uuid1)),
            vec![msg.server_timestamp],
            delivered_2,
        )
        .len();
    assert_eq!(num_delivered, 1);
    let receipts = storage.fetch_message_receipts(msg.id);
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].0.delivered, Some(delivered_1));

    // First read receipt

    let receipts: Vec<(Receipt, Recipient)> = storage
        .fetch_message_receipts(msg.id)
        .into_iter()
        .filter(|(r, _)| r.read.is_some())
        .collect();
    assert!(receipts.is_empty());
    let num_read = storage
        .mark_messages_read(
            ServiceId::from(Aci::from(uuid1)),
            vec![msg.server_timestamp],
            read_1,
        )
        .len();
    assert_eq!(num_read, 1);
    let receipts: Vec<(Receipt, Recipient)> = storage
        .fetch_message_receipts(msg.id)
        .into_iter()
        .filter(|(r, _)| r.read.is_some())
        .collect();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].0.read, Some(read_1));

    // Second, later, read receipt doesn't update the previous one

    let num_read = storage
        .mark_messages_read(
            ServiceId::from(Aci::from(uuid1)),
            vec![msg.server_timestamp],
            read_2,
        )
        .len();
    assert_eq!(num_read, 1);
    let receipts: Vec<(Receipt, Recipient)> = storage
        .fetch_message_receipts(msg.id)
        .into_iter()
        .filter(|(r, _)| r.read.is_some())
        .collect();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].0.read, Some(read_1));

    // END DELIVERY AND READ RECEIPTS

    let reaction = Reaction {
        emoji: Some("❤".into()),
        remove: Some(false),
        target_author_aci: Some(recip.uuid.unwrap().to_string()),
        target_sent_timestamp: Some(naive_chrono_to_millis(msg.server_timestamp)),
    };
    let data_msg = DataMessage {
        body: None,
        attachments: [].to_vec(),
        group_v2: None,
        flags: None,
        expire_timer: None,
        expire_timer_version: None,
        profile_key: Some([0].to_vec()),
        timestamp: Some(naive_chrono_to_millis(msg.server_timestamp)),
        quote: None,
        contact: [].to_vec(),
        preview: [].to_vec(),
        sticker: None,
        required_protocol_version: None,
        is_view_once: None,
        reaction: Some(reaction.clone()),
        delete: None,
        body_ranges: [].to_vec(),
        group_call_update: None,
        payment: None,
        story_context: None,
        gift_badge: None,
    };
    let (m, s) = match storage.process_reaction(&recip, &data_msg, &reaction) {
        Ok(Some((m, s))) => (m, s),
        _ => unreachable!(),
    };
    assert_eq!(m.id, msg.id);
    assert_eq!(s.id, session.id);
    let r = storage.fetch_reactions_for_message(msg.id);
    assert!(!r.is_empty());
    assert_eq!(r.first().unwrap().0.emoji, String::from("❤"));

    let m = storage
        .fetch_last_message_by_session_id_augmented(session.id)
        .unwrap();
    assert_eq!(m.text.as_ref().unwrap(), &msg.text.unwrap());

    assert!(!msg.sending_has_failed);
    storage.fail_message(msg.id);
    let msg = storage.fetch_message_by_id(msg.id).unwrap();
    assert!(msg.sending_has_failed);

    assert_eq!(storage.mark_pending_messages_failed(), 0);

    assert!(storage.fetch_group_sessions().is_empty());
    assert!(storage
        .fetch_session_by_id_augmented(session.id + 1)
        .is_none());
    assert!(storage.fetch_session_by_id_augmented(session.id).is_some());
    assert!(storage.fetch_attachment(42).is_none());
    assert!(storage.fetch_attachments_for_message(msg.id).is_empty());

    assert_eq!(
        storage.fetch_all_messages_augmented(session.id, true).len(),
        1
    );

    assert_eq!(storage.fetch_all_sessions_augmented().len(), 1);

    assert!(!msg.is_remote_deleted);
    assert!(msg.text.is_some());
    storage.delete_message(msg.id);
    let msg = storage.fetch_message_by_id(msg.id).unwrap();
    assert!(msg.is_remote_deleted);
    assert!(msg.text.is_none());
    let r = storage.fetch_reactions_for_message(msg.id);
    assert!(r.is_empty());
    drop(msg);

    // The one deleted message is "only" marked as deleted
    assert_eq!(
        storage.fetch_all_messages_augmented(session.id, true).len(),
        1
    );

    storage.delete_session(session.id);
    assert_eq!(storage.fetch_all_sessions_augmented().len(), 0);
}

// XXX: These tests worked back when Storage had the message_handler implemented.
// This has since been moved to ClientActor, and testing that requires Qt-enabled tests.
// https://gitlab.com/whisperfish/whisperfish/-/issues/82

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

#[test]
/// Test the regex we use to make sure we don't remove attachmets
/// from anywhere else than from 'storage/[attachments|camera]' folders.
fn test_remove_attachment_filenames() {
    let regex = SignalConfig::default().attachments_regex();

    // List of known good and bad locations, feel free to add samples.
    let test_data: [(bool, &str); 21]= [
        // defaultuser, new
        (true, "~/.local/share/be.rubdos/harbour-whisperfish/storage/attachments/5da77b73f271bd460956d3807643f6b8.png"),
        (true, "~/.local/share/be.rubdos/harbour-whisperfish/storage/attachments/Photo_20220417_233207.jpg"),
        (true, "~/.local/share/be.rubdos/harbour-whisperfish/storage/camera/Photo_20220617_183938.jpg"),
        // defaultuser, old
        (true, "~/.local/share/harbour-whisperfish/storage/attachments/d801caeea1cc119aac4fe6a64d1ecc3e.jpg"),
        (true, "~/.local/share/harbour-whisperfish/storage/camera/Photo_20220617_192842.jpg"),
        // nemo, new
        (true, "~/.local/share/be.rubdos/harbour-whisperfish/storage/attachments/3a9f821ec8395b9a6565df0e1a952a85.jpg"),
        (true, "~/.local/share/be.rubdos/harbour-whisperfish/storage/camera/Photo_20230703_174003.jpg"),
        // nemo, old
        (true, "~/.local/share/harbour-whisperfish/storage/attachments/bd09cdd805f5aa07aa3ee950a9b1fef9.pdf"),
        (true, "~/.local/share/harbour-whisperfish/storage/camera/Photo_20221108_202942.jpg"),
        // Android storage
        (false, "~/android_storage/Download/cat-meme.jpg"),
        // Downloads
        (false, "~/Downloads/Photo_20220422_144241.jpg"),
        // Pictures
        (false, "~/Pictures/AdvancedCam/IMG_20210730_160213.jpg"),
        (false, "~/Pictures/Camera/20211103_184820.jpg"),
        (false, "~/Pictures/MMS/mms-20230409.jpg"),
        (false, "~/Pictures/Screenshots/Näyttökuva_20210502_001.png"),
        (false, "~/Pictures/totally_legit_png_just_without_extension"),
        // Videos
        (false, "~/Videos/Camera/20210812_232017.mp4"),
        // MicroSD card
        (false, "/run/media/defaultuser/0123-4567/Pictures/Camera/20220611_203625.jpg"),
        (false, "/run/media/defaultuser/0123-4567/Videos/Camera/20230703_144325.mp4"),
        (false, "/run/media/defaultuser/6738bbbc-5a3b-4505-971e-9f40ff14d51f/Pictures/Camera/20210425_134502.jpg"),
        // Local storage
        (false, "~/.local/share/commhistory/data/1241/image000000.jpg"),
    ];

    let _ = test_data.map(|(deleted, filename)| {
        assert_eq!(
            deleted,
            regex.is_match(filename),
            "{} should {} deleted",
            filename,
            if deleted { "BE" } else { "NOT BE" }
        );
    });
}

#[tokio::test]
async fn settings_table() {
    use rand::distributions::Alphanumeric;
    use rand::Rng;

    let location = whisperfish_store::temp();
    let rng = rand::thread_rng();

    // Signaling password for REST API
    let password: String = rng
        .sample_iter(&Alphanumeric)
        .take(24)
        .map(char::from)
        .collect();

    // Registration ID
    let regid = 12345;
    let pni_regid = 12345;

    let storage = SimpleStorage::new(
        Arc::new(SignalConfig::default()),
        &location,
        None,
        regid,
        pni_regid,
        &password,
        None,
        None,
    )
    .await;
    assert!(storage.is_ok(), "{}", storage.err().unwrap());
    let storage = storage.unwrap();

    let k1 = "key1";
    let k2 = "key2";
    let k3 = "key3";
    let v1 = "value1".to_string();
    let v2 = "value2".to_string();
    let v3 = "value3".to_string();

    // Read non-existing key
    assert_eq!(storage.read_setting(k1), None);

    // Write non-eximsting key
    storage.write_setting(k1, &v1);

    // Read existing key
    assert_eq!(storage.read_setting(k1), Some(v1.to_owned()));

    // Update existing key
    storage.write_setting(k1, &v2);
    assert_eq!(storage.read_setting(k1), Some(v2.to_owned()));

    // Delete existing key
    storage.delete_setting(k1);
    assert_eq!(storage.read_setting(k1), None);

    // Multiple keys
    storage.write_setting(k1, &v1);
    storage.write_setting(k2, &v2);
    storage.write_setting(k3, &v3);

    assert_eq!(storage.read_setting(k1), Some(v1));
    assert_eq!(storage.read_setting(k2), Some(v2));
    assert_eq!(storage.read_setting(k3), Some(v3));
}
