use crate::{schema, store::orm};

use super::Interest;

impl orm::Session {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::sessions::table, self.id))
        // TODO:
        // - If group, watch the group
        // - If 1:1, watch the recipient
    }
}

impl orm::AugmentedSession {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        self.inner
            .interests()
            .chain(
                self.last_message
                    .iter()
                    .flat_map(orm::AugmentedMessage::interests),
            )
            // Watch new group members
            .chain(match &self.inner.r#type {
                orm::SessionType::DirectMessage(_) => None,
                orm::SessionType::GroupV1(g) => Some(Interest::whole_table_with_relation(
                    schema::group_v1_members::table,
                    schema::group_v1s::table,
                    g.id.clone(),
                )),
                orm::SessionType::GroupV2(g) => Some(Interest::whole_table_with_relation(
                    schema::group_v2_members::table,
                    schema::group_v2s::table,
                    g.id.clone(),
                )),
            })
            .chain(std::iter::once(Interest::whole_table_with_relation(
                schema::messages::table,
                schema::sessions::table,
                self.id,
            )))
    }
}

impl orm::Recipient {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::recipients::table, self.id))
    }
}

impl orm::AugmentedMessage {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        self.inner
            .interests()
            .chain(std::iter::once(Interest::whole_table_with_relation(
                schema::attachments::table,
                schema::messages::table,
                self.id,
            )))
            .chain(std::iter::once(Interest::whole_table_with_relation(
                schema::reactions::table,
                schema::messages::table,
                self.id,
            )))
            .chain(std::iter::once(Interest::whole_table_with_relation(
                schema::receipts::table,
                schema::messages::table,
                self.id,
            )))
    }
}

impl orm::Message {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::messages::table, self.id))
    }
}

impl orm::Attachment {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::attachments::table, self.id))
    }
}

impl orm::Reaction {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::reactions::table, self.reaction_id))
    }
}

impl orm::GroupedReaction {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        // XXX: We should register interest for message_id + the specific emoji
        std::iter::once(Interest::whole_table_with_relation(
            schema::reactions::table,
            schema::messages::table,
            self.message_id,
        ))
    }
}

impl orm::Receipt {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        // XXX This is a composite primary key, but we're only watching one foreign key
        std::iter::once(Interest::whole_table_with_relation(
            schema::receipts::table,
            schema::messages::table,
            self.message_id,
        ))
    }
}

impl orm::GroupV1 {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::group_v1s::table, self.id.clone()))
    }
}

impl orm::GroupV2 {
    pub fn interests(&self) -> impl Iterator<Item = Interest> + '_ {
        std::iter::once(Interest::row(schema::group_v2s::table, self.id.clone()))
    }
}
