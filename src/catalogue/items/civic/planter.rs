//! Planter — a marble box planted with a clipped box standard and a
//! trailing variegated ivy. A prosperity-Rich scatter prop: formal,
//! gardened planting signals upkeep and disposable means in any setting.
//!
//! The planting is two real L-system species nested in the prop (#972, the
//! first catalogue prop to nest one): the
//! [`topiary standard`](crate::catalogue::items::plants::lsys_topiary_standard)
//! at the centre and the
//! [`trailing ivy`](crate::catalogue::items::plants::lsys_trailing_ivy) in
//! the front-left corner, aimed diagonally so its shoots spill over the
//! front and the left-hand rim and nowhere else. It replaces a mound of
//! green spheres with coloured balls on top.
//!
//! **The soil is the plants' slab** (#972 lessons 8, 19, 36). The coping is
//! four stones round an opening, not a lid, so the mulch can sit
//! a few centimetres below the rim the way a real planter is filled; both
//! plants stand at the soil's top, the same value the slab is cut from, and
//! are children of it in the tree. The slab laps under the coping stones so no
//! edge of it is ever seen, and its top is its own plane — neither the
//! coping's nor the body's (lesson 7).
//!
//! Tree: foot → body → [pilasters, relief, four coping stones, soil →
//! [standard, ivy]]. Both nested plants share one cached derivation across
//! every copy in a room: a seeded room registers one generator per prop
//! slug, so every copy's plant has the same `base_ref/path` cache key.

use std::f32::consts::FRAC_PI_4;

use crate::catalogue::items::plants::{lsys_topiary_standard, lsys_trailing_ivy};
use crate::catalogue::items::util::{cuboid_tapered, id_quat, nest, prim, quat_y, soil, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use super::{MARBLE, marble};

pub struct Planter;

impl CatalogueEntry for Planter {
    fn slug(&self) -> &'static str {
        "planter"
    }
    fn name(&self) -> &'static str {
        "Planter"
    }
    fn description(&self) -> &'static str {
        "Marble planter with a clipped box standard and trailing ivy."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        super::all_themes()
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::only(ProsperityTier::Rich)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.1,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree(&Layout::SHIPPED)
    }
}

/// Top of the marble body — the coping stones are centred on it.
const BOX_H: f32 = 0.5;
/// Plan width of the body.
const BODY: f32 = 1.0;
/// The proud base foot.
const FOOT: [f32; 3] = [1.1, 0.08, 1.1];
/// Coping: outer plan width, the width of one stone, and its thickness.
const COPING_W: f32 = 1.14;
const COPING_BAR: f32 = 0.14;
const COPING_T: f32 = 0.1;
const COPING_TOP: f32 = BOX_H + COPING_T * 0.5;
/// The opening the coping stones frame, and the soil's plan inside it.
const OPENING: f32 = COPING_W - 2.0 * COPING_BAR;
/// How far the soil slab runs in under the coping on every side.
const SOIL_LAP: f32 = 0.02;
const SOIL_T: f32 = 0.06;
/// Relief panel and its depth; it stands proud of the body's front face.
const RELIEF: [f32; 3] = [0.62, 0.3, 0.04];
/// How far the relief laps into the body it is carved from.
const RELIEF_LAP: f32 = 0.005;
/// The corner pilasters' plan and their centre's distance from the axis.
const PILASTER: f32 = 0.1;
const PILASTER_AT: f32 = 0.485;
/// What the whole planter may mesh to, marble and planting together.
/// Measured at 5 814 (standard 4 500, ivy 1 170, marble 144) against the
/// sphere planting's 3 948 it replaced; the headroom is ~12%.
#[cfg(test)]
const PLANTER_TRIANGLES: usize = 6_500;

/// The planting's placement decisions, gathered so the guards can be shown
/// to bite: each test builds the shipped layout and one with a single
/// decision broken, and asserts the guard names the break (#972 item 30).
struct Layout {
    /// How far below the coping's top the planter is filled.
    soil_drop: f32,
    /// How far above the soil the plants' crowns stand (zero: on it).
    plant_lift: f32,
    /// Where the ivy is planted in plan: the front-left quarter.
    ivy_at: [f32; 2],
    /// The ivy's turn about Y. Its fan is authored toward its own `-Z`,
    /// so an eighth turn aims it at the front-left corner, `(-1, 0, -1)`.
    ivy_turn: f32,
}

impl Layout {
    const SHIPPED: Layout = Layout {
        soil_drop: 0.035,
        plant_lift: 0.0,
        ivy_at: [-0.34, -0.34],
        ivy_turn: FRAC_PI_4,
    };
}

fn build_tree(layout: &Layout) -> Generator {
    let trim = marble([0.8, 0.79, 0.76]);
    let foot_top = FOOT[1];
    // The soil's top: every plant stands here.
    let soil_top = COPING_TOP - layout.soil_drop;
    let crown_y = soil_top + layout.plant_lift;

    // The planting, a child of the soil it grows from.
    let standard = prim(
        lsys_topiary_standard::build_kind(),
        [0.0, crown_y, 0.0],
        id_quat(),
    );
    let ivy = prim(
        lsys_trailing_ivy::build_kind(),
        [layout.ivy_at[0], crown_y, layout.ivy_at[1]],
        quat_y(layout.ivy_turn),
    );
    let soil_w = OPENING + 2.0 * SOIL_LAP;
    let bed = nest(
        prim(
            cuboid_tapered([soil_w, SOIL_T, soil_w], 0.0, soil()),
            [0.0, soil_top - SOIL_T * 0.5, 0.0],
            id_quat(),
        ),
        vec![standard, ivy],
    );

    let mut on_body = vec![bed];
    // Coping: the front and back stones run the full width and the two side
    // stones fit between them, so every joint is a butt joint and the
    // stones abut rather than overlap.
    let stone_at = (COPING_W - COPING_BAR) * 0.5;
    for sz in [-1.0_f32, 1.0] {
        on_body.push(prim(
            solid(cuboid_tapered(
                [COPING_W, COPING_T, COPING_BAR],
                0.0,
                trim.clone(),
            )),
            [0.0, BOX_H, sz * stone_at],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        on_body.push(prim(
            solid(cuboid_tapered(
                [COPING_BAR, COPING_T, OPENING],
                0.0,
                trim.clone(),
            )),
            [sx * stone_at, BOX_H, 0.0],
            id_quat(),
        ));
    }
    // Corner pilasters, proud of the body faces, rising from inside the
    // foot to inside the coping. Neither end shares the body's planes, and
    // the inner faces stand clear of the coping stones' inner faces.
    let pilaster_bottom = foot_top * 0.4;
    let pilaster_top = BOX_H - 0.02;
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        on_body.push(prim(
            solid(cuboid_tapered(
                [PILASTER, pilaster_top - pilaster_bottom, PILASTER],
                0.0,
                marble(MARBLE),
            )),
            [
                sx * PILASTER_AT,
                (pilaster_top + pilaster_bottom) * 0.5,
                sz * PILASTER_AT,
            ],
            id_quat(),
        ));
    }
    // Carved relief panel, its standoff taken from the body's half-depth
    // (lesson 11): lapped into the face, the rest of it proud.
    on_body.push(prim(
        cuboid_tapered(RELIEF, 0.0, trim.clone()),
        [0.0, 0.26, -(BODY * 0.5 + RELIEF[2] * 0.5 - RELIEF_LAP)],
        id_quat(),
    ));

    // The body stands in the foot rather than on the ground beside it, so
    // the two bottoms are not one plane.
    let body_bottom = foot_top * 0.5;
    let body = nest(
        prim(
            solid(cuboid_tapered(
                [BODY, BOX_H - body_bottom, BODY],
                0.0,
                marble(MARBLE),
            )),
            [0.0, (BOX_H + body_bottom) * 0.5, 0.0],
            id_quat(),
        ),
        on_body,
    );
    nest(
        prim(
            solid(cuboid_tapered(FOOT, 0.0, trim)),
            [0.0, foot_top * 0.5, 0.0],
            id_quat(),
        ),
        vec![body],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{
        assert_no_coplanar_faces, assert_no_tilted_parents, assert_plants_clear_solids,
        assert_plants_stand_on_their_parent, assert_sanitize_stable,
        assert_soil_sits_under_its_rim, nested_plants, triangle_count,
    };
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    fn shipped() -> Generator {
        Planter.build("")
    }

    /// Build `layout` and run `guard` on it, asserting it panics with a
    /// message naming `needle` — the guard bites on the fault it is for,
    /// not on some other assertion that happens to fire (#972 lesson 34).
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

    fn walk(g: &Generator, at: [f32; 3], f: &mut dyn FnMut(&Generator, [f32; 3])) {
        let t = g.transform.translation.0;
        let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
        f(g, here);
        for c in &g.children {
            walk(c, here, f);
        }
    }

    /// Every cuboid as (centre, size, is-solid, wears-soil), in the prop frame.
    fn cuboids(root: &Generator) -> Vec<([f32; 3], [f32; 3], bool, bool)> {
        let mut out = Vec::new();
        walk(root, [0.0; 3], &mut |g, at| {
            if let GeneratorKind::Cuboid { size, common } = &g.kind {
                let soil = matches!(
                    common.material.texture,
                    SovereignTextureConfig::ForestFloor(_)
                );
                out.push((at, size.0, common.solid, soil));
            }
        });
        out
    }

    #[test]
    fn planter_round_trips_through_sanitize() {
        assert_sanitize_stable(&shipped(), "planter");
    }

    #[test]
    fn planter_has_no_tilted_parents() {
        assert_no_tilted_parents(&shipped(), "planter");
    }

    #[test]
    fn planter_has_no_coplanar_faces() {
        assert_no_coplanar_faces(&shipped(), "planter");
    }

    fn stand_on_soil(root: &Generator) {
        assert_eq!(
            assert_plants_stand_on_their_parent(root, "planter", 0.05),
            2,
            "the planter carries a standard and an ivy"
        );
        // ...and the slab they stand on is the soil, not the coping.
        let soil = cuboids(root).into_iter().filter(|c| c.3).count();
        assert_eq!(soil, 1, "exactly one soil slab");
    }

    /// Both plants are children of the soil and their crowns are on its top
    /// face (lessons 8, 19, 36): the soil is the plants' slab.
    #[test]
    fn the_planting_stands_on_the_soil() {
        stand_on_soil(&shipped());
        bites(
            Layout {
                plant_lift: 0.05,
                ..Layout::SHIPPED
            },
            stand_on_soil,
            "off the soil",
        );
    }

    fn soil_below_rim(root: &Generator) {
        assert_soil_sits_under_its_rim(root, "planter", 0.02);
    }

    /// Filled a few centimetres below the coping, and lapped in under the
    /// stones so no edge of the slab can be seen (lesson 7).
    #[test]
    fn the_soil_is_below_the_rim_and_laps_under_it() {
        soil_below_rim(&shipped());
        bites(
            Layout {
                soil_drop: -0.02,
                ..Layout::SHIPPED
            },
            soil_below_rim,
            "stands above the rim",
        );
    }

    fn clears(root: &Generator) {
        assert_plants_clear_solids(root, "planter", 0.01);
    }

    /// No stem or leaf runs through the marble: the ivy's shoots rise over
    /// the coping before they fall, and nothing droops through a stone.
    #[test]
    fn the_planting_clears_the_container() {
        clears(&shipped());
        bites(
            Layout {
                ivy_at: [-0.05, -0.05],
                ..Layout::SHIPPED
            },
            clears,
            "run through the container",
        );
    }

    fn trails_forward_left(root: &Generator) {
        let half = COPING_W * 0.5;
        let plants = nested_plants(root);
        assert_eq!(plants.len(), 2);
        for p in &plants {
            let (lo, hi) = p
                .points()
                .fold(([f32::MAX; 3], [f32::MIN; 3]), |(lo, hi), q| {
                    (
                        [lo[0].min(q[0]), lo[1].min(q[1]), lo[2].min(q[2])],
                        [hi[0].max(q[0]), hi[1].max(q[1]), hi[2].max(q[2])],
                    )
                });
            let is_standard = p.crown[0].abs() < 1e-4 && p.crown[2].abs() < 1e-4;
            if is_standard {
                for axis in [0, 2] {
                    assert!(
                        lo[axis] > -half && hi[axis] < half,
                        "the standard spans {:.3}..{:.3} on axis {axis} — outside the \
                         planter's plan ({half})",
                        lo[axis],
                        hi[axis]
                    );
                }
            } else {
                assert!(
                    hi[0] < half && hi[2] < half,
                    "the ivy reaches x {:.3}, z {:.3} — it spills over the back or the right \
                     (the plan edge is {half}); it is planted to trail forward-left",
                    hi[0],
                    hi[2]
                );
                assert!(
                    lo[0] < -half && lo[2] < -half,
                    "the ivy reaches only x {:.3}, z {:.3} — it does not trail over both the \
                     front and the left rim",
                    lo[0],
                    lo[2]
                );
            }
        }
    }

    /// The standard stays inside the planter's plan; the ivy spills over the
    /// front and left rims and never over the back or right.
    #[test]
    fn only_the_ivy_trails_and_only_forward_left() {
        trails_forward_left(&shipped());
        bites(
            // Planted in the back-right corner and aimed out of it.
            Layout {
                ivy_at: [0.34, 0.34],
                ivy_turn: std::f32::consts::PI + FRAC_PI_4,
                ..Layout::SHIPPED
            },
            trails_forward_left,
            "spills over the back",
        );
    }

    /// The planter is a scatter prop in every theme: pin what it costs, so a
    /// grammar edit that doubles the foliage is a decision, not a drift.
    #[test]
    fn the_planter_stays_within_its_triangle_budget() {
        let tris = triangle_count(&shipped());
        assert!(
            tris <= PLANTER_TRIANGLES,
            "the planter meshes to {tris} triangles, over its {PLANTER_TRIANGLES} budget"
        );
    }

    /// The editability contract (lesson 3): the body carries its ten parts,
    /// the soil carries the planting, and nothing else carries anything.
    #[test]
    fn planter_subtree_sizes() {
        let root = shipped();
        assert_eq!(root.children.len(), 1, "the foot carries the body");
        let body = &root.children[0];
        assert_eq!(
            body.children.len(),
            10,
            "soil, four coping stones, four pilasters, relief"
        );
        let soil = &body.children[0];
        assert_eq!(soil.children.len(), 2, "the soil carries both plants");
        assert!(
            soil.children
                .iter()
                .all(|c| matches!(c.kind, GeneratorKind::LSystem { .. }) && c.children.is_empty())
        );
    }
}
