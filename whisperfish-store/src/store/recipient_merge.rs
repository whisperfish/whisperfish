use crate::diesel::connection::SimpleConnection;
use crate::orm;
use crate::orm::Recipient;
use crate::schema;
use crate::TrustLevel;
use diesel::prelude::*;
use libsignal_service::prelude::*;
use phonenumber::PhoneNumber;
use std::fmt::Debug;
use uuid::Uuid;

#[derive(Debug)]
enum RecipientOperation {
    SetPni(i32, Option<Uuid>),
    SetAci(i32, Option<Uuid>),
    SetE164(i32, Option<PhoneNumber>),
    Merge(i32, i32),
    /// ACI, PNI, E.164
    Create(Option<Uuid>, Option<Uuid>, Option<PhoneNumber>),
}

struct MergeRecipients {
    pub by_aci: Option<Recipient>,
    pub by_e164: Option<Recipient>,
    pub by_pni: Option<Recipient>,
}

#[derive(Default)]
pub struct RecipientResults {
    pub id: Option<i32>,
    // XXX Maybe make these Aci/Pni strong types
    pub aci: Option<Uuid>,
    pub pni: Option<Uuid>,
    pub e164: Option<PhoneNumber>,
    pub changed: bool,
}

fn fetch_separate_recipients(
    db: &mut SqliteConnection,
    aci: Option<&Uuid>,
    pni: Option<&Uuid>,
    e164: Option<&PhoneNumber>,
) -> Result<MergeRecipients, diesel::result::Error> {
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

    Ok(MergeRecipients {
        by_aci,
        by_pni,
        by_e164,
    })
}

#[tracing::instrument(
    skip(db, e164),
    fields(
        aci = aci.as_ref().map_or("None".into(), |u| u.to_string()),
        pni = pni.as_ref().map_or("None".into(), |u| u.to_string()),
        e164 = e164.as_ref().map_or("None".into(), |u| u.to_string()),
    ))]
pub fn merge_and_fetch_recipient_inner(
    db: &mut SqliteConnection,
    e164: Option<PhoneNumber>,
    aci: Option<Uuid>,
    pni: Option<Uuid>,
    trust_level: TrustLevel,
    change_self: bool,
) -> Result<RecipientResults, diesel::result::Error> {
    if e164.is_none() && aci.is_none() && pni.is_none() {
        panic!("merge_and_fetch_recipient requires at least one of e164 or uuid");
    }

    let criteria_count = [e164.is_some(), aci.is_some(), pni.is_some()]
        .into_iter()
        .filter(|c| *c)
        .count();
    let merge = fetch_separate_recipients(db, aci.as_ref(), pni.as_ref(), e164.as_ref())?;
    let (by_aci, by_pni, by_e164) = (merge.by_aci, merge.by_pni, merge.by_e164);

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
            return Ok(RecipientResults {
                id: Some(common.id),
                ..Default::default()
            });
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
            RecipientOperation::Merge(id, into_id) => merge_recipients_inner(db, id, into_id),
            RecipientOperation::SetPni(id, pni) => set_pni_inner(db, id, pni.as_ref()),
            RecipientOperation::SetAci(id, aci) => set_aci_inner(db, id, aci.as_ref()),
            RecipientOperation::SetE164(id, e164) => set_e164_inner(db, id, e164.as_ref()),
            RecipientOperation::Create(aci, pni, e164) => insert_recipient_inner(db, aci.as_ref(), pni.as_ref(), e164.as_ref()),
        }?;
    }

    // Fetch new results after migration
    let merge = fetch_separate_recipients(db, aci.as_ref(), pni.as_ref(), e164.as_ref())?;
    let (new_by_aci, new_by_pni, new_by_e164) = (merge.by_aci, merge.by_pni, merge.by_e164);

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
        return Ok(RecipientResults::default());
    }

    // This is why the order matters - we return the first match.
    let rcpt = by_all[0];

    if aci.is_some() && new_by_aci.is_none() {
        // XXX && (change_self || not_self)
        set_aci_inner(db, rcpt.id, aci.as_ref())?;
        // XXX session switchover event?
    }

    if e164.is_some() && new_by_e164.is_none() {
        // XXX && (change_self || not_self)
        set_e164_inner(db, rcpt.id, e164.as_ref())?;
    }

    if pni.is_some() && new_by_pni.is_none() {
        set_pni_inner(db, rcpt.id, pni.as_ref())?;
    }

    if new_by_pni.is_some() && pni != new_by_pni.as_ref().unwrap().pni {
        // XXX session switchover event
    }

    if new_by_aci.is_some() && aci != new_by_aci.as_ref().unwrap().uuid {
        // XXX session switchover event
    }

    Ok(RecipientResults {
        id: Some(rcpt.id),
        ..Default::default()
    })
}

// Inner method because the coverage report is then sensible.
#[tracing::instrument(skip(db))]
pub fn merge_recipients_inner(
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
    diesel::update(recipients::table)
        .filter(recipients::id.eq(rcpt_id))
        .set(recipients::pni.eq(rcpt_pni.map(Uuid::to_string)))
        .returning(recipients::id)
        .get_result(db)
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
    diesel::update(recipients::table)
        .filter(recipients::id.eq(rcpt_id))
        .set(recipients::uuid.eq(rcpt_aci.map(Uuid::to_string)))
        .returning(recipients::id)
        .get_result(db)
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
    diesel::update(recipients::table)
        .filter(recipients::id.eq(rcpt_id))
        .set(recipients::e164.eq(rcpt_e164.map(PhoneNumber::to_string)))
        .returning(recipients::id)
        .get_result(db)
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
    diesel::insert_into(recipients::table)
        .values((
            recipients::uuid.eq(rcpt_aci.map(Uuid::to_string)),
            recipients::pni.eq(rcpt_pni.map(Uuid::to_string)),
            recipients::e164.eq(rcpt_e164.map(PhoneNumber::to_string)),
        ))
        .returning(recipients::id)
        .get_result(db)
}
