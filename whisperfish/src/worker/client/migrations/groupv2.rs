use super::*;
use actix::prelude::*;
use diesel::prelude::*;

/// Find GroupV1 sessions without v2 pending id,
/// and populate that field.
#[derive(Message)]
#[rtype(result = "()")]
pub struct ComputeGroupV2ExpectedIds;

impl Handler<ComputeGroupV2ExpectedIds> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;
    fn handle(&mut self, _: ComputeGroupV2ExpectedIds, _ctx: &mut Self::Context) -> Self::Result {
        let storage = self.storage.clone().unwrap();

        Box::pin(
            async move {
                use crate::store::schema::group_v1s::dsl::*;
                let pending_ids: Vec<String> = {
                    group_v1s
                        .select(id)
                        .filter(expected_v2_id.is_null())
                        .load(&mut *storage.db())
                        .expect("db")
                };
                for pending_v1_id_hex in pending_ids {
                    let pending_v1_id =
                        hex::decode(&pending_v1_id_hex).expect("correct hex values in db");
                    if pending_v1_id.len() != 16 {
                        tracing::warn!("Illegal group ID in db");
                        continue;
                    }

                    let master_key =
                        libsignal_service::groups_v2::utils::derive_v2_migration_master_key(
                            &pending_v1_id,
                        )
                        .expect("signal protocol library");
                    let secret = GroupSecretParams::derive_from_master_key(master_key);
                    let pending_v2_id = secret.get_group_identifier();
                    let pending_v2_id = hex::encode(pending_v2_id);

                    let mut db = storage.db();
                    let affected = diesel::update(group_v1s)
                        .set(expected_v2_id.eq(pending_v2_id))
                        .filter(id.eq(pending_v1_id_hex))
                        .execute(&mut *db)
                        .expect("db");
                    assert_eq!(affected, 1, "update groupv1 expected upgrade id");
                }
            }
            .instrument(tracing::debug_span!("compute groupv2 expected IDs"))
            .into_actor(self)
            .map(|(), act, _| act.migration_state.notify_groupv2_expected_ids()),
        )
    }
}
