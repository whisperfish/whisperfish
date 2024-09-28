use chrono::Utc;
use libsignal_service::{
    content::Metadata,
    proto::{
        call_message::{offer, Answer, Busy, Hangup, IceUpdate, Offer, Opaque},
        CallMessage,
    },
    push_service::DEFAULT_DEVICE_ID,
};
use ringrtc::{
    common::CallId,
    core::{
        call_manager::CallManager,
        signaling::{IceCandidate, ReceivedAnswer, ReceivedIce, ReceivedOffer},
    },
    lite::http::DelegatingClient,
};
use std::collections::HashMap;
use whisperfish_store::{millis_to_naive_chrono, store::orm::Recipient};

mod call_manager;

#[derive(Debug)]
pub(super) struct CallState {
    sub_state: CallSubState,

    call_setup_states: HashMap<CallId, CallSetupState>,

    manager: CallManager<ringrtc::native::NativePlatform>,
}

impl Default for CallState {
    fn default() -> Self {
        let client = DelegatingClient::new(call_manager::WhisperfishRingRtcHttpClient::default());
        let platform = call_manager::new_native_platform().unwrap();
        Self {
            sub_state: CallSubState::default(),
            call_setup_states: HashMap::new(),
            manager: CallManager::new(platform, client).expect("initialized call manager"),
        }
    }
}

#[derive(Debug)]
struct CallSetupState {
    enable_video_on_create: bool,
    // Note that we store Offer Type instead of a isRemoteVideoOffer flag
    offer_type: offer::Type,
    accept_with_video: bool,
    sent_joined_message: bool,
    #[allow(dead_code)]
    ring_group: bool,
    ring_id: i64,
    // XXX: This doesn't support groups; we'd need a Session instead, but we don't handle GroupCall
    //      yet anyway.
    ringer_recipient: Recipient,
    // This is for Telepathy integration
    // wait_for_telecom_approval: bool,
    // telecom_approved: bool,
    ice_servers: Vec<()>,
    always_turn_servers: bool,
}

#[derive(Debug, Default)]
enum CallSubState {
    #[default]
    Idle,
}

impl CallState {}

impl super::ClientActor {
    /// Dispatch the CallMessage to the appropriate handlers.
    pub(super) fn handle_call_message(
        &mut self,
        ctx: &mut <Self as actix::Actor>::Context,
        metadata: Metadata,
        call: CallMessage,
    ) {
        // XXX is this unwrap_or correct?
        let destination_id = call.destination_device_id.unwrap_or(DEFAULT_DEVICE_ID);

        if call.destination_device_id.is_none() {
            tracing::warn!("CallMessage did not have a destination_device_id set. Defaulting.");
        }

        let num_fields_set = [
            call.offer.is_some(),
            call.answer.is_some(),
            !call.ice_update.is_empty(),
        ]
        .iter()
        .filter(|&&x| x)
        .count();

        let _span =
            tracing::trace_span!("handle_call_message", sender = ?metadata.sender, destination_id, ?metadata, ?call).entered();

        if num_fields_set > 1 {
            tracing::warn!(
                "CallMessage has more than one field set.  Handling all, but this is unexpected."
            );
        }

        let peer = self
            .storage
            .as_ref()
            .expect("initialized storage")
            .fetch_or_insert_recipient_by_address(&metadata.sender);

        if let Some(offer) = call.offer {
            self.handle_call_offer(ctx, &metadata, destination_id, &peer, offer);
        }

        if let Some(answer) = call.answer {
            self.handle_call_answer(ctx, &metadata, destination_id, &peer, answer);
        }

        if !call.ice_update.is_empty() {
            self.handle_call_ice(ctx, &metadata, destination_id, &peer, call.ice_update);
        }

        if let Some(busy) = call.busy {
            self.handle_call_busy(ctx, &metadata, destination_id, &peer, busy);
        }

        if let Some(hangup) = call.hangup {
            self.handle_call_hangup(ctx, &metadata, destination_id, &peer, hangup);
        }

        if let Some(opaque) = call.opaque {
            tracing::info!("{} sent an opaque message.", peer);

            let Some(opaque) = opaque.data else {
                tracing::warn!("Opaque message did not have data. Ignoring.");
                return;
            };

            let sent_time = millis_to_naive_chrono(metadata.timestamp).and_utc();
            let age = Utc::now() - sent_time;

            self.call_state
                .manager
                .received_call_message(
                    metadata.sender.uuid.to_string().into_bytes(),
                    metadata.sender_device,
                    self.config.get_device_id().into(),
                    opaque,
                    age.to_std().unwrap_or(std::time::Duration::ZERO),
                )
                .expect("handled opaque message");
        }
    }

    // Equiv. of WebRtcActionProcessor::handleReceivedOffer
    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_offer(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        offer: Offer,
    ) {
        tracing::info!("{} is calling.", peer);
        // TODO: decline call if:
        // - [ ] Call is not from a trusted contact
        // - [ ] Phone is already in a call (from any other Telepathy client)
        // - [X] No opaque data is provided
        // - [ ] Recipient is blocked through a notification profile
        // Otherwise: ring!

        let Some(call_id) = offer.id.map(CallId::from) else {
            tracing::warn!("Call offer did not have a call ID. Ignoring.");
            return;
        };
        let _span = tracing::trace_span!("processing offer with id", call_id = ?call_id).entered();

        if let Some(setup) = self.call_state.call_setup_states.remove(&call_id) {
            tracing::warn!(?setup, "Call setup already exists. replacing.");
        }

        let storage = self.storage.as_ref().expect("initialized storage");

        let setup = CallSetupState {
            enable_video_on_create: false,
            offer_type: offer.r#type(),
            accept_with_video: false,
            sent_joined_message: false,
            ring_group: true,
            ring_id: 0,
            ringer_recipient: storage.fetch_or_insert_recipient_by_address(&metadata.sender),
            ice_servers: Vec::new(),
            always_turn_servers: false,
        };

        let sent_time = millis_to_naive_chrono(metadata.timestamp).and_utc();
        let age = Utc::now() - sent_time;
        let seconds = std::cmp::max(age.num_seconds(), 0);
        tracing::debug!(
            %sent_time,
            ringer = %setup.ringer_recipient,
            "Call offer is {seconds} seconds old.",
        );

        let call_media_type = match offer.r#type() {
            offer::Type::OfferAudioCall => ringrtc::common::CallMediaType::Audio,
            offer::Type::OfferVideoCall => ringrtc::common::CallMediaType::Video,
        };

        let Some(opaque) = offer.opaque else {
            tracing::warn!("Call offer did not have opaque data. Ignoring.");
            return;
        };

        let offer = match ringrtc::core::signaling::Offer::new(call_media_type, opaque) {
            Ok(x) => x,
            Err(e) => {
                tracing::error!("Failed to parse call offer: {:?}", e);
                return;
            }
        };
        let remote_peer = peer.id.to_string();
        let mut call_manager = self.call_state.manager.clone();

        let protocol_address = peer
            .to_service_address()
            .expect("existing session for peer")
            .to_protocol_address(DEFAULT_DEVICE_ID);
        let self_device_id = u32::from(self.config.get_device_id());
        let sender_device_id = metadata.sender_device;
        let destination_identity = metadata.destination.identity;

        let protocol_storage = storage.aci_or_pni(destination_identity);

        let receive_offer = async move {
            use libsignal_service::protocol::IdentityKeyStore;

            let receiver_identity_key = protocol_storage
                .get_identity_key_pair()
                .await
                .expect("identity stored")
                .public_key()
                .serialize()
                .into();
            let sender_identity_key = protocol_storage
                .get_identity(&protocol_address)
                .await
                .expect("protocol store")
                .expect("identity exists for remote peer")
                .serialize()
                .into();

            let received_offer = ReceivedOffer {
                offer,
                age: age.to_std().unwrap_or(std::time::Duration::ZERO),
                sender_device_id,
                receiver_device_id: self_device_id,
                receiver_device_is_primary: self_device_id == DEFAULT_DEVICE_ID,
                sender_identity_key,
                receiver_identity_key,
            };

            call_manager
                .received_offer(remote_peer, call_id, received_offer)
                .expect("handled call offer");
        };
        actix::spawn(receive_offer);

        assert!(self
            .call_state
            .call_setup_states
            .insert(call_id, setup)
            .is_none());
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_answer(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        answer: Answer,
    ) {
        tracing::info!("{} answered.", peer);
        let Some(call_id) = answer.id.map(CallId::from) else {
            tracing::warn!("Call answer did not have a call ID. Ignoring.");
            return;
        };

        let mut manager = self.call_state.manager.clone();

        let Some(opaque) = answer.opaque else {
            tracing::warn!("Call answer did not have opaque data. Ignoring.");
            return;
        };

        let protocol_address = peer
            .to_service_address()
            .expect("existing session for peer")
            .to_protocol_address(DEFAULT_DEVICE_ID);

        let protocol_storage = self
            .storage
            .as_ref()
            .expect("storage initialized")
            .aci_or_pni(metadata.destination.identity);
        let sender_device_id = metadata.sender_device;

        let handle_answer = async move {
            use libsignal_service::protocol::IdentityKeyStore;

            let receiver_identity_key = protocol_storage
                .get_identity_key_pair()
                .await
                .expect("identity stored")
                .public_key()
                .serialize()
                .into();
            let sender_identity_key = protocol_storage
                .get_identity(&protocol_address)
                .await
                .expect("protocol store")
                .expect("identity exists for remote peer")
                .serialize()
                .into();

            let answer = ReceivedAnswer {
                answer: ringrtc::core::signaling::Answer::new(opaque).expect("parsed answer"),
                sender_device_id,
                sender_identity_key,
                receiver_identity_key,
            };

            manager
                .received_answer(call_id, answer)
                .expect("handled call answer");
        };
        actix::spawn(handle_answer);
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_ice(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        ice_updates: Vec<IceUpdate>,
    ) {
        tracing::info!("{} is sending ICE updates.", peer);
        for update in ice_updates {
            let Some(id) = update.id else {
                tracing::warn!("ICE update did not have an ID. Ignoring.");
                continue;
            };
            let call_id = CallId::from(id);
            let Some(opaque) = update.opaque else {
                tracing::warn!("ICE update did not have opaque data. Ignoring.");
                continue;
            };
            let received_ice = ReceivedIce {
                ice: ringrtc::core::signaling::Ice {
                    candidates: vec![IceCandidate::new(opaque)],
                },
                sender_device_id: metadata.sender_device,
            };
            self.call_state
                .manager
                .received_ice(call_id, received_ice)
                .expect("handled ICE update");
        }
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_busy(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        busy: Busy,
    ) {
        tracing::info!("{} is busy.", peer);
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_hangup(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        hangup: Hangup,
    ) {
        tracing::info!("{} hung up.", peer);
    }
}
