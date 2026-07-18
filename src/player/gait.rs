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
//! root) and preset hot-swaps stay drift-free. Amplitudes come from the
//! record's optional `gait` section when present (#874 — authorable in
//! the avatar editor, so both the local preview and remote peers render
//! the published tuning); a record without one falls back to the owner-DID
//! derivation via [`AvatarGait::for_did`] — the same derivation the seeded
//! locomotion defaults use.
//!
//! The local player's gait pauses (root held at the rest pose) while the
//! Avatar editor window is open (#741) — see [`animate_avatar_gait`]
//! for why. Remote peers are unaffected.

use avian3d::prelude::{AngularVelocity, LinearVelocity};
use bevy::prelude::*;

use crate::pds::{AvatarRecord, LocomotionConfig};
use crate::seeded_defaults::AvatarGait;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
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
/// Midpoint of the seeded `idle_sway_frequency` range (0.4–1.2 Hz). The
/// skiff and airship profiles have characteristic frequencies of their own
/// (engine buzz, lazy drift) far from the human sway band, so they consume
/// the authored frequency as a *ratio* against this nominal — the slider
/// modulates their pace proportionally (#878) while the seeded midpoint
/// reproduces the historical `veh` constants exactly.
const NOMINAL_SWAY_HZ: f32 = 0.8;

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
    /// Airship drift frequency (Hz) at the seeded-nominal sway frequency —
    /// a slow lazy wander, scaled by the authored ratio (#878).
    pub const AIRSHIP_DRIFT_HZ: f32 = 0.06;
    /// Airship vertical drift as a multiple of `idle_sway_amplitude`.
    pub const AIRSHIP_HEAVE: f32 = 2.0;
    /// Skiff idle suspension shiver (vertical) as a multiple of
    /// `idle_sway_amplitude` — a small fast tremble at rest.
    pub const SKIFF_SHIVER: f32 = 0.6;
    /// Skiff shiver frequency (Hz) at the seeded-nominal sway frequency —
    /// an idling-engine buzz, much faster than the boat swell, scaled by
    /// the authored ratio (#878).
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
    /// chassis, so a visual-root bank would double it). `pub(crate)` so the
    /// avatar editor's Idle-motion section (#875) can share this mapping
    /// instead of duplicating the preset→profile table.
    pub(crate) fn for_locomotion(loco: &LocomotionConfig) -> Option<Self> {
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
    /// Overall idle-motion multiplier from the record's
    /// [`GaitParams::idle_intensity`](crate::pds::GaitParams) — scales the
    /// composed offsets in [`Self::advance`]. `1.0` for the DID-seeded
    /// fallback.
    intensity: f32,
    /// The avatar's tuned humanoid walk speed (m/s), refreshed from the
    /// record each frame while the humanoid preset is active. Footfall
    /// cadence reaches the seeded/authored `step_cadence` at exactly this
    /// speed (#877); `None` (vehicle presets, missing record) falls back
    /// to the seeded `walk_speed = cadence × 4.0 / 2.2` mapping.
    walk_speed_hint: Option<f32>,
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
            intensity: 1.0,
            walk_speed_hint: None,
            mode,
            phase: 0.0,
            moving_blend: 0.0,
            prev_pos: None,
            prev_yaw: None,
            salt,
        }
    }

    /// Refresh the animated amplitudes from the avatar record (#874): an
    /// explicit `gait` section overrides the DID-seeded attach-time values
    /// (its absence keeps them — the legacy derivation), and the humanoid
    /// walk speed becomes the cadence hint. Called every frame from
    /// [`animate_avatar_gait`] — a copy of six floats, so cheap enough
    /// that slider edits (local) and record hot-swaps (remote) apply
    /// without any change-detection plumbing.
    fn refresh_from_record(&mut self, record: &AvatarRecord) {
        if let Some(gp) = &record.gait {
            self.gait = gp.to_runtime();
            self.intensity = gp.idle_intensity.0;
        }
        self.walk_speed_hint = match &record.locomotion {
            LocomotionConfig::Humanoid(p) => Some(p.walk_speed.0),
            _ => None,
        };
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
    live: Option<Res<LiveAvatarRecord>>,
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
        .map(|e| e.holds_avatar_still())
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

        // Record-authored amplitudes override the DID-seeded attach-time
        // values (#874) — the local player's from its live (unpublished)
        // record so slider edits preview immediately, remote peers' from
        // their last-applied published record.
        let record = if is_local {
            live.as_deref().map(|l| &l.0)
        } else {
            peer.and_then(|p| p.avatar.as_ref())
        };
        if let Some(record) = record {
            anim.refresh_from_record(record);
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
        let (offset, yaw, roll) = match self.mode {
            GaitMode::Humanoid => self.advance_humanoid(dt, t, speed),
            GaitMode::Boat => self.advance_boat(t),
            GaitMode::Airship => self.advance_airship(t),
            GaitMode::Skiff => self.advance_skiff(dt, t, speed, yaw_rate),
        };
        // The record's overall intensity scales the composed pose, not the
        // amplitudes, so cadence maths above read authored values and 0.0
        // stills the avatar completely.
        (
            offset * self.intensity,
            yaw * self.intensity,
            roll * self.intensity,
        )
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

        // Footfalls per second: the authored cadence is reached at exactly
        // the avatar's tuned walk speed (#877) — scaled by how fast the
        // avatar actually moves relative to it (wading halves it, standing
        // stops it). Without a tuned speed (no record yet), fall back to
        // the seeded `walk_speed = 4.0 · cadence / 2.2` mapping, which is
        // exactly the pre-#877 behavior.
        let nominal_speed = self
            .walk_speed_hint
            .unwrap_or(g.step_cadence * NOMINAL_SPEED_PER_CADENCE)
            .max(0.1);
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

    /// Airship: a lazy nose-wander yaw + a slight vertical drift. The
    /// drift pace scales with the authored sway frequency (#878) — a
    /// 0-frequency record hangs dead still.
    fn advance_airship(&self, t: f32) -> (Vec3, f32, f32) {
        use std::f32::consts::TAU;
        let g = self.gait;
        let ts = t + self.salt;
        let drift_hz = veh::AIRSHIP_DRIFT_HZ * (g.idle_sway_frequency / NOMINAL_SWAY_HZ);
        let yaw = g.head_turn_variance_degrees.to_radians()
            * veh::AIRSHIP_YAW
            * (TAU * drift_hz * ts).sin();
        let heave =
            g.idle_sway_amplitude * veh::AIRSHIP_HEAVE * (TAU * drift_hz * 1.3 * ts + 0.7).sin();
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

        // Buzz pace scales with the authored sway frequency (#878): the
        // seeded midpoint reproduces the historical 9 Hz engine idle, a
        // low record trembles lazily, zero sits perfectly still.
        let shiver_hz = veh::SKIFF_SHIVER_HZ * (g.idle_sway_frequency / NOMINAL_SWAY_HZ);
        let shiver =
            g.idle_sway_amplitude * veh::SKIFF_SHIVER * (TAU * shiver_hz * ts).sin() * idle;
        let bank = (veh::SKIFF_BANK_GAIN * yaw_rate * speed)
            .clamp(-veh::SKIFF_BANK_MAX, veh::SKIFF_BANK_MAX)
            * mv;
        (Vec3::new(0.0, shiver, 0.0), 0.0, bank)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::avatar::LocomotionPreset as _;

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

    /// Count sign changes of a profile's oscillating output over 20 s of
    /// idle time with the record's sway frequency pinned to `freq` —
    /// skiff/boat read the vertical offset, airship its wander yaw.
    fn idle_sign_changes(mode: GaitMode, freq: f32) -> usize {
        let mut record = AvatarRecord::default_for_did("did:plc:freq-test");
        record
            .gait
            .as_mut()
            .expect("seeded default carries gait")
            .idle_sway_frequency = crate::pds::Fp(freq);
        let mut a = anim_mode(mode);
        a.refresh_from_record(&record);
        let mut count = 0;
        let mut prev = 0.0f32;
        for i in 0..1200 {
            let (o, yaw, _) = a.advance(1.0 / 60.0, i as f32 / 60.0, 0.0, 0.0);
            let v = if matches!(mode, GaitMode::Airship) {
                yaw
            } else {
                o.y
            };
            if v * prev < 0.0 {
                count += 1;
            }
            if v != 0.0 {
                prev = v;
            }
        }
        count
    }

    #[test]
    fn authored_sway_frequency_paces_every_vehicle_profile() {
        // #878: the skiff buzz and airship drift ran at fixed veh::*
        // frequencies, so the Idle-motion "Sway frequency" slider did
        // nothing on those presets. All three vehicle profiles must now
        // oscillate faster when the record's frequency rises.
        for mode in [GaitMode::Skiff, GaitMode::Airship, GaitMode::Boat] {
            let slow = idle_sign_changes(mode, 0.8);
            let fast = idle_sign_changes(mode, 2.4);
            assert!(
                slow >= 1,
                "{mode:?}: nominal frequency must visibly oscillate"
            );
            assert!(
                fast > slow,
                "{mode:?}: tripled sway frequency must oscillate faster \
                 (got {slow} → {fast} sign changes)"
            );
        }
    }

    #[test]
    fn zero_sway_frequency_stills_the_skiff_and_airship() {
        // 0 Hz is an authored "off": no shiver, no drift. (The boat
        // deliberately floors its swell — a hull always rides water.)
        assert_eq!(idle_sign_changes(GaitMode::Skiff, 0.0), 0);
        assert_eq!(idle_sign_changes(GaitMode::Airship, 0.0), 0);
    }

    #[test]
    fn record_gait_overrides_seeded_amplitudes() {
        let mut record = AvatarRecord::default_for_did("did:plc:someone-else");
        // The seeded chassis family (and thus locomotion) varies with the
        // DID — pin the humanoid preset so the cadence-hint assertion
        // doesn't depend on the dice.
        record.locomotion = crate::pds::HumanoidParams::default().into_config();
        let gp = record.gait.as_mut().expect("seeded default carries gait");
        gp.step_bounce_amplitude = crate::pds::Fp(0.123);
        gp.idle_sway_amplitude = crate::pds::Fp(0.045);

        let mut a = anim();
        a.refresh_from_record(&record);
        assert_eq!(a.gait.step_bounce_amplitude, 0.123);
        assert_eq!(a.gait.idle_sway_amplitude, 0.045);
        // The humanoid record also supplies the cadence hint (#877).
        assert!(a.walk_speed_hint.is_some());
    }

    #[test]
    fn recordless_refresh_keeps_seeded_amplitudes() {
        let mut record = AvatarRecord::default_for_did("did:plc:someone-else");
        record.gait = None;
        let mut a = anim();
        let seeded = a.gait;
        a.refresh_from_record(&record);
        assert_eq!(a.gait.step_bounce_amplitude, seeded.step_bounce_amplitude);
        assert_eq!(a.intensity, 1.0);
    }

    #[test]
    fn zero_intensity_stills_the_avatar_completely() {
        let mut record = AvatarRecord::default_for_did("did:plc:still");
        record.gait.as_mut().unwrap().idle_intensity = crate::pds::Fp(0.0);
        let mut a = anim();
        a.refresh_from_record(&record);
        for i in 0..300 {
            let (o, yaw, roll) = a.advance(1.0 / 60.0, i as f32 / 60.0, 3.0, 0.5);
            assert_eq!(o, Vec3::ZERO);
            assert_eq!(yaw, 0.0);
            assert_eq!(roll, 0.0);
        }
    }

    #[test]
    fn tuned_walk_speed_reaches_full_cadence_instead_of_sliding() {
        // #877: with walk_speed tuned far above the seeded nominal, the
        // old formula capped footfalls at 1.6× cadence while the avatar
        // covered 2.5× the ground — reading as a slide. The hint re-bases
        // the ratio on the tuned speed, so full speed = authored cadence.
        let mut record = AvatarRecord::default_for_did("did:plc:sprinter");
        let p = crate::pds::HumanoidParams {
            walk_speed: crate::pds::Fp(10.0),
            ..Default::default()
        };
        record.locomotion = p.into_config();
        let mut a = anim();
        a.refresh_from_record(&record);

        let dt = 1.0 / 60.0;
        let before = a.phase;
        a.advance(dt, 0.0, 10.0, 0.0);
        let rate = (a.phase - before) / dt;
        assert!(
            (rate - a.gait.step_cadence).abs() < 1e-3,
            "at tuned walk speed the phase advances at exactly the cadence \
             (got {rate}, want {})",
            a.gait.step_cadence
        );
    }

    #[test]
    fn airplane_and_unknown_have_no_gait_profile() {
        use crate::pds::AirplaneParams;
        let airplane = LocomotionConfig::Airplane(Box::<AirplaneParams>::default());
        assert_eq!(GaitMode::for_locomotion(&airplane), None);
        assert_eq!(GaitMode::for_locomotion(&LocomotionConfig::Unknown), None);
    }
}
