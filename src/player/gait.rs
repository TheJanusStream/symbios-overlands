//! Cosmetic idle / gait animation for *every* avatar family — the consumer
//! of the seeded [`AvatarGait`] bounce / sway / head-turn fields (#659,
//! extended to vehicles by #797).
//!
//! Animates the avatar's **visual root** (the [`AvatarVisualRoot`] child
//! the spawner tags under the chassis), never the physics body, so the
//! collider, camera anchor and network transform stay untouched. The
//! [`GaitMode`] — chosen from the runtime locomotion *preset* — selects the
//! profile:
//!
//! - **Humanoid** — a per-footfall vertical bounce (phase advances with the
//!   avatar's actual horizontal speed) crossfaded with an idle weight-shift
//!   sway + gentle look-around yaw (sized by the gait's head-turn variance).
//! - **Boat** — an always-on slow hull heave + gentle list (roll).
//! - **Airship** — a lazy nose-wander yaw + slight vertical drift.
//! - **Skiff** — a faint suspension shiver at idle, crossfading into a
//!   banking lean into turns under way (roll ∝ yaw-rate × speed).
//!
//! The vehicle modes reuse the same seeded [`AvatarGait`] amplitudes as the
//! humanoid (no new seeded fields — the per-avatar individuality is free),
//! scaled into vehicle ranges by the [`veh`] constants.
//!
//! All offsets compose onto the root's authored base transform captured
//! in [`AvatarVisualRoot`], so avatar-editor rebuilds (which respawn the
//! root) and preset hot-swaps stay drift-free. Amplitudes derive from the
//! owner DID via [`AvatarGait::for_did`] — the same derivation the seeded
//! locomotion defaults use.
//!
//! The local player's gait pauses (root held at the rest pose) while the
//! Avatar editor window is open (#741) — see [`animate_avatar_gait`]
//! for why. Remote peers are unaffected.

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;

use crate::pds::LocomotionConfig;
use crate::seeded_defaults::AvatarGait;
use crate::state::{LocalPlayer, RemotePeer};
use crate::world_builder::AvatarVisualRoot;

use super::{CarPreset, HelicopterPreset, HoverBoatPreset, HumanoidPreset};

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

/// Vehicle idle-motion tuning. The vehicle modes reuse the seeded
/// [`AvatarGait`] amplitudes (no new seeded fields — the individuality is
/// free), scaled into vehicle-appropriate ranges by these constants.
mod veh {
    /// Boat hull heave (vertical bob) as a multiple of `idle_sway_amplitude`
    /// — a hull rides a swell far more than a person shifts weight.
    pub const BOAT_HEAVE: f32 = 3.0;
    /// Boat list (roll about the fore-aft axis) as a multiple of
    /// `idle_sway_amplitude`, in radians (≈1–5°).
    pub const BOAT_ROLL: f32 = 3.5;
    /// Boat swell frequency floor (Hz) so a low-`idle_sway_frequency` seed
    /// still visibly rocks.
    pub const BOAT_SWELL_HZ: f32 = 0.35;
    /// Airship nose-wander yaw as a fraction of `head_turn_variance_degrees`.
    pub const AIRSHIP_YAW: f32 = 0.4;
    /// Airship drift frequency (Hz) — a slow lazy wander.
    pub const AIRSHIP_DRIFT_HZ: f32 = 0.06;
    /// Airship vertical drift as a multiple of `idle_sway_amplitude`.
    pub const AIRSHIP_HEAVE: f32 = 2.0;
    /// Skiff idle suspension shiver (vertical) as a multiple of
    /// `idle_sway_amplitude` — a small fast tremble at rest.
    pub const SKIFF_SHIVER: f32 = 0.6;
    /// Skiff shiver frequency (Hz) — an idling-engine buzz, much faster than
    /// the boat swell.
    pub const SKIFF_SHIVER_HZ: f32 = 9.0;
    /// Skiff banking lean gain: radians of roll per (rad/s of yaw-rate × m/s
    /// of speed). Turning harder / faster leans harder into the corner.
    pub const SKIFF_BANK_GAIN: f32 = 0.06;
    /// Skiff maximum banking lean (radians, ≈17°) so a hard swerve can't flip
    /// the visual on its side.
    pub const SKIFF_BANK_MAX: f32 = 0.3;
}

/// Which animation profile drives an avatar's visual root — chosen from the
/// runtime locomotion *preset* (physics), never the seeded chassis family, so
/// a boat-visualled avatar the owner drives as a car banks like a car (#797).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum GaitMode {
    /// Bipedal walk bounce + idle weight-shift / look-around.
    Humanoid,
    /// Slow hull heave + gentle list.
    Boat,
    /// Lazy nose-wander drift.
    Airship,
    /// Idle suspension shiver + banking lean into turns.
    Skiff,
}

impl GaitMode {
    /// The mode for a locomotion preset, or `None` for a preset with no idle
    /// profile (the fixed-wing [`super::AirplanePreset`], which rolls its own
    /// chassis, so a visual-root bank would double it).
    fn for_locomotion(loco: &LocomotionConfig) -> Option<Self> {
        match loco {
            LocomotionConfig::Humanoid(_) => Some(Self::Humanoid),
            LocomotionConfig::HoverBoat(_) => Some(Self::Boat),
            LocomotionConfig::Helicopter(_) => Some(Self::Airship),
            LocomotionConfig::Car(_) => Some(Self::Skiff),
            LocomotionConfig::Airplane(_) | LocomotionConfig::Unknown => None,
        }
    }
}

/// Per-avatar idle/gait-animation state, attached to any animated avatar
/// chassis (local and remote — every family with a [`GaitMode`]) by
/// [`attach_gait_animation`] and stripped with the preset markers on hot-swap.
#[derive(Component)]
pub struct GaitAnimation {
    gait: AvatarGait,
    /// Which per-family profile [`Self::advance`] drives.
    mode: GaitMode,
    /// Step phase in footfalls (wraps at 1.0).
    phase: f32,
    /// Smoothed 0 = idle … 1 = walking / under-way crossfade.
    moving_blend: f32,
    /// Last frame's chassis translation — the finite-difference speed
    /// fallback for remote peers (no `LinearVelocity` component).
    prev_pos: Option<Vec3>,
    /// Last frame's chassis yaw (radians) — the finite-difference yaw-rate
    /// fallback for remote peers (no `AngularVelocity`), for banking.
    prev_yaw: Option<f32>,
    /// Per-avatar phase offset so a crowd doesn't sway in lockstep.
    salt: f32,
}

impl GaitAnimation {
    fn for_did(did: &str, mode: GaitMode) -> Self {
        let gait = AvatarGait::for_did(did);
        // Cheap stable per-DID phase offset in [0, τ).
        let hash = crate::seeded_defaults::hash::fnv1a_64(did);
        let salt = (hash >> 32) as f32 / u32::MAX as f32 * std::f32::consts::TAU;
        Self {
            gait,
            mode,
            phase: 0.0,
            moving_blend: 0.0,
            prev_pos: None,
            prev_yaw: None,
            salt,
        }
    }
}

/// The gait profile a remote peer currently renders, from its record's
/// locomotion (which also selects the visual family), or `None` for a preset
/// with no idle profile.
fn peer_gait_mode(peer: &RemotePeer) -> Option<GaitMode> {
    peer.avatar
        .as_ref()
        .and_then(|a| GaitMode::for_locomotion(&a.locomotion))
}

/// The gait profile of the local player from its preset marker (present on the
/// chassis once `build_preset_components` lands).
fn local_gait_mode(
    has_humanoid: bool,
    has_boat: bool,
    has_airship: bool,
    has_skiff: bool,
) -> Option<GaitMode> {
    if has_humanoid {
        Some(GaitMode::Humanoid)
    } else if has_boat {
        Some(GaitMode::Boat)
    } else if has_airship {
        Some(GaitMode::Airship)
    } else if has_skiff {
        Some(GaitMode::Skiff)
    } else {
        None
    }
}

/// Lazily attach [`GaitAnimation`] to avatars that lack it: the local player
/// once one of its preset markers lands, remote peers once their avatar record
/// resolves to a preset with an idle profile. Runs every frame; the `Without`
/// filter makes the steady state a no-op.
#[allow(clippy::type_complexity)]
pub(super) fn attach_gait_animation(
    mut commands: Commands,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    locals: Query<
        (
            Entity,
            Has<HumanoidPreset>,
            Has<HoverBoatPreset>,
            Has<HelicopterPreset>,
            Has<CarPreset>,
        ),
        (With<LocalPlayer>, Without<GaitAnimation>),
    >,
    remotes: Query<(Entity, &RemotePeer), Without<GaitAnimation>>,
) {
    if let Some(session) = session.as_deref() {
        for (entity, human, boat, airship, skiff) in &locals {
            let Some(mode) = local_gait_mode(human, boat, airship, skiff) else {
                continue;
            };
            commands
                .entity(entity)
                .insert(GaitAnimation::for_did(&session.did, mode));
        }
    }
    for (entity, peer) in &remotes {
        let Some(mode) = peer_gait_mode(peer) else {
            continue;
        };
        let Some(did) = peer.did.as_deref() else {
            continue;
        };
        commands
            .entity(entity)
            .insert(GaitAnimation::for_did(did, mode));
    }
}

/// Drive the per-family heave / sway / bank / look-around offsets onto each
/// animated avatar's visual root. Removes [`GaitAnimation`] from an avatar
/// whose current profile no longer matches its attached [`GaitMode`] — a local
/// preset hot-swap or a remote record hot-swap — so its fresh visual root
/// spawns unanimated and no offset lingers.
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
pub(super) fn animate_avatar_gait(
    time: Res<Time>,
    mut commands: Commands,
    avatar_editor: Option<Res<crate::ui::avatar::AvatarEditorState>>,
    mut avatars: Query<(
        Entity,
        &Children,
        &Transform,
        Option<&LinearVelocity>,
        Option<&AngularVelocity>,
        Option<&RemotePeer>,
        Has<HumanoidPreset>,
        Has<HoverBoatPreset>,
        Has<HelicopterPreset>,
        Has<CarPreset>,
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

    for (
        entity,
        children,
        chassis_tf,
        velocity,
        angular,
        peer,
        human,
        boat,
        airship,
        skiff,
        is_local,
        mut anim,
    ) in &mut avatars
    {
        // Drop the animation when the entity's current profile no longer
        // matches the one it was attached with: a local preset hot-swap (its
        // fresh visual root re-attaches with the new mode) or a remote record
        // hot-swap (e.g. a peer switching from a boat to a humanoid).
        let current = if is_local {
            local_gait_mode(human, boat, airship, skiff)
        } else {
            peer.and_then(peer_gait_mode)
        };
        if current != Some(anim.mode) {
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
        // Yaw-rate (heading change, rad/s) for skiff banking: physics angular
        // velocity when present (local), else a 1-frame finite difference of
        // the replicated heading, wrapped to the shortest arc.
        let yaw_rate = match angular {
            Some(av) => av.0.y,
            None => {
                use std::f32::consts::{PI, TAU};
                let yaw_now = chassis_tf.rotation.to_euler(EulerRot::YXZ).0;
                let prev = anim.prev_yaw.replace(yaw_now);
                match prev {
                    Some(py) if dt > 1e-6 => ((yaw_now - py + PI).rem_euclid(TAU) - PI) / dt,
                    _ => 0.0,
                }
            }
        };

        let (offset, yaw, roll) = anim.advance(dt, t, speed, yaw_rate);

        // The chassis's one visual-root child (bursts/FX children lack
        // the marker, so the lookup skips them). Yaw is a heading turn (Y),
        // roll a bank / list about the fore-aft axis (Z); both compose in the
        // root's local frame onto the authored base pose.
        for child in children.iter() {
            if let Ok((root, mut tf)) = roots.get_mut(child) {
                tf.translation = root.base_translation + offset;
                tf.rotation =
                    root.base_rotation * Quat::from_rotation_y(yaw) * Quat::from_rotation_z(roll);
                break;
            }
        }
    }
}

impl GaitAnimation {
    /// Advance the phase/blend state and return this frame's root-local
    /// `(translation offset, yaw, roll)` for the animation's [`GaitMode`] —
    /// pure math, unit-tested below. `yaw_rate` (rad/s) drives skiff banking;
    /// the other modes ignore it.
    fn advance(&mut self, dt: f32, t: f32, speed: f32, yaw_rate: f32) -> (Vec3, f32, f32) {
        match self.mode {
            GaitMode::Humanoid => self.advance_humanoid(dt, t, speed),
            GaitMode::Boat => self.advance_boat(t),
            GaitMode::Airship => self.advance_airship(t),
            GaitMode::Skiff => self.advance_skiff(dt, t, speed, yaw_rate),
        }
    }

    /// Smooth the idle↔moving crossfade toward `target` (0 idle, 1 moving) and
    /// return the moving weight. Shared by the walk-bounce and skiff-shiver.
    fn blend_toward(&mut self, target: f32, dt: f32) -> f32 {
        self.moving_blend += (target - self.moving_blend) * (BLEND_RATE * dt).min(1.0);
        self.moving_blend
    }

    /// Bipedal: per-footfall vertical bounce crossfaded with an idle
    /// weight-shift sway + look-around yaw.
    fn advance_humanoid(&mut self, dt: f32, t: f32, speed: f32) -> (Vec3, f32, f32) {
        use std::f32::consts::TAU;
        let g = self.gait;
        let moving = if speed > MOVING_SPEED_THRESHOLD {
            1.0
        } else {
            0.0
        };
        let walk = self.blend_toward(moving, dt);

        // Footfalls per second: the seeded cadence, scaled by how fast the
        // avatar actually moves relative to the cadence's nominal walk
        // speed (wading halves it, standing stops it).
        let nominal_speed = (g.step_cadence * NOMINAL_SPEED_PER_CADENCE).max(0.1);
        let step_rate = g.step_cadence * (speed / nominal_speed).clamp(0.0, 1.6);
        self.phase = (self.phase + step_rate * dt).fract();

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

        (Vec3::new(sway_x, bounce, sway_z), yaw, 0.0)
    }

    /// Boat: an always-on slow hull heave (vertical) + a gentle list (roll)
    /// on a slightly detuned period so it wallows rather than pumps in sync.
    fn advance_boat(&self, t: f32) -> (Vec3, f32, f32) {
        use std::f32::consts::TAU;
        let g = self.gait;
        let ts = t + self.salt;
        let f = g.idle_sway_frequency.max(veh::BOAT_SWELL_HZ);
        let heave = g.idle_sway_amplitude * veh::BOAT_HEAVE * (TAU * f * ts).sin();
        let roll = g.idle_sway_amplitude * veh::BOAT_ROLL * (TAU * f * 0.73 * ts + 1.1).sin();
        (Vec3::new(0.0, heave, 0.0), 0.0, roll)
    }

    /// Airship: a lazy nose-wander yaw + a slight vertical drift.
    fn advance_airship(&self, t: f32) -> (Vec3, f32, f32) {
        use std::f32::consts::TAU;
        let g = self.gait;
        let ts = t + self.salt;
        let yaw = g.head_turn_variance_degrees.to_radians()
            * veh::AIRSHIP_YAW
            * (TAU * veh::AIRSHIP_DRIFT_HZ * ts).sin();
        let heave = g.idle_sway_amplitude
            * veh::AIRSHIP_HEAVE
            * (TAU * veh::AIRSHIP_DRIFT_HZ * 1.3 * ts + 0.7).sin();
        (Vec3::new(0.0, heave, 0.0), yaw, 0.0)
    }

    /// Skiff: a faint fast suspension shiver at idle, crossfading into a
    /// banking lean into turns under way (roll ∝ yaw-rate × speed, clamped).
    ///
    /// Bank sign: positive `yaw_rate` (turning left about +Y) leans the hull
    /// to the inside of the turn. If in-app it reads as leaning *out* of the
    /// corner, flip the sign of [`veh::SKIFF_BANK_GAIN`]'s use here.
    fn advance_skiff(&mut self, dt: f32, t: f32, speed: f32, yaw_rate: f32) -> (Vec3, f32, f32) {
        use std::f32::consts::TAU;
        let g = self.gait;
        let ts = t + self.salt;
        let moving = if speed > MOVING_SPEED_THRESHOLD {
            1.0
        } else {
            0.0
        };
        let mv = self.blend_toward(moving, dt);
        let idle = 1.0 - mv;

        let shiver = g.idle_sway_amplitude
            * veh::SKIFF_SHIVER
            * (TAU * veh::SKIFF_SHIVER_HZ * ts).sin()
            * idle;
        let bank = (veh::SKIFF_BANK_GAIN * yaw_rate * speed)
            .clamp(-veh::SKIFF_BANK_MAX, veh::SKIFF_BANK_MAX)
            * mv;
        (Vec3::new(0.0, shiver, 0.0), 0.0, bank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anim() -> GaitAnimation {
        GaitAnimation::for_did("did:plc:gait-test", GaitMode::Humanoid)
    }

    fn anim_mode(mode: GaitMode) -> GaitAnimation {
        GaitAnimation::for_did("did:plc:gait-test", mode)
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
            a.advance(1.0 / 60.0, 1.0, 0.0, 0.0);
        }
        let (offset, yaw, roll) = a.advance(1.0 / 60.0, 1.0, 0.0, 0.0);
        // No bounce; sway + yaw bounded by the seeded amplitudes; no roll.
        assert!(offset.y.abs() <= a.gait.step_bounce_amplitude * 0.01);
        assert!(offset.x.abs() <= a.gait.idle_sway_amplitude + 1e-6);
        assert!(offset.z.abs() <= a.gait.idle_sway_amplitude + 1e-6);
        assert!(yaw.abs() <= a.gait.head_turn_variance_degrees.to_radians() + 1e-6);
        assert_eq!(roll, 0.0, "a humanoid never rolls");
    }

    #[test]
    fn walking_bounces_and_suppresses_idle_motion() {
        let mut a = anim();
        let mut max_bounce: f32 = 0.0;
        let mut last = (Vec3::ZERO, 0.0, 0.0);
        // Walk at the nominal speed for a while; sample the peak bounce.
        let speed = a.gait.step_cadence * NOMINAL_SPEED_PER_CADENCE;
        for i in 0..600 {
            last = a.advance(1.0 / 60.0, i as f32 / 60.0, speed, 0.0);
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
        a.advance(0.5, 0.0, 2.0, 0.0);
        let p = a.phase;
        a.advance(0.5, 0.5, 0.0, 0.0);
        assert_eq!(a.phase, p, "no footfalls while standing");
    }

    #[test]
    fn boat_heaves_and_lists_but_never_yaws_or_walks() {
        let mut a = anim_mode(GaitMode::Boat);
        let (mut max_heave, mut max_roll): (f32, f32) = (0.0, 0.0);
        for i in 0..1200 {
            // Speed / yaw-rate must not matter — a hull rocks whether moored
            // or under way, and never bounces or steps.
            let (o, yaw, roll) = a.advance(1.0 / 60.0, i as f32 / 60.0, 5.0, 1.0);
            assert_eq!(yaw, 0.0);
            assert_eq!(o.x, 0.0);
            assert_eq!(o.z, 0.0);
            max_heave = max_heave.max(o.y.abs());
            max_roll = max_roll.max(roll.abs());
        }
        // Heave and list are non-trivial and bounded by the seeded amplitude.
        assert!(
            max_heave > 0.0 && max_heave <= a.gait.idle_sway_amplitude * veh::BOAT_HEAVE + 1e-6
        );
        assert!(max_roll > 0.0 && max_roll <= a.gait.idle_sway_amplitude * veh::BOAT_ROLL + 1e-6);
    }

    #[test]
    fn airship_wanders_its_nose_within_the_seeded_variance() {
        let mut a = anim_mode(GaitMode::Airship);
        let mut max_yaw: f32 = 0.0;
        for i in 0..2000 {
            let (o, yaw, roll) = a.advance(1.0 / 60.0, i as f32 / 60.0, 3.0, 0.0);
            assert_eq!(roll, 0.0, "an airship drifts, it does not bank");
            assert_eq!(o.x, 0.0);
            assert_eq!(o.z, 0.0);
            max_yaw = max_yaw.max(yaw.abs());
        }
        let cap = a.gait.head_turn_variance_degrees.to_radians() * veh::AIRSHIP_YAW;
        assert!(max_yaw > 0.0 && max_yaw <= cap + 1e-6);
    }

    #[test]
    fn skiff_shivers_at_idle_and_banks_only_under_way() {
        // At rest with no steering: a faint vertical shiver, no bank.
        let mut a = anim_mode(GaitMode::Skiff);
        let mut idle_shiver: f32 = 0.0;
        for i in 0..400 {
            let (o, _, roll) = a.advance(1.0 / 60.0, i as f32 / 60.0, 0.0, 0.0);
            idle_shiver = idle_shiver.max(o.y.abs());
            assert!(roll.abs() < 1e-6, "no bank while parked");
        }
        assert!(idle_shiver > 0.0);

        // Driving through a turn: a bank appears, the shiver blends away.
        let mut b = anim_mode(GaitMode::Skiff);
        let mut last = (Vec3::ZERO, 0.0, 0.0);
        for i in 0..400 {
            last = b.advance(1.0 / 60.0, i as f32 / 60.0, 8.0, 1.5);
        }
        assert!(last.2.abs() > 0.01, "should bank into a sustained turn");
        assert!(
            last.2.abs() <= veh::SKIFF_BANK_MAX + 1e-6,
            "bank is clamped"
        );
        assert!(
            last.0.y.abs() < b.gait.idle_sway_amplitude * veh::SKIFF_SHIVER * 0.1,
            "shiver blends out under way"
        );
    }

    #[test]
    fn bank_magnitude_grows_with_turn_sharpness_and_speed() {
        let sample = |speed: f32, yaw_rate: f32| {
            let mut a = anim_mode(GaitMode::Skiff);
            let mut last = 0.0;
            for i in 0..400 {
                last = a.advance(1.0 / 60.0, i as f32 / 60.0, speed, yaw_rate).2;
            }
            last.abs()
        };
        assert!(
            sample(8.0, 1.0) > sample(8.0, 0.3),
            "sharper turn banks harder"
        );
        assert!(sample(8.0, 1.0) > sample(3.0, 1.0), "faster banks harder");
    }

    #[test]
    fn airplane_and_unknown_have_no_gait_profile() {
        use crate::pds::AirplaneParams;
        let airplane = LocomotionConfig::Airplane(Box::<AirplaneParams>::default());
        assert_eq!(GaitMode::for_locomotion(&airplane), None);
        assert_eq!(GaitMode::for_locomotion(&LocomotionConfig::Unknown), None);
    }
}
