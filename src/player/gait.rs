//! Cosmetic gait animation for humanoid avatars — the consumer of the
//! seeded [`AvatarGait`] bounce / sway / head-turn fields (#659).
//!
//! Animates the avatar's **visual root** (the [`AvatarVisualRoot`] child
//! the spawner tags under the chassis), never the physics body, so the
//! collider, camera anchor and network transform stay untouched:
//!
//! - **Walking** — a per-footfall vertical bounce whose phase advances
//!   with the avatar's actual horizontal speed (a wading, speed-scaled
//!   stride slows its bob instead of jittering in place).
//! - **Idle** — a slow positional sway (weight shifting) plus a gentle
//!   look-around yaw sized by the gait's head-turn variance. The yaw is
//!   applied at the root for want of a per-part head handle; at ≤20° it
//!   reads as the figure glancing about.
//!
//! All offsets compose onto the root's authored base transform captured
//! in [`AvatarVisualRoot`], so avatar-editor rebuilds (which respawn the
//! root) and preset hot-swaps stay drift-free. Amplitudes derive from the
//! owner DID via [`AvatarGait::for_did`] — the same derivation the seeded
//! locomotion defaults use.
//!
//! The local player's gait pauses (root held at the rest pose) while the
//! Avatar editor window is open (#741) — see [`animate_humanoid_gait`]
//! for why. Remote peers are unaffected.

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::seeded_defaults::AvatarGait;
use crate::state::{LocalPlayer, RemotePeer};
use crate::world_builder::AvatarVisualRoot;

use super::HumanoidPreset;

/// Horizontal speed (m/s) above which the avatar counts as walking for
/// the bounce/sway crossfade.
const MOVING_SPEED_THRESHOLD: f32 = 0.3;
/// Crossfade rate (1/s) between the idle and walking animation poses —
/// fast enough to feel responsive, slow enough not to pop.
const BLEND_RATE: f32 = 6.0;
/// Idle look-around frequency (Hz) — deliberately much slower than the
/// weight-shift sway so the two don't read as one wobble.
const HEAD_TURN_FREQ_HZ: f32 = 0.08;
/// The nominal cadence↔walk-speed mapping the seeded locomotion default
/// uses (`walk_speed = 4.0 * cadence / 2.2`); the bounce phase advances
/// at full rate when moving at that speed.
const NOMINAL_SPEED_PER_CADENCE: f32 = 4.0 / 2.2;

/// Per-avatar gait-animation state, attached to humanoid chassis
/// entities (local and remote) by [`attach_gait_animation`] and stripped
/// with the preset markers on hot-swap.
#[derive(Component)]
pub struct GaitAnimation {
    gait: AvatarGait,
    /// Step phase in footfalls (wraps at 1.0).
    phase: f32,
    /// Smoothed 0 = idle … 1 = walking crossfade.
    moving_blend: f32,
    /// Last frame's chassis translation — the finite-difference speed
    /// fallback for remote peers (no `LinearVelocity` component).
    prev_pos: Option<Vec3>,
    /// Per-avatar phase offset so a crowd doesn't sway in lockstep.
    salt: f32,
}

impl GaitAnimation {
    fn for_did(did: &str) -> Self {
        let gait = AvatarGait::for_did(did);
        // Cheap stable per-DID phase offset in [0, τ).
        let hash = crate::seeded_defaults::hash::fnv1a_64(did);
        let salt = (hash >> 32) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
        Self {
            gait,
            phase: 0.0,
            moving_blend: 0.0,
            prev_pos: None,
            salt,
        }
    }
}

/// Whether a remote peer currently renders a humanoid chassis (the
/// record's locomotion selects the visual family).
fn peer_is_humanoid(peer: &RemotePeer) -> bool {
    matches!(
        peer.avatar.as_ref().map(|a| &a.locomotion),
        Some(LocomotionConfig::Humanoid(_))
    )
}

/// Lazily attach [`GaitAnimation`] to humanoid avatars that lack it: the
/// local player once its `HumanoidPreset` marker lands, remote peers once
/// their avatar record resolves to a humanoid. Runs every frame; the
/// `Without` filter makes the steady state a no-op.
#[allow(clippy::type_complexity)]
pub(super) fn attach_gait_animation(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    locals: Query<
        Entity,
        (
            With<LocalPlayer>,
            With<HumanoidPreset>,
            Without<GaitAnimation>,
        ),
    >,
    remotes: Query<(Entity, &RemotePeer), Without<GaitAnimation>>,
) {
    if let Some(session) = session.as_deref() {
        for entity in &locals {
            commands
                .entity(entity)
                .insert(GaitAnimation::for_did(&session.did));
        }
    }
    for (entity, peer) in &remotes {
        if !peer_is_humanoid(peer) {
            continue;
        }
        let Some(did) = peer.did.as_deref() else {
            continue;
        };
        commands.entity(entity).insert(GaitAnimation::for_did(did));
    }
}

/// Drive the bounce / sway / look-around offsets onto each animated
/// avatar's visual root. Removes [`GaitAnimation`] from avatars that are
/// no longer humanoid (remote record hot-swap) — their fresh visual root
/// spawns unanimated, so no offset lingers.
///
/// While the local player's Avatar editor window is open (or, belt and
/// braces, a visuals row is still selected during the close-frame gap),
/// its gait pauses and the visual root is held at the authored rest pose
/// (#737, widened to the whole window by #741). Sway is time-based, so it
/// would keep oscillating right through the physics freeze — moving every
/// part of the avatar *except* the gizmo-detached prim being edited, and
/// drifting the parent transforms the drag commit's world→local
/// conversion reads. Selection-scoped pausing wasn't enough (#741): the
/// sway ran again the moment a row was deselected or the selection moved
/// between rows, so the rendered pose shifted under the editor between
/// edits while the record's transforms stayed put — rest pose now holds
/// for the whole editing session, walking bounce included. Snapping to
/// the base pose (rather than holding the mid-sway offset) means the
/// owner edits the avatar in its neutral stance; collapsing the window
/// (not just closing it) resumes the live animation for previewing.
/// Per-entity rather than a `run_if` so remote peers keep swaying while
/// the owner edits.
#[allow(clippy::type_complexity)]
pub(super) fn animate_humanoid_gait(
    time: Res<Time>,
    mut commands: Commands,
    avatar_editor: Option<Res<crate::ui::avatar::AvatarEditorState>>,
    mut avatars: Query<(
        Entity,
        &Children,
        &Transform,
        Option<&LinearVelocity>,
        Option<&RemotePeer>,
        Has<HumanoidPreset>,
        Has<LocalPlayer>,
        &mut GaitAnimation,
    )>,
    mut roots: Query<(&AvatarVisualRoot, &mut Transform), Without<GaitAnimation>>,
) {
    let dt = time.delta_secs();
    let t = time.elapsed_secs();
    let hold_rest_pose = avatar_editor
        .map(|e| e.window_visible() || e.has_visuals_selection())
        .unwrap_or(false);

    for (entity, children, chassis_tf, velocity, peer, has_humanoid_preset, is_local, mut anim) in
        &mut avatars
    {
        // Local avatars are gated by the preset marker, remote ones by
        // their current record — drop the animation state when neither
        // holds any more (e.g. the peer hot-swapped to a boat).
        let still_humanoid = has_humanoid_preset || peer.map(peer_is_humanoid).unwrap_or(false);
        if !still_humanoid {
            commands.entity(entity).remove::<GaitAnimation>();
            continue;
        }

        // Editing freeze: hold the local avatar at its rest pose. Written
        // every frame (not edge-triggered) so a visuals rebuild mid-edit
        // re-neutralizes the freshly-spawned root — same state-synced
        // shape as the chassis freeze in `player::mod`. When the *root*
        // node itself is gizmo-detached it is no longer in `children`,
        // so this can't fight the gizmo for its transform.
        if is_local && hold_rest_pose {
            for child in children.iter() {
                if let Ok((root, mut tf)) = roots.get_mut(child) {
                    tf.translation = root.base_translation;
                    tf.rotation = root.base_rotation;
                    break;
                }
            }
            continue;
        }

        // Horizontal speed: physics velocity when present (local player),
        // else a 1-frame finite difference of the replicated transform.
        let speed = match velocity {
            Some(v) => Vec2::new(v.0.x, v.0.z).length(),
            None => {
                let prev = anim.prev_pos.replace(chassis_tf.translation);
                match prev {
                    Some(p) if dt > 1e-6 => {
                        let d = chassis_tf.translation - p;
                        Vec2::new(d.x, d.z).length() / dt
                    }
                    _ => 0.0,
                }
            }
        };

        let (offset, yaw) = anim.advance(dt, t, speed);

        // The chassis's one visual-root child (bursts/FX children lack
        // the marker, so the lookup skips them).
        for child in children.iter() {
            if let Ok((root, mut tf)) = roots.get_mut(child) {
                tf.translation = root.base_translation + offset;
                tf.rotation = root.base_rotation * Quat::from_rotation_y(yaw);
                break;
            }
        }
    }
}

impl GaitAnimation {
    /// Advance the phase/blend state and return this frame's root-local
    /// `(translation offset, yaw)` — pure math, unit-tested below.
    fn advance(&mut self, dt: f32, t: f32, speed: f32) -> (Vec3, f32) {
        use std::f32::consts::TAU;
        let g = self.gait;

        let moving = if speed > MOVING_SPEED_THRESHOLD {
            1.0
        } else {
            0.0
        };
        self.moving_blend += (moving - self.moving_blend) * (BLEND_RATE * dt).min(1.0);

        // Footfalls per second: the seeded cadence, scaled by how fast the
        // avatar actually moves relative to the cadence's nominal walk
        // speed (wading halves it, standing stops it).
        let nominal_speed = (g.step_cadence * NOMINAL_SPEED_PER_CADENCE).max(0.1);
        let step_rate = g.step_cadence * (speed / nominal_speed).clamp(0.0, 1.6);
        self.phase = (self.phase + step_rate * dt).fract();

        let walk = self.moving_blend;
        let idle = 1.0 - walk;
        let ts = t + self.salt;

        // One smooth rise-and-fall per footfall.
        let bounce = g.step_bounce_amplitude * 0.5 * (1.0 - (self.phase * TAU).cos()) * walk;
        // Idle weight shift: a lateral figure-eight, X dominant.
        let sway_phase = TAU * g.idle_sway_frequency * ts;
        let sway_x = g.idle_sway_amplitude * sway_phase.sin() * idle;
        let sway_z = g.idle_sway_amplitude * 0.6 * (sway_phase * 0.63 + 1.7).sin() * idle;
        // Idle look-around, ± the seeded variance.
        let yaw = g.head_turn_variance_degrees.to_radians()
            * (TAU * HEAD_TURN_FREQ_HZ * ts + 0.9 * self.salt).sin()
            * idle;

        (Vec3::new(sway_x, bounce, sway_z), yaw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim() -> GaitAnimation {
        GaitAnimation::for_did("did:plc:gait-test")
    }

    #[test]
    fn deterministic_per_did() {
        let a = anim();
        let b = anim();
        assert_eq!(a.gait.step_cadence, b.gait.step_cadence);
        assert_eq!(a.salt, b.salt);
    }

    #[test]
    fn standing_still_converges_to_pure_idle() {
        let mut a = anim();
        // Settle the blend at zero speed.
        for _ in 0..200 {
            a.advance(1.0 / 60.0, 1.0, 0.0);
        }
        let (offset, yaw) = a.advance(1.0 / 60.0, 1.0, 0.0);
        // No bounce; sway + yaw bounded by the seeded amplitudes.
        assert!(offset.y.abs() <= a.gait.step_bounce_amplitude * 0.01);
        assert!(offset.x.abs() <= a.gait.idle_sway_amplitude + 1e-6);
        assert!(offset.z.abs() <= a.gait.idle_sway_amplitude + 1e-6);
        assert!(yaw.abs() <= a.gait.head_turn_variance_degrees.to_radians() + 1e-6);
    }

    #[test]
    fn walking_bounces_and_suppresses_idle_motion() {
        let mut a = anim();
        let mut max_bounce: f32 = 0.0;
        let mut last = (Vec3::ZERO, 0.0);
        // Walk at the nominal speed for a while; sample the peak bounce.
        let speed = a.gait.step_cadence * NOMINAL_SPEED_PER_CADENCE;
        for i in 0..600 {
            last = a.advance(1.0 / 60.0, i as f32 / 60.0, speed);
            max_bounce = max_bounce.max(last.0.y);
        }
        // The bounce reaches (most of) its seeded amplitude…
        assert!(max_bounce > a.gait.step_bounce_amplitude * 0.5);
        assert!(max_bounce <= a.gait.step_bounce_amplitude + 1e-6);
        // …and the idle sway/yaw are blended out while walking.
        assert!(last.0.x.abs() < a.gait.idle_sway_amplitude * 0.05);
        assert!(last.1.abs() < a.gait.head_turn_variance_degrees.to_radians() * 0.05);
    }

    #[test]
    fn phase_stops_advancing_at_zero_speed() {
        let mut a = anim();
        a.advance(0.5, 0.0, 2.0);
        let p = a.phase;
        a.advance(0.5, 0.5, 0.0);
        assert_eq!(a.phase, p, "no footfalls while standing");
    }
}
