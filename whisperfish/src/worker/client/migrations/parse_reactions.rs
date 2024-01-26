use super::*;
use anyhow::Context;
use chrono::Utc;
use diesel::prelude::*;
use whisperfish_store::schema;
use whisperfish_store::store::orm;

#[derive(Message)]
#[rtype(result = "()")]
pub struct ParseOldReaction;

impl Handler<ParseOldReaction> for ClientActor {
    type Result = ();
    fn handle(&mut self, _: ParseOldReaction, _ctx: &mut Self::Context) -> Self::Result {
        let _span = tracing::info_span!("parsing old reactions").entered();
        let storage = self.storage.clone().unwrap();
        let myself = storage.fetch_self_recipient().expect("myself in db");

        let reaction_messages: Vec<orm::Message> = {
            use schema::messages::dsl::*;
            messages
                .filter(text.like("R@%:%"))
                .order_by((text, sender_recipient_id, received_timestamp))
                .get_results(&mut *storage.db())
                .expect("fetch reaction messages")
        };

        if !reaction_messages.is_empty() {
            tracing::info!(
                "Found {} R@{{}}:{{}} emoji reactions. Migrating.",
                reaction_messages.len()
            );
        }
        storage.db().transaction::<(), diesel::result::Error, _>(|db| {
            let regex = regex::Regex::new(r"R@(\d+):(.*)").expect("reaction regex");
            let mut reaction_messages = reaction_messages.into_iter().peekable();
            while let Some(reaction) = reaction_messages.next() {
                let reaction_text = reaction.text.as_ref().expect("non-null text because of query");
                let m = regex.captures_iter(reaction_text).next().expect("match because of matching query");
                let ts: u64 = (m[1]).parse().expect("parse as int because of matching regex");

                if let Some(next) = reaction_messages.peek() {
                    // .order_by((message_id, sender_recipient_id, received_timestamp))
                    let reaction_text = reaction.text.as_ref().expect("non-null text because of query");
                    let m = regex.captures_iter(reaction_text).next().expect("match because of matching query");
                    let next_ts: u64 = (m[1]).parse().expect("parse as int because of matching regex");
                    if reaction.sender_recipient_id == next.sender_recipient_id && ts == next_ts {
                        tracing::trace!("Next reaction is same author and same target, deleting and skipping this one.");

                        use schema::messages::dsl::*;
                        diesel::delete(messages)
                            .filter(id.eq(reaction.id))
                            .execute(db).context("deleting R-reaction").or(Err(diesel::result::Error::RollbackTransaction))?;
                        continue;
                    }
                }

                let ts = millis_to_naive_chrono(ts);
                let emoji_text = &m[2];

                match schema::messages::table
                    .filter(schema::messages::server_timestamp.eq(ts))
                    .first::<orm::Message>(db)
                    .optional()
                    .expect("db") {
                    Some(target_message) => {
                        let author_id = reaction.sender_recipient_id.unwrap_or(myself.id);
                        let reaction_sent_timestamp = reaction.sent_timestamp.unwrap_or(reaction.server_timestamp);

                        use schema::reactions::dsl::*;
                        // First delete the reactions that may already exist for this author and
                        // message. There should not be any, but better safe than sorry.
                        diesel::delete(reactions)
                            .filter(author.eq(author_id))
                            .filter(message_id.eq(target_message.id))
                            .filter(sent_time.nullable().le(reaction_sent_timestamp))
                            .execute(db)
                            .context("deleting R-reaction").or(Err(diesel::result::Error::RollbackTransaction))?;
                        let res = diesel::insert_into(reactions)
                            .values((
                                message_id.eq(target_message.id),
                                author.eq(author_id),
                                emoji.eq(emoji_text),
                                sent_time.eq(reaction_sent_timestamp),
                                received_time.eq(reaction.received_timestamp.unwrap_or_else(|| Utc::now().naive_utc()))
                            ))
                            .execute(db);
                        match res {
                            Ok(_) => (),
                            Err(e @ diesel::result::Error::DatabaseError(diesel::result::DatabaseErrorKind::UniqueViolation, _)) => {
                                tracing::info!("Got an already newer reaction for this message. Dropping. Reason: {:?}", e);
                            }
                            Err(e) => Err(e).context("inserting R-reaction").unwrap(),
                        }
                    }
                    None => {
                        tracing::warn!("No message found for reaction with ts={ts}.  Dropping as-is, bye bye.");
                    }
                }

                use schema::messages::dsl::*;
                diesel::delete(messages)
                    .filter(id.eq(reaction.id))
                    .execute(db)
                    .context("deleting R-reaction")
                    .or(Err(diesel::result::Error::RollbackTransaction))?;
            }
            Ok(())
        })
        .expect("migrate reactions");

        self.migration_state.notify_reactions_ready();
    }
}
