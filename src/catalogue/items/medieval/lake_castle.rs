//! Lake castle - a Medieval secondary that stands with its feet in the
//! water. A square ashlar keep with four round corner turrets and a steep
//! slate roof inside its battlements, a low rubble curtain round a small
//! paved court, a water gate in the -Z wall opening onto a timber landing
//! stage, and a small gatehouse on the land side.
//!
//! The [castle](super::medieval_castle) is a lord's whole household spread
//! over wards and wings; this is one tower and its court, the house of a
//! keeper of a crossing or a mill-lake, read across the water by its
//! keep. The settlement turns a shore building's front to the water, so
//! the water gate and the stage are the -Z hero face.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, quat_x,
    solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    HERALD_BLUE, HERALD_GOLD, IRON_DARK, SLATE_GREY, STONE_GREY, STONE_PALE, WOOD_DARK, WOOD_OAK,
    cloth, crenellations, iron, rough_stone, shingle, stone, timber,
};

/// Spacing circle the settlement keeps round the castle.
const CLEARANCE: f32 = 14.0;

/// Curtain wall: outer half-extents, thickness and height to its top.
const CURTAIN_X: f32 = 10.5;
const CURTAIN_Z: f32 = 8.5;
const WALL_T: f32 = 0.9;
const WALL_TOP: f32 = 4.6;

/// The court's paving, a little above the footing's reveal.
const COURT_TOP: f32 = 0.3;

/// The keep: centre on Z, half side, height to its parapet walk.
const KEEP_Z: f32 = 2.2;
const KEEP_HALF: f32 = 4.5;
const KEEP_TOP: f32 = 13.0;

/// The water gate: half its clear width, and the springing of its arch.
const GATE_HALF: f32 = 1.6;
const GATE_SPRING: f32 = 2.7;

/// The landing stage: half its width, its far end, and its deck top.
const STAGE_HALF: f32 = 3.4;
const STAGE_END: f32 = -13.8;
const DECK_TOP: f32 = 0.42;

pub struct LakeCastle;

impl CatalogueEntry for LakeCastle {
    fn slug(&self) -> &'static str {
        "lake_castle"
    }
    fn name(&self) -> &'static str {
        "Lake Castle"
    }
    fn description(&self) -> &'static str {
        "Square keep with corner turrets in a small walled court, its water gate opening onto a landing stage."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// A water castle belongs where the land meets the lake.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The unlit dark of an opening with a room or a passage behind it.
fn void() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.04, 0.035, 0.03]),
        roughness: Fp(1.0),
        ..Default::default()
    }
}

/// A window whose room is lit from dusk, under a stone hood mould, set in
/// the wall face at `face` (its sign the outward direction) on `axis_x`
/// (`true`: the face is normal to X).
fn window(at: [f32; 3], face: f32, axis_x: bool) -> Vec<Generator> {
    let n = face.signum();
    let light = lit_from_dusk(glow([0.95, 0.58, 0.24], 2.0));
    let (glass, hood) = if axis_x {
        ([0.08, 1.5, 0.7], [0.24, 0.18, 1.0])
    } else {
        ([0.7, 1.5, 0.08], [1.0, 0.18, 0.24])
    };
    let put = |d: f32| {
        if axis_x {
            [face + n * d, at[1], at[2]]
        } else {
            [at[0], at[1], face + n * d]
        }
    };
    let glass_at = put(0.02);
    let mut hood_at = put(0.08);
    hood_at[1] += 0.84;
    vec![
        prim(cuboid_tapered(glass, 0.0, light), glass_at, id_quat()),
        prim(
            solid(cuboid_tapered(hood, 0.0, stone(STONE_PALE))),
            hood_at,
            id_quat(),
        ),
    ]
}

/// The curtain round the court, the court's paving as its root: four
/// rubble walls under a ring of merlons, square corner piers, and the
/// water gate cut through the -Z wall under a round arch, its portcullis
/// raised.
fn curtain() -> Generator {
    let wall = || rough_stone(STONE_GREY);
    let mid = WALL_TOP * 0.5;
    let (ix, iz) = (CURTAIN_X - WALL_T * 0.5, CURTAIN_Z - WALL_T * 0.5);
    let mut parts = Vec::new();

    // Side walls, stopped 2 cm short of the front and back walls' faces.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [WALL_T, WALL_TOP, 2.0 * CURTAIN_Z - 0.04],
                0.0,
                wall(),
            )),
            [sx * ix, mid, 0.0],
            id_quat(),
        ));
    }
    // Back wall, broken where the land gatehouse stands over it.
    let gh = 2.3_f32;
    for sx in [-1.0_f32, 1.0] {
        let len = CURTAIN_X - 0.02 - (gh - 0.3);
        parts.push(prim(
            solid(cuboid_tapered([len, WALL_TOP, WALL_T], 0.0, wall())),
            [sx * (gh - 0.3 + len * 0.5), mid, iz],
            id_quat(),
        ));
    }
    // Front wall either side of the water gate, and the arch course over it.
    let side = CURTAIN_X - 0.02 - GATE_HALF;
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([side, WALL_TOP, WALL_T], 0.0, wall())),
            [sx * (GATE_HALF + side * 0.5), mid, -iz],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered(
            [
                2.0 * GATE_HALF + 0.2,
                WALL_TOP - 0.02 - GATE_SPRING,
                WALL_T - 0.06,
            ],
            0.0,
            wall(),
        )),
        [0.0, (GATE_SPRING + WALL_TOP - 0.02) * 0.5, -iz],
        id_quat(),
    ));
    // The arch's head: the passage's dark half-round through the course,
    // its flat face a little under the course's soffit, and a dressed-stone
    // rib round it, proud of the face.
    parts.push(prim(
        with_cut(
            cylinder_tapered(GATE_HALF, WALL_T + 0.04, 24, 0.0, void()),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, GATE_SPRING - 0.03, -iz],
        quat_x(-FRAC_PI_2),
    ));
    parts.push(prim(
        with_cut(
            torus(0.16, GATE_HALF + 0.12, stone(STONE_PALE)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        [0.0, GATE_SPRING, -CURTAIN_Z - 0.04],
        quat_x(-FRAC_PI_2),
    ));
    // The raised portcullis, its teeth and bottom rail showing under the
    // arch course it is drawn up into.
    let port_z = -iz + 0.2;
    for k in 0..4 {
        let x = -GATE_HALF + 0.3 + k as f32 * (2.0 * GATE_HALF - 0.6) / 3.0;
        parts.push(prim(
            cylinder_tapered(0.05, 0.9, 6, 0.0, iron(IRON_DARK)),
            [x, GATE_SPRING - 0.35, port_z],
            id_quat(),
        ));
    }
    parts.push(prim(
        cuboid_tapered([2.0 * GATE_HALF + 0.1, 0.12, 0.12], 0.0, iron(IRON_DARK)),
        [0.0, GATE_SPRING - 0.65, port_z],
        id_quat(),
    ));
    // Square piers over the four corners, standing above the wall walk.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered([1.5, WALL_TOP + 1.2, 1.5], 0.0, wall())),
                [
                    sx * (CURTAIN_X - 0.55),
                    (WALL_TOP + 1.2) * 0.5,
                    sz * (CURTAIN_Z - 0.55),
                ],
                id_quat(),
            ));
        }
    }
    // Merlons along the wall top, sunk a little into it.
    parts.extend(crenellations(
        [0.0, WALL_TOP - 0.05, 0.0],
        ix,
        iz,
        0.85,
        1.0,
        WALL_T * 0.7,
        wall(),
    ));

    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * CURTAIN_X - 0.4, 0.2, 2.0 * CURTAIN_Z - 0.4],
                0.0,
                stone(STONE_GREY),
            )),
            [0.0, COURT_TOP - 0.1, 0.0],
            id_quat(),
        ),
        parts,
    )
}

/// The keep: an ashlar tower with a round turret on each corner, capped by
/// a slate cone, a ring of merlons round its parapet and a steep slate
/// pyramid roof inside it; windows lit from dusk and a door to the court.
fn keep() -> Generator {
    let (h, cz) = (KEEP_HALF, KEEP_Z);
    let front = cz - h;
    let turret_r = 1.35_f32;
    let turret_top = KEEP_TOP + 2.6;
    let mut parts = Vec::new();

    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            let (x, z) = (sx * h, cz + sz * h);
            parts.push(prim(
                solid(cylinder_tapered(
                    turret_r,
                    turret_top + 0.3 - (COURT_TOP - 0.1),
                    20,
                    0.0,
                    stone(STONE_PALE),
                )),
                [x, (COURT_TOP - 0.1 + turret_top + 0.3) * 0.5, z],
                id_quat(),
            ));
            // A corbelled ring under each turret's cap.
            parts.push(prim(
                solid(cylinder_tapered(
                    turret_r + 0.22,
                    0.5,
                    20,
                    0.0,
                    stone(STONE_GREY),
                )),
                [x, turret_top - 0.2, z],
                id_quat(),
            ));
            parts.push(prim(
                solid(cone(turret_r + 0.45, 3.4, 20, shingle(SLATE_GREY))),
                [x, turret_top + 1.7, z],
                id_quat(),
            ));
        }
    }
    // Merlons round the parapet, between the turrets.
    parts.extend(
        crenellations(
            [0.0, KEEP_TOP - 0.05, cz],
            h - 0.25,
            h - 0.25,
            0.95,
            0.85,
            0.55,
            stone(STONE_PALE),
        )
        .into_iter()
        .filter(|m| {
            let [x, _, z] = m.transform.translation.0;
            // The turrets take the corners.
            (x.abs() < h - 1.4) || ((z - cz).abs() < h - 1.4)
        }),
    );
    // Steep slate pyramid inside the parapet.
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * h - 1.4, 5.2, 2.0 * h - 1.4],
            0.97,
            shingle(SLATE_GREY),
        )),
        [0.0, KEEP_TOP - 0.1 + 2.6, cz],
        id_quat(),
    ));
    // Windows: two storeys over the water on the front, one each side.
    for y in [6.0_f32, 9.6] {
        for x in [-1.7_f32, 1.7] {
            parts.extend(window([x, y, 0.0], front, false));
        }
    }
    for sx in [-1.0_f32, 1.0] {
        parts.extend(window([0.0, 8.0, cz], sx * h, true));
    }
    // Door to the court, up two steps, under a stone lintel.
    parts.push(prim(
        solid(cuboid_tapered([1.4, 2.5, 0.12], 0.0, timber(WOOD_OAK))),
        [0.0, COURT_TOP + 0.4 + 1.25, front - 0.04],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered([1.9, 0.3, 0.3], 0.0, stone(STONE_PALE))),
        [0.0, COURT_TOP + 0.4 + 2.6, front - 0.1],
        id_quat(),
    ));
    for (k, d) in [(0, 1.0_f32), (1, 0.6)] {
        parts.push(prim(
            solid(cuboid_tapered([2.2, 0.2, d], 0.0, stone(STONE_GREY))),
            [
                0.0,
                COURT_TOP + 0.08 + k as f32 * 0.2,
                front - d * 0.5 + 0.05,
            ],
            id_quat(),
        ));
    }
    // A banner over the roof, the keeper's colours.
    let apex = KEEP_TOP - 0.1 + 5.2;
    parts.push(prim(
        solid(cylinder_tapered(0.07, 3.0, 8, 0.2, timber(WOOD_DARK))),
        [0.0, apex + 0.9, cz],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([1.4, 0.9, 0.06], 0.0, cloth(HERALD_BLUE, HERALD_GOLD)),
        [0.74, apex + 1.9, cz],
        id_quat(),
    ));

    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * h, KEEP_TOP - COURT_TOP + 0.1, 2.0 * h],
                0.0,
                stone(STONE_PALE),
            )),
            [0.0, (COURT_TOP - 0.1 + KEEP_TOP) * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// The land gate: a small square tower over the back wall with a passage
/// through it and merlons round its top.
fn land_gate() -> Generator {
    let gh = 2.3_f32;
    let top = 7.4_f32;
    let z = CURTAIN_Z - WALL_T * 0.5;
    let pass = 1.4_f32;
    let mut parts = Vec::new();
    // The two cheeks of the passage.
    let cheek = gh - pass;
    parts.push(prim(
        solid(cuboid_tapered(
            [cheek, 3.4, 2.0 * gh],
            0.0,
            stone(STONE_PALE),
        )),
        [pass + cheek * 0.5, 1.7, z],
        id_quat(),
    ));
    // The passage's dark depth, set back in it.
    parts.push(prim(
        cuboid_tapered([2.0 * pass + 0.1, 3.1, 0.1], 0.0, void()),
        [0.0, 1.55 + 0.05, z + 0.6],
        id_quat(),
    ));
    // The chamber over the passage.
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * gh, top - 3.4, 2.0 * gh],
            0.0,
            stone(STONE_PALE),
        )),
        [0.0, (3.4 + top) * 0.5, z],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.18, 1.0, 0.1], 0.0, void()),
        [0.0, 5.4, z + gh + 0.02],
        id_quat(),
    ));
    // Merlons standing a little proud of the chamber's faces.
    parts.extend(crenellations(
        [0.0, top - 0.05, z],
        gh - 0.22,
        gh - 0.22,
        0.8,
        0.8,
        0.5,
        stone(STONE_PALE),
    ));
    nest(
        prim(
            solid(cuboid_tapered(
                [cheek, 3.4, 2.0 * gh],
                0.0,
                stone(STONE_PALE),
            )),
            [-(pass + cheek * 0.5), 1.7, z],
            id_quat(),
        ),
        parts,
    )
}

/// The landing stage: a plank deck on piles running out from the water
/// gate, the outer piles standing up as mooring posts.
fn landing_stage() -> Generator {
    let near = -CURTAIN_Z + 0.1;
    let len = near - STAGE_END;
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for (z, post) in [(-9.6_f32, false), (-11.7, false), (STAGE_END + 0.3, true)] {
            let top = if post {
                DECK_TOP + 0.9
            } else {
                DECK_TOP - 0.05
            };
            let bottom = -1.4_f32;
            parts.push(prim(
                solid(cylinder_tapered(
                    0.17,
                    top - bottom,
                    8,
                    0.0,
                    timber(WOOD_DARK),
                )),
                [sx * (STAGE_HALF - 0.25), (top + bottom) * 0.5, z],
                id_quat(),
            ));
        }
    }
    // Bearers under the deck, across the piles.
    for z in [-9.6_f32, -11.7, STAGE_END + 0.3] {
        parts.push(prim(
            solid(cuboid_tapered(
                [2.0 * STAGE_HALF - 0.1, 0.2, 0.26],
                0.0,
                timber(WOOD_DARK),
            )),
            [0.0, DECK_TOP - 0.26, z],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * STAGE_HALF, 0.18, len],
                0.0,
                timber(WOOD_OAK),
            )),
            [0.0, DECK_TOP - 0.09, (near + STAGE_END) * 0.5],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let mut root = footing(
        2.0 * CURTAIN_X + 0.5,
        2.0 * CURTAIN_Z + 0.5,
        [0.0, 0.0],
        CLEARANCE,
    );
    attach(&mut root, curtain());
    attach(&mut root, keep());
    attach(&mut root, land_gate());
    attach(&mut root, landing_stage());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&LakeCastle.build(""), "lake_castle");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = LakeCastle.build("");
        assert_nothing_floats(&g, "lake_castle");
        assert_no_z_fighting(&g, "lake_castle");
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&LakeCastle.build("")).unwrap().len();
        eprintln!("lake_castle record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
