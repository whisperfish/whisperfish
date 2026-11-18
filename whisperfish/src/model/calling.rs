use crate::worker::{
    ClientActor,
    client::{AnswerCall, HangupCall, InitiateCall},
};
use actix::Addr;
use qmetaobject::prelude::*;
use ringrtc::{
    common::{CallId, CallMediaType},
    native::CallState,
};

/// A call that is currently in progress.
struct LiveCall {
    call_id: CallId,
    remote_peer_id: i32,
    media_type: CallMediaType,
    is_outgoing: bool,
}

#[derive(Default, QObject)]
pub struct Calls {
    base: qt_base_class!(trait QObject),

    client: Option<Addr<ClientActor>>,

    live_call: Option<LiveCall>,

    // 0 - audio, 1 - video
    // -1 if no call is in progress.
    call_type: qt_property!(i32; READ call_type NOTIFY ringing_changed ALIAS callType),
    // 0 - outgoing, 1 - incoming
    // -1 if no call is in progress.
    direction: qt_property!(i32; READ direction NOTIFY ringing_changed),

    // Recipient id that's currently calling
    // -1 if no call is in progress.
    ringing_recipient_id: qt_property!(i32; READ ringing_recipient_id NOTIFY ringing_changed ALIAS ringingRecipientId),

    ringing_changed: qt_signal!(),
    hungup: qt_signal!(),

    answer: qt_method!(fn(&self)),
    hangup: qt_method!(fn(&self)),
    call: qt_method!(fn(&self, recipient_id: i32, video: bool)),
}

impl Calls {
    pub fn new() -> Calls {
        Calls::default()
    }

    fn ringing_recipient_id(&self) -> i32 {
        let Some(live_call) = &self.live_call else {
            return -1;
        };
        live_call.remote_peer_id
    }

    fn call_type(&self) -> i32 {
        let Some(live_call) = &self.live_call else {
            return -1;
        };
        match live_call.media_type {
            CallMediaType::Audio => 0,
            CallMediaType::Video => 1,
        }
    }

    fn direction(&self) -> i32 {
        let Some(live_call) = &self.live_call else {
            return -1;
        };
        if live_call.is_outgoing { 0 } else { 1 }
    }

    fn client(&self) -> Addr<ClientActor> {
        self.client.clone().unwrap()
    }

    pub fn set_client(&mut self, client: Addr<ClientActor>) {
        self.client = Some(client);
    }

    pub fn handle_state(&mut self, remote_peer_id: i32, call_id: CallId, state: CallState) {
        match state {
            CallState::Incoming(incoming) => {
                self.live_call = Some(LiveCall {
                    call_id,
                    remote_peer_id,
                    media_type: incoming,
                    is_outgoing: false,
                });
                self.ringing_changed();
            }
            CallState::Outgoing(outgoing) => {
                self.live_call = Some(LiveCall {
                    call_id,
                    remote_peer_id,
                    media_type: outgoing,
                    is_outgoing: true,
                });
                self.ringing_changed();
            }
            CallState::Ended(..) | CallState::Rejected(_) => {
                // The end reason is rendered from the call log in the database.
                self.live_call = None;
                self.ringing_changed();
                self.hungup();
            }
            CallState::Ringing | CallState::Connected | CallState::Connecting => {
                // No UI state change for now
            }
            CallState::Concluded => {
                // Ended or Rejected should have reset the call state already.
                if self.live_call.as_ref().map(|call| call.call_id) == Some(call_id) {
                    tracing::error!(
                        "Call {call_id} concluded without an end state; resetting call state."
                    );
                    self.live_call = None;
                    self.ringing_changed();
                    self.hungup();
                }
            }
        }
    }

    pub fn call(&self, recipient_id: i32, video: bool) {
        self.client().do_send(InitiateCall {
            recipient_id,
            r#type: if video {
                CallMediaType::Video
            } else {
                CallMediaType::Audio
            },
        });
    }

    pub fn answer(&self) {
        let Some(call) = &self.live_call else {
            tracing::error!("No call to answer");
            return;
        };
        self.client().do_send(AnswerCall {
            call_id: call.call_id,
        });
    }

    pub fn hangup(&self) {
        let Some(call) = &self.live_call else {
            tracing::error!("No call to hang up");
            return;
        };
        self.client().do_send(HangupCall {
            call_id: call.call_id,
        });
    }
}
