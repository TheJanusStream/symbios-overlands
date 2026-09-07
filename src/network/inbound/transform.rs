//! `Transform` — a peer's pose sample (#1161).
//!
//! The highest-rate message on the wire by a wide margin, and the only one
//! whose entire validation lives upstream: [`push_sample`](bevy_symbios_multiuser::smoother::TransformBuffer::push_sample)
//! performs the NaN/Inf rejection, the magnitude clamp, the quaternion
//! normalisation and the playout-timestamp anchoring, so the worst a
//! malicious peer achieves here is a silently discarded packet.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::PeerId;

use super::{InboundBuffers, PeerParts};

/// Apply one peer's pose sample to its jitter buffer.
///
/// The parameter list is the point (#1161): it is the complete set of state
/// a `Transform` message can reach. A ctx struct carrying every one of the
/// system's seventeen parameters would have been shorter to write and
/// strictly worse to audit.
pub(super) fn handle(
    sender: PeerId,
    position: [f32; 3],
    rotation: [f32; 4],
    peers: &mut Query<PeerParts>,
    bufs: &mut InboundBuffers,
    metrics: &mut crate::diagnostics::MetricsRegistry,
    now: f64,
) {
    // Hand the wire-supplied pose to the upstream jitter buffer.
    // `push_sample` performs every guard the local code used to
    // do inline (NaN / Inf rejection, magnitude clamp via
    // `max_coord_abs`, quaternion normalisation, playout-timestamp
    // anchoring against same-frame bursts and clock-skew drift)
    // so the worst a malicious peer can do is have their packet
    // silently discarded.
    for (entity, peer, _tf, mut buf) in peers.iter_mut() {
        if peer.peer_id == sender {
            let accepted = buf.push_sample(
                Vec3::from_array(position),
                Quat::from_array(rotation),
                now,
                &bufs.smoother_cfg.0,
            );
            // A rejected sample (NaN/Inf or out-of-bounds) is silently
            // discarded by the smoother; count it (E-4).
            if !accepted {
                crate::diagnostics::samplers::transform_rejected(metrics);
                continue;
            }
            // The liveness fact the client already had and never
            // asked for (#1224 f335) — the jitter buffer stops
            // receiving, and nothing read that. Written to
            // `PeerResolve`, never to `RemotePeer`: a per-packet
            // write through a `Mut<RemotePeer>` would raise
            // `Changed<RemotePeer>` continuously.
            if let Ok(mut resolve) = bufs.resolve.get_mut(entity) {
                resolve.last_sample_at = Some(now);
            }
        }
    }
}
