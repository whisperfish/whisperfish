use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::schema::message;
use crate::schema::sentq;
use crate::schema::session;

use diesel::debug_query;
use diesel::expression::sql_literal::sql;
use diesel::prelude::*;

use futures::io::AsyncRead;

use failure::*;

mod protocol_store;
use protocol_store::ProtocolStore;

/// Session as it relates to the schema
#[derive(Queryable, Debug, Clone)]
pub struct Session {
    pub id: i64,
    pub source: String,
    pub message: String,
    pub timestamp: i64,
    pub sent: bool,
    pub received: bool,
    pub unread: bool,
    pub is_group: bool,
    pub group_members: Option<String>,
    #[allow(dead_code)]
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub has_attachment: bool,
}

/// ID-free Session model for insertions
#[derive(Insertable, Debug)]
#[table_name = "session"]
pub struct NewSession {
    pub source: String,
    pub message: String,
    pub timestamp: i64,
    pub sent: bool,
    pub received: bool,
    pub unread: bool,
    pub is_group: bool,
    pub group_members: Option<String>,
    #[allow(dead_code)]
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub has_attachment: bool,
}

/// Message as it relates to the schema
#[derive(Queryable, Debug)]
pub struct Message {
    pub id: i32,
    pub sid: i64,
    pub source: String,
    pub message: String, // NOTE: "text" in schema, doesn't apparently matter
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

/// ID-free Message model for insertions
#[derive(Insertable)]
#[table_name = "message"]
pub struct NewMessage {
    pub session_id: Option<i64>,
    pub source: String,
    pub text: String,
    pub timestamp: i64,
    pub sent: bool,
    pub received: bool,
    pub flags: i32,
    pub attachment: Option<String>,
    pub mime_type: Option<String>,
    pub has_attachment: bool,
    pub outgoing: bool,
}

/// ID-free Group model for insertions
#[derive(Clone, Debug)]
pub struct NewGroup<'a> {
    pub id: &'a [u8],
    /// Group name
    pub name: String,
    /// List of E164
    pub members: Vec<String>,
}

/// Saves a given attachment into a random-generated path. Returns the path.
///
/// This was a Message method in Go
pub async fn save_attachment(
    dir: impl AsRef<Path>,
    ext: &str,
    mut attachment: impl AsyncRead + Unpin,
) -> PathBuf {
    use std::fs::File;
    use uuid::Uuid;

    let fname = Uuid::new_v4().to_simple();
    let fname_formatted = format!("{}", fname);
    let fname_path = Path::new(&fname_formatted);

    let mut path = dir.as_ref().join(fname_path);
    path.set_extension(ext);

    let file = File::create(&path).expect("Could not create file");

    // https://github.com/rust-lang/futures-rs/issues/2105
    // https://github.com/tokio-rs/tokio/pull/1744
    let mut file = futures::io::AllowStdIo::new(file);
    futures::io::copy(&mut attachment, &mut file).await.unwrap();

    return path;
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

#[cfg_attr(not(test), allow(unused))]
#[cfg(unix)]
pub fn temp() -> StorageLocation<tempdir::TempDir> {
    StorageLocation::Path(tempdir::TempDir::new("harbour-whisperfish-temp").unwrap())
}

pub fn default_location() -> Result<StorageLocation<PathBuf>, Error> {
    let data_dir = dirs::data_local_dir().ok_or(format_err!("Could not find data directory."))?;

    Ok(StorageLocation::Path(
        data_dir.join("harbour-whisperfish").into(),
    ))
}

impl<P: AsRef<Path>> std::ops::Deref for StorageLocation<P> {
    type Target = Path;
    fn deref(&self) -> &Path {
        match self {
            StorageLocation::Memory => unimplemented!(":memory: deref"),
            StorageLocation::Path(p) => p.as_ref(),
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
    // aesKey + macKey
    keys: Option<[u8; 16 + 20]>,
    protocol_store: Arc<Mutex<ProtocolStore>>,
    path: PathBuf,
}

// Cannot borrow password/salt because threadpool requires 'static...
async fn derive_storage_key(password: String, salt_path: PathBuf) -> Result<[u8; 16 + 20], Error> {
    use actix_threadpool::BlockingError;
    use std::io::Read;

    actix_threadpool::run(move || -> Result<_, failure::Error> {
        let mut salt_file = std::fs::File::open(salt_path)?;
        let mut salt = [0u8; 8];
        ensure!(salt_file.read(&mut salt)? == 8, "salt file not 8 bytes");

        let mut key = [0u8; 16 + 20];
        // Please don't blame me, I'm only the implementer.
        pbkdf2::pbkdf2::<hmac::Hmac<sha1::Sha1>>(password.as_bytes(), &salt, 1024, &mut key);
        log::trace!("Computed the key, salt was {:?}", salt);

        Ok(key)
    })
    .await
    .map_err(|e| match e {
        BlockingError::Canceled => format_err!("Threadpool Canceled"),
        BlockingError::Error(e) => e,
    })
}

// Cannot borrow password/salt because threadpool requires 'static...
async fn derive_db_key(password: String, salt_path: PathBuf) -> Result<[u8; 32], Error> {
    use actix_threadpool::BlockingError;
    use std::io::Read;

    actix_threadpool::run(move || -> Result<_, failure::Error> {
        let mut salt_file = std::fs::File::open(salt_path)?;
        let mut salt = [0u8; 8];
        ensure!(salt_file.read(&mut salt)? == 8, "salt file not 8 bytes");

        let params = scrypt::ScryptParams::new(14, 8, 1)?;
        let mut key = [0u8; 32];
        scrypt::scrypt(password.as_bytes(), &salt, &params, &mut key)?;
        log::trace!("Computed the key, salt was {:?}", salt);
        Ok(key)
    })
    .await
    .map_err(|e| match e {
        BlockingError::Canceled => format_err!("Threadpool Canceled"),
        BlockingError::Error(e) => e,
    })
}

fn write_file_sync(keys: [u8; 16 + 20], path: PathBuf, contents: &[u8]) -> Result<(), Error> {
    log::trace!("Writing file {:?}", path);

    // Generate random IV
    use rand::RngCore;
    let mut iv = [0u8; 16];
    rand::thread_rng().fill_bytes(&mut iv);

    // Encrypt
    use aes::Aes128;
    use block_modes::block_padding::Pkcs7;
    use block_modes::{BlockMode, Cbc};
    let ciphertext = {
        let cipher = Cbc::<Aes128, Pkcs7>::new_var(&keys[0..16], &iv)
            .map_err(|_| format_err!("CBC initialization error"))?;
        cipher.encrypt_vec(contents).to_owned()
    };

    let mac = {
        use hmac::{Hmac, Mac, NewMac};
        use sha2::Sha256;
        // Verify HMAC SHA256, 32 last bytes
        let mut mac = Hmac::<Sha256>::new_varkey(&keys[16..])
            .map_err(|_| format_err!("MAC keylength error"))?;
        mac.update(&iv);
        mac.update(&ciphertext);
        mac.finalize().into_bytes()
    };

    // Write iv, ciphertext, mac
    use std::io::Write;
    let mut file = std::fs::File::create(&path)?;
    file.write(&iv)?;
    file.write(&ciphertext)?;
    file.write(&mac)?;

    Ok(())
}

fn load_file_sync(keys: [u8; 16 + 20], path: PathBuf) -> Result<Vec<u8>, Error> {
    // XXX This is *full* of bad practices.
    // Let's try to migrate to nacl or something alike in the future.

    log::trace!("Opening file {:?}", path);
    let mut contents = std::fs::read(&path)?;
    let count = contents.len();

    log::trace!("Read {:?}, {} bytes", path, count);
    ensure!(count >= 16 + 32, "File smaller than cryptographic overhead");

    let (iv, contents) = contents.split_at_mut(16);
    let count = count - 16;
    let (contents, mac) = contents.split_at_mut(count - 32);

    {
        use hmac::{Hmac, Mac, NewMac};
        use sha2::Sha256;
        // Verify HMAC SHA256, 32 last bytes
        let mut verifier = Hmac::<Sha256>::new_varkey(&keys[16..])
            .map_err(|_| format_err!("MAC keylength error"))?;
        verifier.update(&iv);
        verifier.update(contents);
        verifier
            .verify(&mac)
            .map_err(|_| format_err!("MAC error"))?;
    }

    use aes::Aes128;
    use block_modes::block_padding::Pkcs7;
    use block_modes::{BlockMode, Cbc};
    // Decrypt password
    let cipher = Cbc::<Aes128, Pkcs7>::new_var(&keys[0..16], &iv)
        .map_err(|_| format_err!("CBC initialization error"))?;
    Ok(cipher
        .decrypt(contents)
        .map_err(|_| format_err!("AES CBC decryption error"))?
        .to_owned())
}

async fn load_file(keys: [u8; 16 + 20], path: PathBuf) -> Result<Vec<u8>, Error> {
    let contents = actix_threadpool::run(move || load_file_sync(keys, path)).await?;

    Ok(contents)
}

impl Storage {
    pub fn open<T: AsRef<Path>>(db_path: &StorageLocation<T>) -> Result<Storage, Error> {
        let db = db_path.open_db()?;

        // XXX
        let protocol_store = ProtocolStore::invalid();

        Ok(Storage {
            db: Arc::new(Mutex::new(db)),
            keys: None,
            protocol_store: Arc::new(Mutex::new(protocol_store)),
            path: db_path.to_path_buf(),
        })
    }

    pub async fn open_with_password<T: AsRef<Path>>(
        db_path: &StorageLocation<T>,
        password: String,
    ) -> Result<Storage, Error> {
        let path: &Path = std::ops::Deref::deref(db_path);
        let db_salt_path = path.join("db").join("salt");
        let storage_salt_path = path.join("storage").join("salt");
        // XXX: The storage_key could already be polled while we're querying the database,
        // but we don't want to wait for it either.
        let db_key = derive_db_key(password.clone(), db_salt_path);
        let storage_key = derive_storage_key(password, storage_salt_path);

        // 1. decrypt DB
        let db = db_path.open_db()?;
        db.execute(&format!(
            "PRAGMA key = \"x'{}'\";",
            hex::encode(db_key.await?)
        ))?;
        db.execute("PRAGMA cipher_page_size = 4096;")?;

        // From the sqlcipher manual:
        // -- if this throws an error, the key was incorrect. If it succeeds and returns a numeric value, the key is correct;
        db.execute("SELECT count(*) FROM sqlite_master;")?;
        // XXX: Do we have to signal somehow that the password was wrong?
        //      Offer retries?

        // 2. decrypt storage
        let keys = Some(storage_key.await?);

        // 3. decrypt protocol store
        let protocol_store =
            ProtocolStore::open_with_key(keys.unwrap(), &crate::store::default_location().unwrap())
                .await?;

        Ok(Storage {
            db: Arc::new(Mutex::new(db)),
            keys,
            protocol_store: Arc::new(Mutex::new(protocol_store)),
            path: path.to_path_buf(),
        })
    }

    /// Asynchronously loads the signal HTTP password from storage and decrypts it.
    pub async fn signal_password(&self) -> Result<String, Error> {
        let contents = self
            .load_file(
                crate::store::default_location()
                    .unwrap()
                    .join("storage")
                    .join("identity")
                    .join("http_password"),
            )
            .await?;
        Ok(String::from_utf8(contents)?)
    }

    /// Asynchronously loads the base64 encoded signaling key.
    pub async fn signaling_key(&self) -> Result<[u8; 52], Error> {
        let v = self
            .load_file(
                crate::store::default_location()
                    .unwrap()
                    .join("storage")
                    .join("identity")
                    .join("http_signaling_key"),
            )
            .await?;
        ensure!(v.len() == 52, "Signaling key is 52 bytes");
        let mut out = [0u8; 52];
        out.copy_from_slice(&v);
        Ok(out)
    }

    async fn load_file<'s>(&'s self, path: PathBuf) -> Result<Vec<u8>, Error> {
        // XXX: unencrypted storage.
        load_file(self.keys.unwrap(), path).await
    }

    /// Process message and store in database and update or create a session
    pub fn process_message(
        &mut self,
        mut new_message: NewMessage,
        group: Option<NewGroup<'_>>,
        is_unread: bool,
    ) -> (Message, Session) {
        let db_session_res = if let Some(group) = group.as_ref() {
            let group_hex_id = hex::encode(group.id);
            self.fetch_session_by_group(&group_hex_id)
        } else {
            self.fetch_session_by_source(&new_message.source)
        };

        // Initialize the session data to work with, modify it in case of a group
        let mut session_data = NewSession {
            source: new_message.source.clone(),
            message: new_message.text.clone(),
            timestamp: new_message.timestamp,
            sent: new_message.sent,
            received: new_message.received,
            unread: is_unread,
            has_attachment: new_message.has_attachment,
            is_group: false,
            group_id: None,
            group_name: None,
            group_members: None,
        };

        if let Some(group) = group.as_ref() {
            let group_hex_id = hex::encode(group.id);
            session_data.is_group = true;
            session_data.source = group_hex_id.clone();
            session_data.group_id = Some(group_hex_id);
            session_data.group_name = Some(group.name.clone());
            session_data.group_members = Some(group.members.join(","));
        };

        let db_session: Session = if db_session_res.is_some() {
            let db_sess = db_session_res.unwrap();
            self.update_session(&db_sess, &session_data, is_unread);
            db_sess
        } else {
            self.create_session(&session_data)
                .expect("Unable to create session yet create_session() did not panic")
        };

        // XXX: Double-checking `is_none()` for this is considered reachable code,
        // yet the type system should make it obvious it can never be `None`.
        new_message.session_id = Some(db_session.id);

        // With the prepared new_message in hand, see if it's an update or a new one
        let update_msg_res = self.update_message_if_needed(&new_message);

        let message = if let Some(update_message) = update_msg_res {
            update_message
        } else {
            self.create_message(&new_message)
        };

        (message, db_session)
    }

    /// Create a new session. This was transparent within SaveSession in Go.
    ///
    /// It needs to be locked from the outside because sqlite sucks.
    pub fn create_session(&self, new_session: &NewSession) -> Option<Session> {
        use crate::schema::session::dsl as schema_dsl;

        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called create_session()");

        let query = diesel::insert_into(schema_dsl::session).values(new_session);

        let res = query.execute(&*conn).expect("inserting a session");

        // Then see if the session was inserted ok and what it was
        drop(conn); // Connection must be dropped because everyone wants a lock here
        let latest_session_res = self.fetch_latest_session();

        if res != 1 || latest_session_res.is_none() {
            panic!("Non-error non-insert!")
        }

        let latest_session = latest_session_res.unwrap();

        // XXX: This is checking that we got the latest one we expect,
        //      because sqlite sucks and some other thread might have inserted
        if latest_session.timestamp != new_session.timestamp
            || latest_session.source != new_session.source
        {
            panic!(
                "Could not match latest session to this one!
                       latest.source {} == new.source {} | latest.tstamp {} == new.timestamp {}",
                latest_session.source,
                new_session.source,
                latest_session.timestamp,
                new_session.timestamp
            );
        }

        // Better hope something panicked before now if something went wrong
        Some(latest_session)
    }

    /// Update an existing session. This was transparent within SaveSession in Go.
    ///
    /// It needs to be locked from the outside because sqlite sucks.
    /// Also with better schema design this whole thing would be moot!
    pub fn update_session(&self, db_session: &Session, new_session: &NewSession, is_unread: bool) {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called update_session()");

        let query = diesel::update(session::table.filter(session::id.eq(db_session.id))).set((
            session::message.eq(&new_session.message),
            session::timestamp.eq(new_session.timestamp),
            session::unread.eq(is_unread),
            session::sent.eq(new_session.sent),
            session::received.eq(new_session.received),
            session::has_attachment.eq(new_session.has_attachment),
        ));
        query.execute(&*conn).expect("updating session");
    }

    /// Marks the message with a certain timestamp as received.
    ///
    /// Copy from Go's MarkMessageReceived.
    pub fn mark_message_received(&self, timestamp: u64) -> Option<(Session, Message)> {
        let message = self.fetch_message_by_timestamp(timestamp)?;
        log::trace!("mark_message_received: {:?}", message);
        let session = self.fetch_session(message.sid)?;
        log::trace!("mark_message_received: {:?}", session);

        let conn = self.db.lock().unwrap();
        conn.transaction(|| -> Result<_, diesel::result::Error> {
            diesel::update(message::table.filter(message::id.eq(&message.id)))
                .set(message::received.eq(true))
                .execute(&*conn)?;

            diesel::update(
                session::table.filter(
                    session::id
                        .eq(&session.id)
                        .and(session::timestamp.eq(timestamp as i64)),
                ),
            )
            .set(session::received.eq(true))
            .execute(&*conn)?;
            Ok(())
        })
        .expect("update received state");

        Some((session, message))
    }

    /// This was implicit in Go, which probably didn't use threads.
    ///
    /// It needs to be locked from the outside because sqlite sucks.
    pub fn fetch_latest_session(&self) -> Option<Session> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_latest_session()");
        session::table
            .order_by(session::columns::id.desc())
            .first(&*conn)
            .ok()
    }

    pub fn fetch_session(&self, sid: i64) -> Option<Session> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_session({})", sid);
        session::table
            .filter(session::columns::id.eq(sid))
            .first(&*conn)
            .ok()
    }

    pub fn fetch_session_by_source(&self, source: &str) -> Option<Session> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_session_by_source({})", source);
        session::table
            .filter(session::columns::source.eq(source))
            .first(&*conn)
            .ok()
    }

    pub fn fetch_session_by_group(&self, group_id: &str) -> Option<Session> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_session_by_group({})", group_id);
        session::table
            .filter(session::columns::group_id.eq(group_id))
            .first(&*conn)
            .ok()
    }

    pub fn delete_session(&self, id: i64) {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called delete_session({})", id);

        // Preserve the Go order of deleting things
        conn.transaction(|| -> Result<_, diesel::result::Error> {
            // SessioN
            // `delete from session where id = ?`
            let query = diesel::delete(session::table.filter(session::columns::id.eq(id)));
            let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
            log::trace!("{:?}", debug);
            query.execute(&*conn)?;

            // SentQ
            // `delete from sentq where message_id in (select id from message where session_id = ?)`
            //
            // XXX: I hate the for loop, but the below fight with Diesel
            //      is not conductive to getting things actually done...

            /*
            let query = diesel::delete(sentq::table).filter(
                sentq::message_id.eq_any(
                    message::table
                        .filter(message::columns::session_id.eq(id))
                        .select(message::columns::session_id),
                ),
            );
            */

            for msg_id in message::table
                .select(message::columns::id)
                .filter(message::columns::session_id.eq(id))
                .load::<i32>(&*conn)
                .unwrap()
            {
                let query =
                    diesel::delete(sentq::table.filter(sentq::columns::message_id.eq(msg_id)));
                let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
                log::trace!("{:?}", debug);
                query.execute(&*conn)?;
            }

            let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
            log::trace!("{:?}", debug);
            query.execute(&*conn)?;

            // Messages
            // `delete from message where session_id = ?`
            let query = diesel::delete(message::table.filter(message::columns::session_id.eq(id)));
            let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
            log::trace!("{:?}", debug);
            query.execute(&*conn)?;

            Ok(())
        })
        .expect("deleting session and its messages");
    }

    pub fn mark_session_read(&self, sess: &Session) {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called mark_session_read({})", sess.id);

        diesel::update(session::table.filter(session::id.eq(sess.id)))
            .set((session::unread.eq(false),))
            .execute(&*conn)
            .expect("Mark session read");
    }

    /// Check if message exists and explicitly update it if required
    ///
    /// This is because during development messages may come in partially
    fn update_message_if_needed(&self, new_message: &NewMessage) -> Option<Message> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!(
            "Called update_message_if_needed({})",
            new_message.session_id.unwrap()
        );

        let mut msg: Message = message::table
            .left_join(sentq::table)
            .select((
                message::columns::id,
                message::columns::session_id,
                message::columns::source,
                message::columns::text,
                message::columns::timestamp,
                message::columns::sent,
                message::columns::received,
                message::columns::flags,
                message::columns::attachment,
                message::columns::mime_type,
                message::columns::has_attachment,
                message::columns::outgoing,
                sql::<diesel::sql_types::Bool>(
                    "CASE WHEN sentq.message_id > 0 THEN 1 ELSE 0 END AS queued",
                ),
            ))
            .filter(message::columns::session_id.eq(new_message.session_id.unwrap()))
            .filter(message::columns::timestamp.eq(new_message.timestamp))
            .filter(message::columns::text.eq(&new_message.text))
            .order_by(message::columns::id.desc())
            .first(&*conn)
            .ok()?;

        // Do not update `(session_id, timestamp, message)` because that's considered unique
        // nor `source` which is correlated with `session_id`
        if msg.sent != new_message.sent
            || msg.received != new_message.received
            || msg.flags != new_message.flags
            || msg.attachment != new_message.attachment
            || msg.mimetype != new_message.mime_type
            || msg.hasattachment != new_message.has_attachment
            || msg.outgoing != new_message.outgoing
        {
            let query = diesel::update(message::table.filter(message::id.eq(msg.id))).set((
                message::sent.eq(new_message.sent),
                message::received.eq(new_message.received),
                message::flags.eq(new_message.flags),
                message::attachment.eq(&new_message.attachment),
                message::mime_type.eq(&new_message.mime_type),
                message::has_attachment.eq(new_message.has_attachment),
                message::outgoing.eq(new_message.outgoing),
            ));

            query.execute(&*conn).expect("updating message");

            // Also update the message we got from the db to match what was updated
            msg.sent = new_message.sent;
            msg.received = new_message.received;
            msg.flags = new_message.flags;
            msg.attachment = new_message.attachment.clone();
            msg.mimetype = new_message.mime_type.clone();
            msg.hasattachment = new_message.has_attachment;
            msg.outgoing = new_message.outgoing;
        }

        Some(msg)
    }

    pub fn register_attachment(&mut self, mid: i32, path: &str, mime_type: &str) {
        // XXX: multiple attachments https://gitlab.com/rubdos/whisperfish/-/issues/11

        let db = self.db.lock();
        let conn = db.unwrap();

        diesel::update(message::table.filter(message::id.eq(mid)))
            .set((
                message::mime_type.eq(mime_type),
                message::has_attachment.eq(true),
                message::attachment.eq(path),
            ))
            .execute(&*conn)
            .expect("set attachment");
    }

    /// Create a new message. This was transparent within SaveMessage in Go.
    pub fn create_message(&self, new_message: &NewMessage) -> Message {
        use crate::schema::message::dsl as schema_dsl;

        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called create_message()");

        let query = diesel::insert_into(schema_dsl::message).values(new_message);

        let res = query.execute(&*conn).expect("inserting a message");

        // Then see if the message was inserted ok and what it was
        drop(conn); // Connection must be dropped because everyone wants a lock here
        let latest_message_res = self.fetch_latest_message();

        if res != 1 || latest_message_res.is_none() {
            panic!("Non-error non-insert!")
        }

        let latest_message = latest_message_res.unwrap();

        // XXX: This is checking that we got the latest one we expect,
        //      because sqlite sucks and some other thread might have inserted

        if latest_message.timestamp != new_message.timestamp
            || latest_message.source != new_message.source
        {
            panic!(
                "Could not match latest message to this one!
                       latest.source {} == new.source {} | latest.tstamp {} == new.timestamp {}",
                latest_message.source,
                new_message.source,
                latest_message.timestamp,
                new_message.timestamp
            );
        }

        log::trace!("Inserted message id {}", latest_message.id);
        latest_message
    }

    /// This was implicit in Go, which probably didn't use threads.
    ///
    /// It needs to be locked from the outside because sqlite sucks.
    pub fn fetch_latest_message(&self) -> Option<Message> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_latest_message()");
        message::table
            .left_join(sentq::table)
            .select((
                message::columns::id,
                message::columns::session_id,
                message::columns::source,
                message::columns::text,
                message::columns::timestamp,
                message::columns::sent,
                message::columns::received,
                message::columns::flags,
                message::columns::attachment,
                message::columns::mime_type,
                message::columns::has_attachment,
                message::columns::outgoing,
                sql::<diesel::sql_types::Bool>(
                    "CASE WHEN sentq.message_id > 0 THEN 1 ELSE 0 END AS queued",
                ),
            ))
            .order_by(message::columns::id.desc())
            .first(&*conn)
            .ok()
    }

    pub fn fetch_message_by_timestamp(&self, ts: u64) -> Option<Message> {
        let db = self.db.lock();
        let conn = db.unwrap();

        // Even a single message needs to know if it's queued to satisfy the `Message` trait
        log::trace!("Called fetch_message_by_timestamp({})", ts);
        let query = message::table
            .left_join(sentq::table)
            .select((
                message::columns::id,
                message::columns::session_id,
                message::columns::source,
                message::columns::text,
                message::columns::timestamp,
                message::columns::sent,
                message::columns::received,
                message::columns::flags,
                message::columns::attachment,
                message::columns::mime_type,
                message::columns::has_attachment,
                message::columns::outgoing,
                sql::<diesel::sql_types::Bool>(
                    "CASE WHEN sentq.message_id > 0 THEN 1 ELSE 0 END AS queued",
                ),
            ))
            .filter(message::columns::timestamp.eq(ts as i64));

        let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
        log::trace!("{}", debug.to_string());

        query.first(&*conn).ok()
    }

    pub fn fetch_message(&self, id: i32) -> Option<Message> {
        let db = self.db.lock();
        let conn = db.unwrap();

        // Even a single message needs to know if it's queued to satisfy the `Message` trait
        log::trace!("Called fetch_message({})", id);
        let query = message::table
            .left_join(sentq::table)
            .select((
                message::columns::id,
                message::columns::session_id,
                message::columns::source,
                message::columns::text,
                message::columns::timestamp,
                message::columns::sent,
                message::columns::received,
                message::columns::flags,
                message::columns::attachment,
                message::columns::mime_type,
                message::columns::has_attachment,
                message::columns::outgoing,
                sql::<diesel::sql_types::Bool>(
                    "CASE WHEN sentq.message_id > 0 THEN 1 ELSE 0 END AS queued",
                ),
            ))
            .filter(message::columns::id.eq(id));

        let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
        log::trace!("{}", debug.to_string());

        query.first(&*conn).ok()
    }

    pub fn fetch_all_messages(&self, sid: i64) -> Option<Vec<Message>> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called fetch_all_messages({})", sid);
        let query = message::table
            .left_join(sentq::table)
            .select((
                message::columns::id,
                message::columns::session_id,
                message::columns::source,
                message::columns::text,
                message::columns::timestamp,
                message::columns::sent,
                message::columns::received,
                message::columns::flags,
                message::columns::attachment,
                message::columns::mime_type,
                message::columns::has_attachment,
                message::columns::outgoing,
                sql::<diesel::sql_types::Bool>(
                    "CASE WHEN sentq.message_id > 0 THEN 1 ELSE 0 END AS queued",
                ),
            ))
            .filter(message::columns::session_id.eq(sid))
            .order_by(message::columns::id.desc());

        let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
        log::trace!("{}", debug.to_string());

        query.load::<Message>(&*conn).ok()
    }

    pub fn delete_message(&self, id: i32) -> Option<usize> {
        let db = self.db.lock();
        let conn = db.unwrap();

        log::trace!("Called delete_message({})", id);

        // XXX: Assume `sentq` has nothing pending, as the Go version does
        let query = diesel::delete(message::table.filter(message::columns::id.eq(id)));

        let debug = debug_query::<diesel::sqlite::Sqlite, _>(&query);
        log::trace!("{}", debug.to_string());

        query.execute(&*conn).ok()
    }

    pub fn queue_message(&self, msg: &Message) {
        let db = self.db.lock();
        let conn = db.unwrap();

        diesel::insert_into(sentq::table)
            .values((
                sentq::message_id.eq(msg.id),
                sentq::timestamp.eq(msg.timestamp),
            ))
            .execute(&*conn)
            .unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::rstest;

    #[test]
    fn open_temp_db() -> Result<(), Error> {
        let temp = temp();
        std::fs::create_dir(temp.join("db"))?;
        std::fs::create_dir(temp.join("storage"))?;
        let _storage = Storage::open(&temp)?;

        Ok(())
    }

    #[rstest(
        mime_type,
        ext,
        case("video/mp4", "mp4"),
        case("image/jpg", "jpg"),
        case("image/jpeg", "jpg"),
        case("image/png", "png"),
        case("text/plain", "txt")
    )]
    #[actix_rt::test]
    async fn test_save_attachment(mime_type: &str, ext: &str) {
        use std::env;
        use std::fs;
        use std::path::Path;

        drop(mime_type); // This is used in client-worker, consider droppin g this argument.

        let dirname = env::temp_dir().to_str().expect("Temp dir fail").to_string();
        let dir = Path::new(&dirname);
        let mut contents = futures::io::Cursor::new([0u8]);
        let fname = save_attachment(dir, ext, &mut contents).await;

        let exists = Path::new(&fname).exists();

        println!("Looking for {}", fname.to_str().unwrap());
        assert!(exists);

        assert_eq!(
            fname.extension().unwrap(),
            ext,
            "{}",
            format!("{} <> {}", fname.to_str().unwrap(), ext)
        );

        fs::remove_file(fname).expect("Could not remove test case file");
    }

    #[test]
    fn encrypt_and_decrypt_file() -> Result<(), Error> {
        let contents = "The funny horse jumped over a river.";

        // Key full of ones.
        let key = [1u8; 16 + 20];
        let dir = temp();

        write_file_sync(
            key,
            dir.join("encrypt-and-decrypt.temp"),
            contents.as_bytes(),
        )?;
        let res = load_file_sync(key, dir.join("encrypt-and-decrypt.temp"))?;
        assert_eq!(std::str::from_utf8(&res).expect("utf8"), contents);

        Ok(())
    }
}
