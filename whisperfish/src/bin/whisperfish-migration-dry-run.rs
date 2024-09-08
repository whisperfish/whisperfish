#[macro_use]
extern crate diesel_migrations;
use crate::diesel_migrations::MigrationHarness;

use diesel::connection::SimpleConnection;
use diesel::dsl::*;
use diesel::prelude::*;
use diesel::sql_types::*;
use diesel_migrations::EmbeddedMigrations;
use std::io::Read;
use std::path::Path;
use whisperfish::store;
use whisperfish_store::schema::migrations as schemas;

#[derive(Queryable, QueryableByName, Debug)]
#[allow(dead_code)]
pub struct ForeignKeyViolation {
    #[diesel(sql_type = Text)]
    table: String,
    #[diesel(sql_type = Integer)]
    rowid: i32,
    #[diesel(sql_type = Text)]
    parent: String,
    #[diesel(sql_type = Integer)]
    fkid: i32,
}

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();

fn derive_db_key(password: &str, salt_path: &Path) -> Result<[u8; 32], anyhow::Error> {
    let mut salt_file = std::fs::File::open(salt_path)?;
    let mut salt = [0u8; 8];
    anyhow::ensure!(salt_file.read(&mut salt)? == 8, "salt file not 8 bytes");

    let params = scrypt::Params::new(14, 8, 1, 32)?;
    let mut key = [0u8; 32];
    scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key)?;
    tracing::trace!("Computed the key, salt was {:?}", salt);
    Ok(key)
}

fn print_original_stats(db: &mut SqliteConnection) -> Result<(), anyhow::Error> {
    use schemas::original as schema;

    {
        use schema::session::dsl::*;
        let session_count: i64 = session.select(count(id)).first(db)?;
        let group_session_count: i64 = session.select(count(id)).filter(is_group).first(db)?;
        let non_group_session_count: i64 = session
            .select(count(id))
            .filter(is_group.eq(false))
            .first(db)?;
        println!("Session count: {}", session_count);
        println!("├ of which groups: {}", group_session_count);
        println!("└ of which direct sessions: {}", non_group_session_count);
    }

    {
        use schema::message::dsl::*;
        let message_count: i64 = message.select(count(id)).first(db)?;

        let session_on_id = schema::session::table.on(schema::session::id.eq(session_id));

        let non_group_message_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group.eq(false))
            .first(db)?;
        let non_group_sent_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group.eq(false))
            .filter(outgoing)
            .first(db)?;
        let non_group_receipt_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group.eq(false))
            .filter(received)
            .filter(outgoing)
            .first(db)?;
        let non_group_received_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group.eq(false))
            .filter(outgoing.eq(false))
            .first(db)?;

        let group_message_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group)
            .first(db)?;
        let group_sent_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group)
            .filter(outgoing)
            .first(db)?;
        let group_receipt_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group)
            .filter(received)
            .filter(outgoing)
            .first(db)?;
        let group_received_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group)
            .filter(outgoing.eq(false))
            .first(db)?;
        let group_ghost_count: i64 = message
            .left_join(session_on_id)
            .select(count(id))
            .filter(schema::session::is_group)
            .filter(outgoing.eq(false).and(source.eq("")))
            .first(db)?;

        let attachment_count: i64 = message.select(count(id)).filter(has_attachment).first(db)?;
        println!("Message count: {}", message_count);
        println!("├ with an attachment: {}", attachment_count);
        println!("├ of which group messages: {}", group_message_count);
        println!("│ ├ of which you sent: {}", group_sent_count);
        println!("│ │ └ of which are received: {}", group_receipt_count);
        println!("│ ├ of which you received: {}", group_received_count);
        println!("│ └ of which are ghost messages: {}", group_ghost_count);
        println!("└ of which direct messages: {}", non_group_message_count);
        println!("  ├ of which you sent: {}", non_group_sent_count);
        println!("  │ └ of which they received: {}", non_group_receipt_count);
        println!("  └ of which you received: {}", non_group_received_count);
    }
    Ok(())
}

fn print_current_stats(db: &mut SqliteConnection) -> Result<(), anyhow::Error> {
    use schemas::current as schema;

    {
        use schema::recipients::dsl::*;
        let recipient_count: i64 = recipients.select(count(id)).first(db)?;
        println!("Recipients count: {}", recipient_count);
        let e164_count: i64 = recipients
            .select(count(id))
            .filter(e164.is_not_null())
            .first(db)?;
        let uuid_count: i64 = recipients
            .select(count(id))
            .filter(uuid.is_not_null())
            .first(db)?;
        let profile_key_count: i64 = recipients
            .select(count(id))
            .filter(profile_key.is_not_null())
            .first(db)?;
        println!("├ of which have a phone number: {}", e164_count);
        println!("├ of which have a uuid: {}", uuid_count);
        println!("└ of which have a profile key: {}", profile_key_count);
    }
    {
        use schema::sessions::dsl::*;
        let session_count: i64 = sessions.select(count(id)).first(db)?;
        let group_v1_session_count: i64 = sessions
            .select(count(id))
            .filter(group_v1_id.is_not_null())
            .first(db)?;
        let group_v2_session_count: i64 = sessions
            .select(count(id))
            .filter(group_v2_id.is_not_null())
            .first(db)?;
        let dm_session_count: i64 = sessions
            .select(count(id))
            .filter(direct_message_recipient_id.is_not_null())
            .first(db)?;
        println!("Session count: {}", session_count);
        println!("├ of which group v1: {}", group_v1_session_count);
        println!("├ of which group v2: {}", group_v2_session_count);
        println!("└ of which direct sessions: {}", dm_session_count);
    }

    {
        use schema::messages::dsl::*;
        let message_count: i64 = messages.select(count(id)).first(db)?;
        let group_v1_message_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::group_v1_id.is_not_null())
            .first(db)?;
        let group_v1_sent_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::group_v1_id.is_not_null())
            .filter(is_outbound)
            .first(db)?;
        let group_v1_receipt_count: i64 = messages
            .left_join(schema::sessions::table)
            .inner_join(schema::receipts::table)
            .select(count(id))
            .filter(schema::sessions::group_v1_id.is_not_null())
            .filter(is_outbound)
            .filter(schema::receipts::delivered.is_not_null())
            .first(db)?;
        let group_v1_received_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::group_v1_id.is_not_null())
            .filter(is_outbound.eq(false))
            .first(db)?;

        let group_v2_message_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::group_v2_id.is_not_null())
            .first(db)?;
        let group_v2_sent_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::group_v2_id.is_not_null())
            .filter(is_outbound)
            .first(db)?;
        let group_v2_receipt_count: i64 = messages
            .left_join(schema::sessions::table)
            .inner_join(schema::receipts::table)
            .select(count(id))
            .filter(schema::sessions::group_v2_id.is_not_null())
            .filter(is_outbound)
            .filter(schema::receipts::delivered.is_not_null())
            .first(db)?;
        let group_v2_received_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::group_v2_id.is_not_null())
            .filter(is_outbound.eq(false))
            .first(db)?;

        let direct_message_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::direct_message_recipient_id.is_not_null())
            .first(db)?;
        let direct_sent_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::direct_message_recipient_id.is_not_null())
            .filter(is_outbound)
            .first(db)?;
        // XXX We can amend this with the different receipt types:
        // - delivered
        // - read
        // - viewed
        // Whisperfish pre-2020 did only store delivery receipts,
        // and group message delivery receipts were only stored once.
        let direct_receipt_count: i64 = messages
            .left_join(schema::sessions::table)
            .inner_join(schema::receipts::table)
            .select(count(id))
            .filter(schema::sessions::direct_message_recipient_id.is_not_null())
            .filter(is_outbound)
            .filter(schema::receipts::delivered.is_not_null())
            .first(db)?;
        let direct_received_count: i64 = messages
            .left_join(schema::sessions::table)
            .select(count(id))
            .filter(schema::sessions::direct_message_recipient_id.is_not_null())
            .filter(is_outbound.eq(false))
            .first(db)?;

        println!("Message count: {}", message_count);
        println!("├ of which group v1 messages: {}", group_v1_message_count);
        println!("│ ├ of which you sent: {}", group_v1_sent_count);
        println!("│ │ └ of which have a receipt: {}", group_v1_receipt_count);
        println!("│ └ of which you received: {}", group_v1_received_count);
        println!("├ of which group v2 messages: {}", group_v2_message_count);
        println!("│ ├ of which you sent: {}", group_v2_sent_count);
        println!("│ │ └ of which have a receipt: {}", group_v2_receipt_count);
        println!("│ └ of which you received: {}", group_v2_received_count);
        println!("├ of which direct messages: {}", direct_message_count);
        println!("│ ├ of which you sent: {}", direct_sent_count);
        println!("│ │ └ of which have a receipt: {}", direct_receipt_count);
        println!("│ └ of which you received: {}", direct_received_count);
    }
    {
        use schema::attachments::dsl::*;
        let attachment_count: i64 = attachments.select(count(id)).first(db)?;
        println!("└ total attachments: {}", attachment_count);
    }
    Ok(())
}

fn main() -> Result<(), anyhow::Error> {
    println!("This utility will test whether the Whisperfish database will successfully get migrated to the most recent format.");
    println!("It is a *dry-run*, which practically means we will work on a copy of the original database.");

    let storage = store::default_location().unwrap();
    let original_db_location = storage.join("db").join("harbour-whisperfish.db");
    println!("Location of the database: {:?}", original_db_location);

    // Copy the database to /tmp
    let db_location = Path::new("/tmp/wf-migration-dry-run.db");
    std::fs::copy(&original_db_location, db_location)?;
    // Don't keep any pointer to the original db, just in case I'm a crappy programmer.
    drop(original_db_location);
    println!("Location of the copied database: {:?}", db_location);

    let mut db = SqliteConnection::establish(db_location.to_str().unwrap())?;
    println!("The copy of the database has been opened.");

    // Good: "55"
    // Bad: "Parse error: file is not a database (26)"
    let result = sql::<Text>("SELECT count(*) FROM sqlite_master;")
        .get_result::<String>(&mut db)
        .unwrap()
        .parse::<String>()
        .unwrap();

    if !result.contains("file is not a database") {
        println!("We now ask you your Whisperfish password.");
        let password = rpassword::prompt_password("Whisperfish storage password: ").unwrap();

        let db_salt_path = storage.join("db").join("salt");
        let db_key = derive_db_key(&password, &db_salt_path)?;
        println!("Derived the db key.");

        db.batch_execute("PRAGMA cipher_compatibility = 3;")?;
        db.batch_execute(&format!("PRAGMA key = \"x'{}'\";", hex::encode(db_key)))?;
        db.batch_execute("PRAGMA cipher_page_size = 4096;")?;

        // Test again whether the db is readable
        db.batch_execute("SELECT count(*) FROM sqlite_master;")?;
        println!("The copy of the database has been decrypted.");
    }

    println!("------");
    if let Err(e) = print_original_stats(&mut db) {
        println!("Could not print \"original schema\" statistics: {}", e);
    }

    // We set the foreign key enforcement *off*,
    // such that we can print violations later.
    db.batch_execute("PRAGMA foreign_keys = OFF;").unwrap();

    println!("------");
    let start = std::time::Instant::now();
    db.run_pending_migrations(MIGRATIONS).unwrap();
    let end = std::time::Instant::now();
    println!("Migrations took {:?}", end - start);
    println!("------");

    print_current_stats(&mut db)?;
    println!("------");
    println!("Here above, the dry run should have produced at least two sets of statistics of your data.");
    println!("These should give a decent indication to whether some data has been lost. Please report a bug if so.");

    db.batch_execute("PRAGMA foreign_keys = ON;").unwrap();
    let violations: Vec<ForeignKeyViolation> = diesel::sql_query("PRAGMA main.foreign_key_check;")
        .load(&mut db)
        .unwrap();

    if !violations.is_empty() {
        println!(
            "In worse news: there are foreign key violations. Here the are: {:?}",
            violations
        );
        println!("This is definitely a bug. Please report. REPORT REPORT.");
    }

    Ok(())
}
