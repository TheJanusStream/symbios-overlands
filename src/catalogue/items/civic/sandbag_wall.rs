//! Sandbag wall — a staggered, stacked-bag emplacement. An
//! escalation-Conflict scatter prop: improvised fortification reads the same
//! across every setting.
//!
//! Bags are superellipsoids, not boxes. A filled sandbag is a *pillow*: it
//! bulges between the courses above and below it, and its edges are round
//! because there is nothing rigid inside to hold a corner. Cuboids — even
//! tapered ones — give a stack of hard-edged slabs that reads as crates or
//! roof tiles, which is exactly what this prop used to look like.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_y, quat_z, solid, sphere,
    superellipsoid, torus, with_cut,
};
use crate::catalogue::items::util::{tile, tiles_per_metre};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};

use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignFabricConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{SANDBAG, WOOD, wood};

pub struct SandbagWall;

impl CatalogueEntry for SandbagWall {
    fn slug(&self) -> &'static str {
        "sandbag_wall"
    }
    fn name(&self) -> &'static str {
        "Sandbag Wall"
    }
    fn description(&self) -> &'static str {
        "Staggered courses of stacked sandbags."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Conflict)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Burlap: the shared [`cloth`](super::cloth) weave is scaled for banners
/// and laundry, where one panel spans a metre or more. On a half-metre bag
/// that same scale reads as basket wicker, so hessian gets its own tighter
/// thread count and a UV scale that puts several weave repeats across each
/// face.
fn burlap(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.98),
        metallic: Fp(0.0),
        uv_scale: tiles_per_metre(tile::FABRIC_THREAD * 30.0),
        texture: SovereignTextureConfig::Fabric(SovereignFabricConfig {
            color_warp: Fp3(color),
            color_weft: Fp3([color[0] * 0.78, color[1] * 0.78, color[2] * 0.78]),
            thread_count: Fp64(30.0),
            fuzz: Fp64(0.42),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Painted olive steel — matte enough to hold a diffuse shade instead of
/// mirroring the sky into black. See the helmet's placement comment.
fn helmet_steel() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.36, 0.39, 0.30]),
        roughness: Fp(0.72),
        metallic: Fp(0.25),
        ..Default::default()
    }
}

/// Per-bag placement wobble, cycled by bag index: `(dx, dy, dz, yaw, roll)`.
///
/// Hand-picked rather than hashed from the index. A hash would want `sin` /
/// `fract` over the index, and neither is bit-identical across platforms —
/// which would make this generator's serialized bytes, and so its
/// content-addressed identity, host-dependent. Seven entries against courses
/// of five, four, three and two also means the pattern never lines up
/// vertically.
const WOBBLE: [(f32, f32, f32, f32, f32); 7] = [
    (0.010, 0.000, 0.018, 0.10, 0.03),
    (-0.014, 0.006, -0.022, -0.07, -0.05),
    (0.004, -0.004, 0.028, 0.14, 0.02),
    (-0.008, 0.003, -0.014, -0.12, -0.02),
    (0.016, -0.002, 0.008, 0.05, 0.06),
    (-0.004, 0.005, -0.026, -0.16, -0.04),
    (0.012, 0.001, 0.020, 0.08, -0.03),
];

/// Half-extents of an unsquashed bag: 0.52 long × 0.21 tall × 0.34 deep.
const BAG: [f32; 3] = [0.26, 0.105, 0.17];

/// How much each course deforms under the stack above it. Bags near the
/// bottom carry the most weight, so they spread wider and flatter; the top
/// course sits plump and unloaded.
const SQUASH: [[f32; 3]; 4] = [
    [1.04, 0.88, 1.03],
    [1.01, 0.94, 1.00],
    [0.98, 0.99, 0.97],
    [0.95, 1.02, 0.95],
];

fn build_tree() -> Generator {
    // Three close sandbag tones so the stack reads as individual filled
    // bags rather than one extruded mass.
    let tone = |i: usize| match i % 3 {
        0 => burlap(SANDBAG),
        1 => burlap([0.50, 0.46, 0.33]),
        _ => burlap([0.71, 0.63, 0.47]),
    };

    // Four staggered courses in running bond, narrowing toward the top with
    // a firing gap left in the centre of the top course.
    let courses: [&[f32]; 4] = [
        &[-1.0, -0.5, 0.0, 0.5, 1.0],
        &[-0.75, -0.25, 0.25, 0.75],
        &[-0.5, 0.0, 0.5],
        &[-0.55, 0.55],
    ];

    // Course centre heights, accumulated so each course beds *into* the one
    // below rather than resting exactly on top of it.
    let bag_h = |row: usize| BAG[1] * SQUASH[row][1];
    let mut course_y = [0.0f32; 4];
    course_y[0] = bag_h(0);
    for row in 1..4 {
        course_y[row] = course_y[row - 1] + (bag_h(row - 1) + bag_h(row)) * 0.86;
    }

    let mut prims = Vec::new();
    let mut i = 0usize;
    for (row, xs) in courses.iter().enumerate() {
        let s = SQUASH[row];
        let half = [BAG[0] * s[0], BAG[1] * s[1], BAG[2] * s[2]];
        for &x in *xs {
            let (dx, dy, dz, yaw, roll) = WOBBLE[i % WOBBLE.len()];
            prims.push(prim(
                solid(superellipsoid(
                    half,
                    // Flatter top and bottom than flanks: a filled bag
                    // slumps onto its bearing faces but stays round at the
                    // sides, where nothing is pressing on it.
                    0.42,
                    0.52,
                    tone(i),
                )),
                [x + dx, course_y[row] + dy, dz],
                quat_mul(quat_y(yaw), quat_z(roll)),
            ));
            i += 1;
        }
    }

    // A cord cinching the neck of one top bag, wrapping its short axis.
    prims.push(prim(
        torus(0.014, 0.125, wood([0.34, 0.28, 0.17])),
        [-0.72, course_y[3], 0.0],
        quat_z(std::f32::consts::FRAC_PI_2),
    ));

    // A wooden ammo crate set beside the wall.
    prims.push(prim(
        solid(cuboid_tapered([0.46, 0.3, 0.32], 0.0, wood(WOOD))),
        [1.25, 0.15, 0.18],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.48, 0.05, 0.34],
            0.0,
            wood([0.3, 0.22, 0.13]),
        )),
        [1.25, 0.32, 0.18],
        id_quat(),
    ));

    // A steel helmet resting on the third course, in the firing gap.
    // Deliberately NOT the `bronze` kit: at 0.9 metallic a dark base colour
    // has almost no diffuse term and nothing but flat sky to reflect, so the
    // dome rendered as a black hole punched through the wall.
    let helmet_y = course_y[2] + bag_h(2) * 0.9;
    prims.push(prim(
        solid(with_cut(
            sphere(0.16, 6, helmet_steel()),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, helmet_y, 0.06],
        id_quat(),
    ));
    prims.push(prim(
        cylinder_tapered(0.21, 0.03, 12, 0.0, helmet_steel()),
        [0.0, helmet_y - 0.02, 0.06],
        id_quat(),
    ));

    super::assemble(prims)
}
