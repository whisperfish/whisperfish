use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::schema::message;
use crate::schema::sentq;
use crate::schema::session;
use crate::model::session::*;
use crate::model::message::*;

use actix::prelude::*;
use diesel::prelude::*;
use failure::*;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct StorageReady(pub Storage);

/// Message as it relates to the schema
#[derive(Queryable)]
pub struct Message {
    pub id: i32,
    pub sid: i64,
    pub source: String,
    pub message: String,  // NOTE: "text" in schema, doesn't apparently matter
    pub timestamp: i64,
    pub sent: bool,
    pub received: bool,
    pub flags: i32,
    pub attachment: Option<String>,
    pub mimetype: Option<String>,
    pub hasattachment: bool,
    pub outgoing: bool,
    pub queued: bool,
}


/// Location of the storage.
///
/// Path is for persistent storage.
/// Memory is for running tests or 'incognito' mode.
#[cfg_attr(not(test), allow(unused))]
pub enum StorageLocation<P> {
    Path(P),
    Memory,
}

impl<'a> From<&'a Path> for StorageLocation<&'a Path> {
    fn from(p: &'a Path) -> Self {
        StorageLocation::Path(p)
    }
}

impl From<PathBuf> for StorageLocation<PathBuf> {
    fn from(p: PathBuf) -> Self {
        StorageLocation::Path(p)
    }
}

#[cfg_attr(not(test), allow(unused))]
pub fn memory() -> StorageLocation<PathBuf> {
    StorageLocation::Memory
}

pub fn default_location() -> Result<StorageLocation<PathBuf>, Error> {
    let data_dir = dirs::data_local_dir().ok_or(format_err!("Could not find data directory."))?;

    Ok(StorageLocation::Path(
        data_dir.join("harbour-whisperfish").into(),
    ))
}

impl std::ops::Deref for StorageLocation<PathBuf> {
    type Target = Path;
    fn deref(&self) -> &Path {
        match self {
            StorageLocation::Memory => unimplemented!(":memory: deref"),
            StorageLocation::Path(p) => p,
        }
    }
}

impl<P: AsRef<Path>> StorageLocation<P> {
    fn open_db(&self) -> Result<SqliteConnection, Error> {
        let database_url = match self {
            StorageLocation::Memory => ":memory:".into(),
            StorageLocation::Path(p) => p
                .as_ref()
                .join("db")
                .join("harbour-whisperfish.db")
                .to_str()
                .ok_or(format_err!(
                    "path to db contains a non-UTF8 character, please file a bug."
                ))?
                .to_string(),
        };

        Ok(SqliteConnection::establish(&database_url)?)
    }
}

#[derive(Clone)]
pub struct Storage {
    pub db: Arc<Mutex<SqliteConnection>>,
}

// Cannot borrow password/salt because threadpool requires 'static...
async fn derive_key(password: String, salt_path: PathBuf) -> Result<[u8; 32], Error> {
    use actix_threadpool::BlockingError;
    use crypto::scrypt;
    use std::io::Read;

    actix_threadpool::run(move || -> Result<_, failure::Error> {
        let mut salt_file = std::fs::File::open(salt_path)?;
        let mut salt = [0u8; 8];
        ensure!(salt_file.read(&mut salt)? == 8, "salt file not 8 bytes");

        let params = scrypt::ScryptParams::new(14, 8, 1);
        let mut key = [0u8; 32];
        scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key);
        log::trace!("Computed the key, salt was {:?}", salt);
        Ok(key)
    })
    .await
    .map_err(|e| match e {
        BlockingError::Canceled => format_err!("Threadpool Canceled"),
        BlockingError::Error(e) => e,
    })
}

impl Storage {
    pub fn open<T: AsRef<Path>>(db_path: &StorageLocation<T>) -> Result<Storage, Error> {
        let db = db_path.open_db()?;

        Ok(Storage { db: Arc::new(Mutex::new(db)) })
    }

    pub async fn open_with_password<T: AsRef<Path>>(
        db_path: &StorageLocation<T>,
        password: String,
    ) -> Result<Storage, Error> {
        let salt_path = crate::store::default_location()
            .unwrap()
            .join("db")
            .join("salt");

        let key = derive_key(password, salt_path).await?;
        let db = db_path.open_db()?;
        db.execute(&format!("PRAGMA key = \"x'{}'\";", hex::encode(key)))?;
        db.execute("PRAGMA cipher_page_size = 4096;")?;

        // From the sqlcipher manual:
        // -- if this throws an error, the key was incorrect. If it succeeds and returns a numeric value, the key is correct;
        db.execute("SELECT count(*) FROM sqlite_master;")?;
        // XXX: Do we have to signal somehow that the password was wrong?
        //      Offer retries?

        Ok(Storage { db: Arc::new(Mutex::new(db)) })
    }

    pub fn fetch_session(&self, sid: i64) -> Option<Session> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_session({})", sid);
        match session::table.filter(session::columns::id.eq(sid))
                            .first(&*conn) {
                                Ok(data) => Some(data),
                                Err(_) => None,
                             }
    }

    pub fn fetch_all_messages(&self, sid: i64) -> Option<Vec<Message>> {
        let db = self.db.lock();
        let conn = db.unwrap();

        use diesel::expression::sql_literal::sql;
        use diesel::debug_query;

        log::trace!("Called fetch_all_messages({})", sid);
        let query = message::table.left_join(sentq::table)
                            .select((message::columns::id, message::columns::session_id, message::columns::source,
                                     message::columns::text, message::columns::timestamp, message::columns::sent,
                                     message::columns::received, message::columns::flags, message::columns::attachment,
                                     message::columns::mime_type, message::columns::has_attachment, message::columns::outgoing,
                            sql::<diesel::sql_types::Bool>("CASE WHEN sentq.message_id > 0 THEN 1 ELSE 0 END AS queued")))
                            .filter(message::columns::session_id.eq(sid))
                            .order_by(message::columns::id.desc());

        let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
        log::trace!("{}", debug.to_string());

        match query.load::<Message>(&*conn) {
                    Ok(data) => Some(data),
                    Err(_) => None,
                }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_memory_db() -> Result<(), Error> {
        let _storage = Storage::open(&memory())?;

        Ok(())
    }
}
