//! P2P networking plugin: peer lifecycle, inbound dispatch, outbound
//! throttling, and the jitter-buffered kinematic smoother.
//!
//! Outbound `Transform` broadcasts are driven by `FixedUpdate` (not `Update`)
//! so the packet rate is independent of render FPS.  When the local avatar
//! is nearly stationary the broadcast rate drops from ~64 Hz to ~2 Hz to
//! save bandwidth and downstream CPU — with a forced "final frame"
//! broadcast on the tick we cross into rest so remote peers land on the
//! true parked pose.
//!
//! Inbound `Transform` samples are pushed into a per-peer ring buffer and
//! replayed `KINEMATIC_RENDER_DELAY_SECS` in the past; the playout position
//! is resolved with a cubic Hermite spline whose endpoint tangents come from
//! central differences of the buffered samples.  Identity messages are
//! authenticated against the relay-signed `PeerSessionMapRes` so a peer
//! cannot impersonate another DID over the unauthenticated data channel.
//!
//! Avatar records are sovereign: after a peer announces its DID, we spawn
//! an async `fetch_avatar_record` task against that peer's PDS. A live
//! preview nudge via `AvatarStateUpdate` lets remote peers mirror
//! mid-slider edits before the author presses "Publish".
//!
//! Peer-to-peer item offers ([`crate::protocol::OverlandsMessage::ItemOffer`]
//! / [`crate::protocol::OverlandsMessage::ItemOfferResponse`]) are
//! dispatched by [`inbound::handle_incoming_messages`]. The handler
//! authenticates both sender and responder DIDs against the same
//! relay-signed session map, sanitises the inbound `Generator` against the
//! shared `RoomRecord` clamps, silently auto-declines muted senders,
//! busy-gates concurrent offers so a single
//! [`crate::state::IncomingOfferDialog`] is ever active, and matches
//! responses against [`crate::state::PendingOutgoingOffers`] so a
//! third-party peer cannot race a forged "accepted" reply onto the wire.
//!
//! ## Sub-module map
//!
//! * [`peer_cache`] — DID-keyed [`PeerAvatarCache`] + the async
//!   peer-avatar fetch task and its drainer.
//! * [`lifecycle`] — peer connect/disconnect, stale-offer-dialog evictor,
//!   mute-visibility sync.
//! * [`inbound`] — [`inbound::handle_incoming_messages`] dispatcher.
//! * [`broadcast`] — outbound `Transform` / `Identity` /
//!   `AvatarStateUpdate` / `RoomStateUpdate` writers.
//! * [`chunk`] — app-layer fragmentation/reassembly that carries reliable
//!   messages past WebRTC's 64 KiB SCTP message ceiling (#716).
//! * [`smoother`] — jitter-buffered playout (cubic Hermite spline).

mod broadcast;
pub mod chunk;
mod inbound;
mod lifecycle;
mod peer_cache;
mod smoother;

pub use peer_cache::PeerAvatarCache;

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::config;
use crate::protocol::OverlandsMessage;
use crate::state::AppState;

/// Bevy resource holding the [`SmootherConfig`] for the per-peer transform
/// jitter buffer. Constructed once at plugin build time from the constants
/// in [`crate::config::network`] so the upstream smoother can be tuned for
/// our broadcast cadence and play-space bounds without forking the module.
///
/// The one value that is *not* a static constant is the expected inter-sample
/// spacing: it is captured from the live `FixedUpdate` timestep (see
/// [`SmootherConfigRes::from_fixed_timestep`]) because broadcasts fire once
/// per fixed tick, so the buffer's assumed cadence must equal the actual tick
/// rate or the synthetic playout clock drifts against wall clock.
#[derive(Resource, Clone, Copy)]
pub struct SmootherConfigRes(pub SmootherConfig);

impl SmootherConfigRes {
    /// Build the smoother config, taking the jitter buffer's expected
    /// inter-sample spacing from the *actual* `FixedUpdate` timestep.
    ///
    /// [`broadcast::broadcast_local_state`] emits one Transform per fixed
    /// tick, so `timestep_secs` is exactly the true inter-broadcast spacing.
    /// Feeding it straight into `expected_send_interval_secs` makes the
    /// upstream `(last + expected).max(now)` playout anchor track wall clock
    /// with zero systematic drift — regardless of whether the fixed rate is
    /// Bevy's 64 Hz default or an explicit override — so the playout timeline
    /// never accumulates toward the `MAX_JITTER_DRIFT_SECS` rebase ceiling.
    pub fn from_fixed_timestep(timestep_secs: f64) -> Self {
        Self(SmootherConfig {
            buffer_capacity: config::network::KINEMATIC_BUFFER_CAPACITY,
            expected_send_interval_secs: timestep_secs,
            max_jitter_drift_secs: config::network::MAX_JITTER_DRIFT_SECS,
            render_delay_secs: config::network::KINEMATIC_RENDER_DELAY_SECS,
            max_coord_abs: config::network::MAX_REMOTE_COORD_ABS,
        })
    }
}

impl Default for SmootherConfigRes {
    /// Fallback used only if `Time<Fixed>` is unavailable at build time; the
    /// constant mirrors Bevy's default fixed timestep. In practice the plugin
    /// always reads the live timestep — see [`NetworkPlugin::build`].
    fn default() -> Self {
        Self::from_fixed_timestep(config::network::EXPECTED_BROADCAST_INTERVAL_SECS)
    }
}

/// Read the real `FixedUpdate` timestep (seconds) that transform broadcasts
/// will actually run at — this is the true inter-broadcast spacing, since
/// [`broadcast::broadcast_local_state`] emits one packet per fixed tick.
///
/// Falls back to the mirrored default constant only if `Time<Fixed>` is
/// unavailable. Factored out of [`NetworkPlugin::build`] so the drift-critical
/// "read the live timestep, never assume it" behaviour is unit-testable.
fn broadcast_interval_secs(world: &World) -> f64 {
    world
        .get_resource::<Time<Fixed>>()
        .map(|t| t.timestep().as_secs_f64())
        .unwrap_or(config::network::EXPECTED_BROADCAST_INTERVAL_SECS)
}

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        // Capture the real `FixedUpdate` timestep so the jitter buffer's
        // expected send interval equals the true broadcast cadence (one packet
        // per fixed tick). `Time<Fixed>` is inserted by Bevy's `TimePlugin`,
        // which builds with `DefaultPlugins` before this plugin.
        let fixed_timestep_secs = broadcast_interval_secs(app.world());

        app.add_plugins(SymbiosMultiuserPlugin::<OverlandsMessage>::deferred())
            .init_resource::<PeerAvatarCache>()
            // #716: buffer for reassembling chunked reliable payloads, and the
            // monotonic counter that stamps outbound chunk `msg_id`s.
            .init_resource::<chunk::ChunkReassembly>()
            .init_resource::<chunk::OutboundChunkSeq>()
            .insert_resource(SmootherConfigRes::from_fixed_timestep(fixed_timestep_secs))
            .add_systems(
                Update,
                (
                    lifecycle::handle_peer_connections,
                    inbound::handle_incoming_messages,
                    peer_cache::poll_peer_avatar_fetches,
                    lifecycle::evict_stale_offer_dialog,
                    lifecycle::dismiss_offer_dialog_from_muted_sender,
                    lifecycle::sweep_stale_pending_offers,
                    smoother::smooth_remote_transforms,
                    lifecycle::sync_mute_visibility,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            // Network broadcast is tied to a fixed tick so the outbound rate
            // is independent of rendering FPS — otherwise a 144 Hz monitor
            // would blast peers with 2.4× the intended packet rate and a
            // 30 Hz machine would stutter.
            .add_systems(
                FixedUpdate,
                broadcast::broadcast_local_state.run_if(in_state(AppState::InGame)),
            )
            // Live-preview avatar and room updates piggyback on `Update` so
            // they fire the frame an editor slider changes the resource,
            // rather than waiting for the next FixedUpdate tick.
            .add_systems(
                Update,
                (
                    broadcast::broadcast_avatar_state,
                    broadcast::broadcast_room_state,
                )
                    .run_if(in_state(AppState::InGame)),
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::MinimalPlugins;

    /// Nanosecond quantisation tolerance for a `Hz -> Duration -> secs`
    /// round-trip (`1/30` is not exactly representable). Far tighter than the
    /// gap between any two candidate cadences (1/30 vs 1/64 differ by ~0.018).
    const ROUND_TRIP_TOL: f64 = 1e-6;

    /// The load-bearing #630 guard: the broadcast interval is read from the
    /// *live* `Time<Fixed>`, not a hardcoded value. Installing a non-default
    /// 30 Hz timestep must flow through to the buffer cadence — a regression
    /// that reverts to a constant (the original `1/60`, or `init_resource`'s
    /// default) would still report ~1/64 here and fail.
    #[test]
    fn build_reads_the_live_fixed_timestep_not_a_constant() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.insert_resource(Time::<Fixed>::from_hz(30.0));

        let secs = broadcast_interval_secs(app.world());
        assert!(
            (secs - 1.0 / 30.0).abs() < ROUND_TRIP_TOL,
            "must track the live 30 Hz timestep, got {secs}s"
        );

        let cfg = SmootherConfigRes::from_fixed_timestep(secs);
        assert!(
            (cfg.0.expected_send_interval_secs - secs).abs() < f64::EPSILON,
            "buffer cadence must equal the timestep it was built from"
        );
    }

    /// With the stock plugin set, the read yields Bevy's 64 Hz default — the
    /// cadence transform broadcasts actually run at — proving the historical
    /// `1/60` assumption (which drifted +6.7%/s into the rebase ceiling) is
    /// gone. `1/64` is a power of two, so the round-trip is exact.
    #[test]
    fn default_fixed_timestep_is_64hz() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        let secs = broadcast_interval_secs(app.world());
        assert!(
            (secs - 1.0 / 64.0).abs() < f64::EPSILON,
            "expected Bevy's 64 Hz default, got {secs}s"
        );
    }

    /// When `Time<Fixed>` is absent the read falls back to the constant, which
    /// must mirror Bevy's default so the fallback never silently reintroduces
    /// the #630 drift.
    #[test]
    fn falls_back_to_the_bevy_default_when_time_fixed_absent() {
        let world = World::new();
        let secs = broadcast_interval_secs(&world);
        assert!(
            (secs - config::network::EXPECTED_BROADCAST_INTERVAL_SECS).abs() < f64::EPSILON,
            "fallback must be the mirrored default constant"
        );
        let default_timestep = Time::<Fixed>::default().timestep().as_secs_f64();
        assert!(
            (config::network::EXPECTED_BROADCAST_INTERVAL_SECS - default_timestep).abs()
                < f64::EPSILON,
            "fallback constant must equal the default fixed timestep"
        );
    }
}
