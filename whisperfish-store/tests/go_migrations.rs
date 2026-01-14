#[macro_use]
extern crate diesel;
#[macro_use]
extern crate diesel_migrations;

mod migrations;

use crate::diesel_migrations::MigrationHarness;
use crate::migrations::orm;
use chrono::prelude::*;
use diesel::connection::SimpleConnection;
use diesel::migration::{Migration, MigrationSource};
use diesel::prelude::*;
use diesel::sqlite::Sqlite;
use diesel_migrations::EmbeddedMigrations;
use rstest::*;
use rstest_reuse::{self, *};
use whisperfish_store::schema::migrations as schemas;

type MigrationList = Vec<Box<dyn Migration<Sqlite> + 'static>>;

fn milliseconds_at(yr: i32, mo: u32, day: u32, h: u32, m: u32, s: u32, ms: u32) -> i64 {
    NaiveDate::from_ymd_opt(yr, mo, day)
        .unwrap()
        .and_hms_milli_opt(h, m, s, ms)
        .unwrap()
        .and_utc()
        .timestamp_millis()
}

mod original_data {
    use super::*;

    use orm::original::*;

    /// Just a 1-1 session
    pub fn dm1() -> NewSession {
        NewSession {
            source: "+32475000000".into(),
            message: "Hoh.".into(),
            timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
            sent: true,
            received: true,
            unread: true,
            is_group: false,
            group_members: None,
            group_id: None,
            group_name: None,
            has_attachment: false,
        }
    }

    /// A group
    pub fn group1() -> NewSession {
        NewSession {
            source: "+32474000000".into(),
            message: "Heh.".into(),
            timestamp: milliseconds_at(2016, 7, 8, 9, 10, 11, 325),
            sent: true,
            received: true,
            unread: true,
            is_group: true,
            group_members: Some("+32475000000,+32476000000,+32770000000".into()),
            group_id: Some("AF88".into()),
            group_name: Some("The first group".into()),
            has_attachment: false,
        }
    }

    /// Another group with members distinct from group1
    pub fn group2() -> NewSession {
        NewSession {
            source: "".into(),
            message: "Heh.".into(),
            timestamp: milliseconds_at(2016, 7, 8, 9, 10, 11, 325),
            sent: true,
            received: true,
            unread: true,
            is_group: true,
            group_members: Some("+33475000000,+33476000000,+33770000000".into()),
            group_id: Some("AF89".into()),
            group_name: Some("The second group".into()),
            has_attachment: false,
        }
    }

    /// Another group, now with some common members between 1 & 2
    pub fn group3() -> NewSession {
        NewSession {
            source: "".into(),
            message: "Heh.".into(),
            timestamp: milliseconds_at(2016, 7, 8, 9, 10, 11, 325),
            sent: true,
            received: true,
            unread: true,
            is_group: true,
            group_members: Some(
                "+32475000000,+32476000000,+33475000000,+33476000000,+33770000000".into(),
            ),
            group_id: Some("AF90".into()),
            group_name: Some("The third group".into()),
            has_attachment: false,
        }
    }
}

fn assert_foreign_keys(db: &mut SqliteConnection) {
    whisperfish_store::check_foreign_keys(db).expect("foreign keys intact");
}

#[fixture]
fn empty_db() -> SqliteConnection {
    let mut conn = SqliteConnection::establish(":memory:").unwrap();
    conn.batch_execute("PRAGMA foreign_keys = OFF;").unwrap();

    conn
}

#[fixture]
fn migration_params() -> MigrationList {
    let migration_path = std::path::Path::new("../migrations");
    let mut migrations: MigrationList =
        diesel_migrations::FileBasedMigrations::from_path(migration_path)
            .unwrap()
            .migrations()
            .unwrap();

    migrations.sort_by_cached_key(|f| f.name().to_string());

    assert!(!migrations.is_empty());

    migrations
}

#[fixture]
fn original_go_db(mut empty_db: SqliteConnection) -> SqliteConnection {
    let message = r#"create table if not exists message
            (id integer primary key, session_id integer, source text, message string, timestamp integer,
    sent integer default 0, received integer default 0, flags integer default 0, attachment text,
            mime_type string, has_attachment integer default 0, outgoing integer default 0)"#;
    let sentq = r#"create table if not exists sentq
		(message_id integer primary key, timestamp timestamp)"#;
    let session = r#"create table if not exists session
		(id integer primary key, source text, message string, timestamp integer,
		 sent integer default 0, received integer default 0, unread integer default 0,
         is_group integer default 0, group_members text, group_id text, group_name text,
		 has_attachment integer default 0)"#;

    diesel::sql_query(message).execute(&mut empty_db).unwrap();
    diesel::sql_query(sentq).execute(&mut empty_db).unwrap();
    diesel::sql_query(session).execute(&mut empty_db).unwrap();

    empty_db
}

#[fixture]
fn fixed_go_db(
    mut empty_db: SqliteConnection,
    mut migration_params: MigrationList,
) -> SqliteConnection {
    drop(migration_params.split_off(3));
    assert_eq!(migration_params.len(), 3);
    assert_eq!(
        migration_params[0].name().to_string(),
        "2020-04-26-145028_0-5-message"
    );
    assert_eq!(
        migration_params[1].name().to_string(),
        "2020-04-26-145033_0-5-sentq"
    );
    assert_eq!(
        migration_params[2].name().to_string(),
        "2020-04-26-145036_0-5-session"
    );

    for migration in migration_params {
        migration.run(&mut empty_db).unwrap();
    }

    assert_foreign_keys(&mut empty_db);
    empty_db
}

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

#[template]
#[rstest(
    db,
    case::empty_db(empty_db()),
    case::original_go_db(original_go_db(empty_db())),
    case::fixed_go_db(fixed_go_db(empty_db(), migration_params()))
)]
fn initial_dbs(db: SqliteConnection) {}

#[apply(initial_dbs)]
fn run_plain_migrations(mut db: SqliteConnection) {
    db.run_pending_migrations(MIGRATIONS).unwrap();
    assert_foreign_keys(&mut db);
}

#[apply(initial_dbs)]
fn one_by_one(mut db: SqliteConnection, migration_params: Vec<Box<dyn Migration<Sqlite>>>) {
    for migration in migration_params {
        dbg!(migration.name().to_string());
        migration.run(&mut db).unwrap();
        assert_foreign_keys(&mut db);
    }
}

#[allow(clippy::type_complexity)]
fn load_sessions(
    db: &mut SqliteConnection,
) -> Vec<(
    orm::current::Session,
    Option<Vec<(orm::current::GroupV1Member, orm::current::Recipient)>>,
    Option<Vec<(orm::current::GroupV2Member, orm::current::Recipient)>>,
)> {
    use orm::current::*;
    use schemas::current::sessions;

    let all_sessions: Vec<DbSession> = sessions::table.load(db).unwrap();

    let mut result = vec![];

    for session in all_sessions {
        dbg!(&session);

        let group = session.group_v1_id.as_ref().map(|g_id| {
            use schemas::current::group_v1s::dsl::*;
            group_v1s.filter(id.eq(g_id)).first(db).unwrap()
        });

        let group_v2 = session.group_v2_id.as_ref().map(|g_id| {
            use schemas::current::group_v2s::dsl::*;
            group_v2s.filter(id.eq(g_id)).first(db).unwrap()
        });

        let recipient = session.direct_message_recipient_id.as_ref().map(|r_id| {
            use schemas::current::recipients::dsl::*;
            recipients.filter(id.eq(r_id)).first(db).unwrap()
        });

        let members = session.group_v1_id.as_ref().map(|g_id| {
            use schemas::current::group_v1_members::dsl::*;
            use schemas::current::recipients::dsl::recipients;
            group_v1_members
                .inner_join(recipients)
                .filter(group_v1_id.eq(g_id))
                .load(db)
                .unwrap()
        });

        let members_v2 = session.group_v2_id.as_ref().map(|g_id| {
            use schemas::current::group_v2_members::dsl::*;
            use schemas::current::recipients::dsl::recipients;
            group_v2_members
                .inner_join(recipients)
                .filter(group_v2_id.eq(g_id))
                .load(db)
                .unwrap()
        });

        if let Some(group) = group.as_ref() {
            dbg!(group);
        }
        if let Some(group_v2) = group_v2.as_ref() {
            dbg!(group_v2);
        }
        if let Some(recipient) = recipient.as_ref() {
            dbg!(recipient);
        }
        result.push((
            Session::from((session, recipient, group, group_v2)),
            members,
            members_v2,
        ));
    }

    result
}

// As of here, we inject data in an old database, and test whether the data is still intact after
// running all the migrations.
// Insertion of the data can be done through the old models (found in `old_schemes`), and
// assertions should be done against `whisperfish::schema`.
//
// Tests usually use the following pattern:
// - a method assert_FOO(db) that puts assertions on the db in the "current" setting.
// - a bunch of `rstest`s that take different kinds of initial dbs, puts in the data and then calls
//   the migrations and the assert function.

fn assert_bunch_of_empty_sessions(mut db: SqliteConnection) {
    use orm::current::*;

    let session_tests = [
        |session: Session, members: Option<Vec<(GroupV1Member, Recipient)>>| {
            assert!(session.is_dm());
            assert!(members.is_none());

            let recipient = session.unwrap_dm();
            let pn = phonenumber::parse(None, "+32475000000").unwrap();
            assert_eq!(recipient.e164.as_ref(), Some(&pn));
        },
        |session: Session, members: Option<Vec<(GroupV1Member, Recipient)>>| {
            assert!(session.is_group_v1());
            let mut members = members.unwrap();
            let test = [
                phonenumber::parse(None, "+32475000000").unwrap(),
                phonenumber::parse(None, "+32476000000").unwrap(),
                phonenumber::parse(None, "+32770000000").unwrap(),
            ];
            members.sort_by_key(|(_, r)| r.e164.as_ref().unwrap().to_string());
            assert_eq!(test.len(), members.len());
            for ((_, r), t) in members.iter().zip(&test) {
                assert_eq!(r.e164.as_ref().unwrap(), t);
            }
        },
        |session: Session, members: Option<Vec<(GroupV1Member, Recipient)>>| {
            assert!(session.is_group_v1());
            let mut members = members.unwrap();
            let test = [
                phonenumber::parse(None, "+33475000000").unwrap(),
                phonenumber::parse(None, "+33476000000").unwrap(),
                phonenumber::parse(None, "+33770000000").unwrap(),
            ];
            members.sort_by_key(|(_, r)| r.e164.as_ref().unwrap().to_string());
            assert_eq!(test.len(), members.len());
            for ((_, r), t) in members.iter().zip(&test) {
                assert_eq!(r.e164.as_ref().unwrap(), t);
            }
        },
        |session: Session, members: Option<Vec<(GroupV1Member, Recipient)>>| {
            assert!(session.is_group_v1());
            let mut members = members.unwrap();
            let test = [
                phonenumber::parse(None, "+32475000000").unwrap(),
                phonenumber::parse(None, "+32476000000").unwrap(),
                phonenumber::parse(None, "+33475000000").unwrap(),
                phonenumber::parse(None, "+33476000000").unwrap(),
                phonenumber::parse(None, "+33770000000").unwrap(),
            ];
            members.sort_by_key(|(_, r)| r.e164.as_ref().unwrap().to_string());
            assert_eq!(test.len(), members.len());
            for ((_, r), t) in members.iter().zip(&test) {
                assert_eq!(r.e164.as_ref().unwrap(), t);
            }
        },
    ];

    let sessions = load_sessions(&mut db);
    assert_eq!(sessions.len(), session_tests.len());
    for ((session, members, members_v2), test) in sessions.into_iter().zip(&session_tests) {
        assert!(members_v2.is_none());
        test(session, members);
    }
}

#[rstest]
fn bunch_of_empty_sessions(original_go_db: SqliteConnection) {
    use schemas::original::session::dsl::*;

    use original_data::*;

    let mut db = original_go_db;

    let sessions = vec![dm1(), group1(), group2(), group3()];

    let count = sessions.len();
    assert_eq!(
        diesel::insert_into(session)
            .values(sessions)
            .execute(&mut db)
            .unwrap(),
        count
    );

    db.run_pending_migrations(MIGRATIONS).unwrap();
    assert_foreign_keys(&mut db);
    assert_bunch_of_empty_sessions(db);
}

fn assert_direct_session_with_messages(mut db: SqliteConnection) {
    use orm::current::*;
    use schemas::current::*;

    let sessions = load_sessions(&mut db);
    assert_eq!(sessions.len(), 1);
    let (session, _members, _members_v2) = &sessions[0];
    assert!(_members.is_none());
    let recipient = session.unwrap_dm();
    assert_eq!(
        recipient.e164.as_ref(),
        Some(&phonenumber::parse(None, "+32475000000").unwrap())
    );

    let messages: Vec<Message> = {
        use schemas::current::messages::dsl::*;

        messages
            .filter(session_id.eq(session.id))
            .load(&mut db)
            .unwrap()
    };

    let message_tests = [
        |message: Message, attachments: Vec<Attachment>| {
            assert!(message.is_outbound);
            assert!(attachments.is_empty());
        },
        |message: Message, attachments: Vec<Attachment>| {
            assert!(!message.is_outbound);
            assert!(attachments.is_empty());
        },
        |message: Message, attachments: Vec<Attachment>| {
            assert!(!message.is_outbound);
            assert_eq!(attachments.len(), 1);
        },
    ];

    assert_eq!(messages.len(), message_tests.len());
    for (message, test) in messages.into_iter().zip(&message_tests) {
        // Get attachment
        let attachments: Vec<Attachment> = attachments::table
            .filter(attachments::message_id.eq(message.id))
            .load(&mut db)
            .unwrap();

        // These may not be true anymore after the Signal-2020 migration.
        assert!(message.sender_recipient_id.is_none());
        assert_eq!(message.sent_timestamp.is_some(), message.is_outbound);
        assert_eq!(message.received_timestamp.is_none(), message.is_outbound);
        test(message, attachments);
    }
}

#[rstest]
fn direct_session_with_messages(original_go_db: SqliteConnection) {
    use orm::original::*;
    use schemas::original::*;

    let mut db = original_go_db;

    let sessions = vec![original_data::dm1()];

    let count = sessions.len();
    assert_eq!(
        diesel::insert_into(session::table)
            .values(sessions)
            .execute(&mut db)
            .unwrap(),
        count
    );

    let ids: Vec<i64> = session::table.select(session::id).load(&mut db).unwrap();
    assert_eq!(ids.len(), count);

    let messages = vec![
        NewMessage {
            session_id: Some(ids[0]),
            source: "+32475000000".into(),
            text: "Hoh.".into(),
            timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
            sent: true,
            received: false,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: true,
            message_type: None,
        },
        NewMessage {
            session_id: Some(ids[0]),
            source: "+32475000000".into(),
            text: "Hoh.".into(),
            timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
            sent: true,
            received: true,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: false,
            message_type: None,
        },
        NewMessage {
            session_id: Some(ids[0]),
            source: "+32475000000".into(),
            text: "Hoh. Attachment!".into(),
            timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 326),
            sent: true,
            received: true,
            flags: 0,
            attachment: Some("/root/foobar.jpg".into()),
            mime_type: Some("image/jpeg".into()),
            has_attachment: true,
            outgoing: false,
            message_type: None,
        },
    ];

    let count = messages.len();
    assert_eq!(
        diesel::insert_into(message::table)
            .values(messages)
            .execute(&mut db)
            .unwrap(),
        count
    );

    db.run_pending_migrations(MIGRATIONS).unwrap();
    assert_foreign_keys(&mut db);
    assert_direct_session_with_messages(db);
}

fn assert_group_sessions_with_messages(mut db: SqliteConnection) {
    use orm::current::*;

    let sessions = load_sessions(&mut db);
    assert_eq!(sessions.len(), 2);
    let (session1, _members, _members_v2) = &sessions[0];
    assert!(_members.is_some());
    let (session2, _members, _members_v2) = &sessions[1];
    assert!(_members.is_some());

    assert!(session1.is_group_v1());
    assert!(session2.is_group_v1());

    let messages1: Vec<Message> = {
        use schemas::current::messages::dsl::*;

        messages
            .filter(session_id.eq(session1.id))
            .load(&mut db)
            .unwrap()
    };
    assert_eq!(messages1.len(), 1);

    let messages2: Vec<Message> = {
        use schemas::current::messages::dsl::*;

        messages
            .filter(session_id.eq(session2.id))
            .load(&mut db)
            .unwrap()
    };
    assert_eq!(messages2.len(), 1);

    let message_tests = [
        |message: Message| {
            assert!(message.is_outbound);
            assert!(message.sender_recipient_id.is_none());
        },
        |message: Message| {
            assert!(!message.is_outbound);
            assert!(message.sender_recipient_id.is_some());
        },
    ];

    for (message, test) in messages1.into_iter().chain(messages2).zip(&message_tests) {
        dbg!(&message);
        test(message)
    }
}

#[rstest]
fn group_sessions_with_messages(original_go_db: SqliteConnection) {
    use orm::original::*;
    use schemas::original::*;

    use original_data::*;

    let mut db = original_go_db;

    let sessions = vec![group1(), group2()];

    let count = sessions.len();
    assert_eq!(
        diesel::insert_into(session::table)
            .values(sessions)
            .execute(&mut db)
            .unwrap(),
        count
    );

    let ids: Vec<i64> = session::table.select(session::id).load(&mut db).unwrap();
    assert_eq!(ids.len(), count);

    let messages = vec![
        NewMessage {
            session_id: Some(ids[0]),
            source: "+32475000000".into(),
            text: "Hoh.".into(),
            timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
            sent: true,
            received: false,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: true,
            message_type: None,
        },
        NewMessage {
            session_id: Some(ids[1]),
            source: "+32475000000".into(),
            text: "Hoh.".into(),
            timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
            sent: true,
            received: true,
            flags: 0,
            attachment: None,
            mime_type: None,
            has_attachment: false,
            outgoing: false,
            message_type: None,
        },
    ];

    let count = messages.len();
    assert_eq!(
        diesel::insert_into(message::table)
            .values(messages)
            .execute(&mut db)
            .unwrap(),
        count
    );

    db.run_pending_migrations(MIGRATIONS).unwrap();
    assert_foreign_keys(&mut db);
    assert_group_sessions_with_messages(db);
}

#[rstest]
// https://gitlab.com/whisperfish/whisperfish/-/issues/319
fn group_message_without_sender_nor_recipient(original_go_db: SqliteConnection) {
    use orm::original::*;
    use schemas::original::*;

    use original_data::*;

    let mut db = original_go_db;

    let sessions = vec![group1()];

    let count = sessions.len();
    assert_eq!(
        diesel::insert_into(session::table)
            .values(sessions)
            .execute(&mut db)
            .unwrap(),
        count
    );

    let ids: Vec<i64> = session::table.select(session::id).load(&mut db).unwrap();
    assert_eq!(ids.len(), count);

    let messages = vec![NewMessage {
        session_id: Some(ids[0]),
        source: "".into(),
        text: "Hoh.".into(),
        timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
        sent: false,
        received: true,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: false,
        message_type: None,
    }];

    let count = messages.len();
    assert_eq!(
        diesel::insert_into(message::table)
            .values(messages)
            .execute(&mut db)
            .unwrap(),
        count
    );

    db.run_pending_migrations(MIGRATIONS).unwrap();

    let messages: Vec<orm::current::Message> = {
        use schemas::current::messages::dsl::*;

        messages
            .filter(session_id.eq(ids[0] as i32))
            .load(&mut db)
            .unwrap()
    };
    assert_eq!(messages.len(), 1);
    // Our workaround inverts the message sender.
    assert!(messages[0].is_outbound);
}

#[rstest]
/// A test that creates a single session and 10^5 random messages and timestamps.
fn timestamp_conversion(original_go_db: SqliteConnection) {
    use orm::original::*;
    use rand::Rng;
    use schemas::original::*;

    let mut db = original_go_db;

    assert_eq!(
        diesel::insert_into(session::table)
            .values(original_data::dm1())
            .execute(&mut db)
            .unwrap(),
        1
    );

    let ids: Vec<i64> = session::table.select(session::id).load(&mut db).unwrap();
    assert_eq!(ids.len(), 1);
    let session_id = Some(ids[0]);

    let mut message = NewMessage {
        session_id,
        source: "+32475".into(),
        text: "Hoh.".into(),
        timestamp: milliseconds_at(2016, 7, 9, 9, 10, 11, 325),
        sent: true,
        received: false,
        flags: 0,
        attachment: None,
        mime_type: None,
        has_attachment: false,
        outgoing: true,
        message_type: None,
    };

    let count = 100_000;

    let mut timestamps = Vec::with_capacity(count);
    let mut rng = rand::rng();

    for _ in 0..count {
        let ts: u64 = rng.random_range(0..=1614425253000);
        message.timestamp = ts as i64;
        let ts = whisperfish_store::millis_to_naive_chrono(ts);
        timestamps.push(ts);

        diesel::insert_into(message::table)
            .values(&message)
            .execute(&mut db)
            .unwrap();
    }

    db.run_pending_migrations(MIGRATIONS).unwrap();
    assert_foreign_keys(&mut db);

    for (i, ts) in timestamps.into_iter().enumerate() {
        use orm::current::*;
        use schemas::current::*;

        let message: Message = messages::table
            .filter(messages::id.eq((i + 1) as i32))
            .first(&mut db)
            .unwrap();
        assert_eq!(message.sent_timestamp, Some(ts), "at message {}", i);
    }
}
