#![recursion_limit = "512"]

#[macro_use]
extern crate diesel;

#[macro_use]
extern crate diesel_migrations;

pub mod config;
pub mod schema;
pub mod store;

pub use self::store::*;

use chrono::Timelike;
use diesel::connection::SimpleConnection;

/// Convert milliseconds timestamp to a NaiveDateTime.
pub fn millis_to_naive_chrono(ts: u64) -> chrono::NaiveDateTime {
    chrono::DateTime::from_timestamp_millis(ts as i64)
        .unwrap()
        .naive_utc()
}

/// Converts a NaiveDateTime to a milliseconds timestamp.
pub fn naive_chrono_to_millis(naive: chrono::NaiveDateTime) -> u64 {
    naive.and_utc().timestamp_millis() as u64
}

/// Round down the NaiveDateTime to its current millisecond.
/// Or, zero out the microseconds and the nanoseconds.
pub fn naive_chrono_rounded_down(naive: chrono::NaiveDateTime) -> chrono::NaiveDateTime {
    naive
        .with_nanosecond(naive.nanosecond() - naive.nanosecond() % 1_000_000)
        .unwrap()
}

pub fn replace_tilde_with_home(path: &str) -> std::borrow::Cow<str> {
    if let Some(path) = path.strip_prefix("~/") {
        let home = std::env::var("HOME").expect("home dir set");
        format!("{home}/{path}").into()
    } else {
        path.into()
    }
}

pub fn replace_home_with_tilde(path: &str) -> std::borrow::Cow<str> {
    if let Some(path) = path.strip_prefix(&std::env::var("HOME").expect("home dir set")) {
        format!("~/{path}").into()
    } else {
        path.into()
    }
}

/// Checks if the db contains foreign key violations.
#[tracing::instrument(skip(db))]
pub fn check_foreign_keys(db: &mut diesel::SqliteConnection) -> Result<(), anyhow::Error> {
    use diesel::prelude::*;
    use diesel::sql_types::*;

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

    db.batch_execute("PRAGMA foreign_keys = ON;").unwrap();
    let violations: Vec<ForeignKeyViolation> = diesel::sql_query("PRAGMA main.foreign_key_check;")
        .load(db)
        .unwrap();

    if !violations.is_empty() {
        anyhow::bail!(
            "There are foreign key violations. Here the are: {:?}",
            violations
        );
    } else {
        Ok(())
    }
}
