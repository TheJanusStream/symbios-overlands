//! The seeded airship family: one builder per craft type, over one hull
//! profile (#1657, epic #1656).
//!
//! The last of the vehicle families to be drawn the way the boats, the
//! skiffs and the aircraft are (#1363, #1364, #1614, #1617): a craft TYPE is
//! a builder, every line and every mount is read off ONE profile - here the
//! [`HullPlan`], the hull a lighter-than-air craft is built round - and the
//! craft is authored at the size it is drawn at, with no parts, no outfit
//! and no root scale. [`craft`] is the only match over [`AirshipType`], so a
//! new type is a compile error here until it is built.
//!
//! Until #1657 the airship was the one family still assembled from a part
//! catalogue - five envelopes, three gondolas and three engine pods drawn at
//! random with no link to the avatar's theme - and the owner asked for a
//! complete overhaul: "compared to all the other vehicles they look clumsy
//! by now". ONE type is built so far: the rigid Zeppelin the owner agreed on
//! the phase-1 renders (#1656), the family's hero and universal floor. The
//! fan-out (#1658-#1663) adds a blimp, a steam dirigible, a sky galleon, a
//! hot-air balloon, a modern hybrid and an organic floater, each on a
//! profile of her own.
//!
//! # Where an airship sits
//!
//! In a box that holds everything she draws - her airscrews' discs swept
//! whole - nose to tail, keel to crown and side to side, MEASURED off the
//! drawn tree ([`swept_bounds`]) as the autogyro's and the coaxial's are
//! (#1624, #1628), so a later type's box is right without a formula of its
//! own. She is drawn centred in it fore and aft, and the travel pose drops
//! her by her box's middle, so the box is centred on her body's origin - the
//! one place a box can reach, since avian centres a collider on its body -
//! and a landed ship rests on her lowest point rather than on air under it
//! (#1612).
//!
//! # What she does at rest
//!
//! What every airship always has (#1656 decision 5): the Airship gait's
//! heave and nose wander, and the lean into her motion; her seeded gait
//! passes through untouched (`super::seeded_gait`).

mod plan;
mod zeppelin;

pub(crate) use plan::{HullPlan, HullProfile, flank_bands};

use bevy::math::{Affine3A, Quat, Vec3};

use crate::catalogue::items::measure::Bounds;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::texture::SovereignMaterialSettings;
use crate::pds::types::Fp3;
use crate::seeded_defaults::{AirshipBlueprint, AirshipType, ParticleAura};
use crate::world_builder::primitive_outline;

/// The airship family's colours, which live in the fleet's one livery home
/// (#1365) rather than beside its geometry.
pub(crate) use crate::pds::avatar::livery::{AirshipColours, airship_colours};

use super::Propulsion;
use super::assemble::apply_travel_pose;
use super::common::{id_quat, prim, spine, with_cut};

/// Smallest dimension anything on an airship is built at (m): a hair over
/// the sanitiser's own 0.01 m floor, as every family's is, so a part the
/// sanitiser would change is never drawn (#1359 rule 8).
pub(crate) const MIN_DIM: f32 = 0.011;

/// Floor a dimension at [`MIN_DIM`].
fn dim(v: f32) -> f32 {
    v.max(MIN_DIM)
}

/// A FLATTENED sweep: a Spine through `points` (drawn frame), squashed to
/// `s` across `axis` (0 is x, 1 is y) by its node's scale, its path
/// pre-divided by the scale so the drawn centreline lands where it is asked
/// for (a node scale moves the path as well as the profile - the sloop's
/// trap), optionally trimmed by `(path_cut, profile_cut)`.
///
/// Never rotate one: a non-uniform scale under a rotation is not a
/// similarity, which the connectivity guard asserts. Lay it out where it
/// stands instead, as the tail's four fins are.
fn flat_sweep(
    points: &[([f32; 3], f32)],
    res: u32,
    samples: u32,
    axis: usize,
    s: f32,
    material: &SovereignMaterialSettings,
    cut: Option<([f32; 2], [f32; 2])>,
) -> Generator {
    let mut scale = [1.0; 3];
    scale[axis] = s;
    let path: Vec<([f32; 3], f32)> = points
        .iter()
        .map(|&(p, r)| (std::array::from_fn(|k| p[k] / scale[k]), dim(r)))
        .collect();
    let mut kind = spine(&path, res, material.clone());
    if let GeneratorKind::Spine {
        samples_per_segment,
        ..
    } = &mut kind
    {
        *samples_per_segment = samples;
    }
    if let Some((path_cut, profile_cut)) = cut {
        kind = with_cut(kind, path_cut, profile_cut, 0.0);
    }
    let mut node = prim(kind, [0.0; 3], id_quat());
    node.transform.scale = Fp3(scale);
    node
}

/// How an airship type flies: the numbers `super::airship_locomotion` scales
/// the Helicopter preset by - the boats' and skiffs' feel, for a ship of the
/// air.
///
/// Build-time only, like theirs: what it writes is `HelicopterParams`, which
/// the record has always carried, so it costs no lexicon change, no
/// sanitiser range and no editor slice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct AirshipFeel {
    /// Mass over the family's reference, before the size re-basing. It
    /// cancels out of everything she does under her own power - every force
    /// and torque here is per kilogram - and sets how hard she shoves.
    pub(super) mass_factor: f32,
    /// Drive acceleration (m/s^2); `cyclic_force` is this times the mass,
    /// and her top speed under it this over `linear_damping`.
    pub(super) drive_accel: f32,
    /// Yaw acceleration per kilogram on the preset's default body, scaled by
    /// her own box's inertia (`rotor_torques`), so she turns as a default
    /// body with this number would, whatever her size.
    pub(super) yaw_accel: f32,
    pub(super) linear_damping: f32,
    pub(super) angular_damping: f32,
}

/// One buildable kind of airship.
pub(super) trait AirshipCraft {
    /// This type's hull at the seeded blueprint's proportions.
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan;

    /// Draw her, nose `+Z`, in true metres, `y = 0` on her hull's axis and
    /// `z = 0` at its middle. The caller centres her in her box and owns the
    /// root pose.
    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator;

    /// How she flies.
    fn feel(&self) -> AirshipFeel;

    /// Where a seeded particle aura issues from, read off her hull (her
    /// own frame, as [`Self::build`] draws it).
    fn fx_mount(&self, aura: ParticleAura, plan: &HullPlan) -> [f32; 3];

    /// How she is driven, and so what she sounds like. Required, with no
    /// default - the boats' rule (#1383).
    fn propulsion(&self) -> Propulsion;
}

/// The builder for an airship type - the one match over [`AirshipType`]. A
/// match over a non-empty enum needs an arm per variant, so ADDING A TYPE IS
/// A COMPILE ERROR HERE until it is built.
fn craft(t: AirshipType) -> &'static dyn AirshipCraft {
    match t {
        AirshipType::Zeppelin => &zeppelin::Zeppelin,
    }
}

/// The builder a seed draws with - its own type's, always.
fn craft_for(seed: u64) -> &'static dyn AirshipCraft {
    craft(AirshipType::for_seed(seed))
}

/// Every built type's feel, by name - the per-type feel guard's table half.
#[cfg(test)]
pub(super) fn every_feel() -> Vec<(&'static str, AirshipFeel)> {
    AirshipType::ALL
        .into_iter()
        .map(|t| (t.label(), craft(t).feel()))
        .collect()
}

// ---------------------------------------------------------------------------
// Where the ship sits
// ---------------------------------------------------------------------------

/// How many points a turning part's every vertex is swept through, round
/// its hub's axis: a 32-gon, whose bounding box falls short of the circle's
/// by at most `1 - cos(pi / 32)` of its radius - under 0.5 %.
const SWEEP_STEPS: usize = 32;

/// The bounds of `root` (her own frame): the mesher's own vertices
/// ([`primitive_outline`], which reads a sweep's without building its
/// mesh) through every part's transform - and what a turning hub carries
/// SWEPT round its axis, so her airscrews' whole discs are in it, not the
/// blades where they happen to stand.
///
/// Unlike an aircraft's box, which leaves its blades out (owner decision 3
/// of #1611: a rotor's disc is metres across), an airship's holds them: her
/// airscrews are small and hang beside and under her, and a landed ship
/// rests on her lowest point (#1612) - an ornate Zeppelin's keel engine
/// turns under her keel.
fn swept_bounds(root: &Generator) -> Bounds {
    /// What a primitive draws, in its own frame: the mesher's own vertices -
    /// but for an untortured, uncut superellipsoid, the eight corners of its
    /// half-extents. It lies inside them and touches every face, so they
    /// bound it exactly along its axes, where the full mesh the outline falls
    /// back to for it - tangents and all - cost two thirds of a measurement
    /// for the Zeppelin's eight gas-valve hoods.
    fn outline(kind: &GeneratorKind) -> Vec<Vec3> {
        if let GeneratorKind::Superellipsoid {
            half_extents,
            common,
            ..
        } = kind
            && common.torture == crate::pds::TortureParams::default()
        {
            let h = Vec3::from(half_extents.0);
            return (0..8)
                .map(|i| {
                    let pick = |bit: u32, v: f32| if i & bit == 0 { -v } else { v };
                    Vec3::new(pick(1, h.x), pick(2, h.y), pick(4, h.z))
                })
                .collect();
        }
        primitive_outline(kind)
    }
    /// Everything `part` draws - its own primitive and every child's - into
    /// `out` through `parent`. A spin turns the node about its own axis
    /// between its rotation and its scale, as the runtime poses it
    /// (`rotation * spin`, then the scale), so what it draws is gathered in
    /// its scaled frame and each point swept round that axis through its
    /// origin before the node is placed.
    fn points_of(part: &Generator, parent: Affine3A, out: &mut Vec<Vec3>) {
        let t = &part.transform;
        let scaled = Affine3A::from_scale(Vec3::from(t.scale.0));
        let placed = parent
            * Affine3A::from_rotation_translation(
                Quat::from_array(t.rotation.0).normalize(),
                Vec3::from(t.translation.0),
            );
        let mut drawn: Vec<Vec3> = outline(&part.kind)
            .into_iter()
            .map(|p| scaled.transform_point3(p))
            .collect();
        for child in &part.children {
            points_of(child, scaled, &mut drawn);
        }
        let axis = part
            .spin
            .as_ref()
            .map_or(Vec3::ZERO, |s| Vec3::from(s.axis.0).normalize_or_zero());
        if axis == Vec3::ZERO {
            out.extend(drawn.into_iter().map(|p| placed.transform_point3(p)));
            return;
        }
        let (u, v) = axis.any_orthonormal_pair();
        for p in drawn {
            let along = axis * p.dot(axis);
            let r = (p - along).length();
            for k in 0..SWEEP_STEPS {
                let a = k as f32 * std::f32::consts::TAU / SWEEP_STEPS as f32;
                out.push(placed.transform_point3(along + (u * a.cos() + v * a.sin()) * r));
            }
        }
    }
    let mut points = Vec::new();
    points_of(root, Affine3A::IDENTITY, &mut points);
    let first = *points.first().expect("she draws something");
    points.into_iter().fold(
        Bounds {
            min: first,
            max: first,
        },
        |b, p| Bounds {
            min: b.min.min(p),
            max: b.max.max(p),
        },
    )
}

/// The seeded ship for `seed` as her type draws her, her own frame - and
/// what she is drawn from, or `None` for a seed that is not an airship.
fn drawn_for(
    seed: u64,
    livery: Option<usize>,
) -> Option<(&'static dyn AirshipCraft, HullPlan, Generator)> {
    let bp = crate::seeded_defaults::VehicleBlueprint::from_seed(seed)
        .and_then(|b| b.airship().copied())?;
    let craft = craft_for(seed);
    let plan = craft.plan(&bp);
    let mut ctx = PartCtx::for_seed(seed);
    ctx.livery = livery;
    let root = craft.build(&ctx, &plan);
    Some((craft, plan, root))
}

/// Half-extents (m) of the chassis collider for a ship drawn as `root`, and
/// the middle of what she draws (her own frame): everything, her airscrews'
/// discs swept whole ([`swept_bounds`]).
fn chassis_box(root: &Generator) -> ([f32; 3], Vec3) {
    let b = swept_bounds(root);
    let half = (b.max - b.min) * 0.5;
    (half.to_array(), (b.max + b.min) * 0.5)
}

/// Assemble the seeded airship for `seed`, posed for travel: centred fore
/// and aft in her box, and dropped by its middle.
pub(super) fn build(seed: u64, livery: Option<usize>) -> Generator {
    let (_, _, mut root) =
        drawn_for(seed, livery).expect("an airship seed carries an airship blueprint");
    let (_, mid) = chassis_box(&root);
    for kid in &mut root.children {
        kid.transform.translation.0[2] -= mid.z;
    }
    apply_travel_pose(&mut root, mid.y);
    root
}

/// How the seeded airship for `seed` flies, the box she flies in and her
/// hull (m) - what the locomotion scales her mass by: its length and its
/// greatest radius.
pub(super) fn feel_and_box(seed: u64) -> (AirshipFeel, [f32; 3], (f32, f32)) {
    // Defensive: an airship seed always has a blueprint. Falling back to the
    // nominal ship's own numbers keeps a locomotion query total, as the
    // helicopters' does.
    let (craft, plan, root) = drawn_for(seed, None).unwrap_or_else(|| {
        let craft = craft(AirshipType::UNIVERSAL);
        let plan = craft.plan(&AirshipBlueprint::NOMINAL);
        let root = craft.build(&PartCtx::for_seed(seed), &plan);
        (craft, plan, root)
    });
    let (half, _) = chassis_box(&root);
    (craft.feel(), half, (plan.length(), plan.girth()))
}

/// How the airship `seed` draws is driven - any seed answers, as the boats'
/// and skiffs' do: the voice asks before it knows the family.
pub(super) fn propulsion(seed: u64) -> Propulsion {
    craft_for(seed).propulsion()
}

/// Where a seeded airship's particle aura issues from (root-local, before
/// the travel pose's yaw and drop) - read off her own hull by the type that
/// drew her, and moved as [`build`] centres her fore and aft.
pub(super) fn fx_mount(seed: u64, aura: ParticleAura) -> Option<[f32; 3]> {
    let (craft, plan, root) = drawn_for(seed, None)?;
    let (_, mid) = chassis_box(&root);
    let [x, y, z] = craft.fx_mount(aura, &plan);
    Some([x, y, z - mid.z])
}

#[cfg(test)]
mod tests;
