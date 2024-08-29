use std::collections::HashSet;
use std::time::Duration;

use ringrtc::{
    common::{ApplicationEvent, CallConfig, CallDirection, CallId, CallMediaType},
    core::{
        call::Call,
        connection::{Connection, ConnectionType},
        group_call::{self, Reaction},
        platform::PlatformItem,
        signaling,
    },
    lite::{
        http,
        sfu::{self, DemuxId, PeekInfo, PeekResult, UserId},
    },
    webrtc::{
        media::{MediaStream, VideoTrack},
        peer_connection::AudioLevel,
        peer_connection_observer::NetworkRoute,
    },
};

#[derive(Default, Debug)]
pub struct WhisperfishRingRtcHttpClient {}

impl http::Delegate for WhisperfishRingRtcHttpClient {
    fn send_request(&self, request_id: u32, request: http::Request) {
        todo!()
    }
}

#[derive(Debug, Default)]
pub struct WhisperfishIncomingMedia {}

impl PlatformItem for WhisperfishIncomingMedia {}

#[derive(Debug, Default, Clone)]
pub struct WhisperfishRemotePeer {}

impl PlatformItem for WhisperfishRemotePeer {}

#[derive(Debug, Default, Clone)]
pub struct WhisperfishConnection {}

impl PlatformItem for WhisperfishConnection {}

#[derive(Debug, Default, Clone)]
pub struct WhisperfishCallContext {}

impl PlatformItem for WhisperfishCallContext {}

#[derive(Debug, Default, Clone)]
pub struct WhisperfishCallManager {}

impl std::fmt::Display for WhisperfishCallManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "WhisperfishCallManager")
    }
}

impl sfu::Delegate for WhisperfishCallManager {
    fn handle_peek_result(&self, request_id: u32, peek_result: PeekResult) {
        todo!()
    }
}

impl ringrtc::core::platform::Platform for WhisperfishCallManager {
    type AppIncomingMedia = WhisperfishIncomingMedia;

    type AppRemotePeer = WhisperfishRemotePeer;

    type AppConnection = WhisperfishConnection;

    type AppCallContext = WhisperfishCallContext;

    fn create_connection(
        &mut self,
        call: &Call<Self>,
        remote_device: ringrtc::common::DeviceId,
        connection_type: ConnectionType,
        signaling_version: signaling::Version,
        call_config: CallConfig,
        audio_levels_interval: Option<Duration>,
    ) -> anyhow::Result<Connection<Self>> {
        todo!()
    }

    fn on_start_call(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        direction: CallDirection,
        call_media_type: CallMediaType,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_event(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        event: ApplicationEvent,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_network_route_changed(
        &self,
        remote_peer: &Self::AppRemotePeer,
        network_route: NetworkRoute,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_audio_levels(
        &self,
        remote_peer: &Self::AppRemotePeer,
        captured_level: AudioLevel,
        received_level: AudioLevel,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_low_bandwidth_for_video(
        &self,
        remote_peer: &Self::AppRemotePeer,
        recovered: bool,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_send_offer(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        offer: signaling::Offer,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_send_answer(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        send: signaling::SendAnswer,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_send_ice(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        send: signaling::SendIce,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_send_hangup(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        send: signaling::SendHangup,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_send_busy(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn send_call_message(
        &self,
        recipient_id: Vec<u8>,
        message: Vec<u8>,
        urgency: group_call::SignalingMessageUrgency,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn send_call_message_to_group(
        &self,
        group_id: Vec<u8>,
        message: Vec<u8>,
        urgency: group_call::SignalingMessageUrgency,
        recipients_override: HashSet<UserId>,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn create_incoming_media(
        &self,
        connection: &Connection<Self>,
        incoming_media: MediaStream,
    ) -> anyhow::Result<Self::AppIncomingMedia> {
        todo!()
    }

    fn connect_incoming_media(
        &self,
        remote_peer: &Self::AppRemotePeer,
        app_call_context: &Self::AppCallContext,
        incoming_media: &Self::AppIncomingMedia,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn compare_remotes(
        &self,
        remote_peer1: &Self::AppRemotePeer,
        remote_peer2: &Self::AppRemotePeer,
    ) -> anyhow::Result<bool> {
        todo!()
    }

    fn on_offer_expired(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
        age: Duration,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn on_call_concluded(
        &self,
        remote_peer: &Self::AppRemotePeer,
        call_id: CallId,
    ) -> anyhow::Result<()> {
        todo!()
    }

    fn group_call_ring_update(
        &self,
        group_id: group_call::GroupId,
        ring_id: group_call::RingId,
        sender: UserId,
        update: group_call::RingUpdate,
    ) {
        todo!()
    }

    fn request_membership_proof(&self, client_id: group_call::ClientId) {
        todo!()
    }

    fn request_group_members(&self, client_id: group_call::ClientId) {
        todo!()
    }

    fn handle_connection_state_changed(
        &self,
        client_id: group_call::ClientId,
        connection_state: group_call::ConnectionState,
    ) {
        todo!()
    }

    fn handle_network_route_changed(
        &self,
        client_id: group_call::ClientId,
        network_route: NetworkRoute,
    ) {
        todo!()
    }

    fn handle_join_state_changed(
        &self,
        client_id: group_call::ClientId,
        join_state: group_call::JoinState,
    ) {
        todo!()
    }

    fn handle_remote_devices_changed(
        &self,
        client_id: group_call::ClientId,
        remote_device_states: &[group_call::RemoteDeviceState],
        _reason: group_call::RemoteDevicesChangedReason,
    ) {
        todo!()
    }

    fn handle_incoming_video_track(
        &self,
        client_id: group_call::ClientId,
        remote_demux_id: DemuxId,
        incoming_video_track: VideoTrack,
    ) {
        todo!()
    }

    fn handle_peek_changed(
        &self,
        client_id: group_call::ClientId,
        peek_info: &PeekInfo,
        joined_members: &HashSet<UserId>,
    ) {
        todo!()
    }

    fn handle_reactions(&self, client_id: group_call::ClientId, reactions: Vec<Reaction>) {
        todo!()
    }

    fn handle_raised_hands(&self, client_id: group_call::ClientId, raised_hands: Vec<DemuxId>) {
        todo!()
    }

    fn handle_ended(&self, client_id: group_call::ClientId, reason: group_call::EndReason) {
        todo!()
    }
}
