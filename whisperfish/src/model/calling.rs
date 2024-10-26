use crate::worker::{
    client::{AnswerCall, HangupCall, InitiateCall},
    ClientActor,
};
use actix::Addr;
use qmetaobject::prelude::*;
use ringrtc::{
    common::{CallId, CallMediaType},
    native::CallState,
};

#[derive(QObject)]
pub struct Calls {
    base: qt_base_class!(trait QObject),

    client: Option<Addr<ClientActor>>,

    // XXX We could gather all these fields in an optional/enumed struct,
    //     and use getters instead.  Would make it less error-prone
    // 0 - audio, 1 - video
    call_type: qt_property!(i32; NOTIFY ringing_changed ALIAS callType),
    call_id: Option<CallId>,
    // 0 - outgoing, 1 - incoming
    direction: qt_property!(i32; NOTIFY ringing_changed),

    // Recipient id that's currently calling
    ringing_recipient_id: qt_property!(i32; NOTIFY ringing_changed ALIAS ringingRecipientId),

    ringing_changed: qt_signal!(),
    hungup: qt_signal!(),

    answer: qt_method!(fn(&self)),
    hangup: qt_method!(fn(&self)),
    call: qt_method!(fn(&self, recipient_id: i32, video: bool)),
}

impl Calls {
    pub fn new() -> Calls {
        Calls {
            base: Default::default(),
            client: None,

            call_type: -1,
            call_id: None,
            ringing_recipient_id: -1,
            direction: -1,

            ringing_changed: Default::default(),
            hungup: Default::default(),
            answer: Default::default(),
            hangup: Default::default(),
            call: Default::default(),
        }
    }

    fn client(&self) -> Addr<ClientActor> {
        self.client.clone().unwrap()
    }

    pub fn set_client(&mut self, client: Addr<ClientActor>) {
        self.client = Some(client);
    }

    pub fn handle_state(&mut self, remote_peer_id: i32, call_id: CallId, state: CallState) {
        use ringrtc::native::{CallState, EndReason};
        match state {
            CallState::Incoming(incoming) => {
                self.ringing_recipient_id = remote_peer_id;
                self.call_id = Some(call_id);
                self.call_type = match incoming {
                    CallMediaType::Audio => 0,
                    CallMediaType::Video => 1,
                };
                self.direction = 1;
                self.ringing_changed();
            }
            CallState::Outgoing(outgoing) => {
                self.ringing_recipient_id = remote_peer_id;
                self.call_id = Some(call_id);
                self.call_type = match outgoing {
                    CallMediaType::Audio => 0,
                    CallMediaType::Video => 1,
                };
                self.direction = 0;
                self.ringing_changed();
            }
            CallState::Ended(reason) => {
                // We probably don't have to care about the reason in the UI,
                // and instead pull it out of the database when it needs rendering.
                match reason {
                    EndReason::LocalHangup
                    | EndReason::RemoteHangup
                    | EndReason::RemoteHangupNeedPermission
                    | EndReason::Declined
                    | EndReason::Busy
                    | EndReason::Glare
                    | EndReason::ReCall
                    | EndReason::ReceivedOfferExpired { age: _ }
                    | EndReason::ReceivedOfferWhileActive
                    | EndReason::ReceivedOfferWithGlare
                    | EndReason::SignalingFailure
                    | EndReason::GlareFailure
                    | EndReason::ConnectionFailure
                    | EndReason::InternalFailure
                    | EndReason::Timeout
                    | EndReason::AcceptedOnAnotherDevice
                    | EndReason::DeclinedOnAnotherDevice
                    | EndReason::BusyOnAnotherDevice => {
                        tracing::warn!("Call ended, unprocessed reason: {:?}", reason);
                    }
                }
                self.ringing_recipient_id = -1;
                self.call_id = None;
                self.direction = -1;

                self.ringing_changed();
                self.hungup();
            }
            CallState::Ringing
            | CallState::Connected
            | CallState::Connecting
            | CallState::Concluded => tracing::error!("unimplemented call state"),
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
        let Some(call_id) = self.call_id else {
            tracing::error!("No call_id to answer");
            return;
        };
        self.client().do_send(AnswerCall { call_id });
    }

    pub fn hangup(&self) {
        let Some(call_id) = self.call_id else {
            tracing::error!("No call_id to hangup");
            return;
        };
        self.client().do_send(HangupCall { call_id });
    }
}
