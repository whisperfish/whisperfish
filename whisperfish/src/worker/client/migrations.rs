/// Migrations related to groupv2
mod groupv2;
/// Migration to ensure the primary device Whisperfish has master key and storage service key
mod master_key;
/// Migration to remove R@ reactions and dump them in the correct table.
mod parse_reactions;
/// Migration to initialize PNI
mod pni;
/// Migration to ensure our own UUID is known.
///
/// Installs before Whisperfish 0.6 do not have their own UUID present in settings.
mod whoami;

use self::groupv2::*;
use self::master_key::*;
use self::parse_reactions::*;
use self::pni::*;
use self::whoami::*;
use super::*;
use crate::store::migrations::session_to_db::SessionStorageMigration;
use actix::prelude::*;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

#[derive(Clone)]
pub(super) struct MigrationCondVar {
    state: Arc<RwLock<MigrationState>>,
    sender: Arc<broadcast::Sender<()>>,
}

impl MigrationCondVar {
    pub fn new() -> Self {
        MigrationCondVar {
            state: Arc::new(RwLock::new(MigrationState::new())),
            sender: Arc::new(broadcast::Sender::new(1)),
        }
    }
}

pub(super) struct MigrationState {
    pub whoami: bool,
    pub protocol_store_in_db: bool,
    pub gv2_expected_ids: bool,
    pub self_profile_ready: bool,
    pub reactions_ready: bool,
    pub pni_distributed: bool,
    pub check_master_key: bool,
}

impl MigrationState {
    fn new() -> MigrationState {
        MigrationState {
            whoami: false,
            protocol_store_in_db: false,
            gv2_expected_ids: false,
            self_profile_ready: false,
            reactions_ready: false,
            pni_distributed: false,
            check_master_key: false,
        }
    }

    /// Signals true if all migrations are complete.
    #[tracing::instrument(skip(self))]
    pub fn is_ready(&self) -> bool {
        tracing::trace!(
            whoami = %self.whoami,
            protocol_store_in_db = %self.protocol_store_in_db,
            gv2_expected_ids = %self.gv2_expected_ids,
            self_profile_ready = %self.self_profile_ready,
            reactions_ready = %self.reactions_ready,
            pni_distributed = %self.pni_distributed,
            check_master_key = %self.check_master_key,
            "is_ready",
        );
        self.whoami
            && self.protocol_store_in_db
            && self.gv2_expected_ids
            && self.self_profile_ready
            && self.reactions_ready
            && self.pni_distributed
            && self.check_master_key
    }

    /// Signals true if the client is ready to connect.
    #[tracing::instrument(skip(self))]
    pub fn connectable(&self) -> bool {
        tracing::trace!(
            whoami = %self.whoami,
            protocol_store_in_db = %self.protocol_store_in_db,
            gv2_expected_ids = %self.gv2_expected_ids,
            self_profile_ready = %self.self_profile_ready,
            "connectable",
        );
        self.whoami && self.protocol_store_in_db && self.gv2_expected_ids && self.self_profile_ready
    }
}

macro_rules! method_for_condition {
    ($method:ident : $state:ident -> $cond:expr) => {
        pub fn $method(&self) -> impl Future<Output = ()> + 'static {
            let mut receiver = self.sender.clone().subscribe();
            let state = self.state.clone();

            async move {
                while {
                    let $state = state.read().await;
                    !$cond
                } {
                    match receiver.recv().await {
                        Ok(_)=> {},
                        Err(broadcast::error::RecvError::Closed) => {
                            panic!("MigrationCondVar sender closed");
                        }
                        Err(broadcast::error::RecvError::Lagged(_)) => {
                            // tracing::warn!("MigrationCondVar lagged");
                            // D/c
                        }
                    }
                }
            }
        }
    };
    ($name:ident) => {
        method_for_condition!($name : state -> state.$name);
    }
}

macro_rules! notify_method_for_var {
    ($method:ident -> $var:ident) => {
        pub fn $method(&self) {
            let sender = self.sender.clone();
            let state = self.state.clone();
            actix::spawn(async move {
                state.write().await.$var = true;
                sender.send(()).ok(); // XXX: handle error?
            });
        }
    };
}

impl MigrationCondVar {
    // method_for_condition!(ready : state -> state.is_ready());
    method_for_condition!(connectable : state -> state.connectable());
    method_for_condition!(self_uuid_is_known : state -> state.whoami);
    // method_for_condition!(protocol_store_in_db);
    method_for_condition!(pni_distributed : state -> state.pni_distributed);

    notify_method_for_var!(notify_whoami -> whoami);
    notify_method_for_var!(notify_protocol_store_in_db -> protocol_store_in_db);
    notify_method_for_var!(notify_groupv2_expected_ids -> gv2_expected_ids);
    notify_method_for_var!(notify_self_profile_ready -> self_profile_ready);
    notify_method_for_var!(notify_reactions_ready -> reactions_ready);
    notify_method_for_var!(notify_pni_distributed -> pni_distributed);
    notify_method_for_var!(notify_check_master_key -> check_master_key);
}

impl ClientActor {
    pub(super) fn queue_migrations(ctx: &mut <Self as Actor>::Context) {
        ctx.notify(WhoAmI);
        ctx.notify(MoveSessionsToDatabase);
        ctx.notify(ComputeGroupV2ExpectedIds);
        ctx.notify(RefreshOwnProfile { force: false });
        ctx.notify(ParseOldReaction);
        ctx.notify(InitializePni);
        ctx.notify(CheckMasterKey);
    }
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct MoveSessionsToDatabase;

impl Handler<MoveSessionsToDatabase> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: MoveSessionsToDatabase, _ctx: &mut Self::Context) -> Self::Result {
        let storage = self.storage.clone().expect("initialized storage");

        let proc = async move {
            let migration = SessionStorageMigration(storage.clone());
            migration.execute().await;
        };

        Box::pin(
            proc.into_actor(self)
                .map(|_, act, _| act.migration_state.notify_protocol_store_in_db()),
        )
    }
}
