//! Garden bed — a low stone-kerbed bed of mixed, naturalistic planting. An
//! escalation-Calm scatter prop: cultivated ground signals a settlement
//! tended rather than fought over, in any setting.
//!
//! Where the Rich [`planter`](super::planter) is formal — one clipped
//! standard and one trailing edge — this bed is mixed and loose (#972): a
//! shrub off-centre at the back, two coneflowers in two colours, and two
//! ferns low at the front, each a real L-system species nested in the prop
//! rather than a sphere. The shrub and ferns are the catalogue's own
//! [`bush`](crate::catalogue::items::plants::lsys_bush) and
//! [`fern`](crate::catalogue::items::plants::lsys_fern), each at a uniform
//! scale (a nested plant's cards scale with its node, so one number
//! instances the whole species) and at a YOUNGER age: in these grammars the
//! iteration count is the plant's age, so a bed-sized specimen is grown
//! rather than shrunk — shorter fern fronds that stop arching into the kerb,
//! and a bush with half the leaves. The coneflower is new, and its second
//! copy wears the species' gold re-skin and its own seed, so the two are
//! different individuals of one grammar. Every copy of the bed in a room
//! shares one derivation per plant: a seeded room registers one generator
//! per prop slug.
//!
//! **The soil is the planting's slab** (#972 lessons 8, 19, 36): the bed is
//! the soil, filled a few centimetres below a kerb of four fieldstones with
//! a proud boulder at each corner, lapped in under the stones so no edge of
//! it shows. It is the tree's root; the stones and all five plants are its
//! children, so one drag moves the bed and everything growing in it.

use crate::catalogue::items::plants::{lsys_bush, lsys_coneflower, lsys_fern};
use crate::catalogue::items::util::{
    cuboid_tapered, id_quat, nest, prim, prim_scaled, quat_y, soil, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Generator, GeneratorKind};
use crate::seeded_defaults::{EscalationBand, EscalationTier, ThemeArchetype};

use super::{STONE, stone};

pub struct GardenBed;

impl CatalogueEntry for GardenBed {
    fn slug(&self) -> &'static str {
        "garden_bed"
    }
    fn name(&self) -> &'static str {
        "Garden Bed"
    }
    fn description(&self) -> &'static str {
        "Stone-kerbed bed of shrub, coneflowers and ferns."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn escalation_band(&self) -> EscalationBand {
        EscalationBand::only(EscalationTier::Calm)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.3,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree(&Layout::SHIPPED)
    }
}

/// The bed's outer plan, `[x, z]`.
const BED: [f32; 2] = [1.5, 1.1];
/// Kerb stones: their width across the kerb, top and (sunk) bottom.
const KERB_W: f32 = 0.12;
const KERB_TOP: f32 = 0.24;
const KERB_BOTTOM: f32 = -0.02;
/// Corner boulders: their size, and how far their outer faces stand proud
/// of the kerb's.
const CORNER: [f32; 3] = [0.26, 0.34, 0.24];
const CORNER_PROUD: f32 = 0.03;
const CORNER_BOTTOM: f32 = -0.04;
/// How far the soil runs in under the kerb on every side.
const SOIL_LAP: f32 = 0.02;

/// One planting: which species, where in plan, its uniform scale, its turn
/// about Y, its seed, and its age — the iteration count, which in these
/// grammars IS the plant's age (playbook §3), so a bed can hold a younger,
/// smaller specimen of a species without touching the species.
struct Planting {
    species: Species,
    at: [f32; 2],
    scale: f32,
    turn: f32,
    seed: u64,
    age: u32,
}

#[derive(Clone, Copy)]
enum Species {
    Bush,
    Fern,
    Coneflower(&'static str),
}

/// The bed's placement decisions, gathered so each guard can be shown to
/// bite on the one decision it is about (#972 item 30).
struct Layout {
    /// How far below the kerb's top the bed is filled.
    soil_drop: f32,
    /// How far above the soil every crown stands (zero: on it).
    plant_lift: f32,
    plantings: [Planting; 5],
}

impl Layout {
    const SHIPPED: Layout = Layout {
        soil_drop: 0.04,
        plant_lift: 0.0,
        plantings: [
            // The shrub, off-centre at the back.
            Planting {
                species: Species::Bush,
                at: [-0.26, 0.06],
                scale: 0.32,
                turn: 0.0,
                seed: 1,
                age: 5,
            },
            // Two coneflowers: pink behind on the right, gold in front.
            Planting {
                species: Species::Coneflower(""),
                at: [0.3, 0.14],
                scale: 1.15,
                turn: 0.0,
                seed: 1,
                age: 11,
            },
            Planting {
                species: Species::Coneflower("gold"),
                at: [0.02, -0.12],
                scale: 1.1,
                turn: 2.1,
                seed: 2,
                age: 11,
            },
            // Two ferns, low at the front corners.
            Planting {
                species: Species::Fern,
                at: [-0.36, -0.17],
                scale: 0.42,
                turn: 0.7,
                seed: 1,
                age: 5,
            },
            Planting {
                species: Species::Fern,
                at: [0.37, -0.12],
                scale: 0.4,
                turn: 2.9,
                seed: 2,
                age: 5,
            },
        ],
    };
}

/// A planting's nested L-system node, its crown at `crown_y`.
fn plant(p: &Planting, crown_y: f32) -> Generator {
    let mut kind = match p.species {
        Species::Bush => lsys_bush::Bush.build("").kind,
        Species::Fern => lsys_fern::Fern.build("").kind,
        Species::Coneflower(variant) => {
            let mut kind = lsys_coneflower::build_kind();
            if let GeneratorKind::LSystem { materials, .. } = &mut kind {
                crate::catalogue::items::plants::variant::apply_named(
                    lsys_coneflower::VARIANTS,
                    variant,
                    materials,
                );
            }
            kind
        }
    };
    if let GeneratorKind::LSystem {
        seed, iterations, ..
    } = &mut kind
    {
        *seed = p.seed;
        *iterations = p.age;
    }
    prim_scaled(
        kind,
        [p.at[0], crown_y, p.at[1]],
        quat_y(p.turn),
        [p.scale; 3],
    )
}

fn build_tree(layout: &Layout) -> Generator {
    let soil_top = KERB_TOP - layout.soil_drop;
    let crown_y = soil_top + layout.plant_lift;
    let (hx, hz) = (BED[0] * 0.5, BED[1] * 0.5);

    let mut on_soil = Vec::new();
    // Kerb: the front and back stones run the bed's full length and the two
    // end stones fit between them — butt joints, abutting faces.
    let kerb_h = KERB_TOP - KERB_BOTTOM;
    let kerb_y = (KERB_TOP + KERB_BOTTOM) * 0.5;
    for sz in [-1.0_f32, 1.0] {
        on_soil.push(prim(
            solid(cuboid_tapered([BED[0], kerb_h, KERB_W], 0.0, stone(STONE))),
            [0.0, kerb_y, sz * (hz - KERB_W * 0.5)],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        on_soil.push(prim(
            solid(cuboid_tapered(
                [KERB_W, kerb_h, BED[1] - 2.0 * KERB_W],
                0.0,
                stone(STONE),
            )),
            [sx * (hx - KERB_W * 0.5), kerb_y, 0.0],
            id_quat(),
        ));
    }
    // Corner boulders, standing proud of the kerb on both faces and above it.
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        on_soil.push(prim(
            solid(cuboid_tapered(CORNER, 0.0, stone([0.55, 0.53, 0.48]))),
            [
                sx * (hx + CORNER_PROUD - CORNER[0] * 0.5),
                CORNER_BOTTOM + CORNER[1] * 0.5,
                sz * (hz + CORNER_PROUD - CORNER[2] * 0.5),
            ],
            id_quat(),
        ));
    }
    on_soil.extend(layout.plantings.iter().map(|p| plant(p, crown_y)));

    // The bed itself: soil from the ground to a few centimetres under the
    // kerb, lapped in under the stones.
    let soil_plan = [
        BED[0] - 2.0 * KERB_W + 2.0 * SOIL_LAP,
        BED[1] - 2.0 * KERB_W + 2.0 * SOIL_LAP,
    ];
    nest(
        prim(
            cuboid_tapered([soil_plan[0], soil_top, soil_plan[1]], 0.0, soil()),
            [0.0, soil_top * 0.5, 0.0],
            id_quat(),
        ),
        on_soil,
    )
}

/// What the whole bed may mesh to, stone and planting together. Measured
/// at 3 212 (bush 2 168, coneflowers 268 + 328, ferns 170 each, stones 108)
/// — under the 3 924 of the spheres and cones it replaced, because the
/// shrub and ferns are nested YOUNGER (fewer iterations) rather than
/// merely smaller.
#[cfg(test)]
const BED_TRIANGLES: usize = 3_600;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_plants_clear_solids,
        assert_plants_stand_on_their_parent, assert_sanitize_stable,
        assert_soil_sits_under_its_rim, nested_plants, triangle_count,
    };

    fn shipped() -> Generator {
        GardenBed.build("")
    }

    /// Build `layout` and run `guard` on it, asserting it panics naming
    /// `needle` (#972 lesson 34: read WHICH assertion fired).
    fn bites(layout: Layout, guard: fn(&Generator), needle: &str) {
        let broken = build_tree(&layout);
        let err = std::panic::catch_unwind(|| guard(&broken))
            .expect_err("the guard passed a broken layout");
        let msg = err
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| err.downcast_ref::<&str>().map(|s| s.to_string()))
            .unwrap_or_default();
        assert!(
            msg.contains(needle),
            "the guard fired, but not on the fault: {msg}"
        );
    }

    /// The shipped layout with one planting replaced.
    fn with_planting(i: usize, p: Planting) -> Layout {
        let mut l = Layout::SHIPPED;
        l.plantings[i] = p;
        l
    }

    #[test]
    fn garden_bed_round_trips_through_sanitize() {
        assert_sanitize_stable(&shipped(), "garden_bed");
    }

    #[test]
    fn garden_bed_has_no_tilted_parents() {
        assert_no_tilted_parents(&shipped(), "garden_bed");
    }

    #[test]
    fn garden_bed_has_no_coplanar_faces() {
        assert_no_coplanar_faces(&shipped(), "garden_bed");
    }

    fn stand_on_soil(root: &Generator) {
        assert_eq!(
            assert_plants_stand_on_their_parent(root, "garden_bed", 0.05),
            5,
            "a shrub, two coneflowers and two ferns"
        );
    }

    /// Every plant is a child of the soil and its crown is on the soil's
    /// top face (lessons 8, 19, 36).
    #[test]
    fn the_planting_stands_on_the_soil() {
        stand_on_soil(&shipped());
        bites(
            Layout {
                plant_lift: 0.04,
                ..Layout::SHIPPED
            },
            stand_on_soil,
            "off the soil",
        );
    }

    fn soil_below_kerb(root: &Generator) {
        assert_soil_sits_under_its_rim(root, "garden_bed", 0.02);
    }

    /// Filled below the kerb, not heaped over it, and lapped in under the
    /// stones on every side.
    #[test]
    fn the_soil_sits_under_the_kerb() {
        soil_below_kerb(&shipped());
        bites(
            Layout {
                soil_drop: -0.07,
                ..Layout::SHIPPED
            },
            soil_below_kerb,
            "stands above the rim",
        );
    }

    fn clears(root: &Generator) {
        assert_plants_clear_solids(root, "garden_bed", 0.01);
    }

    /// No stem or leaf runs through a kerb stone or a corner boulder.
    #[test]
    fn the_planting_clears_the_stones() {
        clears(&shipped());
        // The shrub planted hard against a corner boulder.
        bites(
            with_planting(
                0,
                Planting {
                    species: Species::Bush,
                    at: [-0.5, 0.3],
                    scale: 0.45,
                    turn: 0.0,
                    seed: 1,
                    age: 5,
                },
            ),
            clears,
            "run through the container",
        );
    }

    fn inside_plan(root: &Generator) {
        let (hx, hz) = (BED[0] * 0.5, BED[1] * 0.5);
        let plants = nested_plants(root);
        assert_eq!(plants.len(), 5);
        for p in &plants {
            for q in p.points() {
                assert!(
                    q[0].abs() < hx && q[2].abs() < hz,
                    "a plant crowned at {:?} reaches [{:.3}, {:.3}, {:.3}] — outside the \
                     bed's plan ({hx} x {hz}); nothing in this bed is meant to trail",
                    p.crown,
                    q[0],
                    q[1],
                    q[2]
                );
            }
        }
    }

    /// A naturalistic bed has no trailing plant: every leaf stays over the
    /// bed's own plan (lesson 36's containment, with the kerb as the edge).
    #[test]
    fn the_planting_stays_over_the_bed() {
        inside_plan(&shipped());
        // A fern scaled up past what the front of the bed can hold.
        bites(
            with_planting(
                3,
                Planting {
                    species: Species::Fern,
                    at: [-0.42, -0.2],
                    scale: 1.0,
                    turn: 0.7,
                    seed: 1,
                    age: 7,
                },
            ),
            inside_plan,
            "outside the bed's plan",
        );
    }

    /// The two coneflowers are two individuals in two colours: different
    /// seeds and different flower slots.
    #[test]
    fn the_coneflowers_are_two_individuals() {
        let root = shipped();
        let flowers: Vec<_> = root
            .children
            .iter()
            .filter_map(|c| match &c.kind {
                GeneratorKind::LSystem {
                    seed, materials, ..
                } if materials.contains_key(&3) => Some((*seed, materials[&3].clone())),
                _ => None,
            })
            .collect();
        assert_eq!(flowers.len(), 2);
        assert_ne!(flowers[0].0, flowers[1].0, "same seed: one plant twice");
        assert_ne!(flowers[0].1, flowers[1].1, "same colour");
    }

    /// A scatter prop in every theme: pin what it costs.
    #[test]
    fn the_bed_stays_within_its_triangle_budget() {
        let tris = triangle_count(&shipped());
        assert!(
            tris <= BED_TRIANGLES,
            "the bed meshes to {tris} triangles, over its {BED_TRIANGLES} budget"
        );
    }

    /// The editability contract (lesson 3): the soil carries the eight
    /// stones and the five plants, and nothing else carries anything.
    #[test]
    fn garden_bed_subtree_sizes() {
        let root = shipped();
        assert_eq!(root.children.len(), 13);
        assert!(root.children.iter().all(|c| c.children.is_empty()));
    }
}
