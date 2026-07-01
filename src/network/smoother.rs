//! Per-peer transform playout: hands the buffered samples to the upstream
//! `bevy_symbios_multiuser::smoother` evaluator.
//!
//! When `LocalSettings::smooth_kinematics` is on we read the cubic-Hermite
//! interpolated playout via [`TransformBuffer::smoothed_at`]; when it's off
//! we snap straight to the most recent sample via
//! [`TransformBuffer::latest_snap`] for raw network-quality debugging.

use bevy::prelude::*;
use bevy_symbios_multiuser::prelude::*;

use crate::state::{LocalSettings, RemotePeer};

use super::SmootherConfigRes;

pub(super) fn smooth_remote_transforms(
    time: Res<Time>,
    settings: Res<LocalSettings>,
    cfg: Res<SmootherConfigRes>,
    mut peers: Query<(&mut Transform, &mut TransformBuffer), With<RemotePeer>>,
    mut metrics: ResMut<crate::diagnostics::MetricsRegistry>,
) {
    let now = time.elapsed_secs_f64();
    let mut smoothed_any = false;
    for (mut tf, mut buf) in peers.iter_mut() {
        let pose = if settings.smooth_kinematics {
            buf.smoothed_at(now, &cfg.0)
        } else {
            buf.latest_snap()
        };
        if let Some((position, rotation)) = pose {
            tf.translation = position;
            tf.rotation = rotation;
            smoothed_any = true;
        }
    }
    // Sample the jitter-buffer playout latency (E-4) while remote peers are
    // actually being played out. The upstream `TransformBuffer` doesn't expose a
    // played-out sample's age, so this records the configured `render_delay_secs`
    // (the delay the buffer deliberately introduces to absorb jitter) rather than
    // a measured per-peer staleness.
    if smoothed_any {
        crate::diagnostics::samplers::jitter_playout_latency_secs(
            &mut metrics,
            cfg.0.render_delay_secs,
        );
    }
}
