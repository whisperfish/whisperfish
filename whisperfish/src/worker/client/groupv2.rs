use super::*;
use crate::store::{observer::PrimaryKey, GroupV2, TrustLevel};
use actix::prelude::*;
use diesel::prelude::*;
use libsignal_service::groups_v2::{self, *};
use qmeta_async::with_executor;
use tokio::io::AsyncWriteExt;
use whisperfish_store::NewMessage;

#[derive(Message)]
#[rtype(result = "()")]
/// Request group v2 metadata from server by session id
pub struct RequestGroupV2InfoBySessionId(pub i32);

#[derive(Message)]
#[rtype(result = "()")]
/// Request group v2 metadata from server
pub struct RequestGroupV2Info(pub GroupV2, pub [u8; zkgroup::GROUP_MASTER_KEY_LEN]);

impl ClientWorker {
    #[with_executor]
    #[tracing::instrument(skip(self))]
    pub fn refresh_group_v2(&self, session_id: usize) {
        tracing::trace!("Request to refresh group v2 by session id = {}", session_id);

        let client = self.actor.clone().unwrap();
        actix::spawn(async move {
            client
                .send(RequestGroupV2InfoBySessionId(session_id as _))
                .await
                .unwrap();
        });
    }
}

impl Handler<RequestGroupV2Info> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        RequestGroupV2Info(request, master_key): RequestGroupV2Info,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        let _span = tracing::info_span!("handle RequestGroupV2Info").entered();
        let storage = self.storage.clone().unwrap();
        let service_ids = self.service_ids().expect("whoami");

        let authenticated_service = self.authenticated_service();
        let zk_params = self.service_cfg().zkgroup_server_public_params;
        let group_id = request.secret.get_group_identifier();
        let group_id_hex = hex::encode(group_id);

        let client = ctx.address();

        Box::pin(
            async move {
                let mut credential_cache = storage.credential_cache_mut().await;
                let mut gm =
                    GroupsManager::new(service_ids, authenticated_service, &mut *credential_cache, zk_params);
                let group = gm.fetch_encrypted_group(&mut rand::thread_rng(), &master_key).await?;
                let group = groups_v2::decrypt_group(&master_key, group)?;
                // let group = gm.decrypt_
                // We now know the group's name and properties
                // XXX this is an assumption that we might want to check.
                let acl = group
                    .access_control
                    .as_ref()
                    .expect("access control present in DecryptedGroup");
                {
                    // XXX if the group does not exist, consider inserting here.
                    use whisperfish_store::schema::group_v2s::dsl::*;
                    diesel::update(group_v2s)
                        .set((
                            name.eq(&group.title),
                            description.eq(&group.description),
                            avatar.eq(if group.avatar.is_empty() {
                                None
                            } else {
                                Some(&group.avatar)
                            }),
                            // TODO: maybe rename the SQLite column to version
                            revision.eq(group.revision as i32),
                            invite_link_password.eq(&group.invite_link_password),
                            access_required_for_attributes.eq(i32::from(acl.attributes)),
                            access_required_for_members.eq(i32::from(acl.members)),
                            access_required_for_add_from_invite_link.eq(i32::from(acl.add_from_invite_link)),
                        ))
                        .filter(id.eq(&group_id_hex))
                        .execute(&mut *storage.db())
                        .expect("update groupv2 name");
                }

                if !group.avatar.is_empty() {
                    client.send(RefreshGroupAvatar(group_id_hex.clone())).await?;
                }

                {
                    let timeout = group
                        .disappearing_messages_timer
                        .as_ref()
                        .map(|d| d.duration as i32);
                    use whisperfish_store::schema::sessions::dsl::*;
                    diesel::update(sessions)
                        .set((expiring_message_timeout.eq(timeout),))
                        .filter(group_v2_id.eq(&group_id_hex))
                        .execute(&mut *storage.db())
                        .expect("update session disappearing_messages_timer");
                }
                storage.observe_update(whisperfish_store::schema::group_v2s::table, group_id_hex.clone());

                // We know the group's members.
                // First assert their existence in the database.
                // We can assert existence for members, pending members, and requesting members.
                let members_to_assert = group
                    .members
                    .iter()
                    .map(|member| (Aci::from(member.uuid).into(), Some(&member.profile_key)))
                    .chain(
                        group
                            .pending_members
                            .iter()
                            .map(|member| (member.address, None)),
                    )
                    .chain(
                        group
                            .requesting_members
                            .iter()
                            .map(|member| (member.aci.into(), Some(&member.profile_key))),
                    );

                // We need all the profile keys and UUIDs in the database.
                for (addr, profile_key) in members_to_assert {
                    let recipient = storage.fetch_or_insert_recipient_by_address(&addr);
                    if let Some(profile_key) = profile_key {
                        let (recipient, _was_changed) = storage.update_profile_key(recipient.e164.clone(), recipient.to_service_address(), &profile_key.get_bytes(), TrustLevel::Uncertain);
                        match recipient.profile_key {
                            Some(key) if key == profile_key.get_bytes() => {
                                tracing::trace!("Profile key matches server-stored profile key");
                            }
                            Some(_key) => {
                                // XXX trigger a profile key update message
                                tracing::warn!("Profile key does not match server-stored profile key.");
                            }
                            None => {
                                tracing::error!("Profile key None but tried to set.  This will probably crash a bit later.");
                            },
                        }
                    }
                }

                // Now the members are stored as recipient in the database.
                // Let's link them with the group in two steps (in one migration):
                // 1. Delete all existing memberships.
                // 2. Insert all memberships from the DecryptedGroup.
                let uuids = group.members.iter().map(|member| {
                    member.uuid.to_string()
                });
                storage.db().transaction::<(), diesel::result::Error, _>(|db| {
                    use whisperfish_store::schema::{group_v2_members, recipients, group_v2s};
                    let stale_members: Vec<i32> = group_v2_members::table
                        .select(group_v2_members::recipient_id)
                        .inner_join(recipients::table)
                        .filter(
                            recipients::uuid
                                .ne_all(uuids)
                                .and(group_v2_members::group_v2_id.eq(&group_id_hex)),
                        )
                        .load(db)?;
                    tracing::trace!("Have {} stale members", stale_members.len());
                    let dropped = diesel::delete(group_v2_members::table)
                        .filter(
                            group_v2_members::group_v2_id
                                .eq(&group_id_hex)
                                .and(group_v2_members::recipient_id.eq_any(&stale_members)),
                        )
                        .execute(db)?;
                    assert_eq!(
                        stale_members.len(),
                        dropped,
                        "didn't drop all stale members"
                    );
                    if dropped > 0 {
                        storage.observe_delete(group_v2_members::table, PrimaryKey::Unknown)
                            .with_relation(group_v2s::table, group_id_hex.clone());
                    }
                    Ok(())
                }).expect("dropping stale members");

                {
                    use whisperfish_store::schema::{group_v2_members, recipients, group_v2s};
                    for member in &group.members {
                        // XXX there's a bit of duplicate work going on here.
                        // XXX What about PNI?
                        let recipient =
                            storage.fetch_or_insert_recipient_by_address(&Aci::from(member.uuid).into());
                        let _span = tracing::trace_span!(
                            "Asserting member of the group",
                            %recipient
                        );

                        // Upsert in Diesel 2.0... Manually for now.
                        let membership: Option<orm::GroupV2Member> = group_v2_members::table
                            .filter(
                                group_v2_members::recipient_id
                                    .eq(recipient.id)
                                    .and(group_v2_members::group_v2_id.eq(&group_id_hex)),
                            )
                            .first(&mut *storage.db())
                            .optional()?;
                        if let Some(membership) = membership {
                            tracing::info!(%membership, "Member already in db. Updating membership.");
                            diesel::update(group_v2_members::table)
                                .set((group_v2_members::role.eq(member.role as i32),))
                                .filter(
                                    group_v2_members::recipient_id
                                        .eq(recipient.id)
                                        .and(group_v2_members::group_v2_id.eq(&group_id_hex)),
                                )
                                .execute(&mut *storage.db())?;
                            storage.observe_update(group_v2_members::table, PrimaryKey::Unknown)
                                .with_relation(group_v2s::table, group_id_hex.clone())
                                .with_relation(recipients::table, recipient.id);
                        } else {
                            tracing::info!("Member is new, inserting.");
                            diesel::insert_into(group_v2_members::table)
                                .values((
                                    group_v2_members::group_v2_id.eq(&group_id_hex.clone()),
                                    group_v2_members::recipient_id.eq(recipient.id),
                                    group_v2_members::joined_at_revision
                                        .eq(member.joined_at_revision as i32),
                                    group_v2_members::role.eq(member.role as i32),
                                ))
                                .execute(&mut *storage.db())?;
                            storage.observe_insert(group_v2_members::table, PrimaryKey::Unknown)
                                .with_relation(group_v2s::table, group_id_hex.clone())
                                .with_relation(recipients::table, recipient.id);
                        }
                    }
                }

                // XXX there's more stuff to store from the DecryptedGroup.

                let session = storage.fetch_session_by_group_v2_id(&group_id_hex).unwrap();

                storage.create_message(&NewMessage {
                    session_id: session.id,
                    sent: true,
                    is_read: true,
                    message_type: Some(MessageType::GroupChange),
                    ..NewMessage::new_outgoing()
                });

                Ok::<_, anyhow::Error>(group)
            }
            .instrument(tracing::info_span!("fetch group"))
            .into_actor(self)
            .map(|result, _act, _ctx| {
                let _group = match result {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::error!("Could not update group: {}", e);
                        return;
                    }
                };
                // XXX send notification of group update to UI for refresh.
            }),
        )
    }
}

impl Handler<RequestGroupV2InfoBySessionId> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        RequestGroupV2InfoBySessionId(sid): RequestGroupV2InfoBySessionId,
        ctx: &mut Self::Context,
    ) -> Self::Result {
        match self
            .storage
            .as_ref()
            .unwrap()
            .fetch_session_by_id(sid)
            .map(|s| s.r#type)
        {
            Some(orm::SessionType::GroupV2(group_v2)) => {
                let mut key_stack = [0u8; zkgroup::GROUP_MASTER_KEY_LEN];
                key_stack.clone_from_slice(&hex::decode(group_v2.master_key).expect("hex in db"));
                let key = GroupMasterKey::new(key_stack);
                let secret = GroupSecretParams::derive_from_master_key(key);

                let store_v2 = crate::store::GroupV2 {
                    secret,
                    revision: group_v2.revision as _,
                };
                ctx.notify(RequestGroupV2Info(store_v2, key_stack));
            }
            _ => {
                tracing::warn!("No group_v2 with session id {}", sid);
            }
        }
    }
}

/// Queue a force-refresh of a group avatar by group hex id
#[derive(Message)]
#[rtype(result = "()")]
pub struct RefreshGroupAvatar(String);

impl Handler<RefreshGroupAvatar> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        RefreshGroupAvatar(group_id): RefreshGroupAvatar,
        ctx: &mut Self::Context,
    ) {
        let _span =
            tracing::trace_span!("Received RefreshGroupAvatar({}), fetching.", group_id).entered();
        let storage = self.storage.clone().unwrap();
        let group = {
            match storage.fetch_session_by_group_v2_id(&group_id) {
                Some(r) => r.unwrap_group_v2().clone(),
                None => {
                    tracing::error!("No group with id {}", group_id);
                    return;
                }
            }
        };
        let (avatar, master_key) = match group.avatar {
            Some(avatar) => (avatar, group.master_key),
            None => {
                tracing::error!("Group without avatar; not refreshing avatar: {:?}", group);
                return;
            }
        };

        let service = self.authenticated_service();
        let zk_params = self.service_cfg().zkgroup_server_public_params;
        let service_ids = self.service_ids().expect("whoami");
        ctx.spawn(
            async move {
                let master_key = hex::decode(&master_key).expect("hex group key in db");
                let mut key_stack = [0u8; zkgroup::GROUP_MASTER_KEY_LEN];
                key_stack.clone_from_slice(master_key.as_ref());
                let key = GroupMasterKey::new(key_stack);
                let secret = GroupSecretParams::derive_from_master_key(key);

                let mut credential_cache = storage.credential_cache_mut().await;
                let mut gm =
                    GroupsManager::new(service_ids, service, &mut *credential_cache, zk_params);

                let avatar = gm.retrieve_avatar(&avatar, secret).await?;
                Ok((group_id, avatar))
            }
            .instrument(tracing::info_span!("fetch avatar"))
            .into_actor(self)
            .map(|res: anyhow::Result<_>, _act, ctx| {
                match res {
                    Ok((group_id, Some(avatar))) => {
                        ctx.notify(GroupAvatarFetched(group_id, avatar))
                    }
                    Ok((group_id, None)) => {
                        tracing::info!("No avatar for group {}", group_id);
                    }
                    Err(e) => {
                        tracing::error!("During avatar fetch: {}", e);
                    }
                };
            }),
        );
    }
}

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct GroupAvatarFetched(String, Vec<u8>);

impl Handler<GroupAvatarFetched> for ClientActor {
    type Result = ResponseActFuture<Self, ()>;

    fn handle(
        &mut self,
        GroupAvatarFetched(group_id, bytes): GroupAvatarFetched,
        _ctx: &mut Self::Context,
    ) -> Self::Result {
        let storage = self.storage.clone().unwrap();
        let _span = tracing::info_span!("handle GroupAvatarFetched", group_id).entered();
        Box::pin(
            async move {
                let settings = crate::config::SettingsBridge::default();
                let avatar_dir = settings.get_string("avatar_dir");
                let avatar_dir = Path::new(&avatar_dir);

                if !avatar_dir.exists() {
                    std::fs::create_dir(avatar_dir)?;
                }

                let out_path = avatar_dir.join(&group_id);

                let mut f = tokio::fs::File::create(out_path).await?;
                f.write_all(&bytes).await?;

                use whisperfish_store::schema;
                storage.observe_update(schema::group_v2s::table, group_id.clone());
                let session_id = storage.fetch_session_by_group_v2_id(&group_id).unwrap().id;
                storage
                    .observe_update(schema::sessions::table, session_id)
                    .with_relation(schema::group_v2s::table, group_id);

                Ok(())
            }
            .instrument(tracing::info_span!("save avatar"))
            .into_actor(self)
            .map(move |res: anyhow::Result<_>, _act, _ctx| {
                match res {
                    Ok(()) => {
                        // XXX this is basically incomplete.
                        // Storage should send out a recipient updated towards interested
                        // listeners.
                    }
                    Err(e) => {
                        tracing::warn!("Error with fetched avatar: {}", e);
                    }
                }
            }),
        )
    }
}

/// Types of post-GroupV2-update message types.
#[derive(PartialEq, Debug)]
enum GroupV2Trigger {
    ObserveUpdate,
    /// Avatar(GroupV2Id)
    Avatar(String),
    Recipient(Uuid),
}

/// Handle an incoming group change message
#[derive(Message)]
#[rtype(result = "()")]
pub struct GroupV2Update(pub GroupContextV2, pub orm::Session);

impl Handler<GroupV2Update> for ClientActor {
    type Result = ();

    fn handle(
        &mut self,
        GroupV2Update(group_v2_ctx, session): GroupV2Update,
        ctx: &mut Self::Context,
    ) {
        let storage = self.storage.clone().unwrap();
        let _span = tracing::info_span!("handle GroupV2Update for session", session.id).entered();

        let service = self.authenticated_service();
        let zk_params = self.service_cfg().zkgroup_server_public_params;
        let service_ids = self.service_ids().expect("whoami");
        ctx.spawn(
            async move {
                let mut db_triggers: Vec<GroupV2Trigger> = Vec::new();
                let mut ctx_triggers: Vec<GroupV2Trigger> = Vec::new();

                let mut credential_cache = storage.credential_cache_mut().await;
                let gm =
                    GroupsManager::new(service_ids, service, &mut *credential_cache, zk_params);

                let changes = gm.decrypt_group_context(group_v2_ctx);
                if changes.is_err() {
                    return Err(changes.unwrap_err());
                }

                if let Some(GroupChanges {
                    // TODO: Propagate editor to QML
                    editor: _editor,
                    revision,
                    changes,
                }) = changes.unwrap()
                {
                    tracing::debug!(
                        "Group (session {}) has {} update(s)",
                        session.id,
                        changes.len()
                    );
                    let group_v2 = session.unwrap_group_v2();
                    for change in changes {
                        match change {
                            GroupChange::AnnouncementOnly(announcement_only) => {
                                // TODO: Database migration (announcement_only)
                                tracing::info!("Announcement: {}", announcement_only);
                            }
                            GroupChange::AttributeAccess(access) => {
                                tracing::info!("Attribute access: {:?}", access);
                            }
                            GroupChange::Avatar(avatar) => {
                                tracing::debug!("Avatar: {:?}", avatar);
                                storage.update_group_v2_avatar(&group_v2, Some(&avatar));
                                ctx_triggers.push(GroupV2Trigger::Avatar(group_v2.id.clone()));
                            }
                            GroupChange::AddBannedMember(banned_member) => {
                                tracing::info!("Add banned member: {:?}", banned_member);
                            }
                            GroupChange::DeleteBannedMember(uuid) => {
                                tracing::info!("Delete banned member: {:?}", uuid);
                            }
                            GroupChange::DeleteMember(uuid) => {
                                tracing::debug!("Delete member: {:?}", uuid);
                                storage.delete_group_v2_member(&group_v2, uuid.into());
                                db_triggers.push(GroupV2Trigger::ObserveUpdate);
                            }
                            GroupChange::DeletePendingMember(member) => {
                                // TODO: Database migration (status)
                                tracing::info!("Delete pending member: {:?}", member);
                            }
                            GroupChange::DeleteRequestingMember(member) => {
                                // TODO: Database migration (status)
                                tracing::info!("Delete requesting member: {:?}", member);
                            }
                            GroupChange::Description(description) => {
                                tracing::debug!("Description: {:?}", description);
                                storage
                                    .update_group_v2_description(&group_v2, description.as_ref());
                                db_triggers.push(GroupV2Trigger::ObserveUpdate);
                            }
                            GroupChange::InviteLinkAccess(access) => {
                                tracing::info!("Invite link access: {:?}", access);
                            }
                            GroupChange::InviteLinkPassword(password) => {
                                tracing::debug!("Invite link password: {:?}", password);
                                storage.update_group_v2_invite_link_password(&group_v2, &password);
                                // TODO: Reftect in UI
                                db_triggers.push(GroupV2Trigger::ObserveUpdate);
                            }
                            GroupChange::MemberAccess(access) => {
                                tracing::info!("Member access: {:?}", access);
                            }
                            GroupChange::ModifyMemberProfileKey { uuid, profile_key } => {
                                tracing::info!(
                                    "Modify member profile key: {:?} {:?}",
                                    uuid,
                                    profile_key
                                );
                            }
                            GroupChange::ModifyMemberRole { aci, role } => {
                                tracing::debug!("Modify member role: {:?} {:?}", aci, role);
                                if let Some(updated) = storage.update_group_v2_member_role(group_v2, aci, role) {
                                    db_triggers.push(GroupV2Trigger::Recipient(updated.uuid.unwrap()));
                                }
                            }
                            GroupChange::NewMember(member) => {
                                tracing::debug!("New member: {:?}", member);
                                if let Some((_, added)) = storage.add_group_v2_member(
                                    &group_v2,
                                    member.uuid.into(),
                                    member.role,
                                    &member.profile_key,
                                    member.joined_at_revision as i32,
                                    None,
                                ) {
                                    db_triggers.push(GroupV2Trigger::Recipient(added.uuid.unwrap()));
                                }
                            }
                            GroupChange::NewPendingMember(member) => {
                                // TODO: Database migration (status)
                                tracing::info!("New pending member: {:?}", member);
                            }
                            GroupChange::NewRequestingMember(member) => {
                                // TODO: Database migration (status)
                                tracing::info!("New requesting member: {:?}", member);
                            }
                            GroupChange::PromotePendingPniAciMemberProfileKey(promoted_member) => {
                                tracing::info!(
                                    "Promote pending PNI member profile key: {:?}",
                                    promoted_member
                                );
                            }
                            GroupChange::PromotePendingMember {
                                address,
                                profile_key,
                            } => {
                                tracing::info!(
                                    "Promote pending member: {:?} {:?}",
                                    address,
                                    profile_key
                                );
                            }
                            GroupChange::PromoteRequestingMember { aci, role } => {
                                tracing::info!("Promote requesting member: {:?} {:?}", aci, role);
                            }
                            GroupChange::Timer(timer) => {
                                tracing::debug!("Timer: {:?}", timer);
                                storage.update_expiration_timer(
                                    &session,
                                    timer.map(|t| t.duration),
                                    None,
                                );
                                db_triggers.push(GroupV2Trigger::ObserveUpdate);
                            }
                            GroupChange::Title(title) => {
                                tracing::debug!("Title: {:?}", title);
                                storage.update_group_v2_title(group_v2, &title);
                                db_triggers.push(GroupV2Trigger::ObserveUpdate);
                            }
                        }
                    }

                    for trigger in db_triggers {
                        match trigger {
                            GroupV2Trigger::Recipient(uuid) => {
                                let aci: Aci = uuid.into();
                                let service_id: ServiceId = aci.into();
                                let recipient = storage.fetch_recipient(&service_id).unwrap();
                                storage.observe_update(
                                    whisperfish_store::schema::recipients::table,
                                    recipient.id,
                                );
                                storage
                                    .observe_update(
                                        whisperfish_store::schema::sessions::table,
                                        session.id,
                                    )
                                    .with_relation(
                                        whisperfish_store::schema::recipients::table,
                                        recipient.id,
                                    );
                            }
                            GroupV2Trigger::ObserveUpdate => {
                                storage.update_group_v2_revision(&group_v2, revision as i32);
                                break;
                            }
                            _ => {
                                tracing::error!(
                                    "Unexpected trigger in database triggers, ignoring: {:?}",
                                    trigger
                                );
                                continue;
                            }
                        }
                    }

                    Ok((ctx_triggers, session.id))
                } else {
                    tracing::warn!("Group change message with no changes");
                    Ok((Vec::new(), session.id))
                }
            }
            .instrument(tracing::info_span!(""))
            .into_actor(self)
            .map(|res, _act, ctx| {
                match res {
                    Ok((triggers, s_id)) => {
                        // XXX handle group.group_change like a real client
                        if triggers.is_empty() {
                            tracing::warn!("Unhandled group change, fallback to full refresh");
                            ctx.notify(RequestGroupV2InfoBySessionId(s_id));
                        } else {
                            for trigger in triggers {
                                match trigger {
                                    GroupV2Trigger::Avatar(group_v2_id) => {
                                        ctx.notify(RefreshGroupAvatar(group_v2_id));
                                    }
                                    GroupV2Trigger::ObserveUpdate | GroupV2Trigger::Recipient(_) => {
                                        tracing::error!(
                                            "Unexpected trigger in context triggers, ignoring: {:?}",
                                            trigger
                                        );
                                    }
                            }
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("{}", e);
                    }
                };
            }),
        );
    }
}
