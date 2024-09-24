#![allow(non_snake_case)]

use std::collections::HashMap;

use crate::model::*;
use crate::store::observer::{EventObserving, Interest};
use crate::store::Storage;
use qmetaobject::prelude::*;
use whisperfish_store::schema;
use whisperfish_store::store::orm;

/// QML-constructable object that interacts with a single message.
#[observing_model]
#[derive(Default, QObject)]
pub struct Reactions {
    base: qt_base_class!(trait QObject),
    message_id: Option<i32>,

    #[qt_property(
        READ: get_message_id,
        WRITE: set_message_id,
        NOTIFY: reactions_changed,
    )]
    messageId: i32,
    #[qt_property(READ: get_valid, NOTIFY: reactions_changed)]
    valid: bool,
    #[qt_property(READ: reactions, NOTIFY: reactions_model_changed)]
    reactions: QVariant,
    #[qt_property(READ: reaction_count, NOTIFY: reactions_changed)]
    count: i32,

    reaction_list: QObjectBox<ReactionListModel>,

    reactions_changed: qt_signal!(),
    reactions_model_changed: qt_signal!(),
}

impl EventObserving for Reactions {
    type Context = ModelContext<Self>;

    fn observe(&mut self, ctx: Self::Context, event: crate::store::observer::Event) {
        if let Some(message_id) = self.message_id {
            self.reaction_list
                .pinned()
                .borrow_mut()
                .observe(ctx, message_id, event);
        }
    }

    fn interests(&self) -> Vec<Interest> {
        self.message_id
            .into_iter()
            .map(|id| {
                Interest::whole_table_with_relation(
                    schema::reactions::table,
                    schema::messages::table,
                    id,
                )
            })
            .chain(
                self.reaction_list
                    .pinned()
                    .borrow()
                    .reactions
                    .iter()
                    .flat_map(|(reaction, recipient)| {
                        reaction.interests().chain(recipient.interests())
                    }),
            )
            .collect()
    }
}

define_model_roles! {
    pub(super) enum ReactionRoles for orm::Reaction [with offset 100] {
        Id(reaction_id): "id",
        MessageId(message_id): "messageId",
        Author(author): "authorRecipientId",
        Reaction(emoji via QString::from): "reaction",
        SentTime(sent_time via qdatetime_from_naive): "sentTime",
        ReceivedTime(received_time via qdatetime_from_naive): "receivedTime",
    }
}

impl Reactions {
    fn get_message_id(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
        self.message_id.unwrap_or(-1)
    }

    fn get_valid(&self, _ctx: Option<ModelContext<Self>>) -> bool {
        self.message_id.is_some()
    }

    fn reaction_count(&self, _ctx: Option<ModelContext<Self>>) -> i32 {
        self.reaction_list.pinned().borrow().row_count()
    }

    fn fetch(&mut self, storage: Storage, id: i32) {
        self.reaction_list
            .pinned()
            .borrow_mut()
            .load_all(storage, id);
    }

    fn set_message_id(&mut self, ctx: Option<ModelContext<Self>>, id: i32) {
        self.message_id = Some(id);
        if let Some(ctx) = ctx {
            self.fetch(ctx.storage(), id);
        }
    }

    fn init(&mut self, ctx: ModelContext<Self>) {
        if let Some(id) = self.message_id {
            self.fetch(ctx.storage(), id);
            self.reactions_changed();
        }
    }

    fn reactions(&self, _ctx: Option<ModelContext<Self>>) -> QVariant {
        self.reaction_list.pinned().into()
    }
}

#[derive(QObject, Default)]
pub struct ReactionListModel {
    base: qt_base_class!(trait QAbstractListModel),
    reactions: Vec<(orm::Reaction, orm::Recipient)>,
}

impl ReactionListModel {
    fn load_all(&mut self, storage: Storage, message_id: i32) {
        self.begin_reset_model();
        self.reactions = storage.fetch_reactions_for_message(message_id);
        self.end_reset_model();
    }

    #[tracing::instrument(level = "trace", skip(self, ctx))]
    fn observe(
        &mut self,
        ctx: ModelContext<Reactions>,
        message_id: i32,
        _event: crate::store::observer::Event,
    ) {
        self.load_all(ctx.storage(), message_id);
    }
}

impl QAbstractListModel for ReactionListModel {
    fn row_count(&self) -> i32 {
        self.reactions.len() as i32
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        const OFFSET: i32 = 100;
        if role > OFFSET {
            let role = ReactionRoles::from(role - OFFSET);
            role.get(&self.reactions[index.row() as usize].0)
        } else {
            let role = RecipientRoles::from(role);
            role.get(&self.reactions[index.row() as usize].1)
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        ReactionRoles::role_names()
            .into_iter()
            .chain(RecipientRoles::role_names())
            .collect()
    }
}
