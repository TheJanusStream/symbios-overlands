//! CPU perturbation pool — the simulation layer between avatar contact
//! events and the water shader's displacement field.
//!
//! Phase 1's first cut fed each avatar's *instantaneous* position
//! straight into the shader, so a "wake" was just "wherever the avatar
//! is this frame" — no trail, no entry splash, no settling after the
//! avatar left. This module replaces that with a pool of typed,
//! aging, decaying *perturbations*: a contact event spawns one or more
//! disturbances that then live and fade on their own, independent of
//! the avatar that shed them.
//!
//! It deliberately mirrors the *lifecycle patterns* of
//! [`crate::world_builder::particles`] (age/lifetime, a per-emitter
//! spawn accumulator, a bounded pool) **without** its entity / quad /
//! atlas machinery — perturbations are plain POD in a `Vec`, never ECS
//! entities, and are "rendered" only as shader displacement.
//!
//! Determinism is intentionally *not* preserved: wakes are local
//! cosmetic detail, so each client simulates its own pool and peers
//! needn't agree frame-for-frame. That removes the seeded-RNG plumbing
//! the entity particle system carries.
//!
//! ## Frame flow
//!
//! ```text
//!   ContactProducerSet
//!     → tick_perturbations   (age++, cull age≥lifetime, enforce cap)
//!     → spawn_perturbations  (read AvatarContacts, apply spawn rules)
//!     → feed_water_wakes     (pack live set per plane into uniforms)
//! ```
//!
//! Ticking *before* spawning means a perturbation spawned this frame
//! renders at `age = 0` (full envelope start) on its first visible
//! frame.

use std::collections::HashMap;

use bevy::prelude::*;

use crate::config::terrain::water::wake as wcfg;

use super::contact::{AvatarContacts, ContactPhase, SurfaceContact, SurfaceKind};

/// What kind of surface disturbance a perturbation represents. The
/// water shader switches on the `f32` encoding ([`Self::as_shader_f32`])
/// to pick a displacement formula.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerturbationKind {
    /// Concentric ring radiating from a fixed point. Shed during slow
    /// Dwell (a wading footfall cadence).
    RadialRipple,
    /// Anisotropic wake elongated along a direction frozen at spawn
    /// (the avatar's heading at that instant, *not* its live
    /// velocity). Shed during fast Dwell.
    DirectionalWake,
    /// One-shot ring whose radius grows with age. Shed on water
    /// Enter (entry splash) and Exit (settling rebound).
    SplashRing,
}

impl PerturbationKind {
    /// Encoding the shader reads from `wake_samples_b[i].z`. Kept as a
    /// small integer-valued float so the shader can branch with exact
    /// equality after a `round`.
    pub fn as_shader_f32(self) -> f32 {
        match self {
            PerturbationKind::RadialRipple => 0.0,
            PerturbationKind::DirectionalWake => 1.0,
            PerturbationKind::SplashRing => 2.0,
        }
    }
}

/// One live disturbance. Plain data — no ECS entity, no handles.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Perturbation {
    /// Index into [`crate::water::WaterSurfaces::planes`] — routes the
    /// perturbation to the right water material at pack time.
    pub plane_idx: usize,
    /// World-XZ spawn position. Perturbations do not move (v1): the
    /// disturbance stays where it was shed and only ages.
    pub pos: Vec2,
    /// Unit heading frozen at spawn. Only meaningful for
    /// [`PerturbationKind::DirectionalWake`]; other kinds ignore it.
    pub dir: Vec2,
    /// Avatar speed (m/s) at spawn — drives the anisotropic stretch of
    /// a directional wake.
    pub speed: f32,
    /// Peak amplitude in `[0, ~3]`, already folded with the contact
    /// intensity and footprint. The shader multiplies this by the
    /// per-volume `wake_strength`.
    pub amplitude: f32,
    /// Seconds since spawn.
    pub age: f32,
    /// Total lifetime (s); culled once `age >= lifetime`.
    pub lifetime: f32,
    pub kind: PerturbationKind,
}

impl Perturbation {
    /// Age fraction in `[0, 1]`. The shader turns this into an
    /// amplitude envelope (fade in fast, fade out toward 1).
    pub fn age_norm(&self) -> f32 {
        if self.lifetime <= 0.0 {
            1.0
        } else {
            (self.age / self.lifetime).clamp(0.0, 1.0)
        }
    }
}

/// The CPU pool of every live perturbation across all water planes.
#[derive(Resource, Default)]
pub struct PerturbationPool {
    pub live: Vec<Perturbation>,
}

/// Per-avatar Dwell emission track — just the spacing anchor: the
/// position at which the last perturbation was shed (or the live
/// position while emission is gated off).
///
/// There is deliberately **no** position low-pass here. The earlier
/// dual-EMA design (a fast `smooth_pos` vs a slow `ref_pos`) was the
/// root cause of the decelerate-to-halt burst (chainlink #254): a
/// position EMA's relaxation tail is proportional to the prior speed,
/// so after a fast straight run the smoothed position kept drifting
/// forward for *seconds after the boat had physically stopped*,
/// holding the gate open and stamping a dense stack of concentric
/// ripples right where it halted. Gating on the **raw** contact-sample
/// velocity (avian `LinearVelocity`) instead has no tail and no
/// seed-decay — it reads ~0 the instant the body stops.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct DwellTrack {
    pub anchor: Vec2,
}

/// Per-avatar Dwell tracks, keyed by avatar entity. Separate resource
/// so the pool itself stays a flat POD list. Entries are pruned when
/// an avatar stops dwelling so a re-entry starts a fresh track.
#[derive(Resource, Default)]
pub struct PerturbationSpawnState {
    tracks: HashMap<Entity, DwellTrack>,
}

// ---------------------------------------------------------------------------
// Pure logic (unit-tested without a Bevy World)
// ---------------------------------------------------------------------------

/// Peak amplitude for a perturbation, from the contact's normalised
/// engagement and the avatar's footprint radius. Deeper / bigger
/// avatars displace more water. Clamped so a pathological footprint
/// can't blow out the height field.
pub(crate) fn spawn_amplitude(intensity: f32, footprint_radius: f32) -> f32 {
    let depth_term = (0.3 + 0.7 * intensity.clamp(0.0, 1.0)).clamp(0.0, 1.0);
    // 0.4 m ≈ a default humanoid footprint; bigger bodies scale up to
    // 2× and tiny ones floor at 0.5× so every avatar still ripples.
    let size_term = (footprint_radius / 0.4).clamp(0.5, 2.0);
    depth_term * size_term
}

/// Advance an avatar's Dwell track by one frame and report how many
/// perturbations to shed.
///
/// Gates, in order:
/// 1. **Teleport** — a raw single-frame jump from the anchor beyond
///    [`wcfg::DWELL_TELEPORT_DIST`] re-anchors with no emission (no
///    line of ripples across a portal warp).
/// 2. **Speed** — emit only while the avatar's *raw* speed is at or
///    above [`wcfg::DWELL_MIN_SPEED`]. Below it (decelerating to a
///    halt, a settling/rocking hull, slow drift, a dead standstill)
///    the track re-anchors at the live position and sheds
///    **nothing**. Raw physics velocity has no relaxation tail, so a
///    boat that has stopped stops waking the water immediately —
///    unlike the abandoned position-EMA gate whose tail kept emitting
///    for seconds after the stop (chainlink #254).
/// 3. **Spacing** — among genuinely-moving avatars, shed one
///    perturbation per [`wcfg::DWELL_SPACING`] of *raw* travel from
///    the anchor (capped at [`wcfg::DWELL_MAX_BURST`]).
///
/// - `track`: previous track, or `None` on first sighting.
/// - `curr_pos`: avatar XZ this frame.
/// - `speed`: avatar's raw speed (m/s) this frame — the magnitude of
///   the contact sample's world velocity.
///
/// First sighting / teleport / sub-speed seed `anchor = curr_pos`.
pub(crate) fn step_dwell_distance(
    track: Option<DwellTrack>,
    curr_pos: Vec2,
    speed: f32,
) -> (DwellTrack, u32) {
    let fresh = DwellTrack { anchor: curr_pos };

    let Some(prev) = track else {
        return (fresh, 0);
    };

    // Gate 1 — teleport guard on the RAW jump from the anchor.
    if (curr_pos - prev.anchor).length() > wcfg::DWELL_TELEPORT_DIST {
        return (fresh, 0);
    }

    // Gate 2 — raw-speed gate. Re-anchor at the live position so that
    // when real motion resumes the trail starts fresh from here rather
    // than discharging a backlog accrued while slow/stopped.
    if speed < wcfg::DWELL_MIN_SPEED {
        return (fresh, 0);
    }

    // Gate 3 — spatial spacing of the trail (raw travel, no smoothing).
    let d = curr_pos - prev.anchor;
    let dist = d.length();
    let spacing = wcfg::DWELL_SPACING.max(1e-3);
    if dist < spacing {
        return (
            DwellTrack {
                anchor: prev.anchor,
            },
            0,
        );
    }

    let raw = (dist / spacing).floor() as u32; // ≥ 1 here
    let count = raw.min(wcfg::DWELL_MAX_BURST);
    let anchor = if raw > wcfg::DWELL_MAX_BURST {
        // Large (but sub-teleport) single-step jump — drop the backlog
        // and re-anchor at the live position.
        curr_pos
    } else {
        let dir = d / dist; // dist ≥ spacing > 0
        prev.anchor + dir * (count as f32 * spacing)
    };
    (DwellTrack { anchor }, count)
}

/// Which kind a Dwell perturbation should be at the given speed.
pub(crate) fn dwell_kind(speed: f32) -> PerturbationKind {
    if speed >= wcfg::DIRECTIONAL_SPEED_THRESHOLD {
        PerturbationKind::DirectionalWake
    } else {
        PerturbationKind::RadialRipple
    }
}

/// Age every perturbation by `dt` and drop the expired ones in place.
pub(crate) fn tick_pool(live: &mut Vec<Perturbation>, dt: f32) {
    for p in live.iter_mut() {
        p.age += dt;
    }
    live.retain(|p| p.age < p.lifetime);
}

/// Enforce the global pool ceiling by dropping the oldest entries.
/// Insertion order tracks spawn order (newer pushed later), so the
/// front of the vec is the oldest — drain the overflow from there.
pub(crate) fn enforce_pool_cap(live: &mut Vec<Perturbation>, max: usize) {
    if live.len() > max {
        let overflow = live.len() - max;
        live.drain(0..overflow);
    }
}

/// Pack the perturbations belonging to one plane into the two parallel
/// `vec4` arrays the shader expects, capped at `cap`. When more than
/// `cap` perturbations share a plane the *newest* (smallest age) win,
/// since a fresh disturbance reads as more visually salient than one
/// already fading out.
///
/// - `a[i] = (pos.x, pos.z, dir.x, dir.z)`
/// - `b[i] = (age_norm, amplitude, kind_f32, speed)`
pub(crate) fn pack_plane(
    plane_idx: usize,
    live: &[Perturbation],
    cap: usize,
) -> (Vec<Vec4>, Vec<Vec4>) {
    let mut picks: Vec<&Perturbation> = live.iter().filter(|p| p.plane_idx == plane_idx).collect();
    // Newest first.
    picks.sort_by(|x, y| {
        x.age
            .partial_cmp(&y.age)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    picks.truncate(cap);

    let mut a = Vec::with_capacity(picks.len());
    let mut b = Vec::with_capacity(picks.len());
    for p in picks {
        a.push(Vec4::new(p.pos.x, p.pos.y, p.dir.x, p.dir.y));
        b.push(Vec4::new(
            p.age_norm(),
            p.amplitude,
            p.kind.as_shader_f32(),
            p.speed,
        ));
    }
    (a, b)
}

/// Build the perturbation a contact event should spawn, if any. `Enter`
/// and `Exit` yield one `SplashRing`; `Dwell` is handled separately by
/// the accumulator (returns `None` here).
fn spawn_for_phase(
    phase: ContactPhase,
    plane_idx: usize,
    pos: Vec2,
    dir: Vec2,
    speed: f32,
    intensity: f32,
    footprint_radius: f32,
) -> Option<Perturbation> {
    let base = spawn_amplitude(intensity, footprint_radius);
    match phase {
        ContactPhase::Enter => Some(Perturbation {
            plane_idx,
            pos,
            dir,
            speed,
            // Entry splash is the most dramatic single event.
            amplitude: base * 1.5,
            age: 0.0,
            lifetime: wcfg::SPLASH_LIFETIME,
            kind: PerturbationKind::SplashRing,
        }),
        ContactPhase::Exit => Some(Perturbation {
            plane_idx,
            pos,
            dir,
            speed,
            // Settling rebound — gentler than the entry splash.
            amplitude: base * 0.7,
            age: 0.0,
            lifetime: wcfg::SPLASH_LIFETIME,
            kind: PerturbationKind::SplashRing,
        }),
        ContactPhase::Dwell => None,
    }
}

/// Build one Dwell perturbation (called once per accumulator unit).
fn spawn_dwell(
    plane_idx: usize,
    pos: Vec2,
    dir: Vec2,
    speed: f32,
    intensity: f32,
    footprint_radius: f32,
) -> Perturbation {
    let base = spawn_amplitude(intensity, footprint_radius);
    let kind = dwell_kind(speed);
    let (amplitude, lifetime) = match kind {
        // The directional trail is the *sum* of many overlapping
        // stamps; keep each one low so the summed ridge stays bounded
        // (the shader shapes it into a single smooth lobe that blends
        // rather than beats).
        PerturbationKind::DirectionalWake => (base * 0.7, wcfg::WAKE_LIFETIME),
        // RadialRipple from a slow wader is subtler.
        _ => (base * 0.8, wcfg::RIPPLE_LIFETIME),
    };
    // Every Dwell perturbation spawns AT the avatar. For a
    // DirectionalWake that point is the apex/tip of a teardrop the
    // shader trails out *behind* it (the whole leaf geometry lives in
    // `wake_height_at`); no CPU-side offset is needed or wanted —
    // offsetting here would push the apex off the vehicle.
    Perturbation {
        plane_idx,
        pos,
        dir,
        speed,
        amplitude,
        age: 0.0,
        lifetime,
        kind,
    }
}

// ---------------------------------------------------------------------------
// Systems
// ---------------------------------------------------------------------------

/// Age the pool and enforce the global cap. Runs before
/// [`spawn_perturbations`] so freshly spawned disturbances render at
/// `age = 0` their first frame.
pub fn tick_perturbations(time: Res<Time>, mut pool: ResMut<PerturbationPool>) {
    let dt = time.delta_secs();
    tick_pool(&mut pool.live, dt);
    enforce_pool_cap(&mut pool.live, wcfg::POOL_MAX);
}

/// Translate this frame's [`AvatarContacts`] into new perturbations.
/// No `Time` dependency: the Dwell gate keys off the contact sample's
/// raw velocity and raw position spacing, both framerate-independent.
pub fn spawn_perturbations(
    contacts: Res<AvatarContacts>,
    mut pool: ResMut<PerturbationPool>,
    mut spawn_state: ResMut<PerturbationSpawnState>,
) {
    // Track which avatars produced a Dwell this frame so track
    // entries for avatars that left the water get pruned.
    let mut dwelling: Vec<Entity> = Vec::new();

    for sample in contacts.iter_kind(SurfaceKind::Water) {
        // Match (not let-else) so each `SurfaceContact` variant added
        // in a later phase forces an explicit decision here rather
        // than silently continuing.
        let plane_idx = match sample.surface {
            SurfaceContact::Water { plane_idx, .. } => plane_idx,
            // `iter_kind(Water)` guarantees this is unreachable; keep
            // the match total so a new surface variant is a compile
            // error to ignore here, not a silent misroute.
            SurfaceContact::Terrain { .. } => continue,
        };
        let pos = Vec2::new(sample.world_pos.x, sample.world_pos.z);
        let vel = Vec2::new(sample.world_vel.x, sample.world_vel.z);
        let speed = vel.length();
        let dir = if speed > 1e-3 { vel / speed } else { Vec2::X };

        match sample.phase {
            ContactPhase::Enter | ContactPhase::Exit => {
                if let Some(p) = spawn_for_phase(
                    sample.phase,
                    plane_idx,
                    pos,
                    dir,
                    speed,
                    sample.intensity,
                    sample.footprint_radius,
                ) {
                    pool.live.push(p);
                }
            }
            ContactPhase::Dwell => {
                dwelling.push(sample.avatar);
                let prev = spawn_state.tracks.get(&sample.avatar).copied();
                let (next_track, count) = step_dwell_distance(prev, pos, speed);
                spawn_state.tracks.insert(sample.avatar, next_track);
                for _ in 0..count {
                    pool.live.push(spawn_dwell(
                        plane_idx,
                        pos,
                        dir,
                        speed,
                        sample.intensity,
                        sample.footprint_radius,
                    ));
                }
            }
        }
    }

    // Drop track state for avatars no longer dwelling so a returning
    // avatar starts a fresh baseline rather than measuring travel
    // across the gap (which would dump a line of ripples).
    spawn_state.tracks.retain(|e, _| dwelling.contains(e));

    // Cap again here: a single frame's Enter/Dwell burst across many
    // avatars could overshoot before next frame's tick runs.
    enforce_pool_cap(&mut pool.live, wcfg::POOL_MAX);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amplitude_grows_with_intensity_and_footprint() {
        let shallow_small = spawn_amplitude(0.0, 0.4);
        let deep_small = spawn_amplitude(1.0, 0.4);
        let deep_big = spawn_amplitude(1.0, 1.6);
        assert!(deep_small > shallow_small);
        assert!(deep_big > deep_small);
        // Size term floors at 0.5×, so even a tiny footprint ripples.
        assert!(spawn_amplitude(1.0, 0.01) > 0.0);
    }

    /// 60 fps frame delta — used only to advance the *position* in
    /// trail tests; the gate itself is time-independent now.
    const DT: f32 = 1.0 / 60.0;

    /// A track whose anchor sits at `at`.
    fn track_at(at: Vec2) -> DwellTrack {
        DwellTrack { anchor: at }
    }

    /// Drive the gate one frame with an explicit raw speed.
    fn step(track: DwellTrack, pos: Vec2, speed: f32) -> (DwellTrack, u32) {
        step_dwell_distance(Some(track), pos, speed)
    }

    /// A raw speed comfortably above the gate.
    const FAST: f32 = wcfg::DWELL_MIN_SPEED + 5.0;

    #[test]
    fn dwell_first_sighting_seeds_anchor() {
        let p = Vec2::new(3.0, 7.0);
        let (track, count) = step_dwell_distance(None, p, FAST);
        assert_eq!(count, 0);
        assert_eq!(track.anchor, p);
    }

    /// #254 REGRESSION — the real failing signal: a long fast straight
    /// run, then a deceleration to a dead stop while the position still
    /// creeps a little. Once raw speed drops below `DWELL_MIN_SPEED`
    /// **nothing more is shed**, even though the avatar keeps inching
    /// forward — exactly the burst the old position-EMA tail produced.
    #[test]
    fn dwell_fast_run_then_decel_stops_emitting() {
        let mut track = step_dwell_distance(None, Vec2::ZERO, FAST).0;
        let mut x = 0.0_f32;

        // Phase A — 3 s straight run at 20 m/s (≈ 60 m → ~24 stamps
        // at the 2.5 m spacing).
        let mut run_total = 0u32;
        for _ in 0..180 {
            x += 20.0 * DT;
            let (t, c) = step(track, Vec2::new(x, 0.0), 20.0);
            track = t;
            run_total += c;
        }
        assert!(
            run_total > 15,
            "the fast run itself must lay a trail, got {run_total}"
        );

        // Phase B — linear decel 20 → 0 over 3 s. Position keeps
        // advancing (momentum) but speed falls through the gate.
        let mut tail_after_cutoff = 0u32;
        for i in 0..180 {
            let speed = 20.0 * (1.0 - i as f32 / 180.0); // 20 → 0
            x += speed * DT;
            let (t, c) = step(track, Vec2::new(x, 0.0), speed);
            track = t;
            if speed < wcfg::DWELL_MIN_SPEED {
                tail_after_cutoff += c;
            }
        }
        assert_eq!(
            tail_after_cutoff, 0,
            "no perturbation may be shed once raw speed < DWELL_MIN_SPEED"
        );
    }

    /// Steady slow travel below the speed gate emits nothing, however
    /// far it ultimately travels (covers slow drift, gentle docking).
    #[test]
    fn dwell_below_min_speed_emits_nothing() {
        let slow = wcfg::DWELL_MIN_SPEED * 0.5;
        let mut track = step_dwell_distance(None, Vec2::ZERO, slow).0;
        let mut total = 0u32;
        let mut x = 0.0_f32;
        for _ in 0..600 {
            x += slow * DT;
            let (t, c) = step(track, Vec2::new(x, 0.0), slow);
            track = t;
            total += c;
        }
        assert_eq!(total, 0, "sub-speed motion must not wake the water");
    }

    /// While gated off (sub-speed) the anchor re-bases to the live
    /// position every frame, so resuming motion does NOT discharge a
    /// backlog accrued while slow/stopped.
    #[test]
    fn dwell_gated_off_reanchors_no_backlog() {
        // Anchor far behind; avatar now well ahead but moving slowly.
        let prev = track_at(Vec2::ZERO);
        let far = Vec2::new(5.0, 0.0); // many spacings from anchor
        let slow = wcfg::DWELL_MIN_SPEED * 0.5;
        let (track, count) = step(prev, far, slow);
        assert_eq!(count, 0, "sub-speed: nothing shed");
        assert_eq!(track.anchor, far, "anchor re-bases to live position");

        // Resume real speed but only a sub-spacing nudge from here —
        // the backlog from the 5 m gap must NOT burst out.
        let nudge = far + Vec2::new(wcfg::DWELL_SPACING * 0.5, 0.0);
        let (_, c2) = step(track, nudge, FAST);
        assert_eq!(c2, 0, "no backlog discharge after re-anchor");
    }

    /// Sub-spacing displacement sheds nothing even at full speed: a
    /// fixed anchor never accrues `DWELL_SPACING` of travel.
    #[test]
    fn dwell_sub_spacing_emits_nothing() {
        let origin = Vec2::new(12.0, -4.0);
        let mut track = track_at(origin);
        let offsets = [0.08_f32, -0.07, 0.06, -0.08, 0.05, -0.06];
        let mut total = 0u32;
        for i in 0..200usize {
            let off = offsets[i % offsets.len()];
            let p = origin + Vec2::new(off, off * 0.5);
            let (t, c) = step(track, p, FAST);
            track = t;
            total += c;
        }
        assert_eq!(total, 0, "sub-spacing motion must not shed");
    }

    /// Genuine brisk travel sheds a dense, roughly even trail at one
    /// stamp per `DWELL_SPACING` of raw travel (no EMA lag now).
    #[test]
    fn dwell_steady_travel_emits_even_trail() {
        let v = 5.0_f32; // m/s ≫ DWELL_MIN_SPEED
        let mut track = step_dwell_distance(None, Vec2::ZERO, v).0;
        let frames = 600; // 10 s → 50 m travelled
        let mut total = 0u32;
        let mut x = 0.0_f32;
        for _ in 0..frames {
            x += v * DT;
            let (t, c) = step(track, Vec2::new(x, 0.0), v);
            track = t;
            total += c;
        }
        // 50 m / 2.5 m spacing ≈ 20, with no EMA warm-up lag.
        assert!(
            (18..=21).contains(&total),
            "expected an even trail (~20), got {total}"
        );
    }

    #[test]
    fn dwell_teleport_reanchors_without_emitting() {
        let prev = track_at(Vec2::ZERO);
        let warped = Vec2::new(wcfg::DWELL_TELEPORT_DIST + 5.0, 0.0);
        let (track, count) = step(prev, warped, FAST);
        assert_eq!(count, 0);
        assert_eq!(track.anchor, warped);
    }

    #[test]
    fn dwell_burst_is_capped_and_reanchored() {
        // Anchor many spacings behind; one fast step jumps 15 m. Raw
        // count (15 / 2.5 = 6) exceeds DWELL_MAX_BURST, so emission is
        // capped and the anchor re-bases to the live position so the
        // backlog can't burst again next frame.
        let prev = track_at(Vec2::ZERO);
        let curr = Vec2::new(15.0, 0.0);
        let (track, count) = step(prev, curr, FAST);
        assert_eq!(count, wcfg::DWELL_MAX_BURST);
        assert!(
            (track.anchor - curr).length() < 1e-3,
            "anchor re-bases at the live position on cap"
        );
    }

    #[test]
    fn dwell_kind_switches_at_threshold() {
        assert_eq!(
            dwell_kind(wcfg::DIRECTIONAL_SPEED_THRESHOLD - 0.01),
            PerturbationKind::RadialRipple
        );
        assert_eq!(
            dwell_kind(wcfg::DIRECTIONAL_SPEED_THRESHOLD + 0.01),
            PerturbationKind::DirectionalWake
        );
    }

    #[test]
    fn enter_and_exit_spawn_splash_rings() {
        let e = spawn_for_phase(ContactPhase::Enter, 0, Vec2::ZERO, Vec2::X, 1.0, 0.5, 0.4);
        let x = spawn_for_phase(ContactPhase::Exit, 0, Vec2::ZERO, Vec2::X, 1.0, 0.5, 0.4);
        let d = spawn_for_phase(ContactPhase::Dwell, 0, Vec2::ZERO, Vec2::X, 1.0, 0.5, 0.4);
        assert_eq!(e.unwrap().kind, PerturbationKind::SplashRing);
        assert_eq!(x.unwrap().kind, PerturbationKind::SplashRing);
        // Entry splash is stronger than the exit settle.
        assert!(e.unwrap().amplitude > x.unwrap().amplitude);
        assert!(d.is_none());
    }

    #[test]
    fn dwell_perturbations_spawn_at_avatar_apex() {
        let avatar = Vec2::new(10.0, 5.0);
        let heading = Vec2::new(1.0, 0.0); // moving +X
        let footprint = 0.6;

        // Fast → DirectionalWake. The apex/tip sits AT the avatar; the
        // trailing teardrop is built shader-side, so the spawn pos is
        // NOT offset.
        let fast = wcfg::DIRECTIONAL_SPEED_THRESHOLD + 1.0;
        let wake = spawn_dwell(0, avatar, heading, fast, 1.0, footprint);
        assert_eq!(wake.kind, PerturbationKind::DirectionalWake);
        assert!(
            (wake.pos - avatar).length() < 1e-5,
            "wake apex must sit at the avatar, got {:?}",
            wake.pos
        );
        // Heading is still carried for the shader's trailing direction.
        assert!((wake.dir - heading).length() < 1e-5);

        // Slow → RadialRipple, also centred under the wader.
        let slow = wcfg::DIRECTIONAL_SPEED_THRESHOLD - 0.1;
        let ripple = spawn_dwell(0, avatar, heading, slow.max(0.0), 1.0, footprint);
        assert_eq!(ripple.kind, PerturbationKind::RadialRipple);
        assert!((ripple.pos - avatar).length() < 1e-5);

        // DirectionalWake stamp amplitude is kept below the radial one
        // so the summed trail ridge stays bounded.
        assert!(wake.amplitude < ripple.amplitude);
    }

    #[test]
    fn tick_ages_and_culls_expired() {
        let mut live = vec![
            Perturbation {
                plane_idx: 0,
                pos: Vec2::ZERO,
                dir: Vec2::X,
                speed: 0.0,
                amplitude: 1.0,
                age: 0.0,
                lifetime: 1.0,
                kind: PerturbationKind::RadialRipple,
            },
            Perturbation {
                plane_idx: 0,
                pos: Vec2::ZERO,
                dir: Vec2::X,
                speed: 0.0,
                amplitude: 1.0,
                age: 0.9,
                lifetime: 1.0,
                kind: PerturbationKind::SplashRing,
            },
        ];
        tick_pool(&mut live, 0.2);
        // Second one crossed its lifetime (0.9 + 0.2 ≥ 1.0) → culled.
        assert_eq!(live.len(), 1);
        assert!((live[0].age - 0.2).abs() < 1e-6);
    }

    #[test]
    fn pool_cap_drops_oldest_first() {
        let mk = |age: f32| Perturbation {
            plane_idx: 0,
            pos: Vec2::ZERO,
            dir: Vec2::X,
            speed: 0.0,
            amplitude: 1.0,
            age,
            lifetime: 100.0,
            kind: PerturbationKind::RadialRipple,
        };
        // Front = oldest by insertion order.
        let mut live = vec![mk(5.0), mk(4.0), mk(1.0), mk(0.0)];
        enforce_pool_cap(&mut live, 2);
        assert_eq!(live.len(), 2);
        // The two most-recently-inserted survive.
        assert!((live[0].age - 1.0).abs() < 1e-6);
        assert!((live[1].age - 0.0).abs() < 1e-6);
    }

    #[test]
    fn pack_plane_filters_sorts_and_caps() {
        let mk = |plane: usize, age: f32, kind: PerturbationKind| Perturbation {
            plane_idx: plane,
            pos: Vec2::new(1.0, 2.0),
            dir: Vec2::new(0.0, 1.0),
            speed: 3.0,
            amplitude: 0.5,
            age,
            lifetime: 10.0,
            kind,
        };
        let live = vec![
            mk(1, 5.0, PerturbationKind::RadialRipple),
            mk(0, 9.0, PerturbationKind::SplashRing),
            mk(0, 1.0, PerturbationKind::DirectionalWake),
            mk(0, 4.0, PerturbationKind::RadialRipple),
        ];
        let (a, b) = pack_plane(0, &live, 2);
        // Only plane-0 entries, newest two (age 1.0 then 4.0).
        assert_eq!(a.len(), 2);
        assert_eq!(b.len(), 2);
        // Newest first → DirectionalWake (kind 1.0) leads.
        assert!((b[0].z - 1.0).abs() < 1e-6);
        assert!((b[1].z - 0.0).abs() < 1e-6); // RadialRipple
        // age_norm = age / lifetime.
        assert!((b[0].x - 0.1).abs() < 1e-6);
        // Position / dir packed into `a`.
        assert!((a[0].x - 1.0).abs() < 1e-6 && (a[0].y - 2.0).abs() < 1e-6);
    }

    #[test]
    fn kind_shader_encoding_is_distinct() {
        assert_eq!(PerturbationKind::RadialRipple.as_shader_f32(), 0.0);
        assert_eq!(PerturbationKind::DirectionalWake.as_shader_f32(), 1.0);
        assert_eq!(PerturbationKind::SplashRing.as_shader_f32(), 2.0);
    }
}
