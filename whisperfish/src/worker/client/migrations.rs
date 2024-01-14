/// Migrations related to groupv2
mod groupv2;
/// Migration to remove R@ reactions and dump them in the correct table.
mod parse_reactions;
/// MIgration to initialize PNI
mod pni;
/// Migration to ensure our own UUID is known.
///
/// Installs before Whisperfish 0.6 do not have their own UUID present in settings.
mod whoami;

use self::groupv2::*;
use self::parse_reactions::*;
use self::pni::*;
use self::whoami::*;
use super::*;
use crate::store::migrations::session_to_db::SessionStorageMigration;
use actix::prelude::*;
use std::sync::Arc;
use tokio::sync::{Notify, RwLock};

#[derive(Clone)]
pub(super) struct MigrationCondVar {
    state: Arc<RwLock<MigrationState>>,
    notify: Arc<Notify>,
}

impl MigrationCondVar {
    pub fn new() -> Self {
        MigrationCondVar {
            state: Arc::new(RwLock::new(MigrationState::new())),
            notify: Arc::new(Notify::new()),
        }
    }
}

pub(super) struct MigrationState {
    pub whoami: bool,
    pub protocol_store_in_db: bool,
    pub sessions_have_uuid: bool,
    pub gv2_expected_ids: bool,
    pub self_profile_ready: bool,
    pub reactions_ready: bool,
    pub pni_distributed: bool,
}

impl MigrationState {
    fn new() -> MigrationState {
        MigrationState {
            whoami: false,
            protocol_store_in_db: false,
            sessions_have_uuid: false,
            gv2_expected_ids: false,
            self_profile_ready: false,
            reactions_ready: false,
            pni_distributed: false,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.whoami
            && self.protocol_store_in_db
            && self.sessions_have_uuid
            && self.gv2_expected_ids
            && self.self_profile_ready
            && self.reactions_ready
            && self.pni_distributed
    }
}

macro_rules! method_for_condition {
    ($method:ident : $state:ident -> $cond:expr) => {
        #[allow(dead_code)]
        pub fn $method(&self) -> impl Future<Output = ()> + 'static {
            let notify = self.notify.clone();
            let state = self.state.clone();

            async move {
                while {
                    let $state = state.read().await;
                    $cond
                } {
                    notify.notified().await;
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
            let notify = self.notify.clone();
            let state = self.state.clone();
            actix::spawn(async move {
                state.write().await.$var = true;
                notify.notify_waiters();
            });
        }
    };
}

impl MigrationCondVar {
    method_for_condition!(ready : state -> state.is_ready());
    method_for_condition!(self_uuid_is_known : state -> state.whoami);
    method_for_condition!(protocol_store_in_db);
    method_for_condition!(pni_distributed : state -> state.pni_distributed);

    notify_method_for_var!(notify_whoami -> whoami);
    notify_method_for_var!(notify_protocol_store_in_db -> protocol_store_in_db);
    notify_method_for_var!(notify_groupv2_expected_ids -> gv2_expected_ids);
    notify_method_for_var!(notify_self_profile_ready -> self_profile_ready);
    notify_method_for_var!(notify_reactions_ready -> reactions_ready);
    notify_method_for_var!(notify_pni_distributed -> pni_distributed);
}

impl ClientActor {
    pub(super) fn queue_migrations(ctx: &mut <Self as Actor>::Context) {
        ctx.notify(WhoAmI);
        ctx.notify(MoveSessionsToDatabase);
        ctx.notify(ComputeGroupV2ExpectedIds);
        ctx.notify(RefreshOwnProfile { force: false });
        ctx.notify(ParseOldReaction);
        // ctx.notify(InitializePni);
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
