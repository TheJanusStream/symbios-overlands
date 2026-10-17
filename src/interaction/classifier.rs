//! Per-frame contact producer for the interaction framework.
//!
//! Joins the otherwise-disjoint local-player and remote-peer queries
//! into one iteration, derives a per-avatar velocity (direct from
//! `LinearVelocity` for locals, 1-frame finite-difference for peers
//! whose components do not currently carry the velocity), probes each
//! surface registry, and emits [`ContactSample`]s into the
//! [`AvatarContacts`] resource - clearing it first so consumers only
//! see this frame's data.
//!
//! Two internal state caches survive across frames:
//!
//! - [`PeerVelocityCache`] - last frame's `(position, time)` per peer
//!   entity, used for the finite-difference velocity fallback. Entries
//!   for despawned peers are pruned each frame.
//! - [`ContactPersistence`] - last frame's [`SurfaceContact`] per
//!   avatar, used to emit `Enter` / `Dwell` / `Exit` phases
//!   automatically. A despawned avatar's last surface is silently
//!   dropped - we don't synthesise an `Exit` for a vanished entity
//!   because the consumer would have no place to spawn an effect.
//!
//! The pure logic (probe, phase computation, intensity) is split out so
//! tests don't have to build a Bevy app.

use std::collections::HashMap;

use avian3d::prelude::LinearVelocity;
use bevy::prelude::*;
use bevy_symbios_ground::TerrainQuery;

use crate::config::terrain::ground as gcfg;
use crate::config::terrain::water::wake as wcfg;
use crate::pds::LocomotionConfig;
use crate::state::{LiveAvatarRecord, LocalPlayer, RemotePeer};
use crate::water::WaterSurfaces;

use super::contact::{AvatarContacts, ContactPhase, ContactSample, SurfaceContact};
use super::locomotion::{
    locomotion_footprint, locomotion_ground_reach, locomotion_total_height, locomotion_water_slack,
};

/// Last-frame position/time cache for remote peers (whose entities do
/// not carry `LinearVelocity`). Pruned each frame to avoid leaking
/// memory for despawned peers.
#[derive(Resource, Default, Debug)]
pub struct PeerVelocityCache {
    pub(crate) entries: HashMap<Entity, (Vec3, f32)>,
}

/// Last-frame surface contact per avatar entity. Used to compute the
/// `ContactPhase` of the current frame's sample by comparing against
/// the just-classified surface. Entries are removed when an avatar
/// transitions to "no contact" so the next contact starts with
/// `ContactPhase::Enter`.
#[derive(Resource, Default, Debug)]
pub struct ContactPersistence {
    pub(crate) last_surface: HashMap<Entity, SurfaceContact>,
}

/// CPU mirror of the terrain surface, the land analogue of
/// [`WaterSurfaces`]. Wraps a [`TerrainQuery`] (heightmap + splat
/// mapper) so the classifier can read ground height, surface normal and
/// splat weights at any world XZ without a physics raycast - the
/// heightfield collider *is* this heightmap, so a height query is
/// equivalent to (and cheaper / deterministic vs.) casting a ray at it.
///
/// Populated by `terrain.rs::apply_splat_textures` once the heightmap
/// and the record's splat rules are both resolved, and rebuilt in place
/// whenever the terrain regenerates.
///
/// Memory: the wrapped [`TerrainQuery`] owns a clone of the heightmap
/// (`grid² × 4` bytes - ~1 MiB at the default 512 grid) plus the tiny
/// [`SplatMapper`](bevy_symbios_ground::SplatMapper). One copy total, on
/// the main world only.
#[derive(Resource)]
pub struct TerrainSurfaceQuery {
    query: TerrainQuery,
    /// Half the world extent. The terrain mesh is rendered offset by
    /// `-half` (`terrain.rs`), so world XZ → heightmap-local XZ is
    /// `world + half` before handing to [`TerrainQuery`] (whose
    /// coordinate origin is the heightmap corner).
    half_extent: f32,
    /// Real Berlin's land use under a geodata region's ground (#1586),
    /// whose weights replace the splat rules': the layers the GPU draws.
    ground: Option<crate::terrain::geo::GeoGround>,
}

impl TerrainSurfaceQuery {
    pub fn new(query: TerrainQuery, half_extent: f32) -> Self {
        Self {
            query,
            half_extent,
            ground: None,
        }
    }

    /// Read the splat weights from `ground`'s land use, as the GPU draws a
    /// geodata region's ground, rather than from the splat rules.
    pub(crate) fn with_ground(mut self, ground: Option<crate::terrain::geo::GeoGround>) -> Self {
        self.ground = ground;
        self
    }

    /// The ground's normalised splat weights at a world XZ, in its four
    /// layers' order - the rules' or, on ground built from real Berlin, its
    /// land use's (#1674's `--cover-map` paints them).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn weights_at(&self, world_x: f32, world_z: f32) -> [f32; 4] {
        self.sample(world_x, world_z).1
    }

    /// Ground height, normalised `[Grass, Dirt, Rock, Snow]` splat
    /// weights and unit surface normal at a world XZ.
    fn sample(&self, world_x: f32, world_z: f32) -> (f32, [f32; 4], Vec3) {
        let lx = world_x + self.half_extent;
        let lz = world_z + self.half_extent;
        let h = self.query.height_at(lx, lz);
        let w = match &self.ground {
            Some(ground) => ground.weights_at_local(lx, lz),
            None => self.query.splat_weights_at(lx, lz),
        };
        let n = self.query.normal_at(lx, lz);
        (h, w, Vec3::new(n[0], n[1], n[2]))
    }
}

/// One transition the classifier might emit for an avatar this frame.
/// In the case of a surface-kind change (water plane A → water plane
/// B, or eventually water → terrain) two transitions are emitted: an
/// `Exit` for the old surface and an `Enter` for the new one.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Transition {
    pub surface: SurfaceContact,
    pub phase: ContactPhase,
}

/// Probe the water registry against an avatar's body bottom. Returns
/// `Some(SurfaceContact::Water)` when in contact, else `None`. Terrain
/// is probed separately by [`probe_terrain`]; [`emit_for_avatar`]
/// selects between them (water wins when both hit - an avatar wading in
/// a shallow pond reads as "in water", not "on ground").
///
/// Contact is tested at the avatar's **body bottom**
/// (`world_pos.y − total_height/2`), not the chassis origin. Probing
/// the origin made emission razor-sensitive to height - a vehicle
/// whose origin floats above the waterline (hull in the water, origin
/// not) produced nothing, since `WaterSurfaces::query` culls points
/// above the surface. Probing the body bottom with `query_signed`
/// (which keeps above-surface hits) mirrors the established
/// `humanoid_water_state` feet-vs-surface test and makes emission
/// depend on the avatar's vertical extent.
///
/// **Above the surface** a body is in contact while its bottom is within
/// `slack` of it - [`locomotion_water_slack`]: a person's feet must touch
/// the water, a vehicle or an aircraft need only skim it, so a floating
/// hover-boat, whose box rides clear of the surface on its buoyancy, and an
/// airframe flying low over a lake both disturb it (#1654). And only over
/// OPEN water: where the ground at that spot (`ground_y`, while the terrain
/// mirror is resident) stands above the water's surface it is a shore, and a
/// body there is in the water only once it is under the surface. Otherwise
/// the band would read a walk or a drive along a beach inside a water
/// plane's rectangle as wading, and water wins over ground.
///
/// The waterline test is a **Schmitt trigger**: `was_in_contact`
/// selects the threshold. Entering uses `slack`; once in contact the
/// avatar stays in contact until its body bottom rises past `slack` plus
/// [`wcfg::EXIT_HYSTERESIS`]. The hysteresis band absorbs the
/// settling bob of a decelerating hull - without it the body bottom
/// chatters across a single threshold every frame, flipping
/// Exit→Enter and spawning a burst of splash rings as the boat halts.
///
/// `depth` in the returned [`SurfaceContact::Water`] is the body
/// bottom's submersion (clamped ≥ 0), so the downstream
/// `intensity_for` (depth ÷ total_height) still reads 0 at the
/// waterline and saturates when fully submerged.
pub(crate) fn probe_water(
    world_pos: Vec3,
    total_height: f32,
    slack: f32,
    was_in_contact: bool,
    water: &WaterSurfaces,
    ground_y: Option<f32>,
) -> Option<SurfaceContact> {
    let body_bottom = Vec3::new(world_pos.x, world_pos.y - 0.5 * total_height, world_pos.z);
    let q = water.query_signed(body_bottom)?;
    // The waterline, from the only point that has it: the probe was taken
    // at the body bottom, so the surface is that Y plus the signed depth
    // (#1254 f320).
    let surface_y = body_bottom.y + q.depth;
    // `q.depth` is positive when the body bottom is submerged. Above the
    // surface the tolerance is the body's own, widened once already in
    // contact (hysteresis) - and none at all over a shore.
    let open = ground_y.is_none_or(|ground| ground < surface_y);
    let above = match (open, was_in_contact) {
        (false, _) => 0.0,
        (true, false) => slack,
        (true, true) => slack + wcfg::EXIT_HYSTERESIS,
    };
    if q.depth < -above {
        return None;
    }
    Some(SurfaceContact::Water {
        plane_idx: q.surface_idx,
        depth: q.depth.max(0.0),
        flow_dir: Vec2::new(q.flow_dir.x, q.flow_dir.z),
        surface_y,
    })
}

/// Pure terrain-contact decision, split out of [`probe_terrain`] so the
/// Schmitt-trigger / slack logic is unit-testable without a
/// [`TerrainQuery`]. `surface_y`, `weights` and `normal` are the
/// heightmap sample at the body-bottom XZ.
///
/// Mirrors the water Schmitt trigger ([`probe_water`]): the body bottom
/// must be within [`gcfg::CONTACT_SLACK`] above the ground to *enter*
/// contact, and stays in contact until it rises past the wider
/// [`gcfg::CONTACT_EXIT_SLACK`] - the hysteresis absorbs the few-cm
/// jitter of a capsule resting on a heightfield so a standing avatar
/// does not chatter Exit→Enter (which would restart footprint
/// stamping every frame).
pub(crate) fn classify_terrain_contact(
    body_bottom_y: f32,
    surface_y: f32,
    was_in_contact: bool,
    weights: [f32; 4],
    normal: Vec3,
) -> Option<SurfaceContact> {
    // Positive when the body bottom is at/below the ground surface.
    let penetration = surface_y - body_bottom_y;
    let slack = if was_in_contact {
        gcfg::CONTACT_EXIT_SLACK
    } else {
        gcfg::CONTACT_SLACK
    };
    if penetration < -slack {
        return None;
    }
    Some(SurfaceContact::Terrain {
        material_blend: weights,
        normal,
        ground_y: surface_y,
    })
}

/// Probe the terrain heightmap against an avatar's body bottom - `reach`
/// below the bottom of its body, where a wheeled or hovering body meets the
/// ground (#1549: a car's box rides on its springs, so measured from the box
/// it never touched down). `None` when the terrain CPU mirror isn't resident
/// yet (still loading) or the avatar is airborne above the ground.
pub(crate) fn probe_terrain(
    world_pos: Vec3,
    total_height: f32,
    reach: f32,
    was_in_contact: bool,
    terrain: &TerrainSurfaceQuery,
) -> Option<SurfaceContact> {
    let body_bottom_y = world_pos.y - 0.5 * total_height - reach;
    let (surface_y, weights, normal) = terrain.sample(world_pos.x, world_pos.z);
    classify_terrain_contact(body_bottom_y, surface_y, was_in_contact, weights, normal)
}

/// Pure phase-transition logic. Independent of the rest of the system
/// so it can be exhaustively unit-tested without spinning up a Bevy
/// `World`.
///
/// Returns a small vector (≤ 2 elements) of transitions to emit and
/// the new persistence value to store. A `None` second tuple element
/// means "drop the entry from persistence" (avatar left every
/// surface).
pub(crate) fn compute_transitions(
    last: Option<SurfaceContact>,
    curr: Option<SurfaceContact>,
) -> (Vec<Transition>, Option<SurfaceContact>) {
    match (last, curr) {
        (None, None) => (Vec::new(), None),
        (None, Some(c)) => (
            vec![Transition {
                surface: c,
                phase: ContactPhase::Enter,
            }],
            Some(c),
        ),
        (Some(p), None) => (
            vec![Transition {
                surface: p,
                phase: ContactPhase::Exit,
            }],
            None,
        ),
        (Some(p), Some(c)) => {
            if same_specific_surface(&p, &c) {
                (
                    vec![Transition {
                        surface: c,
                        phase: ContactPhase::Dwell,
                    }],
                    Some(c),
                )
            } else {
                (
                    vec![
                        Transition {
                            surface: p,
                            phase: ContactPhase::Exit,
                        },
                        Transition {
                            surface: c,
                            phase: ContactPhase::Enter,
                        },
                    ],
                    Some(c),
                )
            }
        }
    }
}

/// "Same surface" for phase-tracking purposes: same kind AND same
/// surface-specific key (water plane index, eventually terrain region
/// id). A change in `depth` does not count as a surface change -
/// that's what `Dwell` is for.
fn same_specific_surface(a: &SurfaceContact, b: &SurfaceContact) -> bool {
    if a.kind() != b.kind() {
        return false;
    }
    match (a, b) {
        (
            SurfaceContact::Water { plane_idx: ai, .. },
            SurfaceContact::Water { plane_idx: bi, .. },
        ) => ai == bi,
        // There is exactly one terrain - any two terrain contacts are
        // the same specific surface (so a stationary avatar dwells
        // rather than re-entering every frame).
        (SurfaceContact::Terrain { .. }, SurfaceContact::Terrain { .. }) => true,
        // Mixed-kind pairs are already filtered by the kind() check
        // above; this arm is unreachable but keeps the match total.
        _ => false,
    }
}

/// 0..1 engagement scalar driven by the surface-specific payload.
///
/// - Water: submersion depth normalised by avatar height - fully
///   submerged reads as 1.0 (`world_vel` unused).
/// - Terrain: downward impact speed normalised by
///   [`gcfg::INTENSITY_VEL_REF`], floored at
///   [`gcfg::INTENSITY_GROUNDED_FLOOR`] so a still, grounded avatar
///   keeps a faint continuous contact (footprints accrue while
///   standing). A hard landing saturates at 1.0.
pub(crate) fn intensity_for(
    contact: &SurfaceContact,
    world_vel: Vec3,
    locomotion: &LocomotionConfig,
) -> f32 {
    match contact {
        SurfaceContact::Water { depth, .. } => {
            let denom = locomotion_total_height(locomotion).max(0.01);
            (depth / denom).clamp(0.0, 1.0)
        }
        SurfaceContact::Terrain { .. } => {
            let down = (-world_vel.y).max(0.0);
            (down / gcfg::INTENSITY_VEL_REF).clamp(gcfg::INTENSITY_GROUNDED_FLOOR, 1.0)
        }
    }
}

/// Per-frame system: rebuild [`AvatarContacts`] from the current state
/// of every avatar entity.
#[allow(clippy::too_many_arguments)]
pub fn classify_contacts(
    time: Res<Time>,
    water: Option<Res<WaterSurfaces>>,
    terrain: Option<Res<TerrainSurfaceQuery>>,
    live_avatar: Option<Res<LiveAvatarRecord>>,
    locals: Query<(Entity, &Transform, &LinearVelocity), With<LocalPlayer>>,
    peers: Query<(Entity, &Transform, &RemotePeer), Without<LocalPlayer>>,
    mut peer_vel_cache: ResMut<PeerVelocityCache>,
    mut persistence: ResMut<ContactPersistence>,
    mut contacts: ResMut<AvatarContacts>,
) {
    contacts.samples.clear();

    let Some(water) = water.as_deref() else {
        // No water registry yet (still loading) - keep persistence
        // intact so a partial frame doesn't fabricate an Exit, and
        // skip emission entirely. (Water is spawned per room before
        // the terrain CPU mirror resolves, so gating on water alone
        // also avoids a terrain-only partial frame.)
        return;
    };
    let terrain = terrain.as_deref();

    let elapsed = time.elapsed_secs();
    let unknown = LocomotionConfig::Unknown;

    // ----- Local players (direct LinearVelocity) -----
    let local_cfg: &LocomotionConfig = live_avatar
        .as_ref()
        .map(|r| &r.0.locomotion)
        .unwrap_or(&unknown);
    let local_footprint = locomotion_footprint(local_cfg);
    for (entity, transform, lin_vel) in locals.iter() {
        emit_for_avatar(
            entity,
            transform.translation,
            lin_vel.0,
            local_footprint,
            local_cfg,
            water,
            terrain,
            &mut persistence,
            &mut contacts.samples,
        );
    }

    // ----- Remote peers (1-frame finite-difference fallback) -----
    let mut alive_peers: Vec<Entity> = Vec::with_capacity(peers.iter().len());
    for (entity, transform, peer) in peers.iter() {
        alive_peers.push(entity);

        let curr_pos = transform.translation;
        let vel = if let Some((prev_pos, prev_time)) = peer_vel_cache.entries.get(&entity).copied()
        {
            let dt = (elapsed - prev_time).max(1e-4);
            (curr_pos - prev_pos) / dt
        } else {
            Vec3::ZERO
        };
        peer_vel_cache.entries.insert(entity, (curr_pos, elapsed));

        // A muted peer emits no contact samples (#1219 f324). Mute's tooltip
        // promises audio, and this list is what feeds the contact one-shots
        // (`interaction::audio`), the particle bursts
        // (`interaction::particle_channel`) and the decals
        // (`interaction::decal`) - so a hidden body's splashes, footsteps and
        // scorch marks all kept arriving from a source the user could no
        // longer see or aim at. Skipped AFTER the velocity cache is updated,
        // so an unmute resumes with a real speed rather than a spurious
        // one-frame teleport, and the `last_surface` state is left standing
        // so the resumed peer transitions from where they actually are.
        if peer.muted {
            continue;
        }

        let cfg = peer
            .avatar
            .as_ref()
            .map(|a| &a.locomotion)
            .unwrap_or(&unknown);
        let footprint = locomotion_footprint(cfg);
        emit_for_avatar(
            entity,
            curr_pos,
            vel,
            footprint,
            cfg,
            water,
            terrain,
            &mut persistence,
            &mut contacts.samples,
        );
    }

    // Prune velocity-cache entries for despawned peers so the cache
    // can't grow unbounded across long sessions.
    peer_vel_cache
        .entries
        .retain(|e, _| alive_peers.contains(e));

    // Prune persistence for avatars that are no longer in either
    // query. We do NOT synthesise an Exit transition for a vanished
    // entity - consumers can't act on a sample for a despawned avatar
    // anyway.
    persistence
        .last_surface
        .retain(|e, _| alive_peers.contains(e) || locals.contains(*e));
}

/// Shared per-avatar emission path used by both the local-player and
/// remote-peer loops. Probes the surface registry, asks
/// [`compute_transitions`] for the right phase set, and pushes one
/// [`ContactSample`] per transition.
#[allow(clippy::too_many_arguments)]
fn emit_for_avatar(
    avatar: Entity,
    world_pos: Vec3,
    world_vel: Vec3,
    footprint_radius: f32,
    locomotion: &LocomotionConfig,
    water: &WaterSurfaces,
    terrain: Option<&TerrainSurfaceQuery>,
    persistence: &mut ContactPersistence,
    out: &mut Vec<ContactSample>,
) {
    let total_height = locomotion_total_height(locomotion);
    let reach = locomotion_ground_reach(locomotion);
    let last = persistence.last_surface.get(&avatar).copied();
    // Per-kind Schmitt trigger: probe with the wide exit threshold only
    // for the surface kind we were *already* on, so a settling bob (or
    // a capsule jittering on the heightfield) can't chatter Exit→Enter.
    let was_water = matches!(last, Some(SurfaceContact::Water { .. }));
    let was_terrain = matches!(last, Some(SurfaceContact::Terrain { .. }));
    // Water wins when both hit: an avatar wading in a shallow pond is
    // "in water", not "on ground". The ground under the avatar tells open
    // water from a shore for a body skimming over it.
    let ground_y = terrain.map(|t| t.sample(world_pos.x, world_pos.z).0);
    let slack = locomotion_water_slack(locomotion);
    let curr =
        probe_water(world_pos, total_height, slack, was_water, water, ground_y).or_else(|| {
            terrain.and_then(|t| probe_terrain(world_pos, total_height, reach, was_terrain, t))
        });
    let (transitions, new_state) = compute_transitions(last, curr);

    for t in transitions {
        let intensity = intensity_for(&t.surface, world_vel, locomotion);
        out.push(ContactSample {
            avatar,
            world_pos,
            world_vel,
            footprint_radius,
            surface: t.surface,
            intensity,
            phase: t.phase,
        });
    }

    match new_state {
        Some(s) => {
            persistence.last_surface.insert(avatar, s);
        }
        None => {
            persistence.last_surface.remove(&avatar);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::contact::SurfaceKind;
    use super::*;

    fn water_contact(plane_idx: usize, depth: f32) -> SurfaceContact {
        SurfaceContact::Water {
            plane_idx,
            depth,
            flow_dir: Vec2::ZERO,
            surface_y: depth,
        }
    }

    /// A single flat water plane at y = 0 large enough that XZ is
    /// always inside its extents for these tests.
    fn flat_water() -> WaterSurfaces {
        use crate::water::WaterPlane;
        WaterSurfaces {
            planes: vec![WaterPlane {
                world_from_local: Transform::from_translation(Vec3::ZERO),
                local_half_extents: Vec2::splat(100.0),
                flow_strength: 0.0,
                owner: WaterPlane::NO_OWNER,
            }],
        }
    }

    /// Place `world_pos` so the probed body bottom sits exactly
    /// `above` metres above the y=0 surface (negative = submerged).
    fn pos_for_body_bottom(above: f32, total_height: f32) -> Vec3 {
        Vec3::new(0.0, above + 0.5 * total_height, 0.0)
    }

    #[test]
    fn contact_uses_hysteresis_to_resist_settling_chatter() {
        let water = flat_water();
        let h = 2.0;

        // A person's tolerance throughout: their feet must touch.
        let person = |pos: Vec3, was: bool| {
            probe_water(pos, h, wcfg::CONTACT_SLACK, was, &water, None).is_some()
        };
        // Body bottom 0.3 m above the surface - beyond the tight
        // enter slack (0.15) but inside the wide exit slack (0.6).
        let just_above = pos_for_body_bottom(0.3, h);
        // Not yet in contact → tight threshold rejects it.
        assert!(!person(just_above, false));
        // Already in contact → hysteresis holds it (no Exit chatter).
        assert!(person(just_above, true));

        // Clearly clear of the water (1.0 m > exit slack) → Exit even
        // with hysteresis: a genuine departure still ends contact.
        let clear = pos_for_body_bottom(1.0, h);
        assert!(!person(clear, true));

        // Submerged → in contact regardless of prior state.
        let under = pos_for_body_bottom(-0.1, h);
        assert!(person(under, false));
        assert!(person(under, true));
    }

    /// A vehicle or an aircraft disturbs the water from the skim band over
    /// it, where a person has to touch it (#1654) - and holds contact until
    /// it rises the hysteresis past its own band.
    #[test]
    fn a_vehicle_skims_the_water_and_a_person_must_touch_it() {
        let water = flat_water();
        let h = 1.0;
        let probe = |above: f32, slack: f32, was: bool| {
            probe_water(pos_for_body_bottom(above, h), h, slack, was, &water, None).is_some()
        };
        assert!(probe(0.4, wcfg::SKIM_SLACK, false), "skimming at 0.4 m");
        assert!(!probe(0.4, wcfg::CONTACT_SLACK, false), "a person at 0.4 m");
        assert!(
            !probe(0.9, wcfg::SKIM_SLACK, false),
            "0.9 m is clear of the band"
        );
        assert!(probe(0.9, wcfg::SKIM_SLACK, true), "but held once in it");
        assert!(!probe(1.2, wcfg::SKIM_SLACK, true), "and let go past it");
    }

    /// Above the surface only open water counts (#1654): where the ground
    /// stands above the water's level - a shore inside a water plane's
    /// rectangle - a body skimming over it is on land, in contact or not.
    #[test]
    fn a_body_over_a_shore_is_not_in_the_water() {
        let water = flat_water();
        let h = 1.0;
        let over = pos_for_body_bottom(0.1, h);
        let probe = |ground: f32, was: bool| {
            probe_water(over, h, wcfg::SKIM_SLACK, was, &water, Some(ground)).is_some()
        };
        assert!(probe(-1.0, false), "open water a metre deep");
        assert!(probe(-0.05, false), "flooded shallows");
        assert!(!probe(0.05, false), "a beach standing proud of the water");
        assert!(!probe(0.05, true), "even straight out of the water");
    }

    /// The reported bug (#1654): every seeded boat floating at rest is in
    /// the water. Buoyancy holds her origin her water rest length over the
    /// surface, and since #1363 that stands her box's bottom 0.23-0.38 m
    /// clear of it - outside a person's tolerance, where no boat ever
    /// touched the water or left a wake. Through the slack her own
    /// locomotion reports, over open water.
    #[test]
    fn every_seeded_boat_floating_at_rest_is_in_the_water() {
        use crate::pds::avatar::AvatarRecord;
        use crate::seeded_defaults::ChassisFamily;
        let water = flat_water();
        let mut checked = 0;
        for s in (0u64..400).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat) {
            let loco = AvatarRecord::default_for_seed(s).locomotion;
            let LocomotionConfig::HoverBoat(p) = &loco else {
                panic!("seed {s}: a boat floats on a hover-boat preset");
            };
            let at_rest = Vec3::new(0.0, p.water_rest_length.0, 0.0);
            let h = locomotion_total_height(&loco);
            let slack = locomotion_water_slack(&loco);
            assert!(
                probe_water(at_rest, h, slack, false, &water, Some(-2.0)).is_some(),
                "seed {s}: her box's bottom floats {} m over the water, clear of a {slack} m band",
                p.water_rest_length.0 - 0.5 * h
            );
            checked += 1;
        }
        assert!(checked > 20, "too few boats sampled: {checked}");
    }

    #[test]
    fn first_contact_emits_enter_only() {
        let curr = water_contact(0, 0.5);
        let (trans, state) = compute_transitions(None, Some(curr));
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Enter);
        assert_eq!(state, Some(curr));
    }

    #[test]
    fn same_surface_continued_emits_dwell() {
        let prev = water_contact(0, 0.3);
        let curr = water_contact(0, 0.5);
        let (trans, state) = compute_transitions(Some(prev), Some(curr));
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Dwell);
        // Dwell carries the *current* surface payload so depth changes
        // are visible to consumers.
        assert_eq!(trans[0].surface, curr);
        assert_eq!(state, Some(curr));
    }

    #[test]
    fn leaving_water_emits_exit_and_clears_state() {
        let prev = water_contact(0, 0.5);
        let (trans, state) = compute_transitions(Some(prev), None);
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Exit);
        assert_eq!(trans[0].surface, prev);
        assert_eq!(state, None);
    }

    #[test]
    fn switching_water_planes_emits_exit_then_enter() {
        let prev = water_contact(0, 0.4);
        let curr = water_contact(1, 0.6);
        let (trans, state) = compute_transitions(Some(prev), Some(curr));
        assert_eq!(trans.len(), 2);
        assert_eq!(trans[0].phase, ContactPhase::Exit);
        assert_eq!(trans[0].surface, prev);
        assert_eq!(trans[1].phase, ContactPhase::Enter);
        assert_eq!(trans[1].surface, curr);
        assert_eq!(state, Some(curr));
    }

    #[test]
    fn no_contact_no_history_emits_nothing() {
        let (trans, state) = compute_transitions(None, None);
        assert!(trans.is_empty());
        assert_eq!(state, None);
    }

    #[test]
    fn intensity_saturates_when_fully_submerged() {
        let cfg = LocomotionConfig::Humanoid(Box::default());
        let height = locomotion_total_height(&cfg);
        let v = Vec3::ZERO;
        let shallow = intensity_for(&water_contact(0, 0.1), v, &cfg);
        let mid = intensity_for(&water_contact(0, height * 0.5), v, &cfg);
        let submerged = intensity_for(&water_contact(0, height * 5.0), v, &cfg);
        assert!(shallow > 0.0 && shallow < 0.2);
        assert!((mid - 0.5).abs() < 1e-3);
        assert!((submerged - 1.0).abs() < 1e-6);
    }

    fn terrain_contact() -> SurfaceContact {
        SurfaceContact::Terrain {
            material_blend: [1.0, 0.0, 0.0, 0.0],
            normal: Vec3::Y,
            ground_y: 0.0,
        }
    }

    /// A car on its wheels is on the ground and a car in the air is not
    /// (#1549): measured from its box alone - 0.47 m up on Jink's car - it
    /// never touched down, so it threw no dust and made no landing sound.
    /// The car stands where the game stands it, at its derived ride height,
    /// over a level map.
    #[test]
    fn a_car_on_its_wheels_is_on_the_ground() {
        let cfg = LocomotionConfig::Car(Box::default());
        let ride = crate::pds::avatar::default_visuals::ground_ride_height(&cfg)
            .expect("a car has a ride height");
        let total = locomotion_total_height(&cfg);
        let reach = locomotion_ground_reach(&cfg);
        let terrain = TerrainSurfaceQuery::new(
            TerrainQuery::new(
                bevy_symbios_ground::HeightMap::new(9, 9, 1.0),
                bevy_symbios_ground::SplatMapper::default(),
            ),
            4.0,
        );
        let at = |y: f32| Vec3::new(0.0, y, 0.0);
        assert!(
            ride - total / 2.0 > gcfg::CONTACT_SLACK,
            "the premise: the box alone rides clear of the ground"
        );
        assert!(
            probe_terrain(at(ride), total, 0.0, false, &terrain).is_none(),
            "measured from the box, the car is in the air"
        );
        assert!(
            probe_terrain(at(ride), total, reach, false, &terrain).is_some(),
            "on its wheels, at rest"
        );
        assert!(
            probe_terrain(at(ride + reach + 1.0), total, reach, true, &terrain).is_none(),
            "a metre clear of where its wheels touch, it is in the air"
        );
    }

    /// #1586: on ground built from real Berlin a footfall reads the layer
    /// the land use paints there - the stone of a street, the grass of a
    /// park - not the splat rules', which put real altitude on the summit
    /// layers. So its dust and its sound are the ground the player sees.
    #[test]
    fn a_footfall_on_berlin_ground_reads_its_land_use() {
        use geodata::berlin::LandUse;
        // 9 x 9 cells 1 m apart at 40 m: street west of x = 0, park east.
        let mut hm = bevy_symbios_ground::HeightMap::new(9, 9, 1.0);
        hm.data_mut().fill(40.0);
        let cover = (0..81)
            .map(|i| (i % 9 > 4).then_some(LandUse::Park))
            .collect();
        let ground = crate::terrain::geo::GeoGround::from_cover(9, 1.0, cover, None);
        let terrain = TerrainSurfaceQuery::new(
            TerrainQuery::new(hm, bevy_symbios_ground::SplatMapper::default()),
            4.0,
        )
        .with_ground(Some(ground));
        let blend = |x: f32| match probe_terrain(Vec3::new(x, 41.0, 0.0), 2.0, 0.0, false, &terrain)
        {
            Some(SurfaceContact::Terrain { material_blend, .. }) => material_blend,
            other => panic!("standing on the ground: {other:?}"),
        };
        assert_eq!(blend(-3.0), [0.0, 0.0, 1.0, 0.0], "a street is stone");
        assert_eq!(
            blend(3.0),
            [1.0, 0.0, 0.0, 0.0],
            "a park is the green layer"
        );
    }

    #[test]
    fn terrain_contact_uses_hysteresis_like_water() {
        let w = [1.0, 0.0, 0.0, 0.0];
        let n = Vec3::Y;
        // Body bottom 0.4 m above ground: past the tight enter slack
        // (0.30) but inside the wide exit slack (0.55).
        // surface_y = 0, body_bottom_y = 0.4.
        assert!(classify_terrain_contact(0.4, 0.0, false, w, n).is_none());
        assert!(classify_terrain_contact(0.4, 0.0, true, w, n).is_some());
        // 0.8 m clear → genuine departure even with hysteresis.
        assert!(classify_terrain_contact(0.8, 0.0, true, w, n).is_none());
        // Penetrating the ground → contact regardless of prior state.
        assert!(classify_terrain_contact(-0.1, 0.0, false, w, n).is_some());
    }

    #[test]
    fn terrain_contact_carries_blend_normal_and_ground_y() {
        let w = [0.1, 0.2, 0.6, 0.1];
        let n = Vec3::new(0.0, 0.8, 0.6).normalize();
        let c = classify_terrain_contact(2.45, 2.5, false, w, n).unwrap();
        match c {
            SurfaceContact::Terrain {
                material_blend,
                normal,
                ground_y,
            } => {
                assert_eq!(material_blend, w);
                assert!((normal - n).length() < 1e-6);
                // The stored ground height is the classifier's own
                // heightmap sample (what the decal stamper anchors to).
                assert!((ground_y - 2.5).abs() < 1e-6);
            }
            _ => panic!("expected Terrain"),
        }
        assert_eq!(c.kind(), SurfaceKind::Terrain);
    }

    #[test]
    fn terrain_intensity_floors_when_grounded_and_saturates_on_hard_landing() {
        let cfg = LocomotionConfig::Humanoid(Box::default());
        let c = terrain_contact();
        // Standing still (no vertical speed) keeps the grounded floor so
        // footprints still accrue.
        let still = intensity_for(&c, Vec3::ZERO, &cfg);
        assert!((still - gcfg::INTENSITY_GROUNDED_FLOOR).abs() < 1e-6);
        // Upward velocity is not an impact - still just the floor.
        let rising = intensity_for(&c, Vec3::new(0.0, 3.0, 0.0), &cfg);
        assert!((rising - gcfg::INTENSITY_GROUNDED_FLOOR).abs() < 1e-6);
        // Half the reference downward speed → ~0.5.
        let mid = intensity_for(
            &c,
            Vec3::new(0.0, -gcfg::INTENSITY_VEL_REF * 0.5, 0.0),
            &cfg,
        );
        assert!((mid - 0.5).abs() < 1e-3);
        // Hard landing saturates at 1.0.
        let slam = intensity_for(&c, Vec3::new(0.0, -100.0, 0.0), &cfg);
        assert!((slam - 1.0).abs() < 1e-6);
    }

    #[test]
    fn terrain_dwell_does_not_re_enter_each_frame() {
        // Two terrain contacts are the same specific surface, so a
        // standing avatar dwells (one sample) rather than Exit+Enter.
        let (trans, state) = compute_transitions(Some(terrain_contact()), Some(terrain_contact()));
        assert_eq!(trans.len(), 1);
        assert_eq!(trans[0].phase, ContactPhase::Dwell);
        assert_eq!(state, Some(terrain_contact()));
    }

    #[test]
    fn water_to_terrain_emits_exit_then_enter() {
        let (trans, _) = compute_transitions(Some(water_contact(0, 0.5)), Some(terrain_contact()));
        assert_eq!(trans.len(), 2);
        assert_eq!(trans[0].phase, ContactPhase::Exit);
        assert_eq!(trans[0].surface.kind(), SurfaceKind::Water);
        assert_eq!(trans[1].phase, ContactPhase::Enter);
        assert_eq!(trans[1].surface.kind(), SurfaceKind::Terrain);
    }
}
