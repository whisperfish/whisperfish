use libsignal_service::proto::CallMessage;
use ringrtc::{
    lite::http,
    native::{CallStateHandler, GroupUpdateHandler, SignalingSender},
    webrtc::peer_connection_factory::{AudioConfig, PeerConnectionFactory},
};

#[derive(Default, Debug)]
pub struct WhisperfishRingRtcHttpClient {}

impl http::Delegate for WhisperfishRingRtcHttpClient {
    fn send_request(&self, request_id: u32, request: http::Request) {
        todo!()
    }
}

#[derive(Default, Debug)]
struct WhisperfishSignalingSender {}

impl SignalingSender for WhisperfishSignalingSender {
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

        todo!("Send {:?}", message);

        Ok(())
    }

    fn send_call_message(
        &self,
        recipient_id: ringrtc::lite::sfu::UserId,
        message: Vec<u8>,
        urgency: ringrtc::core::group_call::SignalingMessageUrgency,
    ) -> ringrtc::common::Result<()> {
        todo!("group calls")
    }

    fn send_call_message_to_group(
        &self,
        group_id: ringrtc::core::group_call::GroupId,
        message: Vec<u8>,
        urgency: ringrtc::core::group_call::SignalingMessageUrgency,
        recipients_override: std::collections::HashSet<ringrtc::lite::sfu::UserId>,
    ) -> ringrtc::common::Result<()> {
        todo!("group calls")
    }
}

#[derive(Default, Debug)]
struct WhisperfishStateHandler {}

impl CallStateHandler for WhisperfishStateHandler {
    fn handle_call_state(
        &self,
        remote_peer_id: &str,
        call_id: ringrtc::common::CallId,
        state: ringrtc::native::CallState,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_remote_audio_state(
        &self,
        remote_peer_id: &str,
        enabled: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_remote_video_state(
        &self,
        remote_peer_id: &str,
        enabled: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_remote_sharing_screen(
        &self,
        remote_peer_id: &str,
        enabled: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_network_route(
        &self,
        remote_peer_id: &str,
        network_route: ringrtc::webrtc::peer_connection_observer::NetworkRoute,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_audio_levels(
        &self,
        remote_peer_id: &str,
        captured_level: ringrtc::webrtc::peer_connection::AudioLevel,
        received_level: ringrtc::webrtc::peer_connection::AudioLevel,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }

    fn handle_low_bandwidth_for_video(
        &self,
        remote_peer_id: &str,
        recovered: bool,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }
}

#[derive(Default, Debug)]
struct WhisperfishGroupUpdateHandler {}

impl GroupUpdateHandler for WhisperfishGroupUpdateHandler {
    fn handle_group_update(
        &self,
        update: ringrtc::native::GroupUpdate,
    ) -> ringrtc::common::Result<()> {
        todo!()
    }
}

pub fn new_native_platform() -> anyhow::Result<ringrtc::native::NativePlatform> {
    let connection_factory = PeerConnectionFactory::new(&AudioConfig::default(), false)?;
    let signaling_sender = Box::new(WhisperfishSignalingSender::default());
    const SHOULD_ASSUME_MESSAGES_SENT: bool = true;
    let state_handler = Box::new(WhisperfishStateHandler::default());
    let group_handler = Box::new(WhisperfishGroupUpdateHandler::default());

    Ok(ringrtc::native::NativePlatform::new(
        connection_factory,
        signaling_sender,
        SHOULD_ASSUME_MESSAGES_SENT,
        state_handler,
        group_handler,
    ))
}
