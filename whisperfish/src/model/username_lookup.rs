//! Username-lookup result carried on the username-lookup observer channel.
//!
//! The observer core does not interpret this payload; it only routes on
//! [`Subject`] (`Subject::of::<UsernameLookup>()`) + [`Event::key`]. The
//! resolver actor emits via
//! `Storage::observe_event(key, relations, UsernameLookup { .. })`, keyed on
//! the submitted query string (`PrimaryKey::StringRowId`) so a future migration
//! to keyed [`Interest`] needs no payload change here. Consumers currently use
//! the unscoped `Interest::on::<UsernameLookup>()` and read `query` from the
//! payload for the "is this mine?" check.
//!
//! Compare [`crate::model::typing::TypingEvent`], the established precedent
//! for a process event whose subject *is* the payload type.

use libsignal_protocol::Aci;
use whisperfish_store::store::observer::PrimaryKey;

/// The terminal state of a username/username-link resolution.
///
/// All variants are safe to surface to QML: link-decryption / parsing failures
/// are already collapsed upstream in `libsignal-service-rs` to a generic
/// `InvalidFrame` error, and an invalid-format query is caught here before any
/// network call. `Failed` therefore only ever carries a sanitized human
/// string, never protocol internals.
#[derive(Clone, Debug, PartialEq)]
pub enum UsernameLookupResult {
    /// The username link decrypted to an opaque/garbage payload, or a
    /// well-formed username resolved to no account. Distinct from `Failed`
    /// (which implies "something went wrong"): this is the normal "user typed a
    /// valid thing that doesn't exist" outcome.
    NotFound,
    /// A well-formed lookup completed and identified the account.
    Resolved { aci: Aci, username: String },
    /// The query was not a username or link, the websocket/HTTP call failed,
    /// or a decrypted link was malformed. The string is a translatable hint,
    /// not a structured error.
    Failed(String),
}

/// One resolution outcome, broadcast on the observer channel.
///
/// `query` is the exact string submitted (a bare username like `johndoe.99`, or
/// a `https://signal.me/#eu/<payload>` link / bare payload). It is duplicated
/// on [`Event::key`] as a `PrimaryKey::StringRowId` so a future keyed-interest
/// migration needs no payload change.
#[derive(Clone, Debug)]
pub struct UsernameLookup {
    /// The exact query string submitted by the caller.
    pub query: String,
    pub result: UsernameLookupResult,
}

impl UsernameLookup {
    /// Build the [`PrimaryKey`] a resolver should emit this lookup under:
    /// the submitted query string. Centralised here so the actor and any future
    /// keyed consumer agree on the key shape.
    pub fn primary_key(query: &str) -> PrimaryKey {
        PrimaryKey::StringRowId(query.to_string())
    }
}
