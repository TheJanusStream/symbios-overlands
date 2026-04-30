//! P2P networking plugin: peer lifecycle, inbound dispatch, outbound
//! throttling, and the jitter-buffered kinematic smoother.
//!
//! Outbound `Transform` broadcasts are driven by `FixedUpdate` (not `Update`)
//! so the packet rate is independent of render FPS.  When the local rover is
//! nearly stationary the broadcast rate drops from ~60 Hz to ~2 Hz to save
//! bandwidth and downstream CPU — with a forced "final frame" broadcast on
//! the tick we cross into rest so remote peers land on the true parked pose.
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
//! * [`smoother`] — jitter-buffered playout (cubic Hermite spline).

mod broadcast;
mod inbound;
mod lifecycle;
mod peer_cache;
mod smoother;

pub use peer_cache::PeerAvatarCache;

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::protocol::OverlandsMessage;
use crate::state::AppState;

pub struct NetworkPlugin;

impl Plugin for NetworkPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(SymbiosMultiuserPlugin::<OverlandsMessage>::deferred())
            .init_resource::<PeerAvatarCache>()
            .add_systems(
                Update,
                (
                    lifecycle::handle_peer_connections,
                    inbound::handle_incoming_messages,
                    peer_cache::poll_peer_avatar_fetches,
                    lifecycle::evict_stale_offer_dialog,
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
