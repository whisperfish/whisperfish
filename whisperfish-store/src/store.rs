pub mod orm;

pub mod body_ranges;
mod encryption;
#[cfg(feature = "diesel-instrumentation")]
mod instrumentation;
pub mod migrations;
pub mod observer;
mod protocol_store;
mod protos;
mod utils;

use self::orm::{AugmentedMessage, MessageType, StoryType, UnidentifiedAccessMode};
use crate::body_ranges::AssociatedValue;
use crate::diesel::connection::SimpleConnection;
use crate::diesel_migrations::MigrationHarness;
use crate::store::observer::{Observable, PrimaryKey};
use crate::{config::SignalConfig, millis_to_naive_chrono};
use crate::{naive_chrono_rounded_down, schema};
use anyhow::Context;
use chrono::prelude::*;
use diesel::dsl::sql;
use diesel::prelude::*;
use diesel::result::*;
use diesel::sql_types::{Bool, Timestamp};
use diesel_migrations::EmbeddedMigrations;
use itertools::Itertools;
use libsignal_service::groups_v2::InMemoryCredentialsCache;
use libsignal_service::proto::{attachment_pointer, data_message::Reaction, DataMessage};
use libsignal_service::protocol::{self, *};
use libsignal_service::zkgroup::api::groups::GroupSecretParams;
use libsignal_service::zkgroup::PROFILE_KEY_LEN;
use libsignal_service::{prelude::*, ServiceIdType};
use phonenumber::PhoneNumber;
pub use protocol_store::AciOrPniStorage;
use protocol_store::ProtocolStore;
use std::fmt::Debug;
use std::fs::File;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use uuid::Uuid;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!();
const DELETE_AFTER: &str = "DATETIME(expiry_started, '+' || expires_in || ' seconds')";

/// How much trust you put into the correctness of the data.
#[derive(Clone, Copy, Eq, Debug, PartialEq)]
pub enum TrustLevel {
    /// Set to Certain if the supplied information is from a trusted source,
    /// such as an envelope.
    Certain,
    Uncertain,
}

/// Session as it relates to the schema
#[derive(Queryable, Debug, Clone)]
pub struct Session {
    pub id: i32,
    pub source: String,
    pub message: String,
    pub timestamp: NaiveDateTime,
    pub sent: bool,
    pub received: bool,
    pub unread: bool,
    pub is_group: bool,
    pub is_muted: bool,
    pub is_archived: bool,
    pub is_pinned: bool,
    pub group_members: Option<String>,
    #[allow(dead_code)]
    pub group_id: Option<String>,
    pub group_name: Option<String>,
    pub has_attachment: bool,
    pub draft: Option<String>,
}

/// Message as it relates to the schema
#[derive(Queryable, Debug)]
pub struct Message {
    pub id: i32,
    pub sid: i32,
    pub source: String,
    pub message: String, // NOTE: "text" in schema, doesn't apparently matter
    pub timestamp: NaiveDateTime,
    pub sent: bool,
    pub received: bool,
    pub flags: i32,
    pub attachment: Option<String>,
    pub mimetype: Option<String>,
    pub hasattachment: bool,
    pub outgoing: bool,
    pub queued: bool,
    pub message_type: Option<MessageType>,
}

#[derive(Debug)]
pub struct MessagePointer {
    pub message_id: i32,
    pub session_id: i32,
}

/// ID-free Message model for insertions
#[derive(Clone, Debug)]
pub struct NewMessage<'a> {
    pub session_id: i32,
    pub source_addr: Option<ServiceAddress>,
    pub server_guid: Option<Uuid>,
    pub text: String,
    pub timestamp: NaiveDateTime,
    pub sent: bool,
    pub received: bool,
    pub is_read: bool,
    pub flags: i32,
    pub outgoing: bool,
    pub is_unidentified: bool,
    pub quote_timestamp: Option<u64>,
    pub expires_in: Option<std::time::Duration>,
    pub story_type: StoryType,
    pub body_ranges: Option<Vec<u8>>,
    pub message_type: Option<MessageType>,

    pub edit: Option<&'a orm::Message>,
}

impl NewMessage<'_> {
    pub fn new_incoming() -> Self {
        Self {
            session_id: 0,
            source_addr: None,
            server_guid: None,
            text: "".to_string(),
            timestamp: chrono::Utc::now().naive_utc(),
            sent: false,
            received: true,
            is_read: false,
            flags: 0,
            outgoing: false,
            is_unidentified: false,
            quote_timestamp: None,
            expires_in: None,
            story_type: StoryType::None,
            body_ranges: None,
            message_type: None,
            edit: None,
        }
    }

    pub fn new_outgoing() -> Self {
        Self {
            session_id: 0,
            source_addr: None,
            server_guid: None,
            text: "".to_string(),
            timestamp: chrono::Utc::now().naive_utc(),
            sent: false,
            received: false,
            is_read: true,
            flags: 0,
            outgoing: true,
            is_unidentified: false,
            quote_timestamp: None,
            expires_in: None,
            story_type: StoryType::None,
            body_ranges: None,
            message_type: None,
            edit: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct StoreProfile {
    pub given_name: Option<String>,
    pub family_name: Option<String>,
    pub joined_name: Option<String>,
    pub about_text: Option<String>,
    pub emoji: Option<String>,
    pub avatar: Option<String>,
    pub unidentified: UnidentifiedAccessMode,
    pub last_fetch: NaiveDateTime,
    pub r_uuid: Uuid,
    pub r_id: i32,
    pub r_key: Option<Vec<u8>>,
}

enum RecipientOperation {
    SetPni(i32, Option<Uuid>),
    SetAci(i32, Option<Uuid>),
    SetE164(i32, Option<PhoneNumber>),
    Merge(i32, i32),
    /// ACI, PNI, E.164
    Create(Option<Uuid>, Option<Uuid>, Option<PhoneNumber>),
}

impl Debug for RecipientOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RecipientOperation::")?;
        match self {
            RecipientOperation::SetPni(id, pni) => write!(f, "SetPni({}, {:?})", id, pni),
            RecipientOperation::SetAci(id, aci) => write!(f, "SetAci({}, {:?})", id, aci),
            RecipientOperation::SetE164(id, e164) => write!(f, "SetE164({}, {:?})", id, e164),
            RecipientOperation::Merge(src, tgt) => write!(f, "Merge({}, {})", src, tgt),
            RecipientOperation::Create(aci, pni, e164) => {
                write!(f, "Create(ACI:{:?}, PNI:{:?}, E164:{:?})", aci, pni, e164,)
            }
        }
    }
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GroupContext {
    GroupV1(GroupV1),
    GroupV2(GroupV2),
}

impl From<GroupV1> for GroupContext {
    fn from(v1: GroupV1) -> GroupContext {
        GroupContext::GroupV1(v1)
    }
}

impl From<GroupV2> for GroupContext {
    fn from(v2: GroupV2) -> GroupContext {
        GroupContext::GroupV2(v2)
    }
}

/// ID-free Group model for insertions
#[derive(Clone, Debug)]
pub struct GroupV1 {
    pub id: Vec<u8>,
    /// Group name
    pub name: String,
    /// List of phone numbers
    pub members: Vec<PhoneNumber>,
}

#[derive(Clone)]
pub struct GroupV2 {
    pub secret: GroupSecretParams,
    pub revision: u32,
}

impl std::fmt::Debug for GroupV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GroupV2")
            .field("id", &self.secret.get_group_identifier())
            .field("revision", &self.revision)
            .finish()
    }
}

/// Location of the storage.
///
/// Path is for persistent storage.
/// Memory is for running tests.
#[cfg_attr(not(test), allow(unused))]
#[derive(Debug)]
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
pub fn temp() -> StorageLocation<tempfile::TempDir> {
    StorageLocation::Path(tempfile::tempdir().unwrap())
}

pub fn default_location() -> Result<StorageLocation<PathBuf>, anyhow::Error> {
    let data_dir = dirs::data_local_dir().context("Could not find data directory.")?;

    Ok(StorageLocation::Path(
        data_dir.join("be.rubdos").join("harbour-whisperfish"),
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
    pub fn open_db(&self) -> Result<SqliteConnection, anyhow::Error> {
        let database_url = match self {
            StorageLocation::Memory => ":memory:".into(),
            StorageLocation::Path(p) => p
                .as_ref()
                .join("db")
                .join("harbour-whisperfish.db")
                .to_str()
                .context("path to db contains a non-UTF8 character, please file a bug.")?
                .to_string(),
        };
        #[allow(unused_mut)]
        let mut conn = SqliteConnection::establish(&database_url)?;
        #[cfg(feature = "diesel-instrumentation")]
        conn.set_instrumentation(instrumentation::Instrumentation::default());
        Ok(conn)
    }
}

#[derive(Clone)]
pub struct Storage<O: Observable> {
    db: Arc<AssertUnwindSafe<Mutex<SqliteConnection>>>,
    observatory: O,
    config: Arc<SignalConfig>,
    store_enc: Option<encryption::StorageEncryption>,
    protocol_store: Arc<tokio::sync::RwLock<ProtocolStore>>,
    credential_cache: Arc<tokio::sync::RwLock<InMemoryCredentialsCache>>,
    path: PathBuf,
    aci_identity_key_pair: Arc<tokio::sync::RwLock<Option<IdentityKeyPair>>>,
    pni_identity_key_pair: Arc<tokio::sync::RwLock<Option<IdentityKeyPair>>>,
    self_recipient: Arc<std::sync::RwLock<Option<orm::Recipient>>>,
}

impl<O: Observable> Debug for Storage<O> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Storage")
            .field("path", &self.path)
            .field("is_encrypted", &self.is_encrypted())
            .finish()
    }
}

/// Fetches an `orm::Session`, for which the supplied closure can impose constraints.
///
/// This *can* in principe be implemented with pure type constraints,
/// but I'm not in the mood for digging a few hours through Diesel's traits.
macro_rules! fetch_session {
    ($db:expr, |$fragment:ident| $b:block ) => {{
        let mut db = $db;
        let query = {
            let $fragment = schema::sessions::table
                .left_join(schema::recipients::table)
                .left_join(schema::group_v1s::table)
                .left_join(schema::group_v2s::table);
            $b
        };
        let triple: Option<(
            orm::DbSession,
            Option<orm::Recipient>,
            Option<orm::GroupV1>,
            Option<orm::GroupV2>,
        )> = query.first(&mut *db).ok();
        triple.map(Into::into)
    }};
}
macro_rules! fetch_sessions {
    ($db:expr, |$fragment:ident| $b:block ) => {{
        let mut db = $db;
        let query = {
            let $fragment = schema::sessions::table
                .left_join(schema::recipients::table)
                .left_join(schema::group_v1s::table)
                .left_join(schema::group_v2s::table);
            $b
        };
        let triples: Vec<(
            orm::DbSession,
            Option<orm::Recipient>,
            Option<orm::GroupV1>,
            Option<orm::GroupV2>,
        )> = query.load(&mut *db).unwrap();
        triples.into_iter().map(orm::Session::from).collect()
    }};
}

impl<O: Observable + Default> Storage<O> {
    /// Writes (*overwrites*) a new Storage object to the provided path.
    #[allow(clippy::too_many_arguments)]
    pub async fn new<T: AsRef<Path> + Debug>(
        config: Arc<SignalConfig>,
        db_path: &StorageLocation<T>,
        password: Option<&str>,
        regid: u32,
        pni_regid: u32,
        http_password: &str,
        aci_identity_key_pair: Option<protocol::IdentityKeyPair>,
        pni_identity_key_pair: Option<protocol::IdentityKeyPair>,
    ) -> Result<Self, anyhow::Error> {
        let path: &Path = std::ops::Deref::deref(db_path);

        tracing::info!("Creating directory structure");
        Self::scaffold_directories(path)?;

        // 1. Generate both salts if needed and create a storage encryption object if necessary
        let store_enc = if let Some(password) = password {
            let db_salt_path = path.join("db").join("salt");
            let storage_salt_path = path.join("storage").join("salt");

            use rand::RngCore;
            tracing::info!("Generating salts");
            let mut db_salt = [0u8; 8];
            let mut storage_salt = [0u8; 8];
            let mut rng = rand::thread_rng();
            rng.fill_bytes(&mut db_salt);
            rng.fill_bytes(&mut storage_salt);

            utils::write_file_async(db_salt_path, &db_salt).await?;
            utils::write_file_async(storage_salt_path, &storage_salt).await?;

            Some(
                encryption::StorageEncryption::new(password.to_string(), storage_salt, db_salt)
                    .await?,
            )
        } else {
            None
        };

        // 2. Open DB
        let db = Self::open_db(db_path, store_enc.as_ref().map(|x| x.get_database_key())).await?;

        // 3. initialize protocol store
        let aci_identity_key_pair = aci_identity_key_pair
            .unwrap_or_else(|| protocol::IdentityKeyPair::generate(&mut rand::thread_rng()));
        let pni_identity_key_pair = pni_identity_key_pair
            .unwrap_or_else(|| protocol::IdentityKeyPair::generate(&mut rand::thread_rng()));

        let protocol_store = ProtocolStore::new(
            store_enc.as_ref(),
            path,
            regid,
            pni_regid,
            aci_identity_key_pair,
            pni_identity_key_pair,
        )
        .await?;

        // 4. save http password and signaling key
        let identity_path = path.join("storage").join("identity");
        utils::write_file_async_encrypted(
            identity_path.join("http_password"),
            http_password.as_bytes(),
            store_enc.as_ref(),
        )
        .await?;

        Ok(Storage {
            db: Arc::new(AssertUnwindSafe(Mutex::new(db))),
            observatory: Default::default(),
            config,
            store_enc,
            protocol_store: Arc::new(tokio::sync::RwLock::new(protocol_store)),
            credential_cache: Arc::new(tokio::sync::RwLock::new(
                InMemoryCredentialsCache::default(),
            )),
            path: path.to_path_buf(),
            aci_identity_key_pair: Arc::new(tokio::sync::RwLock::new(Some(aci_identity_key_pair))),
            pni_identity_key_pair: Arc::new(tokio::sync::RwLock::new(Some(pni_identity_key_pair))),
            self_recipient: Arc::new(std::sync::RwLock::new(None)),
        })
    }

    #[tracing::instrument(skip(config, password))]
    pub async fn open<T: AsRef<Path> + Debug>(
        config: Arc<SignalConfig>,
        db_path: &StorageLocation<T>,
        password: Option<String>,
    ) -> Result<Self, anyhow::Error> {
        let path: &Path = std::ops::Deref::deref(db_path);

        let store_enc = if let Some(password) = password {
            // Get storage and db salt
            let storage_salt = utils::read_salt_file(path.join("storage").join("salt")).await?;
            let db_salt = utils::read_salt_file(path.join("db").join("salt")).await?;

            Some(
                encryption::StorageEncryption::new(password.to_string(), storage_salt, db_salt)
                    .await?,
            )
        } else {
            None
        };

        let db = Self::open_db(db_path, store_enc.as_ref().map(|x| x.get_database_key()))
            .await
            .context("Opening database")?;

        let protocol_store = ProtocolStore::open().await;

        let storage = Storage {
            db: Arc::new(AssertUnwindSafe(Mutex::new(db))),
            observatory: Default::default(),
            config,
            store_enc,
            protocol_store: Arc::new(tokio::sync::RwLock::new(protocol_store)),
            credential_cache: Arc::new(tokio::sync::RwLock::new(
                InMemoryCredentialsCache::default(),
            )),
            path: path.to_path_buf(),
            // XXX load them from storage already?
            aci_identity_key_pair: Arc::new(tokio::sync::RwLock::new(None)),
            pni_identity_key_pair: Arc::new(tokio::sync::RwLock::new(None)),
            self_recipient: Arc::new(std::sync::RwLock::new(None)),
        };

        Ok(storage)
    }
}

impl<O: Observable> Storage<O> {
    /// Returns the path to the storage.
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn db(&self) -> MutexGuard<'_, SqliteConnection> {
        self.db.lock().expect("storage is alive")
    }

    pub fn is_encrypted(&self) -> bool {
        self.store_enc.is_some()
    }

    pub fn clear_old_logs(
        path: &std::path::PathBuf,
        keep_count: usize,
        filename_regex: &str,
    ) -> bool {
        self::utils::clear_old_logs(path, keep_count, filename_regex)
    }

    fn scaffold_directories(root: impl AsRef<Path>) -> Result<(), anyhow::Error> {
        let root = root.as_ref();

        let directories = [
            root.to_path_buf() as PathBuf,
            root.join("db"),
            root.join("storage"),
            root.join("storage").join("identity"),
            root.join("storage").join("attachments"),
            root.join("storage").join("avatars"),
        ];

        for dir in &directories {
            if dir.exists() {
                if dir.is_dir() {
                    continue;
                } else {
                    anyhow::bail!(
                        "Trying to create directory {:?}, but already exists as non-directory.",
                        dir
                    );
                }
            }
            std::fs::create_dir(dir)?;
        }
        Ok(())
    }

    #[tracing::instrument]
    async fn open_db<T: AsRef<Path> + Debug>(
        db_path: &StorageLocation<T>,
        database_key: Option<&[u8]>,
    ) -> anyhow::Result<SqliteConnection, anyhow::Error> {
        let mut db = db_path.open_db()?;

        if let Some(database_key) = database_key {
            let _span = tracing::info_span!("Setting DB encryption").entered();

            // db.batch_execute("PRAGMA cipher_log = stderr;")
            //     .context("setting sqlcipher log output to stderr")?;
            // db.batch_execute("PRAGMA cipher_log_level = DEBUG;")
            //     .context("setting sqlcipher log level to debug")?;

            db.batch_execute(&format!(
                "PRAGMA key = \"x'{}'\";",
                hex::encode(database_key)
            ))
            .context("setting key")?;
            // `cipher_compatibility = 3` sets cipher_page_size = 1024,
            // while Go-Whisperfish used to use 4096.
            // Therefore,
            // ```
            // db.batch_execute("PRAGMA cipher_compatibility = 3;")?;
            // ```
            // does not work.  We manually set the parameters of Sqlcipher 3.4 now,
            // and we postpone migration until we see that this sufficiencly works.
            db.batch_execute("PRAGMA cipher_page_size = 4096;")
                .context("setting cipher_page_size")?;
            db.batch_execute("PRAGMA kdf_iter = 64000;")
                .context("setting kdf_iter")?;
            db.batch_execute("PRAGMA cipher_hmac_algorithm = HMAC_SHA1;")
                .context("setting cipher_hmac_algorithm")?;
            db.batch_execute("PRAGMA cipher_kdf_algorithm = PBKDF2_HMAC_SHA1;")
                .context("setting cipher_kdf_algorithm")?;
        }

        // From the sqlcipher manual:
        // -- if this throws an error, the key was incorrect. If it succeeds and returns a numeric value, the key is correct;
        db.batch_execute("SELECT count(*) FROM sqlite_master;")
            .context("attempting a read; probably wrong password")?;
        // XXX: Do we have to signal somehow that the password was wrong?
        //      Offer retries?

        // Run migrations.
        // We execute the transactions without foreign key checking enabled.
        // This is because foreign_keys=OFF implies that foreign key references are
        // not renamed when their parent table is renamed on *old SQLite version*.
        // https://stackoverflow.com/questions/67006159/how-to-re-parent-a-table-foreign-key-in-sqlite-after-recreating-the-parent
        // We can very probably do normal foreign_key checking again when we are on a more recent
        // SQLite.
        // That said, our check_foreign_keys() does output more useful information for when things
        // go haywire, albeit a bit later.
        let _span = tracing::info_span!("Running migrations").entered();
        db.batch_execute("PRAGMA foreign_keys = OFF;").unwrap();
        db.transaction::<_, anyhow::Error, _>(|db| {
            let migrations = db
                .pending_migrations(MIGRATIONS)
                .map_err(|e| anyhow::anyhow!("Filtering migrations: {}", e))?;
            if !migrations.is_empty() {
                db.run_migrations(&migrations)
                    .map_err(|e| anyhow::anyhow!("Running migrations: {}", e))?;
                crate::check_foreign_keys(db)?;
            }
            Ok(())
        })?;
        db.batch_execute("PRAGMA foreign_keys = ON;").unwrap();

        Ok(db)
    }

    /// Asynchronously loads the signal HTTP password from storage and decrypts it.
    #[tracing::instrument(skip(self))]
    pub async fn signal_password(&self) -> Result<String, anyhow::Error> {
        let contents = self
            .read_file(
                &self
                    .path
                    .join("storage")
                    .join("identity")
                    .join("http_password"),
            )
            .await?;
        Ok(String::from_utf8(contents)?)
    }

    /// Asynchronously loads the base64 encoded signaling key.
    #[tracing::instrument(skip(self))]
    pub async fn signaling_key(&self) -> Result<Option<[u8; 52]>, anyhow::Error> {
        let path = self
            .path
            .join("storage")
            .join("identity")
            .join("http_signaling_key");
        if !path.exists() {
            return Ok(None);
        }
        let v = self.read_file(&path).await?;
        anyhow::ensure!(v.len() == 52, "Signaling key is 52 bytes");
        let mut out = [0u8; 52];
        out.copy_from_slice(&v);
        Ok(Some(out))
    }

    // This is public for session_to_db migration
    #[tracing::instrument]
    pub async fn read_file(
        &self,
        path: impl AsRef<std::path::Path> + Debug,
    ) -> Result<Vec<u8>, anyhow::Error> {
        utils::read_file_async_encrypted(path, self.store_enc.as_ref()).await
    }

    #[tracing::instrument]
    pub async fn write_file(
        &self,
        path: impl AsRef<std::path::Path> + Debug,
        content: impl Into<Vec<u8>> + Debug,
    ) -> Result<(), anyhow::Error> {
        utils::write_file_async_encrypted(path, content, self.store_enc.as_ref()).await
    }

    /// Process reaction and store in database.
    #[tracing::instrument(skip(self, sender, data_message), fields(sender = %sender))]
    pub fn process_reaction(
        &mut self,
        sender: &orm::Recipient,
        data_message: &DataMessage,
        reaction: &Reaction,
    ) -> Option<(orm::Message, orm::Session)> {
        // XXX error handling...
        let ts = reaction.target_sent_timestamp.expect("target timestamp");
        let ts = millis_to_naive_chrono(ts);
        let message = self.fetch_message_by_timestamp(ts)?;
        let session = self
            .fetch_session_by_id(message.session_id)
            .expect("session exists");

        let target_author_uuid = Uuid::parse_str(reaction.target_author_aci())
            .map_err(|_| tracing::error!("ignoring reaction with invalid uuid"))
            .ok()?;

        if let Some(uuid) = sender.uuid {
            if uuid != target_author_uuid {
                tracing::warn!(
                    "uuid != reaction.target_author_uuid ({} != {}). Continuing, but this is a bug or attack.",
                    uuid,
                    target_author_uuid,
                );
            }
        }

        // Two options, either it's a *removal* or an update-or-replace
        // Both cases, we remove existing reactions for this author-message pair.
        if reaction.remove() {
            self.remove_reaction(message.id, sender.id);
        } else {
            // If this was not a removal action, we have a replacement
            let message_sent_time = millis_to_naive_chrono(data_message.timestamp());
            self.save_reaction(
                message.id,
                sender.id,
                reaction.emoji.to_owned().unwrap(),
                message_sent_time,
            );
        }

        Some((message, session))
    }

    #[tracing::instrument(skip(self))]
    pub fn save_reaction(
        &mut self,
        msg_id: i32,
        sender_id: i32,
        new_emoji: String,
        sent_ts: NaiveDateTime,
    ) {
        use crate::schema::reactions::dsl::*;
        use diesel::dsl::*;
        diesel::insert_into(reactions)
            .values((
                message_id.eq(msg_id),
                author.eq(sender_id),
                emoji.eq(new_emoji.clone()),
                sent_time.eq(sent_ts),
                received_time.eq(now),
            ))
            .on_conflict((author, message_id))
            .do_update()
            .set((
                emoji.eq(new_emoji),
                sent_time.eq(sent_ts),
                received_time.eq(now),
            ))
            .execute(&mut *self.db())
            .expect("insert reaction into database");
        tracing::trace!("Saved reaction for message {} from {}", msg_id, sender_id);
        self.observe_upsert(reactions, PrimaryKey::Unknown)
            .with_relation(schema::messages::table, msg_id);
    }

    #[tracing::instrument(skip(self))]
    pub fn remove_reaction(&mut self, msg_id: i32, sender_id: i32) {
        use crate::schema::reactions::dsl::*;
        diesel::delete(reactions)
            .filter(author.eq(sender_id))
            .filter(message_id.eq(msg_id))
            .execute(&mut *self.db())
            .expect("remove old reaction from database");
        tracing::trace!("Removed reaction for message {}", msg_id);
        self.observe_delete(reactions, PrimaryKey::Unknown)
            .with_relation(schema::messages::table, msg_id);
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_self_recipient(&self) -> Option<orm::Recipient> {
        let read_lock = self.self_recipient.read();
        if read_lock.is_ok() {
            if let Some(recipient) = (*read_lock.unwrap()).as_ref() {
                return Some(recipient.to_owned());
            }
        }

        let e164 = self.config.get_tel();
        let aci = self.config.get_aci().map(ServiceAddress::new_aci);
        let pni = self.config.get_pni().map(ServiceAddress::new_pni);
        if e164.is_none() {
            tracing::warn!("No E.164 set, cannot fetch self.");
            return None;
        }
        if aci.is_none() {
            tracing::warn!(
                "No ACI set. Continuing with E.164 {}",
                if pni.is_some() { "and PNI" } else { "only" }
            );
        }
        let self_rcpt = Some(self.merge_and_fetch_self_recipient(e164, aci, pni));

        let write_lock = self.self_recipient.write();
        if write_lock.is_ok() {
            write_lock.unwrap().clone_from(&self_rcpt);
        }

        self_rcpt
    }

    #[tracing::instrument(skip(self))]
    pub fn invalidate_self_recipient(&self) {
        let write_lock = self.self_recipient.write();
        if write_lock.is_ok() {
            *write_lock.unwrap() = None;
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_self_recipient_profile_key(&self) -> Option<Vec<u8>> {
        let read_lock = self.self_recipient.read();
        if read_lock.is_ok() {
            if let Some(recipient) = (*read_lock.unwrap()).as_ref() {
                return recipient.profile_key.clone();
            }
        }

        let recipient = self
            .fetch_self_recipient()
            .expect("no self recipient to retreive profile key from");
        return recipient.profile_key;
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_self_recipient_id(&self) -> i32 {
        let read_lock = self.self_recipient.read();
        if read_lock.is_ok() {
            if let Some(recipient) = (*read_lock.unwrap()).as_ref() {
                return recipient.id;
            }
        }

        let recipient = self
            .fetch_self_recipient()
            .expect("no self recipient to retreive db id from");
        return recipient.id;
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_self_service_address_aci(&self) -> Option<ServiceAddress> {
        self.config.get_aci().map(ServiceAddress::new_aci)
    }

    #[tracing::instrument(skip(self, rcpt_e164), fields(rcpt_e164 = %rcpt_e164))]
    pub fn fetch_recipient_by_phonenumber(
        &self,
        rcpt_e164: &PhoneNumber,
    ) -> Option<orm::Recipient> {
        use crate::schema::recipients::dsl::*;

        recipients
            .filter(e164.eq(rcpt_e164.to_string()))
            .first(&mut *self.db())
            .ok()
    }

    #[tracing::instrument(skip(self))]
    pub fn mark_recipient_needs_pni_signature(&self, recipient: &orm::Recipient, val: bool) {
        use crate::schema::recipients::dsl::*;

        let affected = diesel::update(recipients)
            .set(needs_pni_signature.eq(val))
            .filter(id.eq(recipient.id))
            .execute(&mut *self.db())
            .expect("db");

        // If updating self, invalidate the cache
        if recipient.uuid == self.config.get_aci() {
            self.invalidate_self_recipient();
        }

        if affected > 0 {
            self.observe_update(recipients, recipient.id);
            tracing::trace!(
                "Recipient {} marked as needing PNI signature: {}",
                recipient.id,
                val
            );
        }
    }

    #[tracing::instrument]
    pub fn compact_db(&self) -> usize {
        let mut db = self.db();
        match db.batch_execute("VACUUM;") {
            Ok(()) => {
                tracing::trace!("Database compacted");
                0
            }
            Err(e) => {
                tracing::error!("Compacting database failed");
                tracing::error!("VACUUM => {}", e);
                1
            }
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_recipients(&self) -> Vec<orm::Recipient> {
        schema::recipients::table.load(&mut *self.db()).expect("db")
    }

    #[tracing::instrument(
        skip(self, addr),
        fields(addr = addr.to_service_id())
    )]
    pub fn fetch_recipient(&self, addr: ServiceAddress) -> Option<orm::Recipient> {
        use crate::schema::recipients::dsl::*;

        let mut query = recipients.into_boxed();

        match addr.identity {
            ServiceIdType::AccountIdentity => {
                query = query.filter(uuid.eq(addr.uuid.to_string()));
            }
            ServiceIdType::PhoneNumberIdentity => {
                query = query.filter(pni.eq(addr.uuid.to_string()));
            }
        }

        query.first(&mut *self.db()).optional().expect("db")
    }

    #[allow(clippy::type_complexity)]
    fn fetch_separate_recipients(
        db: &mut SqliteConnection,
        aci: Option<&Uuid>,
        pni: Option<&Uuid>,
        e164: Option<&PhoneNumber>,
    ) -> Result<
        (
            Option<orm::Recipient>,
            Option<orm::Recipient>,
            Option<orm::Recipient>,
        ),
        diesel::result::Error,
    > {
        use crate::schema::recipients;
        let by_aci: Option<orm::Recipient> = aci
            .map(|u| {
                recipients::table
                    .filter(recipients::uuid.eq(u.to_string()))
                    .first(db)
                    .optional()
            })
            .transpose()?
            .flatten();
        let by_pni: Option<orm::Recipient> = pni
            .map(|u| {
                recipients::table
                    .filter(recipients::pni.eq(u.to_string()))
                    .first(db)
                    .optional()
            })
            .transpose()?
            .flatten();
        let by_e164: Option<orm::Recipient> = e164
            .as_ref()
            .map(|phonenumber| {
                recipients::table
                    .filter(recipients::e164.eq(phonenumber.to_string()))
                    .first(db)
                    .optional()
            })
            .transpose()?
            .flatten();

        Ok((by_aci, by_pni, by_e164))
    }

    #[tracing::instrument(skip(self))]
    pub fn set_recipient_unidentified(
        &self,
        recipient: &orm::Recipient,
        mode: UnidentifiedAccessMode,
    ) -> bool {
        use crate::schema::recipients::dsl::*;
        let affected = diesel::update(recipients)
            .set(unidentified_access_mode.eq(mode))
            .filter(id.eq(recipient.id))
            .execute(&mut *self.db())
            .expect("existing record updated");
        if affected > 0 {
            self.observe_update(recipients, recipient.id);
        }
        // If updating self, invalidate the cache
        if recipient.uuid == self.config.get_aci() {
            self.invalidate_self_recipient();
        }
        affected > 0
    }

    #[tracing::instrument(skip(self, recipient), fields(recipient_uuid = recipient.uuid.as_ref().map(Uuid::to_string)))]
    pub fn mark_profile_outdated(&self, recipient: &orm::Recipient) -> Option<orm::Recipient> {
        use crate::schema::recipients::dsl::*;
        if let Some(aci) = recipient.uuid {
            diesel::update(recipients)
                .set(last_profile_fetch.eq(Option::<NaiveDateTime>::None))
                .filter(uuid.eq(aci.to_string()))
                .execute(&mut *self.db())
                .expect("existing record updated");
            // If updating self, invalidate the cache
            if recipient.uuid == self.config.get_aci() {
                self.invalidate_self_recipient();
            }
            let recipient = self.fetch_recipient_by_service_address(&ServiceAddress::new_aci(aci));
            if let Some(recipient) = &recipient {
                self.observe_update(recipients, recipient.id);
            }
            recipient
        } else {
            tracing::error!(
                "Recipient without ACI; can't mark outdated: {:?}",
                recipient
            );
            None
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn update_profile_details(
        &self,
        profile_uuid: &Uuid,
        new_given_name: &Option<String>,
        new_family_name: &Option<String>,
        new_about: &Option<String>,
        new_emoji: &Option<String>,
    ) {
        let new_joined_name = match (new_given_name.clone(), new_family_name.clone()) {
            (Some(g), Some(f)) => Some(format!("{} {}", g, f)),
            (Some(g), None) => Some(g),
            (None, Some(f)) => Some(f),
            _ => None,
        };

        let recipient = self
            .fetch_recipient_by_service_address(&ServiceAddress::new_aci(*profile_uuid))
            .unwrap();
        use crate::schema::recipients::dsl::*;
        let affected_rows = diesel::update(recipients)
            .set((
                profile_family_name.eq(new_family_name),
                profile_given_name.eq(new_given_name),
                profile_joined_name.eq(new_joined_name),
                about.eq(new_about),
                about_emoji.eq(new_emoji),
            ))
            .filter(id.eq(recipient.id))
            .execute(&mut *self.db())
            .expect("existing record updated");
        // If updating self, invalidate the cache
        if recipient.uuid == self.config.get_aci() {
            self.invalidate_self_recipient();
        }
        if affected_rows > 0 {
            self.observe_update(recipients, recipient.id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn update_expiration_timer(&self, session_id: i32, timer: Option<u32>) {
        // Carry out the update only if the timer changes
        use crate::schema::sessions::dsl::*;
        let affected_rows = diesel::update(sessions)
            .set((expiring_message_timeout.eq(timer.map(|i| i as i32)),))
            .filter(id.eq(session_id))
            .execute(&mut *self.db())
            .expect("existing record updated");

        if affected_rows > 0 {
            self.observe_update(sessions, session_id);
        }
    }

    #[tracing::instrument(
        skip(self, rcpt_e164, new_profile_key),
        fields(
            rcpt_e164 = rcpt_e164
                .as_ref()
                .map(|p| p.to_string()).as_deref(),
        ))]

    pub fn update_profile_key(
        &self,
        rcpt_e164: Option<PhoneNumber>,
        addr: Option<ServiceAddress>,
        new_profile_key: &[u8],
        trust_level: TrustLevel,
    ) -> (orm::Recipient, bool) {
        let recipient =
            self.merge_and_fetch_recipient_by_address(rcpt_e164, addr.unwrap(), trust_level);

        if new_profile_key.len() != PROFILE_KEY_LEN {
            tracing::error!(
                "Profile key is not {} but {} bytes long",
                PROFILE_KEY_LEN,
                new_profile_key.len()
            );
            return (recipient, false);
        }

        if let Some(addr) = addr {
            if addr.identity != ServiceIdType::AccountIdentity {
                tracing::warn!("Ignoring profile key update for non-ACI {:?}", addr);
                return (recipient, false);
            }
        }

        let is_unset = recipient.profile_key.is_none()
            || recipient.profile_key.as_ref().map(Vec::len) == Some(0);

        if is_unset || trust_level == TrustLevel::Certain {
            if recipient.profile_key.as_deref() == Some(new_profile_key) {
                tracing::trace!("Profile key up-to-date");
                // Key remained the same, but we got an assertion on the profile key, so we will
                // retry sending unidentified.
                if recipient.unidentified_access_mode == UnidentifiedAccessMode::Disabled {
                    let _affected_rows = diesel::update(recipients)
                        .set((unidentified_access_mode.eq(UnidentifiedAccessMode::Unknown),))
                        .filter(id.eq(recipient.id))
                        .execute(&mut *self.db())
                        .expect("existing record updated");
                }
                // If updating self, invalidate the cache
                if recipient.uuid == self.config.get_aci() {
                    self.invalidate_self_recipient();
                }
                return (recipient, false);
            }

            use crate::schema::recipients::dsl::*;
            let affected_rows = diesel::update(recipients)
                .set((
                    profile_key.eq(new_profile_key),
                    unidentified_access_mode.eq(UnidentifiedAccessMode::Unknown),
                ))
                .filter(id.eq(recipient.id))
                .execute(&mut *self.db())
                .expect("existing record updated");
            tracing::info!("Updated profile key for {}", recipient.e164_or_address());

            // If updating self, invalidate the cache
            if recipient.uuid == self.config.get_aci() {
                self.invalidate_self_recipient();
            }

            if affected_rows > 0 {
                self.observe_update(recipients, recipient.id);
            }
        }
        // Re-fetch recipient with updated key
        (
            self.fetch_recipient_by_id(recipient.id)
                .expect("fetch existing record"),
            true,
        )
    }

    /// Save profile data to db and trigger GUI update.
    /// Assumes the avatar image has been saved/deleted in advance.
    #[tracing::instrument(skip(self))]
    pub fn save_profile(&self, profile: StoreProfile) {
        use crate::store::schema::recipients::dsl::*;
        use diesel::prelude::*;
        diesel::update(recipients)
            .set((
                profile_given_name.eq(profile.given_name),
                profile_family_name.eq(profile.family_name),
                profile_joined_name.eq(profile.joined_name),
                about.eq(profile.about_text),
                about_emoji.eq(profile.emoji),
                unidentified_access_mode.eq(profile.unidentified),
                signal_profile_avatar.eq(profile.avatar),
                last_profile_fetch.eq(profile.last_fetch),
            ))
            .filter(uuid.nullable().eq(&profile.r_uuid.to_string()))
            .execute(&mut *self.db())
            .expect("db");

        // If updating self, invalidate the cache
        if Some(profile.r_uuid) == self.config.get_aci() {
            self.invalidate_self_recipient();
        }

        tracing::trace!("Profile saved to database");

        self.observe_update(schema::recipients::table, profile.r_id);
    }

    /// Helper for guaranteed ACI or PNI cases, with or without E.164.
    /// XXX: This does *not* trigger observations for removed recipients.
    pub fn merge_and_fetch_recipient_by_address(
        &self,
        e164: Option<PhoneNumber>,
        addr: ServiceAddress,
        trust_level: TrustLevel,
    ) -> orm::Recipient {
        match addr.identity {
            ServiceIdType::AccountIdentity => {
                self.merge_and_fetch_recipient(e164, Some(addr), None, trust_level)
            }
            ServiceIdType::PhoneNumberIdentity => {
                self.merge_and_fetch_recipient(e164, None, Some(addr), trust_level)
            }
        }
    }

    /// Equivalent of Androids `RecipientDatabase::getAndPossiblyMerge` with `change_self` set to `true.
    /// Assumes ACI, PNI and E164 to be self-recipient as well.
    pub fn merge_and_fetch_self_recipient(
        &self,
        e164: Option<PhoneNumber>,
        aci: Option<ServiceAddress>,
        pni: Option<ServiceAddress>,
    ) -> orm::Recipient {
        let (id, aci_uuid, pni_uuid, phonenumber, changed) = self
            .db()
            .transaction::<_, Error, _>(|db| {
                Self::merge_and_fetch_recipient_inner(
                    db,
                    e164,
                    aci.map(|u| u.uuid),
                    pni.map(|u| u.uuid),
                    TrustLevel::Certain,
                    true,
                )
            })
            .expect("database");
        let recipient = match (id, aci_uuid, pni_uuid, phonenumber) {
            (Some(id), _, _, _) => self
                .fetch_recipient_by_id(id)
                .expect("existing updated recipient"),
            (_, Some(_), _, _) => self
                .fetch_recipient_by_service_address(&aci.unwrap())
                .expect("existing updated recipient by aci"),
            (_, _, Some(_), _) => self
                .fetch_recipient_by_service_address(&pni.unwrap())
                .expect("existing updated recipient by pni"),
            (_, _, _, Some(e164)) => self
                .fetch_recipient_by_phonenumber(&e164)
                .expect("existing updated recipient"),
            (None, None, None, None) => {
                unreachable!("this should get implemented with an Either or custom enum instead")
            }
        };
        if changed {
            self.observe_update(crate::schema::recipients::table, recipient.id);
        }

        tracing::trace!("Fetched recipient: {}", recipient);

        recipient
    }

    /// Equivalent of Androids `RecipientDatabase::getAndPossiblyMerge`.
    /// Always sets `change_self` to `false`.
    ///
    /// XXX: This does *not* trigger observations for removed recipients.
    pub fn merge_and_fetch_recipient(
        &self,
        e164: Option<PhoneNumber>,
        aci: Option<ServiceAddress>,
        pni: Option<ServiceAddress>,
        trust_level: TrustLevel,
    ) -> orm::Recipient {
        let (id, aci_uuid, pni_uuid, phonenumber, changed) = self
            .db()
            .transaction::<_, Error, _>(|db| {
                Self::merge_and_fetch_recipient_inner(
                    db,
                    e164,
                    aci.map(|u| u.uuid),
                    pni.map(|u| u.uuid),
                    trust_level,
                    false,
                )
            })
            .expect("database");
        let recipient = match (id, aci_uuid, pni_uuid, &phonenumber) {
            (Some(id), _, _, _) => self
                .fetch_recipient_by_id(id)
                .expect("existing updated recipient by id"),
            (_, Some(_), _, _) => self
                .fetch_recipient_by_service_address(&aci.unwrap())
                .expect("existing updated recipient by aci"),
            (_, _, Some(_), _) => self
                .fetch_recipient_by_service_address(&pni.unwrap())
                .expect("existing updated recipient by pni"),
            (_, _, _, Some(e164)) => self
                .fetch_recipient_by_phonenumber(e164)
                .expect("existing updated recipient by e164"),
            (None, None, None, None) => {
                unreachable!("this should get implemented with an Either or custom enum instead")
            }
        };
        if changed {
            self.observe_update(crate::schema::recipients::table, recipient.id);
        }

        tracing::trace!("Fetched recipient: {}", recipient);

        recipient
    }

    // Inner method because the coverage report is then sensible.
    #[allow(clippy::type_complexity)]
    #[tracing::instrument(
        skip(db, e164),
        fields(
            aci = aci.as_ref().map_or("None".into(), |u| u.to_string()),
            pni = pni.as_ref().map_or("None".into(), |u| u.to_string()),
            e164 = e164.as_ref().map_or("None".into(), |u| u.to_string()),
        ))]
    // XXX this should get implemented with an Either or custom enum instead
    fn merge_and_fetch_recipient_inner(
        db: &mut SqliteConnection,
        e164: Option<PhoneNumber>,
        aci: Option<Uuid>,
        pni: Option<Uuid>,
        trust_level: TrustLevel,
        change_self: bool,
    ) -> Result<
        (
            Option<i32>,
            Option<Uuid>, // ACI
            Option<Uuid>, // PNI
            Option<PhoneNumber>,
            bool,
        ),
        Error,
    > {
        if e164.is_none() && aci.is_none() && pni.is_none() {
            panic!("merge_and_fetch_recipient requires at least one of e164 or uuid");
        }

        let criteria_count = [e164.is_some(), aci.is_some(), pni.is_some()]
            .iter()
            .filter(|c| **c)
            .count();
        let (by_aci, by_pni, by_e164) =
            Self::fetch_separate_recipients(db, aci.as_ref(), pni.as_ref(), e164.as_ref())?;

        // Get the common recipient, if one exists
        let mut by_all: Vec<&orm::Recipient> = [&by_aci, &by_pni, &by_e164]
            .iter()
            .filter_map(|r| r.as_ref())
            .collect();
        let match_count = by_all.len();
        by_all.sort_by(|a, b| b.id.cmp(&a.id));
        by_all.dedup_by(|a, b| a.id == b.id);
        let common = if by_all.len() == 1 {
            Some(by_all[0])
        } else {
            None
        };

        // Things can get quite cumbersome later on, so let's just use the operations queue for everything.
        let mut ops: Vec<RecipientOperation> = Vec::new();

        // If nothing matches, create a new recipient
        if by_all.is_empty() {
            tracing::debug!("No matches at all - creating a new recipient");
            let insert_e164 = (trust_level == TrustLevel::Certain) || aci.is_none();
            let new_e164 = if insert_e164 { e164.clone() } else { None };

            ops.push(RecipientOperation::Create(aci, pni, new_e164.clone()));
        }

        // This block mimics processNonMergePnpUpdate()
        if let Some(common) = common {
            // If there's a common recipient, and every criteria given matches, we're done!
            if match_count == criteria_count {
                return Ok((Some(common.id), None, None, None, false));
            }
            tracing::debug!(
                "Found incomplete ({}/{}) common recipient {}",
                match_count,
                criteria_count,
                common.id
            );

            // This is a special case. The ACI passed in doesn't match the common record. We can't change ACIs, so we need to make a new record.
            if aci.is_some() && common.uuid.is_some() && aci != common.uuid {
                tracing::warn!("ACI mismatch -- creating a new recipient");
                if e164.is_some() && e164 == common.e164
                // XXX && (change_self || not_self)
                {
                    tracing::debug!("Removing E164 and PNI from existing recipient");
                    ops.push(RecipientOperation::SetE164(common.id, None));
                    ops.push(RecipientOperation::SetPni(common.id, None));
                } else if pni.is_some() && pni == common.pni {
                    tracing::debug!("Removing PNI from existing recipient");
                    ops.push(RecipientOperation::SetPni(common.id, None));
                }

                // XXX (change_self || not_self)

                tracing::debug!(
                    "Creating new recipient with E164({}) and PNI({})",
                    e164.is_some(),
                    pni.is_some()
                );
                ops.push(RecipientOperation::Create(None, pni, e164.clone()));
            } else {
                if e164.is_some() && e164 != common.e164 && trust_level == TrustLevel::Certain {
                    tracing::debug!("Updating E164 in existing recipient");
                    ops.push(RecipientOperation::SetE164(common.id, e164.clone()));
                    if common.e164.is_some() {
                        tracing::warn!(
                            "TODO: Phone number change from {:?} to {:?}",
                            common.e164.as_ref().unwrap().to_string(),
                            e164
                        );
                    }
                }

                if pni.is_some() && pni != common.pni {
                    tracing::debug!("Updating PNI in existing recipient");
                    ops.push(RecipientOperation::SetPni(common.id, pni));
                }

                if aci.is_some() && common.uuid.is_none() {
                    tracing::debug!("Setting UUID in existing recipient");
                    ops.push(RecipientOperation::SetAci(common.id, aci));
                }

                let old_service_id = common.uuid.or(common.pni);
                let new_service_id = aci.or(pni.or(old_service_id));

                if old_service_id.is_some()
                    && old_service_id != new_service_id
                    && trust_level == TrustLevel::Certain
                /* && has_session(old_service_id) */
                {
                    tracing::warn!(
                        "TODO: Session switchover event change from {:?} to {:?}",
                        old_service_id,
                        new_service_id
                    );
                }
            }
        }

        // At this point, there is two or more different search results. We need to merge something.
        // Since we may have to do a plenty of things, we need an operation queue to handle it.

        // Merge PNI into E.164
        // This block resembles processPossibleE164PniMerge()
        // XXX && (change_self || not_self)
        if e164.is_some()
            && pni.is_some()
            && by_e164.is_some()
            && by_pni.is_some()
            && by_e164.as_ref().unwrap().id != by_pni.as_ref().unwrap().id
        {
            let by_pni = by_pni.as_ref().unwrap();
            let by_e164 = by_e164.as_ref().unwrap();
            tracing::info!(
                "Merging contact {} (by E.164) into {} (by PNI)",
                by_pni.id,
                by_e164.id
            );

            if by_pni.uuid.is_none() && by_pni.e164.is_none() {
                ops.push(RecipientOperation::SetPni(by_e164.id, None));
                ops.push(RecipientOperation::Merge(by_pni.id, by_e164.id));
            } else {
                ops.push(RecipientOperation::SetPni(by_pni.id, None));
                ops.push(RecipientOperation::SetPni(by_e164.id, pni));
                // XXX session switchover event?
            }
        }

        // Merge PNI into ACI
        // NB! We must never merge/move ACI!
        // This block resembles processPossiblePniAciMerge()
        // XXX && (change_self || not_self)
        if aci.is_some()
            && pni.is_some()
            && by_aci.is_some()
            && by_pni.is_some()
            && by_aci.as_ref().unwrap().id != by_pni.as_ref().unwrap().id
        {
            let by_pni = by_pni.as_ref().unwrap();
            let by_aci = by_aci.as_ref().unwrap();
            tracing::info!(
                "Merging contact {} (by PNI) into {} (by ACI)",
                by_pni.id,
                by_aci.id
            );

            if by_pni.uuid.is_none() && by_pni.e164.is_none() {
                if by_aci.pni.is_some() {
                    ops.push(RecipientOperation::SetPni(by_aci.id, None));
                }
                ops.push(RecipientOperation::Merge(by_pni.id, by_aci.id));
            } else if by_pni.uuid.is_none() && (e164.is_none() || by_pni.e164 == e164) {
                ops.push(RecipientOperation::SetPni(by_aci.id, None));
                let new_e164 = by_pni.e164.as_ref().or(e164.as_ref()).cloned();
                if new_e164.is_some() && by_aci.e164.is_none() && by_aci.e164 != new_e164 {
                    ops.push(RecipientOperation::SetE164(by_aci.id, None));
                }
                ops.push(RecipientOperation::Merge(by_pni.id, by_aci.id));
            } else {
                ops.push(RecipientOperation::SetPni(by_pni.id, None));
                ops.push(RecipientOperation::SetPni(by_aci.id, pni));
                if e164.is_some() && by_aci.e164 != e164 {
                    if by_pni.e164 == e164 {
                        ops.push(RecipientOperation::SetE164(by_pni.id, None));
                    }
                    if e164.is_some() && by_aci.e164 != e164 {
                        // XXX Phone number change event
                    }
                    ops.push(RecipientOperation::SetE164(by_aci.id, e164.clone()));
                }
            }
        }

        // Merge E.164 into ACI
        // XXX && (change_self || not_self)
        // This block resembles processPossibleE164AciMerge()
        if e164.is_some()
            && aci.is_some()
            && by_e164.is_some()
            && by_aci.is_some()
            && by_e164.as_ref().unwrap().id != by_aci.as_ref().unwrap().id
        {
            let by_e164 = by_e164.as_ref().unwrap();
            let by_aci = by_aci.as_ref().unwrap();
            let e164 = e164.as_ref().unwrap();
            tracing::info!(
                "Merging contact {} (by E.164) into {} (by ACI)",
                by_e164.id,
                by_aci.id
            );

            if by_e164.uuid.is_none() && by_e164.pni.is_none() {
                if by_aci.e164.is_some() {
                    ops.push(RecipientOperation::SetE164(by_aci.id, None));
                }
                ops.push(RecipientOperation::SetE164(by_e164.id, None));
                ops.push(RecipientOperation::SetE164(by_aci.id, Some(e164.clone()))); // XXX This should be handled in merge func
                ops.push(RecipientOperation::Merge(by_e164.id, by_aci.id));
                if by_aci.e164.is_some() && by_aci.e164.as_ref().unwrap() != e164 { // XXX && (change_self || not_self) && !by_aci.blocked
                     // XXX Phone number change event
                }
            } else if pni.is_some() && by_e164.pni != pni {
                if by_aci.pni.is_some() {
                    ops.push(RecipientOperation::SetPni(by_aci.id, None));
                }
                if by_aci.e164.as_ref().unwrap() != e164 {
                    ops.push(RecipientOperation::SetE164(by_aci.id, None));
                }
                ops.push(RecipientOperation::Merge(by_e164.id, by_aci.id));
                // - if byAci.e164 changed, not self, not blocked
                if by_aci.e164.is_some() && by_aci.e164.as_ref().unwrap() != e164 { // XXX && (change_self || not_self) && !by_aci.blocked
                     // XXX Phone number change event
                }
            } else if pni.is_some() && by_e164.pni != pni || trust_level == TrustLevel::Certain {
                ops.push(RecipientOperation::SetE164(by_e164.id, None));
                ops.push(RecipientOperation::SetE164(by_aci.id, Some(e164.clone()))); // XXX This should be handled in merge func
                if by_aci.e164.is_some() && by_aci.e164.as_ref().unwrap() != e164 { // XXX && (change_self || not_self) && !by_aci.blocked
                     // XXX Phone number change event
                }
            }
        }

        if !ops.is_empty() {
            tracing::trace!("Queue: {:?}", ops);
        }

        for op in ops.into_iter() {
            #[rustfmt::skip]
            match op {
                RecipientOperation::Merge(id, into_id) => Self::merge_recipients_inner(db, id, into_id),
                RecipientOperation::SetPni(id, pni) => Self::set_pni_inner(db, id, pni.as_ref()),
                RecipientOperation::SetAci(id, aci) => Self::set_aci_inner(db, id, aci.as_ref()),
                RecipientOperation::SetE164(id, e164) => Self::set_e164_inner(db, id, e164.as_ref()),
                RecipientOperation::Create(aci, pni, e164) => Self::insert_recipient_inner(db, aci.as_ref(), pni.as_ref(), e164.as_ref()),
            }?;
        }

        // Fetch new results after migration
        let (new_by_aci, new_by_pni, new_by_e164) =
            Self::fetch_separate_recipients(db, aci.as_ref(), pni.as_ref(), e164.as_ref())?;

        // NB! The order matters here! ACI > E.164 > PNI
        let by_all: Vec<&orm::Recipient> = [&new_by_aci, &new_by_e164, &new_by_pni]
            .iter()
            .filter_map(|r| r.as_ref())
            .collect();

        if by_all.is_empty() {
            tracing::error!("Recipient merge should have resulted in at least one match!");
            tracing::error!(
                "Searched with: ACI:{:?}, PNI:{:?}, E164:{:?}",
                aci.as_ref().map_or("None".into(), Uuid::to_string),
                pni.as_ref().map_or("None".into(), Uuid::to_string),
                e164.as_ref().map_or("None".into(), PhoneNumber::to_string)
            );
            return Ok((None, None, None, None, false));
        }

        // This is why the order matters - we return the first match.
        let rcpt = by_all[0];

        if aci.is_some() && new_by_aci.is_none() {
            // XXX && (change_self || not_self)
            Self::set_aci_inner(db, rcpt.id, aci.as_ref())?;
            // XXX session switchover event?
        }

        if e164.is_some() && new_by_e164.is_none() {
            // XXX && (change_self || not_self)
            Self::set_e164_inner(db, rcpt.id, e164.as_ref())?;
        }

        if pni.is_some() && new_by_pni.is_none() {
            Self::set_pni_inner(db, rcpt.id, pni.as_ref())?;
        }

        if new_by_pni.is_some() && pni != new_by_pni.as_ref().unwrap().pni {
            // XXX session switchover event
        }

        if new_by_aci.is_some() && aci != new_by_aci.as_ref().unwrap().uuid {
            // XXX session switchover event
        }

        return Ok((Some(rcpt.id), None, None, None, true));
    }

    /// Merge source_id into dest_id.
    ///
    /// Executes `merge_recipient_inner` inside a transaction, and then returns the result.
    #[allow(unused)]
    #[tracing::instrument(skip(self))]
    fn merge_recipients(&self, source_id: i32, dest_id: i32) -> orm::Recipient {
        let mut db = self.db();
        let merged_id = db
            .transaction::<_, Error, _>(|db| Self::merge_recipients_inner(db, source_id, dest_id))
            .expect("consistent migration");

        tracing::trace!("Contact merge committed.");

        self.observe_delete(schema::recipients::table, source_id);
        self.observe_update(schema::recipients::table, dest_id);

        self.fetch_recipient_by_id(merged_id)
            .expect("existing contact")
    }

    // Inner method because the coverage report is then sensible.
    #[tracing::instrument(skip(db))]
    fn merge_recipients_inner(
        db: &mut SqliteConnection,
        source_id: i32,
        dest_id: i32,
    ) -> Result<i32, diesel::result::Error> {
        tracing::info!(
            "Merge of contacts {} and {}. Will move all into {}",
            source_id,
            dest_id,
            dest_id
        );

        // Defer constraints, we're moving a lot of data, inside of a transaction,
        // and if we have a bug it definitely needs more research anyway.
        db.batch_execute("PRAGMA defer_foreign_keys = ON;")?;

        use schema::*;

        // 1. Merge messages senders.
        let message_count = diesel::update(messages::table)
            .filter(messages::sender_recipient_id.eq(source_id))
            .set(messages::sender_recipient_id.eq(dest_id))
            .execute(db)?;
        tracing::trace!("Merging messages: {}", message_count);

        // 2. Merge group V1 membership:
        //    - Delete duplicate memberships.
        //      We fetch the dest_id group memberships,
        //      and delete the source_id memberships that have the same group.
        //      Ideally, this would be a single self-join query,
        //      but Diesel doesn't like that yet.
        let target_memberships_v1: Vec<String> = group_v1_members::table
            .select(group_v1_members::group_v1_id)
            .filter(group_v1_members::recipient_id.eq(dest_id))
            .load(db)?;
        let deleted_memberships_v1 = diesel::delete(group_v1_members::table)
            .filter(
                group_v1_members::group_v1_id
                    .eq_any(&target_memberships_v1)
                    .and(group_v1_members::recipient_id.eq(source_id)),
            )
            .execute(db)?;
        //    - Update the rest
        let updated_memberships_v1 = diesel::update(group_v1_members::table)
            .filter(group_v1_members::recipient_id.eq(source_id))
            .set(group_v1_members::recipient_id.eq(dest_id))
            .execute(db)?;
        tracing::trace!(
            "Merging Group V1 memberships: deleted duplicate {}/{}, moved {}/{}.",
            deleted_memberships_v1,
            target_memberships_v1.len(),
            updated_memberships_v1,
            target_memberships_v1.len()
        );

        // 3. Merge sessions:
        let source_session: Option<orm::DbSession> = sessions::table
            .filter(sessions::direct_message_recipient_id.eq(source_id))
            .first(db)
            .optional()?;
        let target_session: Option<orm::DbSession> = sessions::table
            .filter(sessions::direct_message_recipient_id.eq(dest_id))
            .first(db)
            .optional()?;
        match (source_session, target_session) {
            (Some(source_session), Some(target_session)) => {
                // Both recipients have a session.
                // Move the source session's messages to the target session,
                // then drop the source session.
                let updated_message_count = diesel::update(messages::table)
                    .filter(messages::session_id.eq(source_session.id))
                    .set(messages::session_id.eq(target_session.id))
                    .execute(db)?;
                let dropped_session_count = diesel::delete(sessions::table)
                    .filter(sessions::id.eq(source_session.id))
                    .execute(db)?;

                assert_eq!(dropped_session_count, 1, "Drop the single source session.");

                tracing::trace!(
                    "Updating source session's messages ({} total). Dropped source session.",
                    updated_message_count
                );
            }
            (Some(source_session), None) => {
                tracing::info!("Strange, no session for the target_id. Updating source.");
                let updated_session = diesel::update(sessions::table)
                    .filter(sessions::id.eq(source_session.id))
                    .set(sessions::direct_message_recipient_id.eq(dest_id))
                    .execute(db)?;
                assert_eq!(updated_session, 1, "Update source session");
            }
            (None, Some(_target_session)) => {
                tracing::info!("Strange, no session for the source_id. Continuing.");
            }
            (None, None) => {
                tracing::warn!("Strange, neither recipient has a session. Continuing.");
            }
        }

        // 4. Merge reactions
        //    This too would benefit from a subquery or self-join.
        let target_reactions: Vec<i32> = reactions::table
            .select(reactions::reaction_id)
            .filter(reactions::author.eq(dest_id))
            .load(db)?;
        // Delete duplicates from source.
        // We're not going to merge based on receive time,
        // although that would be the "right" thing to do.
        // Let's hope we never really take this path.
        let deleted_reactions = diesel::delete(reactions::table)
            .filter(
                reactions::author
                    .eq(source_id)
                    .and(reactions::message_id.eq_any(target_reactions)),
            )
            .execute(db)?;
        if deleted_reactions > 0 {
            tracing::warn!(
                "Deleted {} reactions; please file an issue!",
                deleted_reactions
            );
        } else {
            tracing::trace!("Deleted {} reactions", deleted_reactions);
        };
        let updated_reactions = diesel::update(reactions::table)
            .filter(reactions::author.eq(source_id))
            .set(reactions::author.eq(dest_id))
            .execute(db)?;
        tracing::trace!("Updated {} reactions", updated_reactions);

        // 5. Update receipts
        //    Same thing: delete the duplicates (although merging would be better),
        //    and update the rest.
        let target_receipts: Vec<i32> = receipts::table
            .select(receipts::message_id)
            .filter(receipts::recipient_id.eq(dest_id))
            .load(db)?;
        let deleted_receipts = diesel::delete(receipts::table)
            .filter(
                receipts::recipient_id
                    .eq(source_id)
                    .and(receipts::message_id.eq_any(target_receipts)),
            )
            .execute(db)?;
        if deleted_receipts > 0 {
            tracing::warn!(
                "Deleted {} receipts; please file an issue!",
                deleted_receipts
            );
        } else {
            tracing::trace!("Deleted {} receipts.", deleted_receipts);
        }
        let updated_receipts = diesel::update(receipts::table)
            .filter(receipts::recipient_id.eq(source_id))
            .set(receipts::recipient_id.eq(dest_id))
            .execute(db)?;
        tracing::trace!("Updated {} receipts", updated_receipts);

        // 6. Delete the source recipient iff it's empty.
        let src: orm::Recipient = recipients::table
            .filter(recipients::id.eq(source_id))
            .first(db)?;
        if src.uuid.is_none() && src.e164.is_none() && src.pni.is_none() {
            let deleted = diesel::delete(recipients::table)
                .filter(recipients::id.eq(source_id))
                .execute(db)?;
            tracing::trace!("Deleted {} recipient", deleted);
            assert_eq!(deleted, 1, "delete only one recipient");
        } else {
            tracing::trace!("Source recipient is non-empty, not removing.");
        }
        Ok(dest_id)
    }

    fn set_pni_inner(
        db: &mut SqliteConnection,
        rcpt_id: i32,
        rcpt_pni: Option<&Uuid>,
    ) -> Result<i32, diesel::result::Error> {
        tracing::debug!(
            "Setting PNI of {} to {:?}",
            rcpt_id,
            rcpt_pni.map(Uuid::to_string)
        );
        use crate::schema::recipients;
        match diesel::update(recipients::table)
            .filter(recipients::id.eq(rcpt_id))
            .set(recipients::pni.eq(rcpt_pni.map(Uuid::to_string)))
            .execute(db)
        {
            Ok(_) => Ok(rcpt_id),
            Err(e) => Err(e),
        }
    }

    fn set_aci_inner(
        db: &mut SqliteConnection,
        rcpt_id: i32,
        rcpt_aci: Option<&Uuid>,
    ) -> Result<i32, diesel::result::Error> {
        tracing::debug!(
            "Setting ACI of {} to {:?}",
            rcpt_id,
            rcpt_aci.map(Uuid::to_string)
        );
        use crate::schema::recipients;
        match diesel::update(recipients::table)
            .filter(recipients::id.eq(rcpt_id))
            .set(recipients::uuid.eq(rcpt_aci.map(Uuid::to_string)))
            .execute(db)
        {
            Ok(_) => Ok(rcpt_id),
            Err(e) => Err(e),
        }
    }

    fn set_e164_inner(
        db: &mut SqliteConnection,
        rcpt_id: i32,
        rcpt_e164: Option<&PhoneNumber>,
    ) -> Result<i32, diesel::result::Error> {
        tracing::debug!(
            "Setting E.164 of {} to {:?}",
            rcpt_id,
            rcpt_e164.map(PhoneNumber::to_string)
        );
        use crate::schema::recipients;
        match diesel::update(recipients::table)
            .filter(recipients::id.eq(rcpt_id))
            .set(recipients::e164.eq(rcpt_e164.map(PhoneNumber::to_string)))
            .execute(db)
        {
            Ok(_) => Ok(rcpt_id),
            Err(e) => Err(e),
        }
    }

    fn insert_recipient_inner(
        db: &mut SqliteConnection,
        rcpt_aci: Option<&Uuid>,
        rcpt_pni: Option<&Uuid>,
        rcpt_e164: Option<&PhoneNumber>,
    ) -> Result<i32, diesel::result::Error> {
        tracing::debug!(
            "Inserting new recipient with ACI:{:?}, PNI:{:?}, E164:{:?}",
            rcpt_aci.map(Uuid::to_string),
            rcpt_pni.map(Uuid::to_string),
            rcpt_e164.map(PhoneNumber::to_string)
        );
        use crate::schema::recipients;
        match diesel::insert_into(recipients::table)
            .values((
                recipients::uuid.eq(rcpt_aci.map(Uuid::to_string)),
                recipients::pni.eq(rcpt_pni.map(Uuid::to_string)),
                recipients::e164.eq(rcpt_e164.map(PhoneNumber::to_string)),
            ))
            .returning(recipients::id)
            .get_result(db)
        {
            Ok(id) => Ok(id),
            Err(e) => Err(e),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_recipient_by_e164(&self, rcpt_e164: &PhoneNumber) -> Option<orm::Recipient> {
        use crate::schema::recipients::dsl::*;

        if let Ok(recipient) = recipients
            .filter(e164.eq(rcpt_e164.to_string()))
            .first(&mut *self.db())
        {
            Some(recipient)
        } else {
            None
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_recipient_by_service_address(
        &self,
        srv_addr: &ServiceAddress,
    ) -> Option<orm::Recipient> {
        use crate::schema::recipients::dsl::*;

        match srv_addr.identity {
            ServiceIdType::AccountIdentity => recipients
                .filter(uuid.eq(&srv_addr.uuid.to_string()))
                .first(&mut *self.db())
                .ok(),
            ServiceIdType::PhoneNumberIdentity => recipients
                .filter(pni.eq(&srv_addr.uuid.to_string()))
                .first(&mut *self.db())
                .ok(),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_or_insert_recipient_by_address(&self, addr: &ServiceAddress) -> orm::Recipient {
        use crate::schema::recipients::dsl::*;

        let mut db = self.db();
        let db = &mut *db;

        let recipient: orm::Recipient = match addr.identity {
            ServiceIdType::AccountIdentity => {
                if let Ok(existing) = recipients.filter(uuid.eq(&addr.uuid.to_string())).first(db) {
                    existing
                } else {
                    let new_rcpt: orm::Recipient = diesel::insert_into(recipients)
                        .values(uuid.eq(&addr.uuid.to_string()))
                        .get_result(db)
                        .expect("insert new recipient");
                    self.observe_insert(recipients, new_rcpt.id);
                    new_rcpt
                }
            }
            ServiceIdType::PhoneNumberIdentity => {
                if let Ok(existing) = recipients.filter(pni.eq(&addr.uuid.to_string())).first(db) {
                    existing
                } else {
                    let new_rcpt: orm::Recipient = diesel::insert_into(recipients)
                        .values(pni.eq(&addr.uuid.to_string()))
                        .get_result(db)
                        .expect("insert new recipient");
                    self.observe_insert(recipients, new_rcpt.id);
                    new_rcpt
                }
            }
        };
        recipient
    }

    #[tracing::instrument(skip(self, rcpt_e164), fields(rcpt_e164 = %rcpt_e164))]
    pub fn fetch_or_insert_recipient_by_phonenumber(
        &self,
        rcpt_e164: &PhoneNumber,
    ) -> orm::Recipient {
        use crate::schema::recipients::dsl::*;

        let mut db = self.db();
        let db = &mut *db;
        if let Ok(recipient) = recipients.filter(e164.eq(rcpt_e164.to_string())).first(db) {
            recipient
        } else {
            let recipient: orm::Recipient = diesel::insert_into(recipients)
                .values(e164.eq(rcpt_e164.to_string()))
                .get_result(db)
                .expect("insert new recipient");
            self.observe_insert(recipients, recipient.id);
            recipient
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_last_message_by_session_id_augmented(
        &self,
        session_id: i32,
    ) -> Option<orm::AugmentedMessage> {
        let msg = self.fetch_last_message_by_session_id(session_id)?;
        self.fetch_augmented_message(msg.id)
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_last_message_by_session_id(&self, session_id: i32) -> Option<orm::Message> {
        use schema::messages;
        messages::table
            .filter(messages::session_id.eq(session_id))
            .order_by(messages::server_timestamp.desc())
            .first(&mut *self.db())
            .ok()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_message_receipts(&self, message_id: i32) -> Vec<(orm::Receipt, orm::Recipient)> {
        use schema::{receipts, recipients};

        receipts::table
            .inner_join(recipients::table)
            .filter(receipts::message_id.eq(message_id))
            .load(&mut *self.db())
            .expect("db")
    }

    /// Marks the message with a certain timestamp as read by a certain person.
    ///
    /// This is e.g. called from Signal Desktop from a sync message
    #[tracing::instrument(skip(self))]
    pub fn mark_message_read(&self, timestamp: NaiveDateTime) -> Option<MessagePointer> {
        use schema::messages::dsl::*;
        let mut row: Vec<(i32, i32)> = diesel::update(messages)
            .filter(server_timestamp.eq(timestamp))
            .set(is_read.eq(true))
            .returning((schema::messages::id, schema::messages::session_id))
            .load(&mut *self.db())
            .unwrap();

        if row.is_empty() {
            tracing::warn!("Could not find message with timestamp {}", timestamp);
            tracing::warn!(
                "This probably indicates out-of-order receipt delivery. Please upvote issue #260"
            );
            return None;
        }

        let pointer = row.pop()?;
        let pointer = MessagePointer {
            message_id: pointer.0,
            session_id: pointer.1,
        };

        self.observe_update(messages, pointer.message_id)
            .with_relation(schema::sessions::table, pointer.session_id);
        Some(pointer)
    }

    /// Handle marking multiple messages as read and potentially starting their expiry timer.
    #[tracing::instrument(skip(self))]
    pub fn mark_messages_read_in_ui(&self, msg_ids: Vec<i32>) {
        use schema::messages::dsl::*;

        // 1) Mark messages as read, if necessary
        let messages_unread: Vec<(i32, i32)> = diesel::update(messages)
            .filter(id.eq_any(&msg_ids))
            .set(is_read.eq(true))
            .returning((schema::messages::id, schema::messages::session_id))
            .load(&mut *self.db())
            .unwrap();

        // 2) Start expiry timer, if necessary
        let messages_expiring: Vec<(i32, i32)> = diesel::update(messages)
            .filter(
                id.eq_any(msg_ids)
                    .and(schema::messages::expires_in.is_not_null())
                    .and(schema::messages::expires_in.gt(0))
                    .and(schema::messages::expiry_started.is_null()),
            )
            .set(schema::messages::expiry_started.eq(Some(chrono::Utc::now().naive_utc())))
            .returning((schema::messages::id, schema::messages::session_id))
            .load(&mut *self.db())
            .expect("set message expiry");

        // Combine the two vectors
        if messages_unread.is_empty() && messages_expiring.is_empty() {
            return;
        }

        let mut messages_changed: Vec<(i32, i32)> = messages_unread
            .into_iter()
            .chain(messages_expiring.into_iter())
            .collect();
        messages_changed.sort();
        messages_changed.dedup();

        // 3) Observe update, if either happened
        for (m_id, s_id) in messages_changed {
            self.observe_update(messages, m_id)
                .with_relation(schema::sessions::table, PrimaryKey::RowId(s_id));
        }
    }

    /// Marks the message with a certain timestamp as received by a certain person.
    #[tracing::instrument(skip(self))]
    pub fn mark_message_received(
        &self,
        receiver_addr: ServiceAddress,
        timestamp: NaiveDateTime,
        delivered_at: Option<chrono::DateTime<Utc>>,
    ) -> Option<MessagePointer> {
        // XXX: probably, the trigger for this method call knows a better time stamp.
        let delivered_at = delivered_at.unwrap_or_else(chrono::Utc::now).naive_utc();

        // Find the recipient
        let recipient =
            self.merge_and_fetch_recipient_by_address(None, receiver_addr, TrustLevel::Certain);
        let row: Option<(i32, i32)> = schema::messages::table
            .select((schema::messages::id, schema::messages::session_id))
            .filter(schema::messages::server_timestamp.eq(timestamp))
            .first(&mut *self.db())
            .optional()
            .expect("db");
        if row.is_none() {
            tracing::warn!("Could not find message with timestamp {}", timestamp);
            tracing::warn!(
                "This probably indicates out-of-order receipt delivery. Please upvote issue #260"
            );
        }
        let pointer = row?;
        let pointer = MessagePointer {
            message_id: pointer.0,
            session_id: pointer.1,
        };

        let upsert = diesel::insert_into(schema::receipts::table)
            .values((
                schema::receipts::message_id.eq(pointer.message_id),
                schema::receipts::recipient_id.eq(recipient.id),
                schema::receipts::delivered.eq(delivered_at),
            ))
            .on_conflict((schema::receipts::message_id, schema::receipts::recipient_id))
            .do_update()
            .set(schema::receipts::delivered.eq(delivered_at))
            .execute(&mut *self.db());

        match upsert {
            Ok(n) => {
                if n != 1 {
                    tracing::warn!(
                        "Read receipt had {} affected rows instead of expected 1. Updating anyway.",
                        n
                    );
                }
                self.observe_upsert(schema::receipts::table, PrimaryKey::Unknown)
                    .with_relation(schema::messages::table, pointer.message_id)
                    .with_relation(schema::recipients::table, recipient.id);
                Some(pointer)
            }
            Err(e) => {
                tracing::error!("Could not insert receipt: {}.", e);
                None
            }
        }
    }

    /// Get all sessions in no particular order.
    ///
    /// Getting them ordered by timestamp would be nice,
    /// but that requires table aliases or complex subqueries,
    /// which are not really a thing in Diesel atm.
    #[tracing::instrument(skip(self))]
    pub fn fetch_sessions(&self) -> Vec<orm::Session> {
        fetch_sessions!(self.db(), |query| { query })
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_group_sessions(&self) -> Vec<orm::Session> {
        fetch_sessions!(self.db(), |query| {
            query.filter(schema::sessions::group_v1_id.is_not_null())
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_session_by_id(&self, sid: i32) -> Option<orm::Session> {
        fetch_session!(self.db(), |query| {
            query.filter(schema::sessions::columns::id.eq(sid))
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_session_by_id_augmented(&self, sid: i32) -> Option<orm::AugmentedSession> {
        let session = self.fetch_session_by_id(sid)?;
        let last_message = self.fetch_last_message_by_session_id_augmented(session.id);

        Some(orm::AugmentedSession {
            inner: session,
            last_message,
        })
    }

    #[tracing::instrument(skip(self, rcpt_e164), fields(rcpt_e164 = %rcpt_e164))]
    pub fn fetch_session_by_phonenumber(&self, rcpt_e164: &PhoneNumber) -> Option<orm::Session> {
        fetch_session!(self.db(), |query| {
            query.filter(schema::recipients::e164.eq(rcpt_e164.to_string()))
        })
    }

    #[tracing::instrument(skip(self, addr))]
    pub fn fetch_session_by_address(&self, addr: &ServiceAddress) -> Option<orm::Session> {
        match addr.identity {
            ServiceIdType::AccountIdentity => fetch_session!(self.db(), |query| {
                query.filter(schema::recipients::uuid.eq(addr.uuid.to_string()))
            }),
            ServiceIdType::PhoneNumberIdentity => fetch_session!(self.db(), |query| {
                query.filter(schema::recipients::pni.eq(addr.uuid.to_string()))
            }),
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_session_by_recipient_id(&self, recipient_id: i32) -> Option<orm::Session> {
        fetch_session!(self.db(), |query| {
            query.filter(schema::recipients::id.eq(recipient_id))
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_attachment(&self, attachment_id: i32) -> Option<orm::Attachment> {
        use schema::attachments::dsl::*;
        attachments
            .filter(id.eq(attachment_id))
            .first(&mut *self.db())
            .optional()
            .unwrap()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_attachments_for_message(&self, mid: i32) -> Vec<orm::Attachment> {
        use schema::attachments::dsl::*;
        attachments
            .filter(message_id.eq(mid))
            .order_by(display_order.asc())
            .load(&mut *self.db())
            .unwrap()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_reactions_for_message(&self, mid: i32) -> Vec<(orm::Reaction, orm::Recipient)> {
        use schema::{reactions, recipients};
        reactions::table
            .inner_join(recipients::table)
            .filter(reactions::message_id.eq(mid))
            .load(&mut *self.db())
            .expect("db")
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_grouped_reactions_for_message(&self, mid: i32) -> Vec<orm::GroupedReaction> {
        use schema::reactions;
        reactions::table
            .filter(reactions::message_id.eq(mid))
            .group_by((reactions::message_id, reactions::emoji))
            .select((
                reactions::message_id,
                reactions::emoji,
                diesel::dsl::count(reactions::emoji),
            ))
            .load(&mut *self.db())
            .expect("db")
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_reaction(&self, msg_id: i32, rcpt_id: i32) -> Option<orm::Reaction> {
        use schema::reactions;
        reactions::table
            .filter(
                reactions::message_id
                    .eq(msg_id)
                    .and(reactions::author.eq(rcpt_id)),
            )
            .first(&mut *self.db())
            .optional()
            .expect("db")
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_group_by_group_v1_id(&self, id: &str) -> Option<orm::GroupV1> {
        schema::group_v1s::table
            .filter(schema::group_v1s::id.eq(id))
            .first(&mut *self.db())
            .optional()
            .unwrap()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_group_by_group_v2_id(&self, id: &str) -> Option<orm::GroupV2> {
        schema::group_v2s::table
            .filter(schema::group_v2s::id.eq(id))
            .first(&mut *self.db())
            .optional()
            .unwrap()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_group_members_by_group_v1_id(
        &self,
        id: &str,
    ) -> Vec<(orm::GroupV1Member, orm::Recipient)> {
        schema::group_v1_members::table
            .inner_join(schema::recipients::table)
            .filter(schema::group_v1_members::group_v1_id.eq(id))
            .load(&mut *self.db())
            .unwrap()
    }

    #[tracing::instrument(skip(self))]
    pub fn group_v2_exists(&self, group: &GroupV2) -> bool {
        let group_id = group.secret.get_group_identifier();
        let group_id_hex = hex::encode(group_id);

        let group: Option<orm::GroupV2> = schema::group_v2s::table
            .filter(schema::group_v2s::id.eq(group_id_hex))
            .first(&mut *self.db())
            .optional()
            .expect("db");
        group.is_some()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_group_members_by_group_v2_id(
        &self,
        id: &str,
    ) -> Vec<(orm::GroupV2Member, orm::Recipient)> {
        schema::group_v2_members::table
            .inner_join(schema::recipients::table)
            .filter(schema::group_v2_members::group_v2_id.eq(id))
            .order(schema::group_v2_members::role.desc())
            .load(&mut *self.db())
            .unwrap()
    }

    #[tracing::instrument(skip(self, e164), fields(e164 = %e164))]
    pub fn fetch_or_insert_session_by_phonenumber(&self, e164: &PhoneNumber) -> orm::Session {
        if let Some(session) = self.fetch_session_by_phonenumber(e164) {
            return session;
        }

        let recipient = self.fetch_or_insert_recipient_by_phonenumber(e164);

        use schema::sessions::dsl::*;
        let session_id = diesel::insert_into(sessions)
            .values((direct_message_recipient_id.eq(recipient.id),))
            // We'd love to retrieve the whole session, but the Session object is a joined object.
            .returning(id)
            .get_result::<i32>(&mut *self.db())
            .unwrap();

        self.observe_insert(sessions, session_id)
            .with_relation(schema::recipients::table, recipient.id);

        self.fetch_session_by_id(session_id).unwrap()
    }

    #[tracing::instrument(skip(self, addr))]
    pub fn fetch_or_insert_session_by_address(&self, addr: &ServiceAddress) -> orm::Session {
        if let Some(session) = self.fetch_session_by_address(addr) {
            return session;
        }

        let recipient = self.fetch_or_insert_recipient_by_address(addr);

        use schema::sessions::dsl::*;
        let session_id = diesel::insert_into(sessions)
            .values(direct_message_recipient_id.eq(recipient.id))
            // We'd love to retrieve the whole session, but the Session object is a joined object.
            .returning(id)
            .get_result::<i32>(&mut *self.db())
            .unwrap();

        self.observe_insert(sessions, session_id)
            .with_relation(schema::recipients::table, recipient.id);

        self.fetch_session_by_id(session_id).unwrap()
    }

    /// Fetches recipient's DM session, or creates the session.
    #[tracing::instrument(skip(self))]
    pub fn fetch_or_insert_session_by_recipient_id(&self, recipient_id: i32) -> orm::Session {
        if let Some(session) = self.fetch_session_by_recipient_id(recipient_id) {
            return session;
        }

        use schema::sessions::dsl::*;
        let session_id = diesel::insert_into(sessions)
            .values((direct_message_recipient_id.eq(recipient_id),))
            .returning(id)
            .get_result::<i32>(&mut *self.db())
            .unwrap();

        self.observe_insert(sessions, session_id)
            .with_relation(schema::recipients::table, recipient_id);

        self.fetch_session_by_id(session_id)
            .expect("a session has been inserted")
    }

    pub fn fetch_or_insert_session_by_group_v1(&self, group: &GroupV1) -> orm::Session {
        let group_id = hex::encode(&group.id);

        let _span = tracing::info_span!(
            "fetch_or_insert_session_by_group_v1",
            group_id = &group_id[..8]
        )
        .entered();

        if let Some(session) = fetch_session!(self.db(), |query| {
            query.filter(schema::sessions::columns::group_v1_id.eq(&group_id))
        }) {
            return session;
        }

        let new_group = orm::GroupV1 {
            id: group_id.clone(),
            name: group.name.clone(),
            expected_v2_id: None,
        };

        // Group does not exist, insert first.
        diesel::insert_into(schema::group_v1s::table)
            .values(&new_group)
            .execute(&mut *self.db())
            .unwrap();
        self.observe_insert(schema::group_v1s::table, new_group.id);

        let now = chrono::Utc::now().naive_utc();
        for member in &group.members {
            use schema::group_v1_members::dsl::*;
            let recipient = self.fetch_or_insert_recipient_by_phonenumber(member);

            diesel::insert_into(group_v1_members)
                .values((
                    group_v1_id.eq(&group_id),
                    recipient_id.eq(recipient.id),
                    member_since.eq(now),
                ))
                .execute(&mut *self.db())
                .unwrap();
            self.observe_insert(schema::group_v1_members::table, PrimaryKey::Unknown)
                .with_relation(schema::recipients::table, recipient.id)
                .with_relation(schema::group_v1s::table, group_id.clone());
        }

        use schema::sessions::dsl::*;
        let session_id = diesel::insert_into(sessions)
            .values((group_v1_id.eq(&group_id),))
            .returning(id)
            .get_result::<i32>(&mut *self.db())
            .unwrap();

        let session = self
            .fetch_session_by_id(session_id)
            .expect("a session has been inserted");
        self.observe_insert(schema::sessions::table, session.id)
            .with_relation(schema::group_v1s::table, group_id);
        session
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_session_by_group_v1_id(&self, group_id_hex: &str) -> Option<orm::Session> {
        if group_id_hex.len() != 32 {
            tracing::warn!(
                "Trying to fetch GV1 with ID of {} != 32 chars",
                group_id_hex.len()
            );
            return None;
        }
        fetch_session!(self.db(), |query| {
            query.filter(schema::sessions::columns::group_v1_id.eq(&group_id_hex))
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_session_by_group_v2_id(&self, group_id_hex: &str) -> Option<orm::Session> {
        if group_id_hex.len() != 64 {
            tracing::warn!(
                "Trying to fetch GV2 with ID of {} != 64 chars",
                group_id_hex.len()
            );
            return None;
        }
        fetch_session!(self.db(), |query| {
            query.filter(schema::sessions::columns::group_v2_id.eq(&group_id_hex))
        })
    }

    pub fn fetch_or_insert_session_by_group_v2(&self, group: &GroupV2) -> orm::Session {
        let group_id = group.secret.get_group_identifier();
        let group_id_hex = hex::encode(group_id);
        let _span = tracing::info_span!(
            "fetch_or_insert_session_by_group_v2",
            group_id = tracing::field::display(&group_id_hex)
        )
        .entered();

        if let Some(session) = fetch_session!(self.db(), |query| {
            query.filter(schema::sessions::columns::group_v2_id.eq(&group_id_hex))
        }) {
            return session;
        }

        // The GroupV2 may still exist, even though the session does not.
        let group_v2: Option<crate::orm::GroupV2> = schema::group_v2s::table
            .filter(schema::group_v2s::id.eq(group_id_hex.clone()))
            .first(&mut *self.db())
            .optional()
            .unwrap();
        if let Some(group) = group_v2 {
            let session_id = diesel::insert_into(sessions)
                .values(group_v2_id.eq(&group.id))
                .returning(id)
                .get_result(&mut *self.db())
                .unwrap();

            let session = self
                .fetch_session_by_id(session_id)
                .expect("a session has been inserted");
            self.observe_insert(sessions, session.id)
                .with_relation(schema::group_v2s::table, group.id);
            return session;
        }

        // At this point neither the GroupV2 nor the session exists.
        let master_key =
            bincode::serialize(&group.secret.get_master_key()).expect("serialized master key");
        let new_group = orm::GroupV2 {
            id: group_id_hex,
            // XXX qTr?
            name: "New V2 group (updating)".into(),
            master_key: hex::encode(master_key),
            revision: 0,

            invite_link_password: None,

            // We don't know the ACL levels.
            // 0 means UNKNOWN
            access_required_for_attributes: 0,
            access_required_for_members: 0,
            access_required_for_add_from_invite_link: 0,

            avatar: None,
            description: Some("Group is being updated".into()),
        };

        // Group does not exist, insert first.
        diesel::insert_into(schema::group_v2s::table)
            .values(&new_group)
            .execute(&mut *self.db())
            .unwrap();
        self.observe_insert(schema::group_v2s::table, new_group.id.clone());

        // XXX somehow schedule this group for member list/name updating.

        // Two things could have happened by now:
        // - Migration: there is an existing session for a groupv1 with this V2 id.
        // - New group

        let migration_v1_session: Option<(orm::GroupV1, Option<orm::DbSession>)> =
            schema::group_v1s::table
                .filter(schema::group_v1s::expected_v2_id.eq(&new_group.id))
                .left_join(schema::sessions::table)
                .first(&mut *self.db())
                .optional()
                .expect("db");

        use schema::sessions::dsl::*;
        match migration_v1_session {
            Some((_group, Some(session))) => {
                tracing::info!(
                    "Group V2 migration detected. Updating session to point to the new group."
                );

                let count = diesel::update(sessions)
                    .set((
                        group_v1_id.eq::<Option<String>>(None),
                        group_v2_id.eq(&new_group.id),
                    ))
                    .filter(id.eq(session.id))
                    .execute(&mut *self.db())
                    .expect("session updated");
                self.observe_update(sessions, session.id)
                    .with_relation(
                        schema::group_v1s::table,
                        session
                            .group_v1_id
                            .clone()
                            .expect("group_v1_id from migration"),
                    )
                    .with_relation(schema::group_v2s::table, new_group.id);

                // XXX consider removing the group_v1
                assert_eq!(count, 1, "session should have been updated");
                // Refetch session because the info therein is stale.
                self.fetch_session_by_id(session.id)
                    .expect("existing session")
            }
            Some((_group, None)) => {
                unreachable!("Former group V1 found.  We expect the branch above to have returned a session for it.");
            }
            None => {
                let session_id = diesel::insert_into(sessions)
                    .values((group_v2_id.eq(&new_group.id),))
                    .returning(id)
                    .get_result(&mut *self.db())
                    .unwrap();

                let session = self
                    .fetch_session_by_id(session_id)
                    .expect("a session has been inserted");
                self.observe_insert(sessions, session.id)
                    .with_relation(schema::group_v2s::table, new_group.id);
                session
            }
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn delete_session(&self, session_id: i32) {
        let affected_rows =
            diesel::delete(schema::sessions::table.filter(schema::sessions::id.eq(session_id)))
                .execute(&mut *self.db())
                .expect("delete session");
        self.observe_delete(schema::sessions::table, session_id);

        tracing::trace!(
            "delete_session({}) affected {} rows",
            session_id,
            affected_rows
        );
    }

    #[tracing::instrument(skip(self))]
    pub fn save_draft(&self, session_id: i32, draft: String) {
        let draft = if draft.is_empty() { None } else { Some(draft) };

        let affected_rows =
            diesel::update(schema::sessions::table.filter(schema::sessions::id.eq(session_id)))
                .set(schema::sessions::draft.eq(draft))
                .execute(&mut *self.db())
                .expect("save draft");

        tracing::trace!("save_draft() affected {} rows", affected_rows);

        if affected_rows > 0 {
            self.observe_update(schema::sessions::table, session_id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn start_message_expiry(&self, message_id: i32) {
        let affected_rows = diesel::update(
            schema::messages::table.filter(
                schema::messages::id
                    .eq(message_id)
                    .and(schema::messages::expiry_started.is_null())
                    .and(schema::messages::message_type.is_null()),
            ),
        )
        .set(schema::messages::expiry_started.eq(Some(chrono::Utc::now().naive_utc())))
        .execute(&mut *self.db())
        .expect("set message expiry");

        tracing::trace!("affected {} rows", affected_rows);

        if affected_rows > 0 {
            self.observe_update(schema::messages::table, message_id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_expired_message_ids(&self) -> Vec<(i32, DateTime<Utc>)> {
        self.fetch_message_ids_by_expiry(true)
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_expiring_message_ids(&self) -> Vec<(i32, DateTime<Utc>)> {
        self.fetch_message_ids_by_expiry(false)
    }

    fn fetch_message_ids_by_expiry(&self, already_expired: bool) -> Vec<(i32, DateTime<Utc>)> {
        schema::messages::table
            .select((
                schema::messages::id,
                sql::<Timestamp>(DELETE_AFTER).sql("AS delete_after"),
            ))
            .filter(
                // This filter is the same as the index
                schema::messages::expiry_started
                    .is_not_null()
                    .and(schema::messages::expires_in.is_not_null())
                    .and(schema::messages::message_type.is_null())
                    .and(
                        sql::<Bool>("delete_after")
                            .sql(if already_expired { "<=" } else { ">" })
                            .sql("DATETIME('now')"),
                    ),
            )
            .order_by(sql::<Timestamp>("delete_after").asc())
            .load(&mut *self.db())
            .expect("messages by expiry timestamp")
            .into_iter()
            .map(|(id, ndt)| (id, DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)))
            .collect()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_next_expiring_message_id(&self) -> Option<(i32, DateTime<Utc>)> {
        schema::messages::table
            .select((
                schema::messages::id,
                sql::<Timestamp>(DELETE_AFTER).sql("AS delete_after"),
            ))
            .filter(
                schema::messages::expiry_started
                    .is_not_null()
                    .and(schema::messages::expires_in.is_not_null())
                    .and(schema::messages::message_type.is_null()),
            )
            .order_by(sql::<Timestamp>("delete_after").asc())
            .first(&mut *self.db())
            .optional()
            .expect("messages by expiry timestamp")
            .map(|(id, ndt)| (id, DateTime::<Utc>::from_naive_utc_and_offset(ndt, Utc)))
    }

    #[tracing::instrument(skip(self))]
    pub fn delete_expired_messages(&mut self) -> usize {
        let deletions: Vec<i32> = diesel::delete(schema::messages::table)
            .filter(
                sql::<Timestamp>(DELETE_AFTER)
                    .le(sql::<Timestamp>("DATETIME('now')"))
                    .and(
                        schema::messages::expiry_started
                            .is_not_null()
                            .and(schema::messages::expires_in.is_not_null())
                            .and(schema::messages::message_type.is_null()),
                    ),
            )
            .returning(schema::messages::id)
            .load(&mut *self.db())
            .expect("delete expired messages");

        tracing::trace_span!("deleting expired attachments").in_scope(|| {
            for message_id in &deletions {
                self.delete_attachments_for_message(*message_id);
            }
        });

        tracing::trace!("affected {} rows", deletions.len());

        for deletion in &deletions {
            self.observe_delete(schema::messages::table, *deletion);
        }

        deletions.len()
    }

    #[tracing::instrument(skip(self))]
    pub fn mark_session_read(&self, session_id: i32) {
        let ids: Vec<i32> = diesel::update(
            schema::messages::table.filter(
                schema::messages::session_id
                    .eq(session_id)
                    .and(schema::messages::is_read.eq(false)),
            ),
        )
        .set((schema::messages::is_read.eq(true),))
        .returning(schema::messages::id)
        .load(&mut *self.db())
        .expect("mark session read");

        for message_id in ids {
            self.observe_update(schema::messages::table, message_id)
                .with_relation(schema::sessions::table, session_id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn mark_session_muted(&self, session_id: i32, muted: bool) {
        use schema::sessions::dsl::*;

        let affected_rows = diesel::update(sessions.filter(id.eq(session_id)))
            .set((is_muted.eq(muted),))
            .execute(&mut *self.db())
            .expect("mark session (un)muted");
        if affected_rows > 0 {
            self.observe_update(schema::sessions::table, session_id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn mark_session_archived(&self, session_id: i32, archived: bool) {
        use schema::sessions::dsl::*;

        let affected_rows = diesel::update(sessions.filter(id.eq(session_id)))
            .set((is_archived.eq(archived),))
            .execute(&mut *self.db())
            .expect("mark session (un)archived");
        if affected_rows > 0 {
            self.observe_update(schema::sessions::table, session_id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn mark_session_pinned(&self, session_id: i32, pinned: bool) {
        use schema::sessions::dsl::*;

        let affected_rows = diesel::update(sessions.filter(id.eq(session_id)))
            .set((is_pinned.eq(pinned),))
            .execute(&mut *self.db())
            .expect("mark session (un)pinned");
        if affected_rows > 0 {
            self.observe_update(schema::sessions::table, session_id);
        }
    }

    #[tracing::instrument(skip(self))]
    pub fn mark_recipient_registered(
        &self,
        service_address: ServiceAddress,
        registered: bool,
    ) -> bool {
        use schema::recipients::dsl::*;

        let rid: Option<i32> = match service_address.identity {
            ServiceIdType::AccountIdentity => {
                diesel::update(recipients.filter(uuid.eq(service_address.uuid.to_string())))
                    .set(is_registered.eq(registered))
                    .returning(id)
                    .get_result(&mut *self.db())
                    .optional()
                    .expect("mark recipient (un)registered")
            }
            ServiceIdType::PhoneNumberIdentity => {
                diesel::update(recipients.filter(pni.eq(service_address.uuid.to_string())))
                    .set(is_registered.eq(registered))
                    .returning(id)
                    .get_result(&mut *self.db())
                    .optional()
                    .expect("mark recipient (un)registered")
            }
        };

        let Some(rid) = rid else {
            tracing::trace!(
                "Registration of {:?} not updated in database",
                service_address
            );
            return false;
        };

        self.observe_update(schema::recipients::table, rid);

        true
    }

    #[tracing::instrument(skip(self))]
    pub fn register_attachment(&mut self, mid: i32, ptr: AttachmentPointer) -> i32 {
        use schema::attachments::dsl::*;

        let inserted_attachment_id = diesel::insert_into(attachments)
            .values((
                // XXX: many more things to store:
                // - display order
                // - transform properties

                // First the fields that borrow, but are `Copy` through an accessor method
                is_voice_note
                    .eq(attachment_pointer::Flags::VoiceMessage as i32 & ptr.flags() as i32 != 0),
                is_borderless
                    .eq(attachment_pointer::Flags::Borderless as i32 & ptr.flags() as i32 != 0),
                upload_timestamp.eq(millis_to_naive_chrono(ptr.upload_timestamp())),
                cdn_number.eq(ptr.cdn_number() as i32),
                content_type.eq(ptr.content_type().to_string()),
                // Then the fields that we immediately access
                is_quote.eq(false),
                message_id.eq(mid),
                visual_hash.eq(&ptr.blur_hash),
                size.eq(&ptr.size.map(|x| x as i32)),
                file_name.eq(&ptr.file_name),
                caption.eq(&ptr.caption),
                data_hash.eq(&ptr.digest),
                width.eq(ptr.width.map(|x| x as i32)),
                height.eq(ptr.height.map(|x| x as i32)),
                pointer.eq(ptr.encode_to_vec()),
            ))
            .returning(id)
            .get_result::<i32>(&mut *self.db())
            .expect("insert attachment");

        self.observe_insert(
            schema::attachments::table,
            PrimaryKey::RowId(inserted_attachment_id),
        )
        .with_relation(schema::messages::table, mid);

        inserted_attachment_id
    }

    #[tracing::instrument(skip(self))]
    pub fn store_attachment_visual_hash(
        &self,
        attachment_id: i32,
        hash: &str,
        new_width: u32,
        new_height: u32,
    ) {
        use schema::attachments::dsl::*;

        let count = diesel::update(attachments.filter(id.eq(attachment_id)))
            .set((
                visual_hash.eq(hash),
                width.eq(new_width as i32),
                height.eq(new_height as i32),
            ))
            .execute(&mut *self.db())
            .expect("store attachment visual hash");

        if count == 1 {
            tracing::trace!("Attachment visual hash saved to id {}", attachment_id);
            self.observe_update(schema::attachments::table, PrimaryKey::RowId(attachment_id));
        } else {
            tracing::error!(
                "Could not save attachment visual hash to attachment {}",
                attachment_id
            );
        };
    }

    #[tracing::instrument(skip(self))]
    pub fn store_attachment_pointer(
        &self,
        attachment_id: i32,
        attachment_pointer: &AttachmentPointer,
    ) {
        use schema::attachments::dsl::*;

        let count = diesel::update(attachments.filter(id.eq(attachment_id)))
            .set(pointer.eq(attachment_pointer.encode_to_vec()))
            .execute(&mut *self.db())
            .expect("store sent attachment pointer");

        if count == 1 {
            tracing::trace!("Attachment pointer saved to id {}", attachment_id);
        } else {
            tracing::error!(
                "Could not save attachment pointer to attachment {}",
                attachment_id
            );
        };
    }

    /// Create a new message. This was transparent within SaveMessage in Go.
    ///
    /// Panics is new_message.session_id is None.
    #[tracing::instrument(skip(self), fields(session_id = new_message.session_id))]
    pub fn create_message(&self, new_message: &NewMessage) -> orm::Message {
        // XXX Storing the message with its attachments should happen in a transaction.
        // Meh.
        let session = new_message.session_id;

        let sender_id = if let Some(sender) = new_message.source_addr {
            self.fetch_recipient(sender).map(|r| r.id)
        } else {
            None
        };

        let quoted_message_id = new_message
            .quote_timestamp
            .and_then(|ts| {
                let msg = self.fetch_message_by_timestamp(millis_to_naive_chrono(ts));
                if msg.is_none() {
                    tracing::warn!("No message to quote for ts={}", ts);
                }
                msg
            })
            .map(|message| message.id);

        // The server time needs to be the rounded-down version; chrono does nanoseconds.
        let server_time = naive_chrono_rounded_down(new_message.timestamp);
        tracing::trace!("Creating message for timestamp {}", server_time);

        let edit_id = new_message.edit.as_ref().map(|x| x.id);

        let computed_revision = if let Some(edit) = &new_message.edit {
            // Compute revision number
            use schema::messages::dsl::*;
            messages
                .select(diesel::dsl::max(revision_number))
                .filter(
                    id.eq(edit.original_message_id())
                        .or(original_message_id.eq(edit.original_message_id())),
                )
                .first::<Option<i32>>(&mut *self.db())
                .expect("revision number")
                .map(|x: i32| x + 1)
                .unwrap_or(0)
        } else {
            0
        };

        let latest_message: orm::Message = {
            use schema::messages::dsl::*;
            diesel::insert_into(messages)
                .values((
                    session_id.eq(session),
                    server_guid.eq(new_message.server_guid.as_ref().map(Uuid::to_string)),
                    text.eq(&new_message.text),
                    sender_recipient_id.eq(sender_id),
                    received_timestamp.eq(if !new_message.outgoing {
                        Some(chrono::Utc::now().naive_utc())
                    } else {
                        None
                    }),
                    sent_timestamp.eq(if new_message.outgoing && new_message.sent {
                        Some(new_message.timestamp)
                    } else {
                        None
                    }),
                    server_timestamp.eq(server_time),
                    is_read.eq(new_message.is_read),
                    is_outbound.eq(new_message.outgoing),
                    use_unidentified.eq(new_message.is_unidentified),
                    flags.eq(new_message.flags),
                    message_type.eq(new_message.message_type.clone()),
                    quote_id.eq(quoted_message_id),
                    expires_in.eq(new_message.expires_in.map(|x| x.as_secs() as i32)),
                    story_type.eq(new_message.story_type as i32),
                    message_ranges.eq(&new_message.body_ranges),
                    original_message_id.eq(edit_id),
                    revision_number.eq(computed_revision),
                ))
                .get_result(&mut *self.db())
                .expect("inserting a message")
        };

        // Then see if the message was inserted ok and what it was
        assert_eq!(
            latest_message.session_id, session,
            "message insert sanity test failed"
        );
        self.observe_insert(schema::messages::table, latest_message.id)
            .with_relation(schema::sessions::table, session);

        // Then we process the edit
        if let Some(edit) = &new_message.edit {
            tracing::trace!("Message was an edit, updating old messages");
            let ids: Vec<i32> = {
                use schema::messages::dsl::*;
                diesel::update(messages)
                    .filter(
                        id.eq(edit.original_message_id())
                            .or(original_message_id.eq(edit.original_message_id())),
                    )
                    .set((
                        // Set the original message id to the series of the edits
                        original_message_id.eq(edit.original_message_id()),
                        // Set the latest revision id to the new inserted message
                        latest_revision_id.eq(latest_message.id),
                    ))
                    .returning(id)
                    .load(&mut *self.db())
                    .expect("update edited messages")
            };
            let affected_rows = ids.len();
            assert!(
                affected_rows >= 1,
                "Did not update any message. Dazed and confused."
            );
            for id in ids {
                self.observe_update(schema::messages::table, id)
                    .with_relation(schema::sessions::table, session);
            }
        }

        // Mark the session as non-archived
        // TODO: Do this only when necessary
        self.mark_session_archived(session, false);

        tracing::trace!("Inserted message id {}", latest_message.id);

        latest_message
    }

    #[tracing::instrument(skip(self))]
    pub fn update_transcription(&mut self, attachment_id: i32, new_transcription: &str) {
        use schema::attachments::dsl::*;

        let count = diesel::update(attachments.filter(id.eq(attachment_id)))
            .set(transcription.eq(new_transcription))
            .execute(&mut *self.db())
            .expect("update transcription");

        if count == 1 {
            tracing::trace!("Transcription updated for attachment id {}", attachment_id);
            self.observe_update(schema::attachments::table, attachment_id);
        } else {
            tracing::error!(
                "Could not update transcription for attachment {}",
                attachment_id
            );
        };
    }

    #[tracing::instrument(skip(self, path), fields(path = %path.as_ref().display()))]
    pub fn insert_local_attachment(
        &self,
        attachment_message_id: i32,
        mime_type: Option<&str>,
        path: impl AsRef<Path>,
        voice_note: bool,
    ) -> i32 {
        let path = path.as_ref();
        let att_file = File::open(path).expect("");
        let att_size = match att_file.metadata() {
            Ok(m) => Some(m.len() as i32),
            Err(_) => None,
        };

        let mime_type = mime_type.map(std::borrow::Cow::from).unwrap_or_else(|| {
            mime_guess::from_path(path)
                .first_or_octet_stream()
                .essence_str()
                // We need to either retain the Mime object or allocate a new string from the
                // temporary.
                .to_string()
                .into()
        });

        let filename = path.file_name().map(|s| s.to_str().unwrap());

        let id = {
            use schema::attachments::dsl::*;
            diesel::insert_into(attachments)
                .values((
                    message_id.eq(attachment_message_id),
                    content_type.eq(mime_type),
                    attachment_path.eq(path.as_os_str().to_str().expect("path UTF-8 compliant")),
                    size.eq(att_size),
                    file_name.eq(filename),
                    is_voice_note.eq(voice_note),
                    is_borderless.eq(false),
                    is_quote.eq(false),
                ))
                .returning(id)
                .get_result::<i32>(&mut *self.db())
                .expect("Insert attachment")
        };

        self.observe_insert(schema::attachments::table, id)
            .with_relation(schema::messages::table, attachment_message_id);

        id
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_message_by_timestamp(&self, ts: NaiveDateTime) -> Option<orm::Message> {
        let query = schema::messages::table.filter(schema::messages::server_timestamp.eq(ts));
        query.first(&mut *self.db()).ok()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_recipient_by_id(&self, id: i32) -> Option<orm::Recipient> {
        schema::recipients::table
            .filter(schema::recipients::id.eq(id))
            .first(&mut *self.db())
            .ok()
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_message_by_id(&self, id: i32) -> Option<orm::Message> {
        // Even a single message needs to know if it's queued to satisfy the `Message` trait
        schema::messages::table
            .filter(schema::messages::id.eq(id))
            .first(&mut *self.db())
            .ok()
    }

    /// Returns a vector of messages for a specific session, ordered by server timestamp.
    #[tracing::instrument(skip(self))]
    pub fn fetch_all_messages(&self, session_id: i32, only_most_recent: bool) -> Vec<orm::Message> {
        if only_most_recent {
            schema::messages::table
                .filter(schema::messages::session_id.eq(session_id).and(
                    schema::messages::latest_revision_id.is_null().or(
                        schema::messages::latest_revision_id.eq(schema::messages::id.nullable()),
                    ),
                ))
                .order_by(schema::messages::columns::server_timestamp.desc())
                .load(&mut *self.db())
                .expect("database")
        } else {
            schema::messages::table
                .filter(schema::messages::session_id.eq(session_id))
                .order_by(schema::messages::columns::server_timestamp.desc())
                .load(&mut *self.db())
                .expect("database")
        }
    }

    /// Return the amount of messages in the database
    #[tracing::instrument(skip(self))]
    pub fn message_count(&self) -> i32 {
        let count: i64 = schema::messages::table
            .count()
            .get_result(&mut *self.db())
            .expect("db");
        count as _
    }

    /// Return the amount of sessions in the database
    #[tracing::instrument(skip(self))]
    pub fn session_count(&self) -> i32 {
        let count: i64 = schema::sessions::table
            .count()
            .get_result(&mut *self.db())
            .expect("db");
        count as _
    }

    /// Return the amount of recipients in the database
    #[tracing::instrument(skip(self))]
    pub fn recipient_count(&self) -> i32 {
        let count: i64 = schema::recipients::table
            .filter(schema::recipients::uuid.is_not_null())
            .count()
            .get_result(&mut *self.db())
            .expect("db");
        count as _
    }

    /// Return the amount of unsent messages in the database
    #[tracing::instrument(skip(self))]
    pub fn unsent_count(&self) -> i32 {
        let count: i64 = schema::messages::table
            .filter(schema::messages::is_outbound.is(true))
            .filter(schema::messages::sending_has_failed.is(true))
            .count()
            .get_result(&mut *self.db())
            .expect("db");
        count as _
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_augmented_message(&self, message_id: i32) -> Option<orm::AugmentedMessage> {
        let message = self.fetch_message_by_id(message_id)?;
        let receipts = self.fetch_message_receipts(message.id);
        let attachments: i64 = schema::attachments::table
            .filter(schema::attachments::message_id.eq(message_id))
            .count()
            .get_result(&mut *self.db())
            .expect("db");
        let reactions: i64 = schema::reactions::table
            .filter(schema::reactions::message_id.eq(message_id))
            .count()
            .get_result(&mut *self.db())
            .expect("db");

        let is_voice_note = if attachments == 1 {
            schema::attachments::table
                .filter(schema::attachments::message_id.eq(message_id))
                .select(schema::attachments::is_voice_note)
                .get_result(&mut *self.db())
                .expect("db")
        } else {
            false
        };

        let body_ranges = if let Some(r) = &message.message_ranges {
            crate::store::body_ranges::deserialize(r)
        } else {
            vec![]
        };

        let mentions = self.fetch_mentions(&body_ranges);

        Some(AugmentedMessage {
            inner: message,
            is_voice_note,
            receipts,
            attachments: attachments as usize,
            reactions: reactions as usize,
            mentions,
            body_ranges,
        })
    }

    #[tracing::instrument(skip(self))]
    pub fn fetch_all_sessions_augmented(&self) -> Vec<orm::AugmentedSession> {
        let mut sessions: Vec<_> = self
            .fetch_sessions()
            .into_iter()
            .map(|session| {
                let last_message = self.fetch_last_message_by_session_id_augmented(session.id);
                orm::AugmentedSession {
                    inner: session,
                    last_message,
                }
            })
            .collect();
        // XXX This could be solved through a sub query.
        sessions.sort_unstable_by_key(|session| {
            std::cmp::Reverse((
                session.last_message.as_ref().map(|m| m.server_timestamp),
                session.id,
            ))
        });

        sessions
    }

    /// Returns a vector of tuples of messages with their sender.
    ///
    /// When the sender is None, it is a sent message, not a received message.
    // XXX maybe this should be `Option<Vec<...>>`.
    #[tracing::instrument(skip(self))]
    pub fn fetch_all_messages_augmented(
        &self,
        sid: i32,
        only_most_recent: bool,
    ) -> Vec<orm::AugmentedMessage> {
        // XXX double/aliased-join would be very useful.
        // Our strategy is to fetch as much as possible, and to augment with as few additional
        // queries as possible. We chose to not join `sender`, and instead use a loop for that
        // part.
        let messages = self.fetch_all_messages(sid, only_most_recent);

        let order = (
            schema::messages::columns::server_timestamp.desc(),
            schema::messages::columns::id.desc(),
        );

        // message_id, is_voice_note, attachment count
        let attachments: Vec<(i32, Option<i16>, i64)> =
            tracing::trace_span!("fetching attachments",).in_scope(|| {
                schema::attachments::table
                    .inner_join(schema::messages::table)
                    .group_by(schema::attachments::message_id)
                    .select((
                        schema::attachments::message_id,
                        // We could also define a boolean or aggregate function...
                        // Googling instructions for that is difficult though, since "diesel aggregate or"
                        // yields you machines that consume fuel.
                        diesel::dsl::max(diesel::dsl::sql::<diesel::sql_types::SmallInt>(
                            "attachments.is_voice_note",
                        )),
                        diesel::dsl::count_distinct(schema::attachments::id),
                    ))
                    .filter(schema::messages::session_id.eq(sid))
                    .order_by(order)
                    .load(&mut *self.db())
                    .expect("db")
            });

        // message_id, reaction count
        let reactions: Vec<(i32, i64)> =
            tracing::trace_span!("fetching reactions",).in_scope(|| {
                schema::reactions::table
                    .inner_join(schema::messages::table)
                    .group_by(schema::reactions::message_id)
                    .select((
                        schema::reactions::message_id,
                        diesel::dsl::count_distinct(schema::reactions::reaction_id),
                    ))
                    .filter(schema::messages::session_id.eq(sid))
                    .order_by(order)
                    .load(&mut *self.db())
                    .expect("db")
            });

        let receipts: Vec<(orm::Receipt, orm::Recipient)> =
            tracing::trace_span!("fetching receipts").in_scope(|| {
                schema::receipts::table
                    .inner_join(schema::recipients::table)
                    .select((
                        schema::receipts::all_columns,
                        schema::recipients::all_columns,
                    ))
                    .inner_join(schema::messages::table.inner_join(schema::sessions::table))
                    .filter(schema::sessions::id.eq(sid))
                    .order_by(order)
                    .load(&mut *self.db())
                    .expect("db")
            });

        let mut aug_messages = Vec::with_capacity(messages.len());
        tracing::trace_span!("joining messages, attachments, receipts into AugmentedMessage")
            .in_scope(|| {
                let mut attachments = attachments.into_iter().peekable();
                let mut reactions = reactions.into_iter().peekable();
                let receipts = receipts
                    .into_iter()
                    .group_by(|(receipt, _recipient)| receipt.message_id);
                let mut receipts = receipts.into_iter().peekable();

                for message in messages {
                    let (attachments, is_voice_note) = if attachments
                        .peek()
                        .map(|(id, _, _)| *id == message.id)
                        .unwrap_or(false)
                    {
                        let (_, voice_note, attachments) = attachments.next().unwrap();
                        (
                            attachments as usize,
                            voice_note.map(|x| x > 0).unwrap_or(false),
                        )
                    } else {
                        (0, false)
                    };

                    let reactions = if reactions
                        .peek()
                        .map(|(id, _)| *id == message.id)
                        .unwrap_or(false)
                    {
                        let (_, reactions) = reactions.next().unwrap();
                        reactions as usize
                    } else {
                        0
                    };

                    let receipts = if receipts
                        .peek()
                        .map(|(id, _)| *id == message.id)
                        .unwrap_or(false)
                    {
                        let (_, receipts) = receipts.next().unwrap();
                        receipts.collect_vec()
                    } else {
                        vec![]
                    };

                    let body_ranges = if let Some(r) = &message.message_ranges {
                        crate::store::body_ranges::deserialize(r)
                    } else {
                        vec![]
                    };

                    let mentions = self.fetch_mentions(&body_ranges);

                    aug_messages.push(orm::AugmentedMessage {
                        inner: message,
                        is_voice_note,
                        attachments,
                        reactions,
                        receipts,
                        body_ranges,
                        mentions,
                    });
                }
            });
        aug_messages
    }

    fn fetch_mentions(
        &self,
        body_ranges: &[crate::store::body_ranges::BodyRange],
    ) -> std::collections::HashMap<uuid::Uuid, orm::Recipient> {
        body_ranges
            .iter()
            .filter_map(|range| range.associated_value.as_ref())
            .filter_map(|av| match av {
                // XXX this silently fails on unparsable UUIDs
                AssociatedValue::MentionUuid(uuid) => match uuid::Uuid::parse_str(uuid) {
                    Ok(uuid) => Some(uuid),
                    Err(_e) => {
                        tracing::warn!("could not parse UUID {uuid}");
                        None
                    }
                },
                _ => None,
            })
            .filter_map(|uuid| {
                self.fetch_recipient_by_service_address(&ServiceAddress::new_aci(uuid))
            })
            .map(|r| (r.uuid.expect("queried by uuid"), r))
            .collect()
    }

    /// Don't actually delete, but mark the message as deleted
    /// and clear the body text, delete its reactions,
    /// and if it was an incoming message, also its attachments from the disk.
    #[tracing::instrument(skip(self))]
    pub fn delete_message(&mut self, message_id: i32) -> bool {
        let message: Option<orm::Message> = diesel::update(schema::messages::table)
            .filter(schema::messages::id.eq(message_id))
            .set((
                schema::messages::is_remote_deleted.eq(true),
                schema::messages::text.eq(None::<String>),
                schema::messages::message_ranges.eq(None::<Vec<u8>>),
            ))
            .get_result(&mut *self.db())
            .optional()
            .unwrap();

        let Some(message) = message else {
            tracing::warn!("Tried to remove non-existing message {}", message_id);
            return false;
        };

        let mut n_attachments: usize = 0;

        let _span = tracing::trace_span!("delete attachments", message_id = message.id).entered();
        if !message.is_outbound {
            tracing::trace!("Message is from someone else, deleting attachments...");
            n_attachments = self.delete_attachments_for_message(message.id);
        }
        drop(_span);

        let _span = tracing::trace_span!("delete reactions", message_id = message.id).entered();
        let reactions: Vec<i32> = diesel::delete(schema::reactions::table)
            .filter(schema::reactions::message_id.eq(message.id))
            .returning(schema::reactions::reaction_id)
            .load(&mut *self.db())
            .unwrap();

        self.observe_update(schema::messages::table, message.id)
            .with_relation(schema::sessions::table, message.session_id);

        for reaction in &reactions {
            self.observe_delete(schema::reactions::table, *reaction)
                .with_relation(schema::messages::table, message.id);
        }

        tracing::trace!("Marked Message {{ id: {} }} deleted", message.id);
        tracing::trace!(
            "Deleted {} attachment(s) and {} reaction(s)",
            n_attachments,
            reactions.len()
        );

        true
    }

    /// Delete all attachments of the message, is no other message references them.
    #[tracing::instrument(skip(self))]
    fn delete_attachments_for_message(&mut self, message_id: i32) -> usize {
        let mut n_attachments = 0;
        let allowed = self.config.attachments_regex();
        // TODO: refactor this with delete-returning-all-columns
        self.fetch_attachments_for_message(message_id)
            .into_iter()
            .for_each(|attachment| {
                diesel::delete(schema::attachments::table)
                    .filter(schema::attachments::id.eq(attachment.id))
                    .execute(&mut *self.db())
                    .unwrap();
                self.observe_delete(schema::attachments::table, attachment.id)
                    .with_relation(schema::messages::table, message_id);

                if let Some(path) = attachment.attachment_path {
                    let _span = tracing::debug_span!("considering attachment file deletion", id = attachment.id, path = %path).entered();
                    let remaining = schema::attachments::table
                        .filter(schema::attachments::attachment_path.eq(&path))
                        .count()
                        .get_result::<i64>(&mut *self.db())
                        .unwrap();
                    if remaining > 0 {
                        tracing::warn!(attachment.id, %path, "references to attachment exist, not deleting");
                    } else if allowed.is_match(&path) {
                        match std::fs::remove_file(&path) {
                            Ok(()) => {
                                tracing::trace!("deleted file");
                                n_attachments += 1;
                            }
                            Err(e) => {
                                tracing::trace!("could not delete file: {:?}", e);
                            }
                        };
                    } else {
                        tracing::warn!(
                            attachment.id,
                            ?path,
                            "not deleting attachment because it does not match the allowed regex"
                        );
                    }
                }
            });
        n_attachments
    }

    /// Marks all messages that are outbound and unsent as failed.
    #[tracing::instrument(skip(self))]
    pub fn mark_pending_messages_failed(&self) -> usize {
        use schema::messages::dsl::*;
        let failed_messages: Vec<i32> = diesel::update(messages)
            .filter(
                sent_timestamp
                    .is_null()
                    .and(is_outbound)
                    .and(sending_has_failed.eq(false)),
            )
            .set(schema::messages::sending_has_failed.eq(true))
            .returning(schema::messages::id)
            .load(&mut *self.db())
            .unwrap();

        for &message in &failed_messages {
            self.observe_update(schema::messages::table, message);
        }

        let count = failed_messages.len();
        if count == 0 {
            tracing::trace!("Set no messages to failed");
        } else {
            tracing::warn!("Set {} messages to failed", count);
        }
        count
    }

    /// Marks a message as failed to send
    #[tracing::instrument(skip(self))]
    pub fn fail_message(&self, message_id: i32) {
        diesel::update(schema::messages::table)
            .filter(schema::messages::id.eq(message_id))
            .set(schema::messages::sending_has_failed.eq(true))
            .execute(&mut *self.db())
            .unwrap();

        self.observe_update(schema::messages::table, message_id);
    }

    #[tracing::instrument(skip(self))]
    pub fn dequeue_message(&self, message_id: i32, sent_time: NaiveDateTime, unidentified: bool) {
        diesel::update(schema::messages::table)
            .filter(schema::messages::id.eq(message_id))
            .set((
                schema::messages::sent_timestamp.eq(sent_time),
                schema::messages::sending_has_failed.eq(false),
                schema::messages::use_unidentified.eq(unidentified),
            ))
            .execute(&mut *self.db())
            .unwrap();

        self.observe_update(schema::messages::table, message_id);
    }

    pub async fn credential_cache(
        &self,
    ) -> tokio::sync::RwLockReadGuard<'_, InMemoryCredentialsCache> {
        self.credential_cache.read().await
    }

    pub async fn credential_cache_mut(
        &self,
    ) -> tokio::sync::RwLockWriteGuard<'_, InMemoryCredentialsCache> {
        self.credential_cache.write().await
    }

    /// Saves a given attachment into a random-generated path. Returns the path.
    #[tracing::instrument(skip(self, attachment), fields(attachment_size = attachment.len()))]
    pub async fn save_attachment(
        &self,
        id: i32,
        dest: &Path,
        ext: &str,
        attachment: &[u8],
    ) -> Result<PathBuf, anyhow::Error> {
        let fname = Uuid::new_v4();
        let fname = fname.as_simple();
        let fname_formatted = format!("{}", fname);
        let fname_path = Path::new(&fname_formatted);

        let mut path = dest.join(fname_path);
        path.set_extension(ext);

        utils::write_file_async(&path, attachment)
            .await
            .with_context(|| {
                format!(
                    "Could not create and write to attachment file: {}",
                    path.display()
                )
            })?;

        diesel::update(schema::attachments::table)
            .filter(schema::attachments::id.eq(id))
            .set(schema::attachments::attachment_path.eq(path.to_str().expect("valid UTF8 path")))
            .execute(&mut *self.db())
            .unwrap();

        self.observe_update(schema::attachments::table, id);

        Ok(path)
    }

    pub fn set_recipient_external_id(&mut self, rcpt_id: i32, ext_id: Option<String>) {
        use crate::schema::recipients::dsl::*;

        let affected = diesel::update(recipients)
            .set(external_id.eq(&ext_id))
            .filter(id.eq(rcpt_id))
            .execute(&mut *self.db())
            .expect("db");

        // If updating self, invalidate the cache
        if rcpt_id == self.fetch_self_recipient_id() {
            self.invalidate_self_recipient();
        }

        if affected > 0 {
            tracing::debug!("Recipient {} external ID changed to {:?}", rcpt_id, ext_id);
            self.observe_update(recipients, rcpt_id);
        }
    }

    #[tracing::instrument]
    pub fn migrate_storage() -> Result<(), anyhow::Error> {
        let data_dir = dirs::data_local_dir().context("No data directory found")?;

        let old_path = data_dir.join("harbour-whisperfish");
        let old_db = &old_path.join("db");
        let old_storage = &old_path.join("storage");

        let new_path = data_dir.join("be.rubdos").join("harbour-whisperfish");
        let new_db = &new_path.join("db");
        let new_storage = &new_path.join("storage");

        if !new_path.exists() {
            eprintln!("Creating new storage path...");
            std::fs::create_dir_all(&new_path)?;
        }

        // Remove unused directories, if empty
        for dir_name in &["groups", "prekeys", "signed_prekeys"] {
            let dir_path = &new_storage.join(dir_name);
            if dir_path.exists() {
                match std::fs::remove_dir(dir_path) {
                    Ok(()) => eprintln!("Empty '{}' directory removed", dir_name),
                    _ => eprintln!("Couldn't remove '{}' directory, is it empty?", dir_name),
                }
            }
        }

        // New paths already in use
        if new_db.exists() && new_storage.exists() {
            return Ok(());
        } else if !new_db.exists() && !new_storage.exists() && !old_db.exists() {
            // No new or old paths exist; must be clean install
            if !old_storage.exists() {
                eprintln!("Creating storage and db folders...");
                std::fs::create_dir(new_db)?;
                std::fs::create_dir(new_storage)?;
                return Ok(());
            }
            // Only old storage path exists -- this indicates that
            // the Whisperfish was previously started but never registered.
            // Create the old database directory, so the migration can continue.
            else {
                eprintln!("No old database found, creating empty directory...");
                std::fs::create_dir(old_db)?;
            }
        }
        // Try to detect incomplete migration state
        else if (new_db.exists() ^ new_storage.exists())
            || (old_db.exists() ^ old_storage.exists())
        {
            eprintln!("Storage state is abnormal, aborting!");
            eprintln!("new db exists: {}", new_db.exists());
            eprintln!("new storage exists: {}", new_storage.exists());
            eprintln!("old db exists: {}", old_db.exists());
            eprintln!("old storage exists: {}", old_storage.exists());
            std::process::exit(1);
        }

        // Sailjail mounts the old and new paths separately, which makes
        // std::fs::rename fail. That means we have to copy-and-delete
        // recursively instead, handled by fs_extra::dir::move_dir.
        let options = fs_extra::dir::CopyOptions::new();
        eprintln!("Migrating old db folder...");
        fs_extra::dir::move_dir(old_db, &new_path, &options)?;
        eprintln!("Migrating old storage folder...");
        fs_extra::dir::move_dir(old_storage, &new_path, &options)?;
        eprintln!("Storage folders migrated");
        Ok(())
    }
}
