//! Jitter-buffered playout for remote-peer transforms. Cubic Hermite
//! spline over central-difference tangents on the buffered samples,
//! evaluated `KINEMATIC_RENDER_DELAY_SECS` in the past so packets have
//! had time to arrive before the camera reads their position.

use bevy::prelude::*;

use crate::config;
use crate::state::{LocalSettings, RemotePeer, TransformBuffer};

/// Resolve each remote peer's displayed transform from the jitter buffer.
///
/// When `smooth_kinematics` is enabled we evaluate a cubic Hermite spline at
/// `now - KINEMATIC_RENDER_DELAY_SECS`, using central-difference tangents of
/// the buffered samples for the translation and `Quat::slerp` for the
/// rotation.  When disabled, we snap straight to the most recent sample — a
/// useful debugging mode for observing raw network latency.
pub(super) fn smooth_remote_transforms(
    time: Res<Time>,
    settings: Res<LocalSettings>,
    mut peers: Query<(&mut Transform, &mut TransformBuffer), With<RemotePeer>>,
) {
    let now = time.elapsed_secs_f64();
    let render_time = now - config::network::KINEMATIC_RENDER_DELAY_SECS;

    for (mut tf, mut buf) in peers.iter_mut() {
        if buf.samples.is_empty() {
            continue;
        }

        // Raw-snap mode — just follow the latest packet and keep the buffer
        // trimmed so a later mode flip doesn't jump back in time.
        if !settings.smooth_kinematics {
            if let Some(last) = buf.samples.back() {
                tf.translation = last.position;
                tf.rotation = last.rotation;
            }
            // Drop all but the most recent sample to bound memory.
            while buf.samples.len() > 1 {
                buf.samples.pop_front();
            }
            continue;
        }

        // Evict samples that are clearly older than render_time to avoid
        // unbounded growth while keeping at least one sample on either side.
        let prune_cutoff =
            render_time - 2.0 * config::network::KINEMATIC_RENDER_DELAY_SECS.max(0.05);
        while buf.samples.len() > 2
            && buf.samples.get(1).map(|s| s.timestamp).unwrap_or(f64::MAX) < prune_cutoff
        {
            buf.samples.pop_front();
        }

        // Find the segment [i, i+1] that brackets render_time.  If render_time
        // is before the first sample we simply snap to the earliest; if it's
        // past the last, we extrapolate by snapping to the latest.
        let samples = &buf.samples;
        if samples.len() == 1 || render_time <= samples.front().unwrap().timestamp {
            let s = samples.front().unwrap();
            tf.translation = s.position;
            tf.rotation = s.rotation;
            continue;
        }
        if render_time >= samples.back().unwrap().timestamp {
            let s = samples.back().unwrap();
            tf.translation = s.position;
            tf.rotation = s.rotation;
            continue;
        }

        // Walk to find the bracketing pair.
        let mut i = 0;
        while i + 1 < samples.len() && samples[i + 1].timestamp < render_time {
            i += 1;
        }
        let a = samples[i];
        let b = samples[i + 1];
        let dt = (b.timestamp - a.timestamp).max(1e-6);
        let t = ((render_time - a.timestamp) / dt).clamp(0.0, 1.0) as f32;

        // Estimate velocity tangents with a central difference.  Fall back to
        // forward/backward differences at the ends of the buffer so we always
        // have a well-defined tangent.
        let dt_f = dt as f32;
        let tangent_a = if i > 0 {
            let prev = samples[i - 1];
            let total = (b.timestamp - prev.timestamp).max(1e-6) as f32;
            (b.position - prev.position) / total * dt_f
        } else {
            b.position - a.position
        };
        let tangent_b = if i + 2 < samples.len() {
            let next = samples[i + 2];
            let total = (next.timestamp - a.timestamp).max(1e-6) as f32;
            (next.position - a.position) / total * dt_f
        } else {
            b.position - a.position
        };

        // Cubic Hermite basis.  Equivalent to bevy_math::CubicHermite over a
        // single segment but skips the Vec allocation and Result unwrapping.
        let t2 = t * t;
        let t3 = t2 * t;
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;
        tf.translation = a.position * h00 + tangent_a * h10 + b.position * h01 + tangent_b * h11;
        tf.rotation = a.rotation.slerp(b.rotation, t);
    }
}
