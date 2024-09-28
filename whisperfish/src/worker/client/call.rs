use chrono::Utc;
use libsignal_service::{
    content::Metadata,
    proto::{
        call_message::{offer, Answer, Busy, Hangup, IceUpdate, Offer, Opaque},
        CallMessage,
    },
    push_service::DEFAULT_DEVICE_ID,
};
use ringrtc::{common::CallId, core::call_manager::CallManager, lite::http::DelegatingClient};
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
            self.handle_call_opaque(ctx, &metadata, destination_id, &peer, opaque);
        }
    }

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
        // - Call is not from a trusted contact
        // - Phone is already in a call (from any other Telepathy client)
        // - No opaque data is provided
        // - Recipient is blocked through a notification profile
        // Otherwise: ring!

        let Some(call_id) = offer.id.map(CallId::from) else {
            tracing::warn!("Call offer did not have a call ID. Ignoring.");
            return;
        };
        let _span = tracing::trace_span!("processing offer with id", call_id = ?call_id).entered();

        if let Some(setup) = self.call_state.call_setup_states.remove(&call_id) {
            tracing::warn!(?setup, "Call setup already exists. replacing.");
        }

        let setup = CallSetupState {
            enable_video_on_create: false,
            offer_type: offer.r#type(),
            accept_with_video: false,
            sent_joined_message: false,
            ring_group: true,
            ring_id: 0,
            ringer_recipient: self
                .storage
                .as_ref()
                .expect("initialized storage")
                .fetch_or_insert_recipient_by_address(&metadata.sender),
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
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_ice(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        ice_update: Vec<IceUpdate>,
    ) {
        tracing::info!("{} is sending ICE update.", peer);
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

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_opaque(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        peer: &Recipient,
        opaque: Opaque,
    ) {
        tracing::info!("{} sent an opaque message.", peer);
    }
}
