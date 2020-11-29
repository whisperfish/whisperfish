use chrono::prelude::*;
use diesel::prelude::*;
use diesel::*;
use diesel_migrations;
use harbour_whisperfish::store::temp;
use harbour_whisperfish::store::{NewMessage, NewSession};
use harbour_whisperfish::store::{Storage, StorageLocation};
use rstest::fixture;

pub type InMemoryDb = (Storage, StorageLocation<tempdir::TempDir>);

diesel::no_arg_sql_function!(
    last_insert_rowid,
    diesel::sql_types::Integer,
    "Represents the SQL last_insert_row() function"
);

/// We do not want to test on a live db, use temporary dir
#[fixture]
pub fn in_memory_db() -> InMemoryDb {
    || -> Result<InMemoryDb, failure::Error> {
        let temp = temp();
        std::fs::create_dir(temp.join("db"))?;
        std::fs::create_dir(temp.join("storage"))?;
        Ok((Storage::open(&temp)?, temp))
    }()
    .expect("initialized storage")
}

/// Setup helper for basic, empty database
pub fn setup_db(in_memory_db: &Storage) {
    let db = in_memory_db.db.lock();
    let conn = db.unwrap();

    diesel_migrations::run_pending_migrations(&*conn).unwrap()
}

/// Setup helper for creating a session
pub fn setup_session(in_memory_db: &Storage, new_session: &NewSession) -> usize {
    use harbour_whisperfish::schema::session::dsl::*;

    let db = in_memory_db.db.lock();
    let conn = db.unwrap();

    let query = diesel::insert_into(session).values(new_session);

    let res = match query.execute(&*conn) {
        Ok(rows_inserted) => rows_inserted,
        Err(error) => panic!(error.to_string()),
    };

    res
}

/// Setup helper for creating a proper chat
/// where each message in `Vec<NewMessage>`
/// would be received by the message processor
///
/// If the session is None, a new session gets created for every message
pub fn setup_messages(in_memory_db: &Storage, mut new_messages: Vec<NewMessage>) -> usize {
    use harbour_whisperfish::schema::message::dsl::*;

    let db = in_memory_db.db.lock();
    let conn = db.unwrap();

    for msg in &mut new_messages {
        if msg.session_id.is_none() {
            let session = NewSession {
                source: String::from("+32474123456"),
                message: String::new(),
                timestamp: NaiveDateTime::from_timestamp(0, 0),
                sent: false,
                received: false,
                unread: false,
                is_group: false,
                group_members: None,
                group_id: None,
                group_name: None,
                has_attachment: false,
            };
            setup_session(in_memory_db, &session);
            let sid = diesel::select(last_insert_rowid)
                .get_result::<i32>(&*conn)
                .unwrap();

            msg.session_id = Some(sid);
        }
    }

    let query = diesel::insert_into(message).values(new_messages);

    query.execute(&*conn).expect("failed")
}
