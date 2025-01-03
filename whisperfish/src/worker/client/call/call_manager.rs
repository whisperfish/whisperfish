use libsignal_service::proto::CallMessage;
use ringrtc::{
    lite::http,
    native::{CallStateHandler, GroupUpdateHandler, SignalingSender},
    webrtc::peer_connection_factory::{AudioConfig, PeerConnectionFactory},
};

#[derive(Default, Debug)]
pub struct WhisperfishRingRtcHttpClient {}

impl http::Delegate for WhisperfishRingRtcHttpClient {
    fn send_request(&self, _request_id: u32, _request: http::Request) {
        todo!()
    }
}

#[derive(Debug)]
struct WhisperfishSignalingSender {
    client: actix::Addr<crate::worker::ClientActor>,
}

impl SignalingSender for WhisperfishSignalingSender {
    #[tracing::instrument(skip(self, message))]
    fn send_signaling(
        &self,
        recipient_id: &str,
        call_id: ringrtc::common::CallId,
        receiver_device_id: Option<ringrtc::common::DeviceId>,
        message: ringrtc::core::signaling::Message,
    ) -> ringrtc::common::Result<()> {
        let mut message = match message {
            ringrtc::core::signaling::Message::Offer(offer) => {
                use libsignal_service::proto::call_message::offer::Type as OfferType;
                use libsignal_service::proto::call_message::Offer;

                tracing::debug!("sending offer");

                let offer = Offer {
                    id: Some(call_id.into()),
                    r#type: Some(
                        match offer.call_media_type {
                            ringrtc::common::CallMediaType::Audio => OfferType::OfferAudioCall,
                            ringrtc::common::CallMediaType::Video => OfferType::OfferVideoCall,
                        }
                        .into(),
                    ),
                    opaque: Some(offer.opaque),
                };
                CallMessage {
                    offer: Some(offer),
                    ..Default::default()
                }
            }
            ringrtc::core::signaling::Message::Answer(answer) => {
                use libsignal_service::proto::call_message::Answer;

                tracing::debug!("sending answer");

                let answer = Answer {
                    id: Some(call_id.into()),
                    opaque: Some(answer.opaque),
                };
                CallMessage {
                    answer: Some(answer),
                    ..Default::default()
                }
            }
            ringrtc::core::signaling::Message::Ice(ice) => {
                use libsignal_service::proto::call_message::IceUpdate;

                tracing::debug!("sending ICE");

                let ice_update: Vec<_> = ice
                    .candidates
                    .into_iter()
                    .map(|c| IceUpdate {
                        id: Some(call_id.into()),
                        opaque: Some(c.opaque),
                    })
                    .collect();
                CallMessage {
                    ice_update,
                    ..Default::default()
                }
            }
            ringrtc::core::signaling::Message::Hangup(hangup) => {
                use libsignal_service::proto::call_message::hangup::Type as ProtoHangupType;
                use libsignal_service::proto::call_message::Hangup;
                use ringrtc::core::signaling::HangupType;

                tracing::debug!("sending hangup");

                let (ty, device_id) = hangup.to_type_and_device_id();
                let hangup = Hangup {
                    id: Some(call_id.into()),
                    device_id,
                    r#type: Some(
                        match ty {
                            HangupType::Normal => ProtoHangupType::HangupNormal,
                            HangupType::AcceptedOnAnotherDevice => ProtoHangupType::HangupAccepted,
                            HangupType::DeclinedOnAnotherDevice => ProtoHangupType::HangupDeclined,
                            HangupType::BusyOnAnotherDevice => ProtoHangupType::HangupBusy,
                            HangupType::NeedPermission => ProtoHangupType::HangupNeedPermission,
                        }
                        .into(),
                    ),
                };
                CallMessage {
                    hangup: Some(hangup),
                    ..Default::default()
                }
            }
            ringrtc::core::signaling::Message::Busy => {
                use libsignal_service::proto::call_message::Busy;

                tracing::debug!("sending busy");

                let busy = Busy {
                    id: Some(call_id.into()),
                };
                CallMessage {
                    busy: Some(busy),
                    ..Default::default()
                }
            }
        };
        message.destination_device_id = receiver_device_id;

        self.client.do_send(super::SendCallMessage {
            recipient_id: recipient_id.parse().expect("recipient id is an u32"),
            content: message,
            urgent: false, // TODO: urgency is a param
        });

        Ok(())
    }

    fn send_call_message(
        &self,
        _recipient_id: ringrtc::lite::sfu::UserId,
        _message: Vec<u8>,
        _urgency: ringrtc::core::group_call::SignalingMessageUrgency,
    ) -> ringrtc::common::Result<()> {
        todo!("group calls")
    }

    fn send_call_message_to_group(
        &self,
        _group_id: ringrtc::core::group_call::GroupId,
        _message: Vec<u8>,
        _urgency: ringrtc::core::group_call::SignalingMessageUrgency,
        _recipients_override: std::collections::HashSet<ringrtc::lite::sfu::UserId>,
    ) -> ringrtc::common::Result<()> {
        todo!("group calls")
    }
}

#[derive(Debug)]
struct WhisperfishStateHandler {
    client: actix::Addr<crate::worker::ClientActor>,
}

impl CallStateHandler for WhisperfishStateHandler {
    #[tracing::instrument(skip(self))]
    fn handle_call_state(
        &self,
        remote_peer_id: &str,
        call_id: ringrtc::common::CallId,
        state: ringrtc::native::CallState,
    ) -> ringrtc::common::Result<()> {
        self.client.do_send(crate::worker::client::call::CallState {
            remote_peer_id: remote_peer_id.parse().expect("remote peer id is an u32"),
            call_id: call_id.into(),
            state,
        });
        Ok(())
    }

    fn handle_remote_audio_state(
        &self,
        _remote_peer_id: &str,
        _enabled: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_remote_video_state(
        &self,
        _remote_peer_id: &str,
        _enabled: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_remote_sharing_screen(
        &self,
        _remote_peer_id: &str,
        _enabled: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    #[tracing::instrument(skip(self))]
    fn handle_network_route(
        &self,
        remote_peer_id: &str,
        network_route: ringrtc::webrtc::peer_connection_observer::NetworkRoute,
    ) -> ringrtc::common::Result<()> {
        tracing::warn!("unimplemented network route");
        todo!()
    }

    fn handle_audio_levels(
        &self,
        _remote_peer_id: &str,
        _captured_level: ringrtc::webrtc::peer_connection::AudioLevel,
        _received_level: ringrtc::webrtc::peer_connection::AudioLevel,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_low_bandwidth_for_video(
        &self,
        _remote_peer_id: &str,
        _recovered: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }
}

#[derive(Debug)]
struct WhisperfishGroupUpdateHandler {
    #[allow(unused)]
    client: actix::Addr<crate::worker::ClientActor>,
}

impl GroupUpdateHandler for WhisperfishGroupUpdateHandler {
    fn handle_group_update(
        &self,
        _update: ringrtc::native::GroupUpdate,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }
}

pub fn new_native_platform(
    client: actix::Addr<crate::worker::ClientActor>,
) -> anyhow::Result<ringrtc::native::NativePlatform> {
    let connection_factory = PeerConnectionFactory::new(&AudioConfig::default(), false)?;
    let signaling_sender = Box::new(WhisperfishSignalingSender {
        client: client.clone(),
    });
    const SHOULD_ASSUME_MESSAGES_SENT: bool = false;
    let state_handler = Box::new(WhisperfishStateHandler {
        client: client.clone(),
    });
    let group_handler = Box::new(WhisperfishGroupUpdateHandler { client });

    Ok(ringrtc::native::NativePlatform::new(
        connection_factory,
        signaling_sender,
        SHOULD_ASSUME_MESSAGES_SENT,
        state_handler,
        group_handler,
    ))
}
