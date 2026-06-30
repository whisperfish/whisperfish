//! Global methods callable by QML

// TODO: this should probably be unified with Client as well at some point

pub mod message;
pub mod session;

pub use self::message::*;
pub use self::session::*;
