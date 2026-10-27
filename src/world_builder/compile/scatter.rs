//! Scatter placement helpers: deterministic sampling inside a
//! [`ScatterBounds`], the placement-naturalness transforms layered on top of
//! it (#912), and the dominant-biome lookup the scatter biome filter
//! consults. The biome lookup delegates to `bevy_symbios_ground::SplatRule`
//! so the splat-rule weight formula stays single-sourced upstream.
//!
//! # The determinism discipline
//!
//! Placement must be bit-stable for a given seed across peers, and - the
//! stronger property this module maintains - **changing a naturalness knob
//! must not move the instances that knob is not about**. That rules out the
//! obvious implementations (draw an extra sample to decide a cluster, draw a
//! rejection roll for edge density), because every extra draw shifts the
//! stream for every later instance.
//!
//! So each knob is built to cost zero draws from the placement RNG:
//!
//! * [`ScatterNaturalness::edge_falloff`] and
//!   [`ScatterNaturalness::clumping`] are pure *remappings* of a sample that
//!   was already drawn - the uniform draw happens either way and is then
//!   warped in place.
//! * The cluster seeds those two need come from a **separate** RNG derived
//!   from the same `local_seed` ([`cluster_centers`]), never from the
//!   placement stream.
//! * `max_slope_deg` only rejects. Because the per-instance decorations moved
//!   off the placement stream (below), a rejection now consumes exactly what
//!   an acceptance would have, so tightening the cutoff *removes* instances
//!   without relocating the survivors.
//! * `scale_jitter` / `tilt_jitter` draw from the per-scatter side stream
//!   ([`JITTER_SEED_SALT`]) in a fixed 4-draw group per placed instance, so
//!   the side stream stays in lockstep with the instance index and toggling
//!   either knob changes nothing else.
//! * `ground_tilt` (#1765) draws from neither stream: it reads the ground's
//!   normal under a copy that has already been placed, so it re-poses the
//!   copies without moving one.

use bevy::prelude::*;
use bevy_symbios_ground::SplatRule;
use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::{RngCore, SeedableRng};

use crate::pds::{
    Fp2, ScatterBounds, ScatterNaturalness, SovereignSplatRule, SovereignTerrainConfig,
};

/// Seed offset for the per-scatter cluster-seed stream. Distinct from
/// [`JITTER_SEED_SALT`] so a scatter's clump layout and its per-instance
/// jitter can't correlate.
const CLUSTER_SEED_SALT: u64 = 0xC105_7E12_C105_7E12;

/// Seed offset for the per-scatter instance-decoration stream (scale, tilt).
pub(crate) const JITTER_SEED_SALT: u64 = 0x1177_E812_1177_E812;

/// Draws taken from the jitter stream per placed instance. Fixed, and taken
/// unconditionally, so the stream index tracks the instance index no matter
/// which knobs are enabled.
pub(crate) const JITTER_DRAWS_PER_INSTANCE: usize = 4;

/// Upper bound on cluster seeds per scatter. Beyond this the clumps are
/// finer than the props themselves and the effect reads as noise.
const MAX_CLUSTERS: usize = 32;

/// Where the point `(lx, lz)` of a rect scatter's own frame lands on the
/// ground: the rect's X runs along `(cos r, sin r)`. This is what a rect's
/// `rotation` means - the outline and the anchor turn by [`rect_turn`] to
/// match it (#1512).
pub(crate) fn rect_point(center: [f32; 2], rotation: f32, lx: f32, lz: f32) -> (f32, f32) {
    // libm (#1132): a rotated rect's instance positions must agree between
    // a native and a browser peer, because the position is what the slope
    // and biome filters are then sampled AT - so a difference here feeds
    // the accept/reject decisions.
    let (sin, cos) = (libm::sinf(rotation), libm::cosf(rotation));
    let rx = lx * cos - lz * sin;
    let rz = lx * sin + lz * cos;
    (center[0] + rx, center[1] + rz)
}

/// The turn a rect scatter's `rotation` stands for, as [`rect_point`] lays
/// the rect out: glam's `from_rotation_y(-r)`, since glam's own
/// `from_rotation_y(r)` turns X to `(cos r, -sin r)`, the other way. The
/// anchor its copies hang from and the World Editor's outline both turn by
/// it; both used `from_rotation_y(r)`, so the outline was drawn mirrored
/// from where the copies stand and a copy with no yaw of its own faced
/// mirrored from the rect's sides (#1512).
pub(crate) fn rect_turn(rotation: f32) -> Quat {
    Quat::from_rotation_y(-rotation)
}

/// The frame a scatter's copies hang from, on the ground plane: at the
/// bounds' centre, and for a rect turned by [`rect_turn`]. The compile
/// spawns its anchor here (then snapped to the ground), and the render
/// tool's preview poses its copies in it, so the two turn them alike.
pub(crate) fn scatter_anchor(bounds: &ScatterBounds) -> Transform {
    match bounds {
        ScatterBounds::Circle { center, .. } => Transform::from_xyz(center.0[0], 0.0, center.0[1]),
        ScatterBounds::Rect {
            center, rotation, ..
        } => {
            Transform::from_xyz(center.0[0], 0.0, center.0[1]).with_rotation(rect_turn(rotation.0))
        }
    }
}

/// Uniform sample inside the scatter region, warped by `edge_falloff`.
/// Circle bounds use rejection sampling so the base distribution stays flat
/// instead of clumping at the centre (which a naïve `radius * random()`
/// would produce).
///
/// `edge_falloff` is an exponent on the normalised radius: `0` leaves the
/// uniform distribution alone, and larger values push samples inward, so the
/// stand thins toward its boundary instead of ending in a mown circular
/// edge. It is applied *after* the draw, so it never changes how many u32s
/// this consumes for a given RNG state.
pub(crate) fn sample_bounds(
    bounds: &ScatterBounds,
    rng: &mut ChaCha8Rng,
    edge_falloff: f32,
) -> (f32, f32) {
    match bounds {
        ScatterBounds::Rect {
            center,
            extents,
            rotation,
        } => {
            // Per-axis falloff: the rect thins toward all four edges.
            let lx = falloff_axis(unit_f32(rng), edge_falloff) * extents.0[0];
            let lz = falloff_axis(unit_f32(rng), edge_falloff) * extents.0[1];
            rect_point(center.0, rotation.0, lx, lz)
        }
        ScatterBounds::Circle { center, radius } => loop {
            let x = unit_f32(rng);
            let z = unit_f32(rng);
            if x * x + z * z <= 1.0 {
                // Radial falloff: `r -> r^(1+falloff)` on the unit disc.
                let (x, z) = if edge_falloff > 0.0 {
                    let r = (x * x + z * z).sqrt();
                    if r > f32::EPSILON {
                        // libm (#1132), same reason as the rect rotation above.
                        let k = libm::powf(r, edge_falloff);
                        (x * k, z * k)
                    } else {
                        (x, z)
                    }
                } else {
                    (x, z)
                };
                return (center.0[0] + x * radius.0, center.0[1] + z * radius.0);
            }
        },
    }
}

/// Signed `|v|^(1+falloff)` for a `[-1, 1]` axis sample - the rect
/// counterpart of the disc's radial remap.
fn falloff_axis(v: f32, falloff: f32) -> f32 {
    if falloff <= 0.0 {
        return v;
    }
    // libm (#1132), same reason as the rect rotation.
    v.signum() * libm::powf(v.abs(), 1.0 + falloff)
}

/// Deterministic cluster seeds for a scatter, drawn from their own stream so
/// they never perturb the placement RNG. Seed count scales with the square
/// root of the instance count - enough clumps that a dense stand still reads
/// as patchy, few enough that a sparse one doesn't degenerate into one
/// instance per clump.
///
/// The seeds carry the same `edge_falloff` warp as the samples do, so
/// contracting samples onto them preserves rather than flattens the stand's
/// radial density profile.
///
/// Always computed (it is a few dozen samples), so enabling `clumping` never
/// changes anything but the contraction itself.
pub(crate) fn cluster_centers(
    bounds: &ScatterBounds,
    count: u32,
    local_seed: u64,
    edge_falloff: f32,
) -> Vec<(f32, f32)> {
    let n = (((count as f32).sqrt() / 1.5).round() as usize).clamp(2, MAX_CLUSTERS);
    let mut rng = ChaCha8Rng::seed_from_u64(local_seed ^ CLUSTER_SEED_SALT);
    (0..n)
        .map(|_| sample_bounds(bounds, &mut rng, edge_falloff))
        .collect()
}

/// Pull a sample toward its nearest cluster seed by `clumping`. `0` is the
/// identity (flat uniform); `0.5` halves each seed's catchment radius, which
/// leaves thickets with clearings between them.
///
/// Contraction toward a point inside the region can never leave it - both
/// bounds shapes are convex - so this cannot push an instance outside the
/// authored stand.
pub(crate) fn apply_clumping(
    pos: (f32, f32),
    clusters: &[(f32, f32)],
    clumping: f32,
) -> (f32, f32) {
    if clumping <= 0.0 || clusters.is_empty() {
        return pos;
    }
    let mut best = clusters[0];
    let mut best_d2 = f32::MAX;
    for &c in clusters {
        let (dx, dz) = (pos.0 - c.0, pos.1 - c.1);
        let d2 = dx * dx + dz * dz;
        if d2 < best_d2 {
            best_d2 = d2;
            best = c;
        }
    }
    let keep = 1.0 - clumping;
    (
        best.0 + (pos.0 - best.0) * keep,
        best.1 + (pos.1 - best.1) * keep,
    )
}

/// One instance's pose decorations, drawn as a fixed group from the
/// scatter's side stream.
pub(crate) struct InstanceJitter {
    /// Spin about the instance's own Y axis, radians.
    pub yaw: f32,
    /// Compass direction the instance leans toward, radians.
    pub tilt_azimuth: f32,
    /// Lean off vertical, radians. Signed - a negative angle is the same
    /// lean 180° round, which is why one draw covers the whole cone.
    pub tilt_angle: f32,
    /// Uniform scale multiplier, log-uniform about `1.0`.
    pub scale: f32,
}

/// Draw one instance's decorations. Takes exactly
/// [`JITTER_DRAWS_PER_INSTANCE`] samples every call regardless of which
/// knobs are enabled - that fixed group size is what keeps the side stream
/// aligned with the instance index, so toggling `tilt_jitter` cannot change
/// the next instance's scale.
///
/// Yaw lives on this stream rather than the placement stream (where it used
/// to sit) so that the placement stream carries *nothing but positions*.
/// That is what makes `max_slope_deg` purely subtractive: rejecting a sample
/// now consumes exactly what accepting it would have, so the survivors stay
/// put instead of the whole stand reshuffling.
pub(crate) fn instance_jitter(rng: &mut ChaCha8Rng, n: &ScatterNaturalness) -> InstanceJitter {
    // Drawn as one fixed-size array rather than four statements: the array
    // length *is* the group-size contract, checked by the compiler instead
    // of by a comment asking the next editor not to add a fifth draw.
    let [yaw, azimuth, tilt, scale]: [f32; JITTER_DRAWS_PER_INSTANCE] =
        std::array::from_fn(|_| unit_f32(rng));
    InstanceJitter {
        yaw: yaw * std::f32::consts::PI,
        tilt_azimuth: azimuth * std::f32::consts::PI,
        tilt_angle: tilt * n.tilt_jitter.0,
        // Log-uniform: a 0.85× and a 1.18× instance are the same one step
        // away from nominal, which an additive spread would not give.
        // libm (#1132). Not a discrete decision - an instance a ULP larger
        // is invisible - but it is seeded derived geometry, and leaving one
        // `f32` transcendental in the path would make "this derivation is
        // bit-identical across targets" a claim with an asterisk.
        scale: libm::expf(scale * n.scale_jitter.0),
    }
}

/// Compose one instance's local pose from its position, a jitter draw and
/// `ground`, the ground's normal under it in the anchor's frame (`Vec3::Y`
/// where there is no ground to read, or the copy floats).
///
/// Shared by the compiler and by the headless render tool's scatter
/// preview, so a contact sheet shows the arrangement the game will
/// actually build rather than a separate approximation of it.
///
/// The scale stays uniform: the generator's own root transform composes on
/// top of this, and a non-uniform parent scale would shear any child that
/// carries a rotation.
pub(crate) fn instance_pose(
    local_pos: Vec3,
    jitter: &InstanceJitter,
    random_yaw: bool,
    n: &ScatterNaturalness,
    ground: Vec3,
) -> Transform {
    let mut rotation = if random_yaw {
        Quat::from_rotation_y(jitter.yaw)
    } else {
        Quat::IDENTITY
    };
    if n.tilt_jitter.0 > 0.0 {
        // A *pure* lean: rotate about a horizontal axis pointing at
        // `tilt_azimuth`, which is `Ry(az) · Rx(θ) · Ry(-az)`. Dropping the
        // trailing `Ry(-az)` would be simpler and wrong - it would leave a
        // net yaw of `az` behind, silently spinning instances in a scatter
        // that explicitly asked for `random_yaw: false`.
        let lean = Quat::from_rotation_y(jitter.tilt_azimuth)
            * Quat::from_rotation_x(jitter.tilt_angle)
            * Quat::from_rotation_y(-jitter.tilt_azimuth);
        rotation = lean * rotation;
    }
    if n.ground_tilt.0 > 0.0 {
        // The copy's up turned toward the ground's normal by the shortest
        // arc, `ground_tilt` of the way: a normalised lerp of the two ups,
        // which bisects the angle at a half and needs no transcendental
        // (#1132). The shortest arc spins nothing about the new up, so the
        // copy keeps its heading, and the lean above rides on the slope.
        let up = Vec3::Y.lerp(ground, n.ground_tilt.0).normalize_or_zero();
        if up != Vec3::ZERO {
            rotation = Quat::from_rotation_arc(Vec3::Y, up) * rotation;
        }
    }
    let mut tf = Transform::from_translation(local_pos).with_rotation(rotation);
    if n.scale_jitter.0 > 0.0 {
        tf.scale = Vec3::splat(jitter.scale);
    }
    tf
}

/// Replay a scatter's arrangement with every terrain-dependent filter
/// skipped, for the headless render tool.
///
/// The `--room` contact sheet has no heightmap, so the biome allow-list and
/// the slope cutoff have nothing to resolve against and
/// [`try_sample`] would correctly refuse to place anything. This keeps the
/// parts that *are* meaningful without terrain - the distribution warps and
/// the per-instance pose - so the sheet still shows what clumping and
/// jitter do. Instances sit on the ground plane rather than terrain-snapped.
///
/// Native-only, like the render tool that is its sole consumer.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) struct ScatterPreview {
    bounds: ScatterBounds,
    naturalness: ScatterNaturalness,
    random_yaw: bool,
    clusters: Vec<(f32, f32)>,
    rng: ChaCha8Rng,
    jitter_rng: ChaCha8Rng,
}

#[cfg(not(target_arch = "wasm32"))]
impl ScatterPreview {
    pub(crate) fn new(
        bounds: &ScatterBounds,
        count: u32,
        local_seed: u64,
        naturalness: &ScatterNaturalness,
        random_yaw: bool,
    ) -> Self {
        Self {
            bounds: bounds.clone(),
            naturalness: *naturalness,
            random_yaw,
            clusters: cluster_centers(bounds, count, local_seed, naturalness.edge_falloff.0),
            rng: ChaCha8Rng::seed_from_u64(local_seed),
            jitter_rng: ChaCha8Rng::seed_from_u64(local_seed ^ JITTER_SEED_SALT),
        }
    }

    /// The next instance's pose on the ground plane: posed in the
    /// scatter's anchor frame and set in the world through it, as the
    /// compile parents a copy to its anchor - so a turned rect's copies turn
    /// with it here too (#1512).
    pub(crate) fn next_pose(&mut self) -> Transform {
        let (x, z) = apply_clumping(
            sample_bounds(&self.bounds, &mut self.rng, self.naturalness.edge_falloff.0),
            &self.clusters,
            self.naturalness.clumping.0,
        );
        let jitter = instance_jitter(&mut self.jitter_rng, &self.naturalness);
        let anchor = scatter_anchor(&self.bounds);
        let local = anchor
            .compute_affine()
            .inverse()
            .transform_point3(Vec3::new(x, 0.0, z));
        // No ground to read: the preview's copies stand on a level plane.
        anchor * instance_pose(local, &jitter, self.random_yaw, &self.naturalness, Vec3::Y)
    }
}

/// Terrain steepness at a world XZ, as `1 - normal.y` - the same measure
/// [`dominant_biome`] scores the splat rules against, so a scatter's slope
/// cutoff and its biome allow-list are talking about the same quantity.
/// `0` is dead flat, `1` is a vertical face.
pub(crate) fn terrain_slope_at(
    heightmap: &crate::terrain::FinishedHeightMap,
    world_x: f32,
    world_z: f32,
) -> f32 {
    (1.0 - heightmap.normal_at(world_x, world_z).y).max(0.0)
}

/// Convert [`ScatterNaturalness::max_slope_deg`] into the `1 - normal.y`
/// cutoff [`terrain_slope_at`] produces, so the trigonometry is paid once
/// per scatter rather than once per sample. `1 - cos θ`: 15° → 0.034,
/// 30° → 0.134, 45° → 0.293.
pub(crate) fn slope_cutoff(naturalness: &ScatterNaturalness) -> Option<f32> {
    naturalness
        .max_slope_deg
        // `libm::cosf`, not `f32::cos` (#1132). This value is COMPARED against
        // a sampled slope to accept or reject each instance, so it is one of
        // the handful of places in the derivation where a one-ULP difference
        // between two peers stops being invisible: a sample sitting within a
        // ULP of the cutoff gets a tree on one peer and bare ground on the
        // other. `f32::cos` links the target's libm - glibc on native,
        // compiler-builtins on wasm - and the two disagree in the last bit on
        // roughly one degree value in eighty (measured over 0-90° at 0.001°
        // steps on x86-64).
        .map(|deg| 1.0 - libm::cosf(deg.0.to_radians()))
}

/// Everything a scatter sample is tested against, resolved once per unit.
/// Grouped so the accept/reject decision can live in one place that both
/// the executor and the offline `--scatter-census` call - the two agreeing
/// is the whole point, since the census exists to report what the compiler
/// will actually place.
pub(crate) struct SampleFilters<'a> {
    pub biome_filter: &'a crate::pds::BiomeFilter,
    /// `None` on a record with no terrain generator, which collapses the
    /// biome allow-list to "never matches".
    pub terrain_cfg: Option<&'a SovereignTerrainConfig>,
    /// The water a sample is read against, at its own place (#1727).
    pub water: &'a dyn crate::water::bodies::WaterAt,
    /// `(centre_x, centre_z, radius²)` road districts to stay out of.
    pub urban_exclusions: &'a [(f32, f32, f32)],
    /// Pre-resolved [`slope_cutoff`].
    pub slope_cutoff: Option<f32>,
    /// The inventory trees a scatter of trees keeps clear of, on ground
    /// built from real Berlin ([`tree_clearance`]).
    pub tree_clearance: Option<&'a crate::terrain::geo::street_level::TreeClearance>,
}

/// The inventory trees a scatter of `generator_ref` keeps clear of (#1588):
/// where its generator grows a tree - an L-system, as every seeded tree
/// stand is - and the ground's street level has trees, which stand where
/// Berlin's do. `None` everywhere else. Ground cover grows under a crown,
/// and so does a seeded fern stand, a lathe since #1711: the clearance is
/// a crown's width and a fern has none. The clearance is the street
/// level's own, built once when it was decoded.
pub(crate) fn tree_clearance<'a>(
    record: &crate::pds::RoomRecord,
    generator_ref: &str,
    heightmap: Option<&'a crate::terrain::FinishedHeightMap>,
) -> Option<&'a crate::terrain::geo::street_level::TreeClearance> {
    let grows_trees = record
        .generators
        .get(generator_ref)
        .is_some_and(|g| matches!(g.kind, crate::pds::GeneratorKind::LSystem { .. }));
    if !grows_trees {
        return None;
    }
    let level = heightmap?.ground()?.street_level()?;
    (!level.trees.is_empty()).then(|| level.tree_clearance())
}

/// The room an arrival stands in at a world's landing (m), and the reach of
/// the clear ground behind it the arrival camera looks from: no scatter
/// grows there, so no visitor lands inside a tree nor first sees the world
/// through one (#1776).
const LANDING_KEEP: f32 = 4.0;

/// How many discs, past the landing's own, keep the ground clear back to
/// where the arrival camera rests (#1776): its rest radius over the ground
/// in steps a little shorter than [`LANDING_KEEP`], so they overlap.
const ARRIVAL_STEPS: usize = 3;

/// `(centre_x, centre_z, radius²)` of the ground every scatter keeps off
/// (#1776): each seeded structure's footprint where the compile stands it -
/// walked as [`super::pad::snapped_absolute_anchor`] walks it - the world's
/// landing as a body is set down there ([`super::water::landing_on`]) with
/// the ground behind it the arrival camera looks from (#1785), and a seeded
/// viewpoint's view down to its landmark (#1777). So no tree,
/// boulder or tuft grows through a seeded building, its gate or its
/// monument, on the spot a visitor arrives at, nor across their first view.
/// A copy the sampler accepts there is suppressed, not drawn again: it
/// spends its count slot and its jitter draws as if grown and grows nothing,
/// so every other copy keeps its place and its pose - a saved world loses
/// only what grew in the way. A seeded structure is a snapped `Absolute`
/// placement opting into Avoid Water, the seeded pipeline's marker (see
/// `pad`), so an editor's own placements are never ground a scatter avoids -
/// and the world's signature feature ([`FEATURE`]) and its crafts' places
/// ([`KEPT_BY_NAME`]), whose giant tree or
/// sea stack avoids no water and keeps its ground all the same (#1781).
/// With no heightmap yet, each stands where its record put it. Its paths,
/// fields and orchard are kept off too, and its hamlet's houses, each where
/// its copy stands ([`hamlet_grounds`], #1797).
pub(crate) fn structure_exclusions(
    record: &crate::pds::RoomRecord,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    water: &crate::water::bodies::RoomWater,
) -> Vec<(f32, f32, f32)> {
    let ground = heightmap.map(|hm| super::pad::AnchorGround::new(hm, water));
    let mut keep: Vec<(f32, f32, f32)> = Vec::new();
    // The settlement's landmark, where it stands, and its ground's radius.
    let mut landmark = None;
    for placement in &record.placements {
        let crate::pds::Placement::Absolute {
            generator_ref,
            transform,
            snap_to_terrain,
            avoid_water,
            avoid_water_clearance,
            ..
        } = placement
        else {
            continue;
        };
        if !(KEPT_BY_NAME.contains(&generator_ref.as_str()) || *snap_to_terrain && *avoid_water) {
            continue;
        }
        let radius = avoid_water_clearance.0 * transform.scale.0[0].max(0.0);
        // A snapped one where the compile stands it; an unsnapped one where
        // its record does - but a seeded place for a craft where the ground
        // takes it, and one it does not keeps nothing clear (#1782).
        let stands = match &ground {
            Some(ground) => match super::pad::place_stands(
                ground,
                generator_ref,
                transform,
                *snap_to_terrain,
                *avoid_water,
                avoid_water_clearance.0,
                crate::pds::find_terrain_config(record).map_or(&[][..], |t| &t.paths.tracks),
            ) {
                super::pad::PlaceStand::Not => continue,
                super::pad::PlaceStand::At(at) => at,
                super::pad::PlaceStand::Other if *snap_to_terrain => {
                    super::pad::snapped_absolute_anchor(
                        ground,
                        transform,
                        *avoid_water,
                        avoid_water_clearance.0,
                    )
                }
                super::pad::PlaceStand::Other => {
                    bevy::math::Vec3::from_array(transform.translation.0)
                }
            },
            None => bevy::math::Vec3::from_array(transform.translation.0),
        };
        if radius > 0.0 {
            keep.push((stands.x, stands.z, radius * radius));
        }
        // The first, as the landing turns to it (`water::seeded_landmark`).
        if generator_ref == LANDMARK && landmark.is_none() {
            landmark = Some((stands.x, stands.z, radius));
        }
    }
    let landing = match heightmap {
        Some(heightmap) => super::water::landing_on(record, heightmap),
        None => record.default_landing,
    };
    if let Some(landing) = &landing {
        let [x, z] = landing.pos.0;
        // Behind a body facing the landing's yaw - its forward is (-sin,
        // -cos) - out to where the camera rests over the ground, at the
        // landing's own pitch (#1777) or the rest. libm (#1132): these discs
        // decide which copies grow, on every peer.
        let yaw = landing.yaw_deg.0.to_radians();
        let behind = [libm::sinf(yaw), libm::cosf(yaw)];
        let pitch = landing
            .pitch_deg
            .map_or(crate::config::camera::ORBIT_PITCH, |pitch| {
                pitch.0.to_radians()
            });
        let reach = crate::config::camera::ORBIT_RADIUS * libm::cosf(pitch);
        for step in 0..=ARRIVAL_STEPS {
            let back = reach * step as f32 / ARRIVAL_STEPS as f32;
            keep.push((
                x + behind[0] * back,
                z + behind[1] * back,
                LANDING_KEEP * LANDING_KEEP,
            ));
        }
        // The view down to the landmark (#1777): a seeded viewpoint's
        // landing - one that avoids water, looking at its landmark - looks
        // at it across ground no copy grows on, discs [`LANDING_KEEP`] round
        // from the landing out to the landmark's own ground. An owner's own
        // landing, or one they turned, may look anywhere, and keeps its trees.
        if let Some((lx, lz, radius)) = landmark
            .filter(|&(lx, lz, _)| landing.avoid_water && super::water::looks_at(landing, [lx, lz]))
        {
            let (dx, dz) = (lx - x, lz - z);
            let length = (dx * dx + dz * dz).sqrt();
            let run = length - radius;
            if run > 0.0 {
                let steps = (run / LANDING_KEEP).ceil() as usize;
                for step in 1..=steps {
                    let t = run * step as f32 / steps as f32 / length;
                    keep.push((x + dx * t, z + dz * t, LANDING_KEEP * LANDING_KEEP));
                }
            }
        }
    }
    // The world's paths (#1779) and tracks (#1782): discs a little wider
    // than each way, each stretch's no further apart than their radius, so
    // they overlap.
    if let Some(paths) = crate::pds::find_terrain_config(record).map(|cfg| &cfg.paths) {
        let on = heightmap.map(|hm| (&hm.0, water));
        for (line, width) in crate::terrain::path_ways(paths, on) {
            let radius = width * 0.5 + PATH_KEEP;
            for pair in line.windows(2) {
                let (a, b) = (pair[0], pair[1]);
                let steps = ((b - a).length() / radius).ceil().max(1.0) as usize;
                for step in 0..=steps {
                    let at = a + (b - a) * (step as f32 / steps as f32);
                    keep.push((at.x, at.y, radius * radius));
                }
            }
        }
        // Its fields (#1791) the ground draws.
        for field in crate::terrain::path_fields(paths, on) {
            keep_off_rectangle(
                &mut keep,
                bevy::math::Vec2::from_array(field.center.0),
                bevy::math::Vec2::from_array(field.along()),
                [field.size.0[0] * 0.5, field.size.0[1] * 0.5],
            );
        }
    }
    // A seeded orchard's trees (#1791) and the ground between them.
    for (middle, along, half) in orchard_grounds(record) {
        keep_off_rectangle(&mut keep, middle, along, half);
    }
    // A seeded hamlet's houses (#1797), each its reach round its lot.
    for (x, z, reach) in hamlet_grounds(record) {
        keep.push((x, z, reach * reach));
    }
    keep
}

/// Discs keeping the scatters off the rectangle round `middle` (#1791) -
/// its length along the unit direction `along`, `half` its half length and
/// half depth - pushed onto `keep`: one at the middle of each cell of a
/// grid over it, cells no wider than [`RECT_KEEP`] (wider on a rectangle so
/// big that more than [`RECT_CELLS`] a side would be needed), each reaching
/// its cell's corners and [`PATH_KEEP`] past. Every point of the rectangle
/// is inside one, and none reaches past its edge by more than about a fifth
/// of a cell and that.
fn keep_off_rectangle(
    keep: &mut Vec<(f32, f32, f32)>,
    middle: bevy::math::Vec2,
    along: bevy::math::Vec2,
    half: [f32; 2],
) {
    let across = bevy::math::Vec2::new(-along.y, along.x);
    let size = half.map(|half| 2.0 * half.max(0.0));
    let cells = size.map(|side| ((side / RECT_KEEP).ceil() as usize).clamp(1, RECT_CELLS));
    let cell = [size[0] / cells[0] as f32, size[1] / cells[1] as f32];
    let radius = 0.5 * (cell[0] * cell[0] + cell[1] * cell[1]).sqrt() + PATH_KEEP;
    for j in 0..cells[1] {
        for i in 0..cells[0] {
            let u = cell[0] * (i as f32 + 0.5) - size[0] * 0.5;
            let v = cell[1] * (j as f32 + 0.5) - size[1] * 0.5;
            let at = middle + along * u + across * v;
            keep.push((at.x, at.y, radius * radius));
        }
    }
}

/// The ground of each seeded orchard (#1791, [`ORCHARD`]), the first
/// [`MAX_KEPT_ORCHARDS`]: the rectangle its grid stands its trees in, as
/// the compile stands a grid's cells (`executor::step_unit`), reaching
/// [`ORCHARD_CROWN`] past its outer trees - its middle, the unit direction
/// its rows run along (the grid's X, turned) and its half length and half
/// depth.
pub(crate) fn orchard_grounds(
    record: &crate::pds::RoomRecord,
) -> Vec<(bevy::math::Vec2, bevy::math::Vec2, [f32; 2])> {
    record
        .placements
        .iter()
        .filter_map(|placement| match placement {
            crate::pds::Placement::Grid {
                generator_ref,
                transform,
                counts: [cx, _, cz],
                gaps,
                ..
            } if generator_ref == ORCHARD => {
                let anchor = bevy::prelude::Transform::from(transform);
                let row = anchor.rotation * bevy::math::Vec3::X;
                let along = bevy::math::Vec2::new(row.x, row.z)
                    .try_normalize()
                    .unwrap_or(bevy::math::Vec2::X);
                let [gx, _, gz] = gaps.0;
                let half = [
                    cx.saturating_sub(1) as f32 * gx * 0.5 + ORCHARD_CROWN,
                    cz.saturating_sub(1) as f32 * gz * 0.5 + ORCHARD_CROWN,
                ];
                let middle = bevy::math::Vec2::new(anchor.translation.x, anchor.translation.z);
                Some((middle, along, half))
            }
            _ => None,
        })
        .take(MAX_KEPT_ORCHARDS)
        .collect()
}

/// How wide a cell of the discs keeping the scatters off a field or an
/// orchard is at the most (m, #1791), and how many cells a side at the most:
/// a field 250 m long is kept off by cells 16 m wide.
const RECT_KEEP: f32 = 6.0;
const RECT_CELLS: usize = 16;

/// How many seeded orchards the scatters keep off at the most (#1791): a
/// seeded world stands one.
const MAX_KEPT_ORCHARDS: usize = 4;

/// How far past a path's edge a scatter keeps off it (m, #1779): a tree's
/// trunk or a boulder stands beside a path, not on its edge.
const PATH_KEEP: f32 = 0.5;

/// The generator a seeded settlement's landmark is placed by.
pub(super) const LANDMARK: &str = "landmark";

/// The generator a seeded world's signature feature is placed by (#1781):
/// its water clearance is the ground the scatters keep off whether it
/// avoids water or not.
pub(super) const FEATURE: &str = "signature_feature";

/// The generators a seeded world's places for its crafts are placed by
/// (#1782) that avoid no water, whose water clearance is the ground the
/// scatters keep off all the same: the jetty's bank, the jetty placed
/// unsnapped at the water's edge, and the windsock and the tyres beside the
/// airstrip and the track.
pub(crate) const JETTY: &str = "craft_jetty";
pub(crate) const WINDSOCK: &str = "craft_windsock";
pub(crate) const TYRES: &str = "craft_tyres";
/// The built places for crafts (#1782): a helicopter's pad, an airship's
/// mast - kept clear of as any building is, and drawn only where the
/// ground the compile draws takes them (`pad::place_stands`).
pub(crate) const PAD: &str = "craft_pad";
pub(crate) const MAST: &str = "craft_mast";

/// The generator a seeded world's orchard is placed by (#1791): a grid of
/// its fruit tree, whose ground the scatters keep off ([`orchard_grounds`]).
pub(crate) const ORCHARD: &str = "seeded_orchard";

/// How far an orchard's trees' crowns reach past them (m, #1791): an
/// apple's is about 7 m across.
pub(crate) const ORCHARD_CROWN: f32 = 4.0;

/// The generator a seeded world's hamlet is placed by (#1797): one set of
/// its theme's detached house, a copy a house, whose ground the scatters
/// keep off ([`hamlet_grounds`]).
pub(crate) const HAMLET: &str = "seeded_hamlet";

/// How many of a seeded hamlet's houses the scatters keep off at the most
/// (#1797): a seeded world stands eight at the most.
const MAX_KEPT_HOUSES: usize = 32;

/// The furthest a hamlet's house is taken to reach (m, #1797): a detached
/// house on its broadest lot reaches 11 m.
const MAX_HOUSE_REACH: f32 = 40.0;

/// How far a detached house on a lot `frontage` by `depth` metres reaches
/// from its lot's middle, turned any way (m, #1797): half its lot's
/// diagonal with the steps and porch its front and back stand out
/// (`street::MAX_REACH_M`) - `StreetFit::reach_m`, by a square root rather
/// than `hypot` (#1132), as it decides where copies grow on every peer.
pub(crate) fn house_reach(frontage: f32, depth: f32) -> f32 {
    let depth = depth + 2.0 * crate::catalogue::items::street::MAX_REACH_M;
    (frontage * frontage + depth * depth).sqrt() / 2.0
}

/// The broadest a hamlet's house's lot is taken to be either way (m,
/// #1797): a detached house's broadest is 16 m.
const MAX_HAMLET_LOT: f32 = 64.0;

/// The lot a seeded hamlet's house of `variant` stands on, its frontage and
/// its depth (m, #1797): by its grammar values, its kind's own where it
/// names none.
pub(crate) fn hamlet_lot(variant: Option<&crate::pds::InstanceVariant>) -> (f32, f32) {
    let own = crate::catalogue::items::street::StreetKind::Detached.default_fit();
    let value = |name: &str, or: f32| {
        variant
            .and_then(|variant| variant.params.0.get(name))
            .map_or(or, |value| value.0)
            .clamp(0.0, MAX_HAMLET_LOT)
    };
    (
        value("Frontage", own.frontage_m()),
        value("Depth", own.depth_m()),
    )
}

/// The ground each house of a seeded hamlet stands on (#1797), as
/// `(x, z, radius)`: where its copy stands on the plan, and its reach by its
/// variant's lot ([`house_reach`], [`hamlet_lot`]) at its copy's size.
pub(crate) fn hamlet_grounds(record: &crate::pds::RoomRecord) -> Vec<(f32, f32, f32)> {
    record
        .placements
        .iter()
        .filter_map(|placement| match placement {
            crate::pds::Placement::Instances {
                generator_ref,
                transform,
                variants,
                copies,
                ..
            } if generator_ref == HAMLET => Some((transform, variants, copies)),
            _ => None,
        })
        .flat_map(|(transform, variants, copies)| {
            let anchor = bevy::prelude::Transform::from(transform);
            copies.iter().map(move |copy| {
                let [x, _, z] = copy.position();
                let at = anchor.translation + anchor.rotation * bevy::math::Vec3::new(x, 0.0, z);
                let (frontage, depth) = hamlet_lot(variants.get(copy.variant as usize));
                let reach = house_reach(frontage, depth) * copy.scale();
                (at.x, at.z, reach.clamp(0.0, MAX_HOUSE_REACH))
            })
        })
        .take(MAX_KEPT_HOUSES)
        .collect()
}

/// Every generator whose ground the scatters keep off by its name, snapped
/// or not: the feature and the crafts' places above.
pub(super) const KEPT_BY_NAME: [&str; 4] = [FEATURE, JETTY, WINDSOCK, TYRES];

/// Whether `(x, z)` lies inside any `(centre_x, centre_z, radius²)` disc.
pub(crate) fn inside_any(discs: &[(f32, f32, f32)], x: f32, z: f32) -> bool {
    discs.iter().any(|&(cx, cz, r2)| {
        let (dx, dz) = (x - cx, z - cz);
        dx * dx + dz * dz < r2
    })
}

/// `(centre_x, centre_z, radius²)` of every road district a scatter that
/// opted into `avoid_urban` must stay out of (#895). Empty unless the
/// scatter opted in *and* the room actually grows an enabled road network.
/// The centre follows the authored district offset (#889); a zero offset is
/// the historical spawn-centred circle.
pub(crate) fn urban_exclusions(
    record: &crate::pds::RoomRecord,
    avoid_urban: bool,
) -> Vec<(f32, f32, f32)> {
    if !avoid_urban {
        return Vec::new();
    }
    crate::pds::find_road_configs(record)
        .into_iter()
        .filter(|c| c.enabled)
        .map(|c| {
            (
                c.center.0[0],
                c.center.0[1],
                c.district_half_extent.0 * c.district_half_extent.0,
            )
        })
        .collect()
}

/// Draw one candidate and decide it: `Some((x, y, z))` in world space when
/// it is kept, `None` when a filter rejected it.
///
/// Exactly one draw group is taken from `rng` per call whatever the outcome,
/// so a rejection costs the stream the same as an acceptance. That is what
/// makes every filter here purely subtractive - tighten one and the
/// instances that survive stay exactly where they were.
pub(crate) fn try_sample(
    bounds: &ScatterBounds,
    naturalness: &ScatterNaturalness,
    clusters: &[(f32, f32)],
    rng: &mut ChaCha8Rng,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    filters: &SampleFilters<'_>,
) -> Option<(f32, f32, f32)> {
    let (world_x, world_z) = apply_clumping(
        sample_bounds(bounds, rng, naturalness.edge_falloff.0),
        clusters,
        naturalness.clumping.0,
    );

    // Road-network districts (#895) keep wild scatter out of the built-up
    // area. Purely positional, so it is checked before the terrain lookups.
    if inside_any(filters.urban_exclusions, world_x, world_z) {
        return None;
    }
    // Berlin's own trees (#1588): a seeded one keeps a crown's width off
    // each, so a park is not wooded twice over.
    if filters
        .tree_clearance
        .is_some_and(|trees| trees.near(world_x, world_z))
    {
        return None;
    }

    let Some(hm_res) = heightmap else {
        // Nothing to resolve against: every terrain filter fails closed
        // rather than silently passing every sample.
        let unconstrained = filters.biome_filter.is_noop()
            && filters.slope_cutoff.is_none()
            && naturalness.above_water_band.is_none()
            && naturalness.altitude_band.is_none();
        return unconstrained.then_some((world_x, 0.0, world_z));
    };

    let y = hm_res.world_height_at(world_x, world_z);

    // Microbiome bands (#913). Checked before the slope work because they
    // need only the height already in hand, where slope costs a normal
    // lookup - and because on a riparian scatter they reject the large
    // majority of samples.
    // Not on ground built from real Berlin (#1586), which its land use
    // zones: a seeded treeline is a fraction of a procedural world's relief,
    // tens of metres below Berlin's lowest ground, and would strip every
    // stand.
    if let Some(Fp2([lo, hi])) = naturalness.altitude_band
        && hm_res.ground().is_none()
        && !(lo..=hi).contains(&y)
    {
        return None;
    }
    if let Some(Fp2([lo, hi])) = naturalness.above_water_band {
        // No water line means the band has nothing to measure from, so it
        // fails closed - the same stance the biome allow-list takes when
        // there is no terrain generator to resolve against.
        let wl = filters.water.water_at(world_x, world_z)?;
        if !(lo..=hi).contains(&(y - wl)) {
            return None;
        }
    }

    // Steepness feeds two consumers - the allow-list (via the dominant
    // splat layer) and the explicit cutoff - so sample it once, and only if
    // one of them is going to read it.
    let slope = (!filters.biome_filter.is_noop() || filters.slope_cutoff.is_some())
        .then(|| terrain_slope_at(hm_res, world_x, world_z));
    if !filters
        .slope_cutoff
        .is_none_or(|cutoff| slope.is_some_and(|s| s <= cutoff))
    {
        return None;
    }
    if !filters.biome_filter.is_noop() {
        // Without a terrain generator the biome allow-list has no channel
        // to resolve against; treat any non-empty list as "never matches"
        // so accidental biome filters on dry-land records don't silently
        // pass through. The water clause still evaluates.
        //
        // Ground built from real Berlin is layered by its land use, not by
        // altitude bands (#1586): the layer the sample's cell is drawn
        // with, where it is natural ground - none on the built city.
        let biome = match (hm_res.ground(), filters.terrain_cfg, slope) {
            (Some(ground), _, _) => ground.scatter_layer_at(world_x, world_z),
            (None, Some(tcfg), Some(s)) => dominant_biome(tcfg, y, s),
            _ => 255,
        };
        if !filters
            .biome_filter
            .accepts(biome, y, filters.water.water_at(world_x, world_z))
        {
            return None;
        }
    }
    Some((world_x, y, world_z))
}

/// Deterministic `[-1, 1]` sample from a `ChaCha8Rng`.
pub(crate) fn unit_f32(rng: &mut ChaCha8Rng) -> f32 {
    let v = rng.next_u32() as f32 / u32::MAX as f32;
    v * 2.0 - 1.0
}

// ---------------------------------------------------------------------------
// Biome evaluation
// ---------------------------------------------------------------------------

/// Convert a wire-format [`SovereignSplatRule`] into an upstream [`SplatRule`]
/// so the weight formula can be evaluated by [`SplatRule::weight`] directly,
/// without re-implementing the smooth-range logic locally.
fn convert_rule(r: &SovereignSplatRule) -> SplatRule {
    SplatRule::new(
        (r.height_min.0, r.height_max.0),
        (r.slope_min.0, r.slope_max.0),
        r.sharpness.0,
    )
}

/// Return the dominant biome index (0=Grass, 1=Dirt, 2=Rock, 3=Snow) at the
/// given world-space (height, slope) pair, using the terrain generator's
/// splat rules. The splat rules expect *normalised* heights so we divide
/// by `height_scale` first.
pub(crate) fn dominant_biome(cfg: &SovereignTerrainConfig, height_world: f32, slope: f32) -> u8 {
    let height_norm = if cfg.height_scale.0.abs() > f32::EPSILON {
        height_world / cfg.height_scale.0
    } else {
        0.0
    };
    let weights = [
        convert_rule(&cfg.material.rules[0]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[1]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[2]).weight(height_norm, slope),
        convert_rule(&cfg.material.rules[3]).weight(height_norm, slope),
    ];
    let mut best = 0;
    let mut max_w = weights[0];
    for (i, &w) in weights.iter().enumerate().skip(1) {
        if w > max_w {
            max_w = w;
            best = i;
        }
    }
    best as u8
}

#[cfg(test)]
mod tests {
    //! The placement-naturalness knobs (#912) carry two kinds of
    //! obligation, and both are tested here:
    //!
    //! * **A determinism contract.** No knob may consume a draw from the
    //!   placement RNG. These are the tests that would fail if someone
    //!   "simplified" a warp into a rejection roll - which would silently
    //!   move every instance in every existing record.
    //! * **An effect.** Each knob has to actually do the thing its name
    //!   claims, which is checked statistically over a few thousand
    //!   samples rather than by pinning exact coordinates.
    use super::*;

    use crate::pds::{Fp, Fp2};

    fn disc(radius: f32) -> ScatterBounds {
        ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(radius),
        }
    }

    fn rect() -> ScatterBounds {
        ScatterBounds::Rect {
            center: Fp2([0.0, 0.0]),
            extents: Fp2([50.0, 30.0]),
            rotation: Fp(0.0),
        }
    }

    /// Mean distance from the bounds centre over `n` samples.
    fn mean_radius(bounds: &ScatterBounds, falloff: f32, n: usize) -> f32 {
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let total: f32 = (0..n)
            .map(|_| {
                let (x, z) = sample_bounds(bounds, &mut rng, falloff);
                (x * x + z * z).sqrt()
            })
            .sum();
        total / n as f32
    }

    /// #1776 across seeded worlds: no scatter's copy - tree, boulder or
    /// tuft - stands inside a seeded structure's footprint where the
    /// compile stands it, nor on the landing or the ground behind it, nor
    /// (#1777) across a viewpoint's view down to its landmark.
    #[test]
    fn no_scatter_grows_through_a_seeded_building_or_on_the_landing() {
        let (mut kept, mut viewpoints) = (0, 0);
        for seed in 0u64..6 {
            let record = crate::seeded_defaults::room::build::build_room_with_source(
                seed,
                "did:plc:keepoff",
                None,
            );
            let terrain = crate::terrain::FinishedHeightMap(
                crate::terrain::rebuild_heightmap_for_record(&record),
                None,
            );
            let water = super::super::water::drawn_water(&record, Some(&terrain));
            let keep_off = structure_exclusions(&record, Some(&terrain), &water);
            assert!(
                keep_off.len() >= 3,
                "seed {seed}: a landmark, a gate and the landing at least"
            );
            // The view is kept halfway down it, where the compile stands
            // the landing and the landmark.
            let landing = super::super::water::landing_on(&record, &terrain)
                .filter(|landing| landing.avoid_water);
            let landmark = record
                .placements
                .iter()
                .find_map(|placement| match placement {
                    crate::pds::Placement::Absolute {
                        generator_ref,
                        transform,
                        avoid_water_clearance,
                        ..
                    } if generator_ref == LANDMARK => {
                        Some(super::super::pad::snapped_absolute_anchor(
                            &super::super::pad::AnchorGround::new(&terrain, &water),
                            transform,
                            true,
                            avoid_water_clearance.0,
                        ))
                    }
                    _ => None,
                });
            if let (Some(landing), Some(landmark)) = (landing, landmark) {
                let [x, z] = landing.pos.0;
                let (mx, mz) = ((x + landmark.x) * 0.5, (z + landmark.z) * 0.5);
                assert!(
                    inside_any(&keep_off, mx, mz),
                    "seed {seed}: the view is not kept at ({mx}, {mz})"
                );
                viewpoints += 1;
            }
            for points in super::super::census::scatter_points(&record, &terrain)
                .into_iter()
                .flatten()
            {
                for (x, z) in points {
                    assert!(
                        !inside_any(&keep_off, x, z),
                        "seed {seed}: a copy grows at ({x}, {z}), inside a seeded footprint"
                    );
                    kept += 1;
                }
            }
        }
        assert!(kept > 1000, "only {kept} copies placed");
        assert!(viewpoints > 0, "no seeded world arrives at a viewpoint");
    }

    /// #1776: the keep-outs are the seeded structures - a snapped
    /// placement opting into Avoid Water - and the landing; an editor's own
    /// placement is none.
    #[test]
    fn only_seeded_structures_and_the_landing_are_kept_off() {
        let placed = |x: f32, avoid_water: bool| crate::pds::Placement::Absolute {
            generator_ref: "g".into(),
            transform: crate::pds::TransformData {
                translation: crate::pds::Fp3([x, 0.0, 0.0]),
                rotation: crate::pds::Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: crate::pds::Fp3([2.0, 2.0, 2.0]),
            },
            snap_to_terrain: true,
            avoid_water,
            avoid_water_clearance: crate::pds::Fp(5.0),
            seed: None,
            params: Default::default(),
        };
        let mut record = crate::pds::RoomRecord {
            placements: vec![placed(20.0, true), placed(-20.0, false)],
            default_landing: Some(crate::pds::room::DefaultLanding {
                pos: crate::pds::Fp2([0.0, 30.0]),
                y: None,
                yaw_deg: crate::pds::Fp(0.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        // The seeded terrain's own paths aside (#1779).
        for generator in record.generators.values_mut() {
            if let crate::pds::GeneratorKind::Terrain(terrain) = &mut generator.kind {
                terrain.paths = Default::default();
            }
        }
        let keep_off =
            structure_exclusions(&record, None, &crate::water::bodies::RoomWater::default());
        assert_eq!(keep_off.len(), 1 + 1 + ARRIVAL_STEPS);
        assert_eq!(keep_off[0], (20.0, 0.0, 100.0));
        assert_eq!(keep_off[1], (0.0, 30.0, LANDING_KEEP * LANDING_KEEP));
        // Facing -Z (yaw 0), the camera rests behind it, toward +Z.
        let last = keep_off[keep_off.len() - 1];
        assert!(last.0.abs() < 1e-4 && last.1 > 30.0 + 10.0, "{last:?}");
        // Each disc reaches the next.
        for pair in keep_off[1..].windows(2) {
            let gap = (pair[1].0 - pair[0].0).hypot(pair[1].1 - pair[0].1);
            assert!(gap < LANDING_KEEP, "{gap}");
        }
    }
    /// #1781: a world's signature feature keeps the scatters off its ground
    /// whether it avoids water or not - a giant tree's clearing, a sea
    /// stack's foot - where any other placement that avoids none is no ground
    /// a scatter keeps off.
    #[test]
    fn a_signature_feature_keeps_its_ground_avoiding_no_water() {
        let kept = |generator_ref: &str| {
            let mut record = crate::pds::RoomRecord {
                placements: vec![crate::pds::Placement::Absolute {
                    generator_ref: generator_ref.into(),
                    transform: crate::pds::TransformData {
                        translation: crate::pds::Fp3([40.0, 0.0, -10.0]),
                        rotation: crate::pds::Fp4([0.0, 0.0, 0.0, 1.0]),
                        scale: crate::pds::Fp3([1.0, 1.0, 1.0]),
                    },
                    snap_to_terrain: true,
                    avoid_water: false,
                    avoid_water_clearance: crate::pds::Fp(12.0),
                    seed: None,
                    params: Default::default(),
                }],
                default_landing: None,
                ..Default::default()
            };
            // The seeded terrain's own paths aside (#1779).
            for generator in record.generators.values_mut() {
                if let crate::pds::GeneratorKind::Terrain(terrain) = &mut generator.kind {
                    terrain.paths = Default::default();
                }
            }
            let keep_off =
                structure_exclusions(&record, None, &crate::water::bodies::RoomWater::default());
            inside_any(&keep_off, 51.0, -10.0)
        };
        assert!(kept(FEATURE));
        assert!(!kept("g"), "an editor's own placement");
    }

    /// #1791: a field's or an orchard's rectangle is kept off whole by its
    /// discs, which reach past its edge by no more than their cells allow,
    /// and one as big as a record allows costs no more than its cap of them.
    #[test]
    fn a_rectangle_is_kept_off_whole_and_no_further() {
        use bevy::math::Vec2;
        let middle = Vec2::new(30.0, -40.0);
        for (half, along) in [
            ([20.0, 15.0], Vec2::X),
            ([125.0, 2.0], Vec2::new(0.6, -0.8)),
            ([3.0, 3.0], Vec2::new(-0.8, -0.6)),
            ([125.0, 125.0], Vec2::new(0.0, 1.0)),
        ] {
            let mut keep = Vec::new();
            keep_off_rectangle(&mut keep, middle, along, half);
            assert!(keep.len() <= RECT_CELLS * RECT_CELLS, "{}", keep.len());
            let across = Vec2::new(-along.y, along.x);
            let at = |u: f32, v: f32| middle + along * u + across * v;
            for i in 0..=40 {
                for j in 0..=40 {
                    let p = at(
                        half[0] * (i as f32 / 20.0 - 1.0),
                        half[1] * (j as f32 / 20.0 - 1.0),
                    );
                    assert!(inside_any(&keep, p.x, p.y), "{half:?}: {p}");
                }
            }
            // The cells the discs stand in, and the most any reaches past
            // the edge: its radius less half its narrower side.
            let cell = half.map(|half| {
                let cells = ((2.0 * half / RECT_KEEP).ceil() as usize).clamp(1, RECT_CELLS);
                2.0 * half / cells as f32
            });
            let reach = 0.5 * cell[0].hypot(cell[1]) + PATH_KEEP - 0.5 * cell[0].min(cell[1]);
            let out = reach + 0.05;
            for k in 0..=40 {
                let t = k as f32 / 20.0 - 1.0;
                for p in [
                    at(half[0] * t, half[1] + out),
                    at(half[0] * t, -half[1] - out),
                    at(half[0] + out, half[1] * t),
                    at(-half[0] - out, half[1] * t),
                ] {
                    assert!(!inside_any(&keep, p.x, p.y), "{half:?}: {p} past {out}");
                }
            }
        }
    }

    /// #1791: a seeded orchard keeps the scatters off its trees and the
    /// ground between them, wherever its grid stands them; an editor's own
    /// grid keeps nothing.
    #[test]
    fn a_seeded_orchard_keeps_its_ground() {
        let orchard = crate::seeded_defaults::room::fields::Orchard {
            middle: [30.0, -40.0],
            along: [0.6, -0.8],
            slug: "lsys_apple",
        };
        let kept = |placement: crate::pds::Placement| {
            let mut record = crate::pds::RoomRecord {
                placements: vec![placement],
                default_landing: None,
                ..Default::default()
            };
            for generator in record.generators.values_mut() {
                if let crate::pds::GeneratorKind::Terrain(terrain) = &mut generator.kind {
                    terrain.paths = Default::default();
                }
            }
            structure_exclusions(&record, None, &crate::water::bodies::RoomWater::default())
        };
        let placement = orchard.placement();
        let keep = kept(placement.clone());
        let crate::pds::Placement::Grid {
            transform,
            counts: [cx, _, cz],
            gaps,
            ..
        } = &placement
        else {
            panic!("a grid");
        };
        let anchor = bevy::prelude::Transform::from(transform);
        let [gx, _, gz] = gaps.0;
        // Every tree as the compile stands it, and every point halfway
        // between it and its neighbours.
        for ix in 0..*cx {
            for iz in 0..*cz {
                for (dx, dz) in [(0.0, 0.0), (0.5, 0.0), (0.0, 0.5), (0.5, 0.5)] {
                    if dx > 0.0 && ix + 1 == *cx || dz > 0.0 && iz + 1 == *cz {
                        continue;
                    }
                    let local = bevy::math::Vec3::new(
                        -((*cx as f32 - 1.0) * gx) / 2.0 + (ix as f32 + dx) * gx,
                        0.0,
                        -((*cz as f32 - 1.0) * gz) / 2.0 + (iz as f32 + dz) * gz,
                    );
                    let at = anchor.transform_point(local);
                    assert!(inside_any(&keep, at.x, at.z), "{ix} {iz}: {at}");
                }
            }
        }
        let crate::pds::Placement::Grid {
            transform,
            counts,
            gaps,
            snap_to_terrain,
            random_yaw,
            ..
        } = placement
        else {
            panic!("a grid");
        };
        let own = kept(crate::pds::Placement::Grid {
            generator_ref: "g".into(),
            transform,
            counts,
            gaps,
            snap_to_terrain,
            random_yaw,
        });
        assert!(own.is_empty(), "an editor's own grid");
    }

    /// #1797: a seeded hamlet keeps the scatters off each of its houses,
    /// its reach round where its copy stands under the set's anchor - by
    /// its variant's lot, at its copy's size - and no further; an editor's
    /// own set keeps nothing.
    #[test]
    fn a_seeded_hamlet_keeps_its_houses_ground() {
        let lot = |frontage: f32, depth: f32| crate::pds::InstanceVariant {
            seed: Some(3),
            params: crate::pds::GrammarParams(std::collections::BTreeMap::from([
                ("Depth".to_string(), crate::pds::Fp(depth)),
                ("Frontage".to_string(), crate::pds::Fp(frontage)),
            ])),
        };
        let copies = vec![
            crate::pds::InstanceCopy::at(10.0, 0.0, 30.0),
            crate::pds::InstanceCopy {
                variant: 1,
                scale_pm: 1_500,
                ..crate::pds::InstanceCopy::at(-40.0, 20.0, 0.0)
            },
        ];
        let set = |generator_ref: &str| crate::pds::Placement::Instances {
            generator_ref: generator_ref.into(),
            transform: crate::pds::TransformData {
                translation: crate::pds::Fp3([5.0, 0.0, -5.0]),
                ..Default::default()
            },
            snap_to_terrain: true,
            solid: crate::pds::InstanceSolid::Shell,
            variants: vec![lot(8.0, 8.0), lot(12.0, 10.0)],
            copies: copies.clone(),
        };
        let kept = |placement: crate::pds::Placement| {
            let mut record = crate::pds::RoomRecord {
                placements: vec![placement],
                default_landing: None,
                ..Default::default()
            };
            for generator in record.generators.values_mut() {
                if let crate::pds::GeneratorKind::Terrain(terrain) = &mut generator.kind {
                    terrain.paths = Default::default();
                }
            }
            structure_exclusions(&record, None, &crate::water::bodies::RoomWater::default())
        };
        let keep = kept(set(HAMLET));
        // Half the diagonal of 8 by 8 + 4, and of 12 by 10 + 4 at 1.5.
        let reaches = [house_reach(8.0, 8.0), house_reach(12.0, 10.0) * 1.5];
        assert!((reaches[0] - 7.2111).abs() < 1e-3 && (reaches[1] - 13.8293).abs() < 1e-3);
        for ([x, z], reach) in [[15.0, -5.0], [-35.0, 15.0]].into_iter().zip(reaches) {
            assert!(
                inside_any(&keep, x + reach - 0.05, z),
                "inside at ({x}, {z})"
            );
            assert!(
                !inside_any(&keep, x + reach + 0.05, z),
                "past at ({x}, {z})"
            );
        }
        assert!(kept(set("kiez_street_detached")).is_empty(), "an own set");
    }

    /// #1777: a seeded viewpoint's view down to its landmark is kept clear -
    /// a landing that avoids water and looks at the landmark - and no other:
    /// one its owner turned away, or one that does not avoid water, keeps
    /// the trees across it.
    #[test]
    fn only_a_landing_looking_at_its_landmark_keeps_its_view_clear() {
        let landmark = crate::pds::Placement::Absolute {
            generator_ref: LANDMARK.into(),
            transform: crate::pds::TransformData {
                translation: crate::pds::Fp3([0.0, 0.0, 0.0]),
                rotation: crate::pds::Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: crate::pds::Fp3([1.0, 1.0, 1.0]),
            },
            snap_to_terrain: true,
            avoid_water: true,
            avoid_water_clearance: crate::pds::Fp(8.0),
            seed: None,
            params: Default::default(),
        };
        let view_kept = |yaw: f32, avoid_water: bool| {
            let record = crate::pds::RoomRecord {
                placements: vec![landmark.clone()],
                default_landing: Some(crate::pds::room::DefaultLanding {
                    pos: crate::pds::Fp2([0.0, 60.0]),
                    yaw_deg: crate::pds::Fp(yaw),
                    avoid_water,
                    ..Default::default()
                }),
                ..Default::default()
            };
            let keep_off =
                structure_exclusions(&record, None, &crate::water::bodies::RoomWater::default());
            inside_any(&keep_off, 0.0, 30.0)
        };
        // Facing -Z, from (0, 60) down to the landmark at the origin.
        assert!(view_kept(0.0, true));
        assert!(!view_kept(90.0, true), "turned away by its owner");
        assert!(!view_kept(0.0, false), "an owner's own landing");
    }

    /// #1779: no scatter grows on a world's paths - along each stretch and
    /// round its bends, a little past its edge - and a world without paths
    /// keeps nothing for them.
    #[test]
    fn no_scatter_grows_on_a_path() {
        use crate::pds::{Fp, Fp2, GeneratorKind, RoomRecord, SovereignPathsConfig};
        // A seeded world's terrain, without its structures, its landing or
        // its own paths.
        let mut record = RoomRecord::default();
        record.placements.clear();
        record.default_landing = None;
        let set_paths = |record: &mut RoomRecord, paths: &SovereignPathsConfig| {
            for generator in record.generators.values_mut() {
                if let GeneratorKind::Terrain(terrain) = &mut generator.kind {
                    terrain.paths = paths.clone();
                }
            }
        };
        let water = crate::water::bodies::RoomWater::default();
        let kept = |record: &RoomRecord, x: f32, z: f32| {
            inside_any(&structure_exclusions(record, None, &water), x, z)
        };
        set_paths(&mut record, &SovereignPathsConfig::default());
        assert!(
            !kept(&record, -15.0, 0.0),
            "a world without paths keeps nothing"
        );
        set_paths(
            &mut record,
            &SovereignPathsConfig {
                lines: vec![vec![Fp2([-30.0, 0.0]), Fp2([0.0, 0.0]), Fp2([0.0, 40.0])]],
                width: Fp(2.0),
                ..Default::default()
            },
        );
        for (x, z) in [
            (-15.0, 0.0),
            (-29.0, 0.4),
            (0.0, 0.0),
            (0.3, 25.0),
            (-1.4, 39.0),
        ] {
            assert!(kept(&record, x, z), "({x}, {z}) on the path grows a copy");
        }
        for (x, z) in [(-15.0, 4.0), (5.0, 20.0), (-10.0, 20.0)] {
            assert!(
                !kept(&record, x, z),
                "({x}, {z}) beside the path is kept off"
            );
        }
    }

    /// #1512: the render tool's preview turns a turned rect's copies as the
    /// compile does - with Random yaw off, each faces along the rect's own X
    /// - and stands each where the sampler put it.
    #[test]
    fn the_preview_turns_a_turned_rects_copies_with_it() {
        let rotation = 0.7_f32;
        let bounds = ScatterBounds::Rect {
            center: Fp2([3.0, -2.0]),
            extents: Fp2([20.0, 4.0]),
            rotation: Fp(rotation),
        };
        let mut preview =
            ScatterPreview::new(&bounds, 8, 42, &ScatterNaturalness::default(), false);
        let mut sampler = ChaCha8Rng::seed_from_u64(42);
        let (ax, az) = rect_point([0.0, 0.0], rotation, 1.0, 0.0);
        for _ in 0..8 {
            let pose = preview.next_pose();
            let (x, z) = sample_bounds(&bounds, &mut sampler, 0.0);
            assert!(
                (pose.translation - Vec3::new(x, 0.0, z)).length() < 1e-3,
                "{pose:?} does not stand at ({x}, {z})"
            );
            let along = pose.rotation * Vec3::X;
            assert!(
                (along - Vec3::new(ax, 0.0, az)).length() < 1e-5,
                "a copy faces {along}, the rect runs along ({ax}, 0, {az})"
            );
        }
    }

    // --- determinism contract ------------------------------------------

    /// The load-bearing one. `edge_falloff` warps a sample that was drawn
    /// either way, so two runs from the same seed must leave the RNG in
    /// *exactly* the same state - otherwise turning the knob on would
    /// reshuffle every instance after the first.
    #[test]
    fn edge_falloff_consumes_no_extra_draws() {
        for bounds in [disc(100.0), rect()] {
            let mut flat = ChaCha8Rng::seed_from_u64(7);
            let mut warped = ChaCha8Rng::seed_from_u64(7);
            for _ in 0..500 {
                sample_bounds(&bounds, &mut flat, 0.0);
                sample_bounds(&bounds, &mut warped, 2.5);
            }
            assert_eq!(
                flat.next_u32(),
                warped.next_u32(),
                "edge_falloff shifted the placement stream"
            );
        }
    }

    /// `clumping` is applied to an already-drawn sample, so it cannot
    /// touch the stream at all - and its cluster seeds come from their own
    /// RNG rather than the placement one.
    #[test]
    fn clumping_consumes_no_draws() {
        let bounds = disc(100.0);
        let clusters = cluster_centers(&bounds, 400, 99, 0.0);
        let mut plain = ChaCha8Rng::seed_from_u64(7);
        let mut clumped = ChaCha8Rng::seed_from_u64(7);
        for _ in 0..500 {
            sample_bounds(&bounds, &mut plain, 0.0);
            apply_clumping(sample_bounds(&bounds, &mut clumped, 0.0), &clusters, 0.8);
        }
        assert_eq!(plain.next_u32(), clumped.next_u32());
    }

    /// The jitter stream advances by a fixed group per instance no matter
    /// which knobs are on, so toggling `tilt_jitter` cannot change the
    /// next instance's scale.
    #[test]
    fn instance_jitter_takes_a_fixed_draw_group() {
        let off = ScatterNaturalness::default();
        let on = ScatterNaturalness {
            scale_jitter: Fp(0.3),
            tilt_jitter: Fp(0.2),
            ..ScatterNaturalness::default()
        };
        for naturalness in [off, on] {
            let mut used = ChaCha8Rng::seed_from_u64(3);
            let mut counted = ChaCha8Rng::seed_from_u64(3);
            instance_jitter(&mut used, &naturalness);
            for _ in 0..JITTER_DRAWS_PER_INSTANCE {
                counted.next_u32();
            }
            assert_eq!(
                used.next_u32(),
                counted.next_u32(),
                "jitter group is not {JITTER_DRAWS_PER_INSTANCE} draws wide"
            );
        }
    }

    /// Cluster seeds must be a pure function of the scatter's own seed -
    /// two peers compiling the same record have to agree on them, and a
    /// re-derivation within one peer must not drift.
    #[test]
    fn cluster_seeds_are_reproducible_and_seed_specific() {
        let bounds = disc(100.0);
        assert_eq!(
            cluster_centers(&bounds, 300, 42, 1.0),
            cluster_centers(&bounds, 300, 42, 1.0)
        );
        assert_ne!(
            cluster_centers(&bounds, 300, 42, 1.0),
            cluster_centers(&bounds, 300, 43, 1.0),
            "two scatters in one room would share a clump layout"
        );
    }

    // --- effects --------------------------------------------------------

    #[test]
    fn edge_falloff_pulls_samples_toward_the_middle() {
        let bounds = disc(100.0);
        let flat = mean_radius(&bounds, 0.0, 4000);
        let mild = mean_radius(&bounds, 1.0, 4000);
        let hard = mean_radius(&bounds, 3.0, 4000);
        // Uniform on a disc has mean radius 2R/3 ≈ 66.7.
        assert!(
            (60.0..72.0).contains(&flat),
            "flat disc sampling drifted: mean radius {flat}"
        );
        assert!(mild < flat * 0.85, "mild falloff {mild} vs flat {flat}");
        assert!(hard < mild, "hard falloff {hard} should beat mild {mild}");
    }

    /// Rect bounds thin toward all four edges rather than radially, so the
    /// check is per-axis.
    #[test]
    fn edge_falloff_thins_a_rect_toward_its_edges() {
        let bounds = rect();
        let mean_abs = |falloff: f32| {
            let mut rng = ChaCha8Rng::seed_from_u64(5);
            let total: f32 = (0..4000)
                .map(|_| sample_bounds(&bounds, &mut rng, falloff).0.abs())
                .sum();
            total / 4000.0
        };
        assert!(mean_abs(2.0) < mean_abs(0.0) * 0.75);
    }

    #[test]
    fn clumping_tightens_samples_onto_their_cluster_seeds() {
        let bounds = disc(100.0);
        let clusters = cluster_centers(&bounds, 400, 21, 0.0);
        let mean_to_seed = |clumping: f32| {
            let mut rng = ChaCha8Rng::seed_from_u64(13);
            let total: f32 = (0..3000)
                .map(|_| {
                    let p =
                        apply_clumping(sample_bounds(&bounds, &mut rng, 0.0), &clusters, clumping);
                    clusters
                        .iter()
                        .map(|c| ((p.0 - c.0).powi(2) + (p.1 - c.1).powi(2)).sqrt())
                        .fold(f32::MAX, f32::min)
                })
                .sum();
            total / 3000.0
        };
        let loose = mean_to_seed(0.0);
        let tight = mean_to_seed(0.6);
        // Contraction by `1 - clumping` should scale the mean distance to
        // the nearest seed by the same factor. Nearest-seed identity can
        // change under contraction, so allow slack rather than pinning.
        assert!(
            tight < loose * 0.5,
            "clumping 0.6 gave mean {tight} vs {loose} flat"
        );
    }

    /// Contraction toward an interior point of a convex region can never
    /// escape it - no instance may end up outside the authored stand.
    #[test]
    fn clumping_keeps_every_sample_inside_the_bounds() {
        let bounds = disc(100.0);
        let clusters = cluster_centers(&bounds, 500, 4, 1.5);
        let mut rng = ChaCha8Rng::seed_from_u64(77);
        for _ in 0..3000 {
            let (x, z) = apply_clumping(sample_bounds(&bounds, &mut rng, 1.5), &clusters, 0.9);
            assert!(
                (x * x + z * z).sqrt() <= 100.0 + 1e-3,
                "clumping pushed a sample outside the bounds: ({x}, {z})"
            );
        }
    }

    #[test]
    fn scale_jitter_is_log_symmetric_about_one() {
        let n = ScatterNaturalness {
            scale_jitter: Fp(0.18),
            ..ScatterNaturalness::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(31);
        let (mut lo, mut hi, mut log_sum) = (f32::MAX, f32::MIN, 0.0f32);
        for _ in 0..4000 {
            let s = instance_jitter(&mut rng, &n).scale;
            lo = lo.min(s);
            hi = hi.max(s);
            log_sum += s.ln();
        }
        // e^±0.18 ≈ 0.835 / 1.197 - the range the doc comment promises.
        assert!((0.83..0.85).contains(&lo), "low end {lo}");
        assert!((1.19..1.21).contains(&hi), "high end {hi}");
        assert!(
            (log_sum / 4000.0).abs() < 0.01,
            "log-mean should sit on 1.0, not {}",
            (log_sum / 4000.0).exp()
        );
    }

    /// With the knob off the decorations must be exact identities, not
    /// merely small - a 1.0000001 scale would defeat the shared-handle
    /// dedup path for no visual gain.
    #[test]
    fn jitter_knobs_off_produce_exact_identities() {
        let n = ScatterNaturalness::default();
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for _ in 0..200 {
            let j = instance_jitter(&mut rng, &n);
            assert_eq!(j.scale, 1.0);
            assert_eq!(j.tilt_angle, 0.0);
        }
    }

    /// The cutoff has to speak the same language as `terrain_slope_at`,
    /// which reports `1 - normal.y` rather than an angle or a gradient.
    #[test]
    fn slope_cutoff_converts_degrees_to_the_normal_measure() {
        let at = |deg: f32| {
            slope_cutoff(&ScatterNaturalness {
                max_slope_deg: Some(Fp(deg)),
                ..ScatterNaturalness::default()
            })
            .unwrap()
        };
        assert!(at(0.0).abs() < 1e-6, "flat ground is 0");
        assert!((at(60.0) - 0.5).abs() < 1e-5, "1 - cos 60 = 0.5");
        assert!((at(90.0) - 1.0).abs() < 1e-5, "a vertical face is 1");
        // Monotone, so a bigger angle is always a looser filter.
        assert!(at(15.0) < at(30.0) && at(30.0) < at(45.0));
        assert!(slope_cutoff(&ScatterNaturalness::default()).is_none());
    }

    /// Tilt must be a pure lean. The naive composition leaves a net yaw
    /// behind, which would spin instances in a scatter that explicitly
    /// turned `random_yaw` off - a silent, hard-to-attribute regression.
    #[test]
    fn tilt_leans_without_introducing_yaw() {
        let n = ScatterNaturalness {
            tilt_jitter: Fp(0.3),
            ..ScatterNaturalness::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(17);
        for _ in 0..200 {
            let j = instance_jitter(&mut rng, &n);
            let tf = instance_pose(Vec3::ZERO, &j, false, &n, Vec3::Y);
            // A pure lean tips the up axis by |tilt_angle| and leaves the
            // horizontal heading alone, so a forward vector projected back
            // onto the ground plane still points at -Z.
            let up = tf.rotation * Vec3::Y;
            assert!(
                (up.angle_between(Vec3::Y) - j.tilt_angle.abs()).abs() < 1e-3,
                "lean magnitude wrong: {} vs {}",
                up.angle_between(Vec3::Y),
                j.tilt_angle.abs()
            );
            // "No spin" means the rotation *axis* is horizontal - that is
            // the definition of a lean. It is deliberately not "the
            // forward vector's ground shadow is unmoved": any rigid lean
            // tips forward out of the horizontal plane, and its shadow
            // swings by a second-order `(1 - cos θ)` amount. The naive
            // `Ry(az) · Rx(θ)` composition fails this; the conjugated form
            // passes it.
            let (axis, angle) = tf.rotation.to_axis_angle();
            if angle.abs() > 1e-3 {
                assert!(
                    axis.y.abs() < 1e-3,
                    "tilt axis is not horizontal (y = {}) - that is a spin, \
                     not a lean",
                    axis.y
                );
            }
        }
    }

    /// The ground tilt (#1765) turns a copy's up toward the ground's normal
    /// as far as it is asked: 0 leaves it upright on any slope, 1 lays it
    /// on the ground (its up is the normal), and a half leans it half the
    /// slope's angle, in the slope's own direction - here a 26.6 degree
    /// slope falling to +X and +Z alike, so the turn is off both axes.
    #[test]
    fn a_copy_tilts_with_the_ground_as_far_as_asked() {
        let ground = Vec3::new(-0.35, 1.0, -0.35).normalize();
        let slope = ground.angle_between(Vec3::Y);
        let jitter = InstanceJitter {
            yaw: 0.0,
            tilt_azimuth: 0.0,
            tilt_angle: 0.0,
            scale: 1.0,
        };
        for (tilt, lean) in [(0.0, 0.0), (0.5, slope * 0.5), (1.0, slope)] {
            let n = ScatterNaturalness {
                ground_tilt: Fp(tilt),
                ..ScatterNaturalness::default()
            };
            let up = instance_pose(Vec3::ZERO, &jitter, false, &n, ground).rotation * Vec3::Y;
            assert!(
                (up.angle_between(Vec3::Y) - lean).abs() < 1e-4,
                "tilt {tilt}: leans {} for {lean}",
                up.angle_between(Vec3::Y)
            );
            // In the plane of the upright and the ground's normal: the lean
            // goes the way the ground falls, not some other way.
            assert!(
                up.cross(Vec3::Y).dot(ground).abs() < 1e-5,
                "tilt {tilt}: leans out of the slope's plane: {up}"
            );
        }
        let n = ScatterNaturalness {
            ground_tilt: Fp(1.0),
            ..ScatterNaturalness::default()
        };
        let up = instance_pose(Vec3::ZERO, &jitter, false, &n, ground).rotation * Vec3::Y;
        assert!(
            (up - ground).length() < 1e-5,
            "lying on it: {up} for {ground}"
        );
    }

    /// The ground tilt spins nothing: it is the shortest arc from the
    /// upright to the slope, its axis horizontal, so a copy keeps the
    /// heading its yaw gave it; and the random lean rides on the slope -
    /// laid on the ground, a copy leans off the ground's normal by its own
    /// jitter, not off the vertical.
    #[test]
    fn the_ground_tilt_keeps_the_heading_and_carries_the_lean() {
        let ground = Vec3::new(0.2, 1.0, -0.45).normalize();
        let lying = ScatterNaturalness {
            ground_tilt: Fp(1.0),
            ..ScatterNaturalness::default()
        };
        let upright = InstanceJitter {
            yaw: 0.0,
            tilt_azimuth: 0.0,
            tilt_angle: 0.0,
            scale: 1.0,
        };
        let tilt = instance_pose(Vec3::ZERO, &upright, false, &lying, ground).rotation;
        let (axis, angle) = tilt.to_axis_angle();
        assert!(
            angle > 0.1 && axis.y.abs() < 1e-5,
            "a lean, not a spin: {axis}"
        );
        // A yawed copy is the same yaw laid on the same slope.
        let yawed = InstanceJitter {
            yaw: 1.1,
            ..upright
        };
        let both = instance_pose(Vec3::ZERO, &yawed, true, &lying, ground).rotation;
        // Compared by what the turns do to the axes: a quaternion angle
        // goes through `acos` near 1, a few ten-thousandths in f32.
        let yaw_then_tilt = tilt * Quat::from_rotation_y(1.1);
        for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
            assert!(
                (both * axis - yaw_then_tilt * axis).length() < 1e-5,
                "{axis}"
            );
        }

        let leaning = ScatterNaturalness {
            ground_tilt: Fp(1.0),
            tilt_jitter: Fp(0.3),
            ..ScatterNaturalness::default()
        };
        let mut rng = ChaCha8Rng::seed_from_u64(29);
        for _ in 0..100 {
            let j = instance_jitter(&mut rng, &leaning);
            let up = instance_pose(Vec3::ZERO, &j, false, &leaning, ground).rotation * Vec3::Y;
            assert!(
                (up.angle_between(ground) - j.tilt_angle.abs()).abs() < 1e-3,
                "the lean is off the slope: {} vs {}",
                up.angle_between(ground),
                j.tilt_angle.abs()
            );
        }
    }

    /// The microbiome bands (#913) must be purely subtractive, exactly like
    /// the slope cutoff: they reject samples without consuming a draw, so
    /// narrowing a band removes instances and leaves every survivor where
    /// it was. Drawing a rejection roll instead would reshuffle the stand.
    #[test]
    fn microbiome_bands_consume_no_draws() {
        let bounds = disc(80.0);
        let banded = ScatterNaturalness {
            above_water_band: Some(Fp2([0.0, 4.0])),
            altitude_band: Some(Fp2([0.0, 20.0])),
            ..ScatterNaturalness::default()
        };
        let mut plain = ChaCha8Rng::seed_from_u64(21);
        let mut with_bands = ChaCha8Rng::seed_from_u64(21);
        for _ in 0..400 {
            sample_bounds(&bounds, &mut plain, 0.0);
            sample_bounds(&bounds, &mut with_bands, banded.edge_falloff.0);
        }
        assert_eq!(plain.next_u32(), with_bands.next_u32());
    }

    /// A band with no terrain to resolve against must place nothing rather
    /// than silently ignoring the constraint - the same stance the biome
    /// allow-list and the slope cutoff take.
    #[test]
    fn bands_without_a_heightmap_fail_closed() {
        let bounds = disc(50.0);
        let filters = SampleFilters {
            biome_filter: &crate::pds::BiomeFilter::default(),
            terrain_cfg: None,
            water: &None,
            urban_exclusions: &[],
            slope_cutoff: None,
            tree_clearance: None,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(2);
        for naturalness in [
            ScatterNaturalness {
                above_water_band: Some(Fp2([0.0, 4.0])),
                ..ScatterNaturalness::default()
            },
            ScatterNaturalness {
                altitude_band: Some(Fp2([0.0, 20.0])),
                ..ScatterNaturalness::default()
            },
        ] {
            assert!(
                try_sample(&bounds, &naturalness, &[], &mut rng, None, &filters).is_none(),
                "a band with no heightmap must reject"
            );
        }
    }

    /// A scatter that asks for a slope limit must place nothing when there
    /// is no heightmap to ask, rather than silently ignoring the limit.
    #[test]
    fn slope_limit_without_a_heightmap_fails_closed() {
        let bounds = disc(50.0);
        let naturalness = ScatterNaturalness {
            max_slope_deg: Some(Fp(30.0)),
            ..ScatterNaturalness::default()
        };
        let filters = SampleFilters {
            biome_filter: &crate::pds::BiomeFilter::default(),
            terrain_cfg: None,
            water: &None,
            urban_exclusions: &[],
            slope_cutoff: slope_cutoff(&naturalness),
            tree_clearance: None,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(1);
        for _ in 0..100 {
            assert!(try_sample(&bounds, &naturalness, &[], &mut rng, None, &filters).is_none());
        }
        // …while the same scatter without the limit places freely.
        let open = ScatterNaturalness::default();
        let filters = SampleFilters {
            slope_cutoff: None,
            tree_clearance: None,
            ..filters
        };
        assert!(try_sample(&bounds, &open, &[], &mut rng, None, &filters).is_some());
    }

    /// #1168: level ground grows grass, not rock's plants.
    ///
    /// The sequence that produced this: upstream's `smooth_range` was a tent,
    /// so a `SovereignSplatRule` scored zero *at* its own endpoints. Three of
    /// the four seeded rules name `slope_min: 0.0`, and the fourth (rock)
    /// names `0.25`, whose tent is also zero at 0.0 - so on an exactly level
    /// texel all four weights were zero, the total fell under `f32::EPSILON`,
    /// and both the splat map and this argmax fell through to the rock
    /// channel. Every scatter with a biome allow-list then read Rock on a lawn
    /// and put rock's plants there, because `dominant_biome` deliberately
    /// shares the upstream weight formula rather than re-implementing it.
    ///
    /// symbios-ground 0.4 makes a range a plateau, and the defaults are
    /// retuned for it (see `SovereignMaterialConfig::default`).
    #[test]
    fn level_ground_is_grass_and_never_rock() {
        let cfg = crate::pds::SovereignTerrainConfig::default();
        let hs = cfg.height_scale.0;

        assert_eq!(
            dominant_biome(&cfg, 0.0, 0.0),
            0,
            "dead-level ground at the bottom of the range should be grass"
        );
        // Rock is the steep-face layer. On a dead-flat texel it must never be
        // the answer at any altitude - below the snow line that is grass or
        // dirt, above it snow.
        for step in 0..=100 {
            let h = step as f32 / 100.0;
            let biome = dominant_biome(&cfg, h * hs, 0.0);
            assert_ne!(
                biome, 2,
                "level ground at normalised height {h} came out as rock"
            );
        }
    }

    /// #1168: no `(height, slope)` pair leaves every rule at zero.
    ///
    /// The rock fallback is what a caller gets when nothing matches, and it is
    /// indistinguishable from a deliberate rock texel - which is how the tent
    /// bug hid for so long. Sweeping the whole domain is the only way to see
    /// a hole: fixing the level-ground case opened a second one higher up
    /// while dirt still stopped at height 0.65, because rock owns every height
    /// but only steep slopes, so a *flat* texel at 0.8 matched nothing.
    #[test]
    fn every_height_and_slope_pair_matches_some_rule() {
        let cfg = crate::pds::SovereignTerrainConfig::default();
        for hs in 0..=50 {
            for ss in 0..=50 {
                let (h, slope) = (hs as f32 / 50.0, ss as f32 / 50.0);
                let total: f32 = (0..4)
                    .map(|i| convert_rule(&cfg.material.rules[i]).weight(h, slope))
                    .sum();
                assert!(
                    total > f32::EPSILON,
                    "height {h} / slope {slope} matches no rule, so it takes the \
                     rock fallback whatever the terrain actually looks like"
                );
            }
        }
    }

    /// #1168: the seeded rules read as a landscape, not just as non-zero.
    ///
    /// Three claims, each of which the pre-0.4 defaults got wrong: a steep
    /// face is rock at every altitude (it was rock only by fallback); the
    /// summit is snow when it is flat enough to hold and rock when it is not
    /// (the old snow rule's slope range `(0.0, 1.0)` was a tent peaking at
    /// slope 0.5, so snow was *strongest* on 45° faces); and the middle
    /// altitudes on gentle ground are dirt.
    #[test]
    fn the_seeded_rules_paint_a_landscape() {
        let cfg = crate::pds::SovereignTerrainConfig::default();
        let hs = cfg.height_scale.0;
        let at = |h: f32, slope: f32| dominant_biome(&cfg, h * hs, slope);

        for h in [0.0f32, 0.25, 0.5, 0.75, 1.0] {
            assert_eq!(at(h, 0.9), 2, "a near-vertical face at height {h} is rock");
        }
        assert_eq!(at(0.97, 0.05), 3, "a flat summit holds snow");
        assert_eq!(at(0.97, 0.8), 2, "a summit cliff does not");
        assert_eq!(at(0.6, 0.05), 1, "gentle mid-altitude ground is dirt");
        assert_eq!(at(0.1, 0.05), 0, "gentle low ground is grass");
    }

    /// #1586: on ground built from real Berlin a biome filter reads the
    /// land use, so a seeded stand of trees (grass and dirt, above water)
    /// takes the park and none of the built city beside it - and its
    /// treeline, a fraction of a procedural world's relief, does not strip
    /// it from ground 40 m up.
    #[test]
    fn a_stand_on_berlin_ground_takes_the_park_and_not_the_city() {
        use geodata::berlin::LandUse;
        // 129 x 129 cells 1 m apart at 40 m: a park west of x = 0, housing
        // east of it.
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        hm.data_mut().fill(40.0);
        let cover = (0..129 * 129)
            .map(|i| {
                Some(if i % 129 < 64 {
                    LandUse::Park
                } else {
                    LandUse::Housing
                })
            })
            .collect();
        let ground = crate::terrain::geo::GeoGround::from_cover(129, 1.0, cover, Vec::new());
        let berlin = crate::terrain::FinishedHeightMap(hm, Some(ground));
        let stand = crate::pds::BiomeFilter {
            biomes: vec![0, 1],
            water: crate::pds::WaterRelation::Above,
        };
        let treeline = ScatterNaturalness {
            altitude_band: Some(Fp2([-10_000.0, 26.0])),
            ..ScatterNaturalness::default()
        };
        let cfg = SovereignTerrainConfig::default();
        let filters = SampleFilters {
            biome_filter: &stand,
            terrain_cfg: Some(&cfg),
            water: &Some(30.5),
            urban_exclusions: &[],
            slope_cutoff: None,
            tree_clearance: None,
        };
        let mut rng = ChaCha8Rng::seed_from_u64(7);
        let placed: Vec<(f32, f32, f32)> = (0..400)
            .filter_map(|_| {
                try_sample(
                    &disc(60.0),
                    &treeline,
                    &[],
                    &mut rng,
                    Some(&berlin),
                    &filters,
                )
            })
            .collect();
        assert!(placed.len() > 100, "the park takes trees: {}", placed.len());
        assert!(
            placed.iter().all(|&(x, _, _)| x < 0.5),
            "no tree in the housing east of x = 0"
        );
    }

    /// #1588: Berlin's own trees stand in its parks, and a seeded stand
    /// keeps a crown's width off each of them - a stand of trees only: the
    /// ground cover under them grows on.
    #[test]
    fn a_stand_on_berlin_ground_keeps_clear_of_berlins_own_trees() {
        use crate::pds::{Generator, GeneratorKind};
        use crate::terrain::geo::street_level::{CoreTree, STAND_CLEAR_M, StreetLevel};
        use geodata::berlin::LandUse;
        let mut hm = bevy_symbios_ground::HeightMap::new(129, 129, 1.0);
        hm.data_mut().fill(40.0);
        // A park, a row of lindens down it.
        let trees: Vec<CoreTree> = (0..13)
            .map(|i| CoreTree {
                id: format!("linden-{i}").into(),
                x: -20.0,
                z: -60.0 + 10.0 * i as f32,
                genus: Some("Tilia".to_owned()),
                height: Some(15.0),
                crown: Some(8.0),
                girth: Some(120.0),
            })
            .collect();
        let level = StreetLevel::new(Vec::new(), trees.clone(), Vec::new());
        let ground = crate::terrain::geo::GeoGround::from_cover(
            129,
            1.0,
            vec![Some(LandUse::Park); 129 * 129],
            Vec::new(),
        )
        .with_street_level(level);
        let berlin = crate::terrain::FinishedHeightMap(hm, Some(ground));
        let oak = crate::catalogue::by_slug("lsys_oak").expect("registered");
        let mut generators = std::collections::HashMap::new();
        generators.insert("tree_scatter_0".to_owned(), oak.build("did:plc:stand"));
        generators.insert("cover".to_owned(), Generator::default_cuboid());
        assert!(matches!(
            generators["tree_scatter_0"].kind,
            GeneratorKind::LSystem { .. }
        ));
        let record = crate::pds::RoomRecord {
            lex_type: "network.symbios.room".to_owned(),
            environment: Default::default(),
            generators,
            placements: Vec::new(),
            traits: std::collections::HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            geo_source: None,
            identity: None,
            opaque_refs: Default::default(),
        };
        assert!(tree_clearance(&record, "cover", Some(&berlin)).is_none());
        assert!(tree_clearance(&record, "tree_scatter_0", None).is_none());
        let clearance =
            tree_clearance(&record, "tree_scatter_0", Some(&berlin)).expect("a stand keeps clear");
        let anywhere = crate::pds::BiomeFilter::default();
        let filters = SampleFilters {
            biome_filter: &anywhere,
            terrain_cfg: None,
            water: &None,
            urban_exclusions: &[],
            slope_cutoff: None,
            tree_clearance: Some(clearance),
        };
        let mut rng = ChaCha8Rng::seed_from_u64(11);
        let placed: Vec<(f32, f32, f32)> = (0..400)
            .filter_map(|_| {
                try_sample(
                    &disc(60.0),
                    &ScatterNaturalness::default(),
                    &[],
                    &mut rng,
                    Some(&berlin),
                    &filters,
                )
            })
            .collect();
        assert!(
            (200..400).contains(&placed.len()),
            "the park keeps most of its stand: {}",
            placed.len()
        );
        for &(x, _, z) in &placed {
            assert!(
                trees
                    .iter()
                    .all(|t| (t.x - x).hypot(t.z - z) >= STAND_CLEAR_M),
                "a seeded tree at ({x}, {z}) under a linden"
            );
        }
    }
}
