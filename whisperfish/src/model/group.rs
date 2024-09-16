#![allow(non_snake_case)]

use std::collections::HashMap;

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::orm::{GroupV1Member, GroupV2Member};
use crate::store::Storage;
use qmeta_async::with_executor;
use qmetaobject::prelude::*;
use whisperfish_store::schema;
use whisperfish_store::store::orm;

/// QML-constructable object that interacts with a single recipient.
#[observing_model]
#[derive(Default, QObject)]
pub struct Group {
    base: qt_base_class!(trait QObject),
    id: Option<String>,
    group_v1: Option<orm::GroupV1>,
    group_v2: Option<orm::GroupV2>,

    #[qt_property(
        READ: get_group_id,
        WRITE: set_group_id,
        NOTIFY: group_changed,
    )]
    groupId: QString,
    #[qt_property(READ: get_is_group_v1, NOTIFY: group_changed)]
    isGroupV1: bool,
    #[qt_property(READ: get_is_group_v2, NOTIFY: group_changed)]
    isGroupV2: bool,

    #[qt_property(READ: get_valid, NOTIFY: group_changed)]
    valid: bool,

    #[qt_property(READ: members, NOTIFY: group_changed)]
    members: QVariant,

    #[qt_property(READ: member_count, NOTIFY: group_changed)]
    member_count: i32,

    membership_list: QObjectBox<GroupMembershipListModel>,

    group_changed: qt_signal!(),
}

impl EventObserving for Group {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, _event: crate::store::observer::Event) {
        if self.id.is_some() {
            self.init(ctx);
        }
    }

    fn interests(&self) -> Vec<Interest> {
        let membership_list = self.membership_list.pinned();
        let new_members = self.id.iter().flat_map(|id| {
            if id.len() == 32 {
                Some(Interest::whole_table_with_relation(
                    schema::group_v1_members::table,
                    schema::group_v1s::table,
                    id.clone(),
                ))
            } else if id.len() == 64 {
                Some(Interest::whole_table_with_relation(
                    schema::group_v2_members::table,
                    schema::group_v2s::table,
                    id.clone(),
                ))
            } else {
                None
            }
        });
        let members = new_members.chain(
            membership_list
                .borrow()
                .content
                .iter()
                .flat_map(|(_membership, recipient)| recipient.interests()),
        );
        self.group_v1
            .iter()
            .flat_map(orm::GroupV1::interests)
            .chain(self.group_v2.iter().flat_map(orm::GroupV2::interests))
            .chain(members)
            .collect()
    }
}

impl Group {
    fn get_group_id(&self, _ctx: Option<ModelContext<Self>>) -> QString {
        self.id.clone().unwrap_or_default().into()
    }

    fn get_is_group_v1(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.group_v1.is_some()
    }

    fn get_is_group_v2(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.group_v2.is_some()
    }

    fn member_count(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
        self.membership_list.pinned().borrow_mut().row_count()
    }

    fn get_valid(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.id.is_some() && (self.group_v1.is_some() || self.group_v2.is_some())
    }

    fn members(&self, _ctx: Option<ModelContext<Self>>) -> QVariant {
        self.membership_list.pinned().into()
    }

    #[with_executor]
    #[tracing::instrument(skip(self, ctx))]
    fn set_group_id(&mut self, ctx: Option<ModelContext<Self>>, id: QString) {
        self.id = Some(id.to_string());
        if let Some(ctx) = ctx {
            self.init(ctx);
        }
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        let storage = ctx.storage();
        if let Some(id) = &self.id {
            self.group_v1 = None;
            self.group_v2 = None;
            if id.len() == 32 {
                self.group_v1 = storage.fetch_group_by_group_v1_id(id);
                self.membership_list
                    .pinned()
                    .borrow_mut()
                    .load_v1(storage, id);
            } else if id.len() == 64 {
                self.group_v2 = storage.fetch_group_by_group_v2_id(id);
                self.membership_list
                    .pinned()
                    .borrow_mut()
                    .load_v2(storage, id);
            } else {
                self.membership_list.pinned().borrow_mut().clear();
            }
        }
    }
}

pub enum GroupMembership {
    V1(GroupV1Member),
    V2(GroupV2Member),
}

impl GroupMembership {
    fn member_since(&self) -> Option<NaiveDateTime> {
        match self {
            Self::V1(v1) => v1.member_since,
            Self::V2(v2) => Some(v2.member_since),
        }
    }

    fn role(&self) -> i32 {
        match self {
            Self::V1(_v1) => -1,
            Self::V2(v2) => v2.role,
        }
    }
}

#[derive(QObject, Default)]
pub struct GroupMembershipListModel {
    base: qt_base_class!(trait QAbstractListModel),
    content: Vec<(GroupMembership, orm::Recipient)>,
}

impl GroupMembershipListModel {
    fn load_v1(&mut self, storage: Storage, id: &str) {
        self.begin_reset_model();
        self.content = storage
            .fetch_group_members_by_group_v1_id(id)
            .into_iter()
            .map(|(membership, member)| (GroupMembership::V1(membership), member))
            .collect();
        self.end_reset_model();
    }

    fn load_v2(&mut self, storage: Storage, id: &str) {
        self.begin_reset_model();
        self.content = storage
            .fetch_group_members_by_group_v2_id(id)
            .into_iter()
            .map(|(membership, member)| (GroupMembership::V2(membership), member))
            .collect();
        self.end_reset_model();
    }

    fn clear(&mut self) {
        self.begin_reset_model();
        self.content.clear();
        self.end_reset_model();
    }
}

define_model_roles! {
    enum GroupMembershipRoles for GroupMembership [with offset 100] {
        MemberSince(fn member_since(&self) via qdatetime_from_naive_option): "memberSince",
        Role(fn role(&self)): "role",
    }
}

impl QAbstractListModel for GroupMembershipListModel {
    fn row_count(&self) -> i32 {
        self.content.len() as _
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        const OFFSET: i32 = 100;
        if role > OFFSET {
            let role = GroupMembershipRoles::from(role - OFFSET);
            role.get(&self.content[index.row() as usize].0)
        } else {
            let role = RecipientRoles::from(role);
            role.get(&self.content[index.row() as usize].1)
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        GroupMembershipRoles::role_names()
            .into_iter()
            .chain(RecipientRoles::role_names())
            .collect()
    }
}
