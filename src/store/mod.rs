use std::path::{Path, PathBuf};
use std::rc::Rc;

use diesel::prelude::*;
use failure::*;

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
    db: Rc<SqliteConnection>,
}

// Cannot borrow password/salt because threadpool requires 'static...
async fn derive_key(password: String, salt_path: PathBuf) -> Result<[u8; 32], Error> {
    use actix_threadpool::BlockingError;
    use crypto::scrypt;
    use std::io::Read;

    actix_threadpool::run(move || -> Result<_, failure::Error> {
        let mut salt_file = std::fs::File::open(salt_path)?;
        let mut salt = [0u8; 8];
        salt_file.read(&mut salt)?;

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

        Ok(Storage { db: Rc::new(db) })
    }

    pub async fn open_with_password<T: AsRef<Path>>(
        db_path: &StorageLocation<T>,
        password: String,
    ) -> Result<Storage, Error> {
        let salt_path = crate::store::default_location()
            .unwrap()
            .join("db")
            .join("salt");
        let key = derive_key(password, salt_path).await;

        let db = db_path.open_db()?;

        // Decrypt db
        // XXX we assume all databases to be encrypted.

        Ok(Storage { db: Rc::new(db) })
    }
}
