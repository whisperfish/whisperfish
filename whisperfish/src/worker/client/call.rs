use libsignal_service::{
    content::Metadata,
    proto::{
        call_message::{Answer, Busy, Hangup, IceUpdate, Offer, Opaque},
        CallMessage,
    },
    push_service::DEFAULT_DEVICE_ID,
};

impl super::ClientActor {
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

        if let Some(offer) = call.offer {
            self.handle_call_offer(ctx, &metadata, destination_id, offer);
        }

        if let Some(answer) = call.answer {
            self.handle_call_answer(ctx, &metadata, destination_id, answer);
        }

        if !call.ice_update.is_empty() {
            self.handle_call_ice(ctx, &metadata, destination_id, call.ice_update);
        }

        if let Some(busy) = call.busy {
            self.handle_call_busy(ctx, &metadata, destination_id, busy);
        }

        if let Some(hangup) = call.hangup {
            self.handle_call_hangup(ctx, &metadata, destination_id, hangup);
        }

        if let Some(opaque) = call.opaque {
            self.handle_call_opaque(ctx, &metadata, destination_id, opaque);
        }
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_offer(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        offer: Offer,
    ) {
        tracing::info!("{:?} is calling.", metadata.sender);
        // XXX
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_answer(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        answer: Answer,
    ) {
        tracing::info!("{} answered.", metadata.sender.to_service_id());
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_ice(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        ice_update: Vec<IceUpdate>,
    ) {
        tracing::info!("{} is sending ICE update.", metadata.sender.to_service_id());
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_busy(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        busy: Busy,
    ) {
        tracing::info!("{} is busy.", metadata.sender.to_service_id());
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_hangup(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        hangup: Hangup,
    ) {
        tracing::info!("{} hung up.", metadata.sender.to_service_id());
    }

    #[tracing::instrument(skip(self, _ctx, metadata, _destination_device_id))]
    fn handle_call_opaque(
        &mut self,
        _ctx: &mut <Self as actix::Actor>::Context,
        metadata: &Metadata,
        _destination_device_id: u32,
        opaque: Opaque,
    ) {
        tracing::info!(
            "{} sent an opaque message.",
            metadata.sender.to_service_id()
        );
    }
}
