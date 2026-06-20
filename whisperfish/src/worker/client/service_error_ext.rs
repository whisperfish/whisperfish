use libsignal_service::content::ServiceError;

/// What to do with a request that failed due to a [`ServiceError`].
///
/// Replaces the `Option<Option<chrono::Duration>>` cake that nobody liked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryAction {
    /// The error is presumed a true failure; the request should not be retried.
    NoRetry,
    /// The request should be retried, but the delay is unknown.
    Retry,
    /// The request should be retried after the given duration.
    RetryAfter(chrono::Duration),
}

impl RetryAction {
    /// Returns the value of RetryAfter or the given value if self is Retry.
    pub fn retry_delay_or(self, duration: chrono::Duration) -> Option<chrono::Duration> {
        match self {
            Self::NoRetry => None,
            Self::Retry => Some(duration),
            Self::RetryAfter(self_duration) => Some(self_duration),
        }
    }
}

pub trait WhisperfishServiceErrorExt {
    /// Whether a request should be rescheduled
    ///
    /// Returns [`RetryAction::NoRetry`] if the error is presumed a true failure.
    /// Returns [`RetryAction::Retry`] if the request should be retried but a back-off
    /// duration is unknown.
    /// Returns [`RetryAction::RetryAfter`] if the request should be retried after
    /// the given duration.
    fn can_retry(&self) -> RetryAction;
}

impl WhisperfishServiceErrorExt for ServiceError {
    fn can_retry(&self) -> RetryAction {
        use ServiceError::*;

        // Handle simple errors
        if matches!(self, Timeout { reason: _ } | WsClosing { reason: _ }) {
            return RetryAction::Retry;
        }

        // Handle IO cases
        if let IO(err) = self {
            use std::io::ErrorKind::*;
            if matches!(err.kind(), ConnectionRefused) {
                return RetryAction::Retry;
            }
        }

        if let WsError(_err) = self {
            match _err.as_ref() {
                // XXX This might not be ideal; we're assuming we'll also be able to retry the
                // handshake.
                reqwest_websocket::Error::Handshake(_) => return RetryAction::Retry,
                reqwest_websocket::Error::Reqwest(error) => {
                    if error.is_connect() | error.is_timeout() {
                        // XXX: maybe we *can* find the timeout case's timeout.
                        return RetryAction::Retry;
                    }
                }
                reqwest_websocket::Error::Tungstenite(error) => {
                    // XXX: these are many, many, nested error variants. I'm sure some of them show
                    // up, but the main ones are handled on upper/non-application layers.
                    tracing::warn!(%error, "ignoring Tungstenite for retry checking; please file an issue about this");
                }
                _ => (),
            }
        }

        if let Http(_err) = self {
            // Not taking any action on this, left unhandled.
            // Might be useful to fill this branch in *some* cases.
        }

        if let RateLimitExceeded { retry_after } = self {
            return match *retry_after {
                Some(duration) => RetryAction::RetryAfter(duration),
                None => RetryAction::Retry,
            };
        }

        RetryAction::NoRetry
    }
}
