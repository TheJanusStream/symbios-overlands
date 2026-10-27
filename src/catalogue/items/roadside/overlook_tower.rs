//! Overlook tower - a Roadside secondary that leads a highway strip from
//! its scenic hill. A braced steel tower climbs on a spiral stair to a
//! railed viewing deck under a red pyramid roof, coin binoculars at the
//! deck's -Z rail; a segmented LOOKOUT board, lit at every hour like the
//! strip's signs, hangs on its front, and a snack hut serves at its foot.
//!
//! Primitive-built in one flat ground-relative frame. The tower hangs off
//! the footing as one sub-assembly rooted on its stair post, the hut as
//! the other.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, glow, helix, id_quat, nest, prim, quat_x,
    quat_y, railing, solid, sphere, strut,
};
use crate::catalogue::{CatalogueEntry, Footprint, SitePreference, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, ENAMEL_CREAM, ENAMEL_RED, NEON_RED, SIGN_AMBER, concrete, enamel, sign_board,
};

/// Ground the tower stands on: the footing's top.
const GRADE: f32 = 0.15;
/// The viewing deck's underside, and its top.
const DECK: f32 = 15.0;
const DECK_TOP: f32 = DECK + 0.3;
/// Half the tower's width at its foot and under the deck.
const FOOT_HALF: f32 = 2.4;
const TOP_HALF: f32 = 1.8;
/// Half the deck's side.
const DECK_HALF: f32 = 2.4;
/// The canopy posts' height over the deck.
const CANOPY: f32 = 2.3;
/// Where the LOOKOUT board hangs, its centre's height.
const SIGN_Y: f32 = 9.5;
/// The snack hut's centre and half size.
const HUT: [f32; 2] = [6.0, -0.4];
const HUT_HALF: [f32; 2] = [1.7, 1.3];
const HUT_H: f32 = 2.7;
/// Spacing circle the settlement keeps round the tower.
const CLEARANCE: f32 = 8.0;

/// Green-grey of the tower's steel.
const TOWER_STEEL: [f32; 3] = [0.34, 0.42, 0.38];
/// Coin binoculars' yellow.
const BINO_YELLOW: [f32; 3] = [0.86, 0.68, 0.14];
/// Dark of the binoculars' lenses.
const LENS: [f32; 3] = [0.06, 0.07, 0.09];
/// The warm light inside the hut's serving hatch, from dusk.
const HATCH_WARM: [f32; 3] = [1.0, 0.76, 0.44];

pub struct OverlookTower;

impl CatalogueEntry for OverlookTower {
    fn slug(&self) -> &'static str {
        "overlook_tower"
    }
    fn name(&self) -> &'static str {
        "Overlook Tower"
    }
    fn description(&self) -> &'static str {
        "Steel lookout tower with a roofed viewing deck, a lit LOOKOUT board and a snack hut."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// The scenic high point the highway pulls off for.
    fn site(&self) -> SitePreference {
        SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Plain painted tower steel, untextured: the tower is many thin parts.
fn paint(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.5),
        metallic: Fp(0.5),
        ..Default::default()
    }
}

/// The tower's half width at height `y`: its legs lean in toward the deck.
fn half_at(y: f32) -> f32 {
    FOOT_HALF + (TOP_HALF - FOOT_HALF) * (y - GRADE) / (DECK - GRADE)
}

/// The corners of the tower at height `y`, round the square in order.
fn corners(y: f32) -> [[f32; 3]; 4] {
    let h = half_at(y);
    [[-h, y, -h], [h, y, -h], [h, y, h], [-h, y, h]]
}

/// A segmented lit sign facing -Z, its cells at `center`: the strip's own
/// [`sign_board`], with a dark face plate behind the cells that carries
/// them on the board's backing.
fn lit_sign(
    center: [f32; 3],
    size: [f32; 2],
    cells: (u32, u32),
    color: [f32; 3],
    strength: f32,
) -> Vec<Generator> {
    let [x, y, z] = center;
    let mut out = sign_board(center, size, cells, color, strength, -1.0);
    out.push(prim(
        cuboid_tapered([size[0] - 0.2, size[1] - 0.2, 0.05], 0.0, paint(LENS)),
        [x, y, z + 0.045],
        id_quat(),
    ));
    out
}

/// A pair of coin binoculars on the deck at (`x`, `z`), looking -Z.
fn binoculars(x: f32, z: f32) -> Generator {
    let head_y = DECK_TOP + 1.15;
    let mut parts = vec![
        prim(
            solid(cuboid_tapered([0.4, 0.32, 0.5], 0.0, paint(BINO_YELLOW))),
            [x, head_y, z],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.42, 0.06, 0.44], 0.0, paint(BINO_YELLOW)),
            [x, head_y + 0.18, z - 0.02],
            id_quat(),
        ),
    ];
    for dx in [-0.1_f32, 0.1] {
        parts.push(prim(
            cylinder_tapered(0.09, 0.2, 12, 0.0, paint(LENS)),
            [x + dx, head_y, z - 0.32],
            quat_x(FRAC_PI_2),
        ));
        parts.push(prim(
            cylinder_tapered(0.05, 0.16, 10, 0.0, paint(LENS)),
            [x + dx, head_y + 0.04, z + 0.3],
            quat_x(FRAC_PI_2),
        ));
    }
    nest(
        prim(
            solid(cylinder_tapered(0.08, 1.0, 10, 0.1, paint(BINO_YELLOW))),
            [x, DECK_TOP + 0.5, z],
            id_quat(),
        ),
        parts,
    )
}

/// The tower, rooted on the spiral stair's post: legs on their pads, the
/// bracing, the stair, the deck with its railing, binoculars and roofed
/// canopy, and the LOOKOUT board on its front.
fn tower() -> Generator {
    let steel = || paint(TOWER_STEEL);
    let mut parts = Vec::new();
    // Four legs leaning in from their pads to the deck's underside.
    let foot = corners(GRADE);
    let head = corners(DECK + 0.15);
    for (f, h) in foot.into_iter().zip(head) {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.9, 0.5, 0.9],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [f[0], GRADE + 0.15, f[2]],
            id_quat(),
        ));
        parts.push(strut(f, h, 0.16, 10, steel()));
    }
    // A tie round the tower at each panel line, and crossed rods in every
    // panel of every face, one stood a little off its face so they pass.
    let lines = [GRADE + 0.5, 4.0, 7.6, 11.2, DECK - 0.2];
    for &y in &lines[1..] {
        let c = corners(y);
        for k in 0..4 {
            parts.push(strut(c[k], c[(k + 1) % 4], 0.08, 8, steel()));
        }
    }
    for pair in lines.windows(2) {
        let (lo, hi) = (corners(pair[0]), corners(pair[1]));
        for k in 0..4 {
            let j = (k + 1) % 4;
            parts.push(strut(lo[k], hi[j], 0.05, 6, steel()));
            // Out of the face: the face's own outward normal.
            let mid = [(lo[k][0] + lo[j][0]) * 0.5, (lo[k][2] + lo[j][2]) * 0.5];
            let len = (mid[0] * mid[0] + mid[1] * mid[1]).sqrt();
            let out = [mid[0] / len * 0.08, mid[1] / len * 0.08];
            parts.push(strut(
                [lo[j][0] + out[0], lo[j][1], lo[j][2] + out[1]],
                [hi[k][0] + out[0], hi[k][1], hi[k][2] + out[1]],
                0.05,
                6,
                steel(),
            ));
        }
    }
    // The spiral stair: a helical stringer round the post, and treads.
    // Sixteen treads a turn, the stringer climbing with them.
    let rise = DECK - GRADE;
    let treads = 40;
    let step = (rise - 0.6) / treads as f32;
    let pitch = 16.0 * step;
    parts.push(prim(
        helix(1.0, 0.07, pitch, rise / pitch, 96, steel()),
        [0.0, GRADE + rise * 0.5, 0.0],
        id_quat(),
    ));
    for i in 0..treads {
        let a = TAU * i as f32 / 16.0;
        let y = GRADE + 0.3 + i as f32 * step;
        parts.push(prim(
            solid(cuboid_tapered([1.0, 0.06, 0.3], 0.0, steel())),
            [0.55 * a.cos(), y, 0.55 * a.sin()],
            quat_y(-a),
        ));
    }
    // The deck, its railing a post short of each corner on the sides, and
    // the binoculars at the front rail.
    parts.push(prim(
        solid(cuboid_tapered(
            [DECK_HALF * 2.0, DECK_TOP - DECK, DECK_HALF * 2.0],
            0.0,
            concrete([0.66, 0.65, 0.62]),
        )),
        [0.0, (DECK + DECK_TOP) * 0.5, 0.0],
        id_quat(),
    ));
    let e = DECK_HALF - 0.05;
    let rail = || paint(ENAMEL_CREAM);
    parts.extend(railing(
        [-e, DECK_TOP, -e],
        [e, DECK_TOP, -e],
        1.1,
        0.42,
        rail(),
    ));
    parts.extend(railing(
        [-e, DECK_TOP, e],
        [e, DECK_TOP, e],
        1.1,
        0.42,
        rail(),
    ));
    for x in [-e, e] {
        parts.extend(railing(
            [x, DECK_TOP, -e + 0.12],
            [x, DECK_TOP, e - 0.12],
            1.1,
            0.42,
            rail(),
        ));
    }
    parts.push(binoculars(-1.0, -e + 0.55));
    parts.push(binoculars(1.0, -e + 0.55));
    // The canopy: four posts and a red pyramid roof, a mast and lamp atop.
    let p = DECK_HALF - 0.25;
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        parts.push(prim(
            solid(cuboid_tapered([0.16, CANOPY + 0.05, 0.16], 0.0, steel())),
            [sx * p, DECK_TOP + (CANOPY + 0.05) * 0.5, sz * p],
            id_quat(),
        ));
    }
    let eaves = DECK_TOP + CANOPY;
    parts.push(prim(
        solid(cuboid_tapered(
            [DECK_HALF * 2.0 + 0.8, 1.6, DECK_HALF * 2.0 + 0.8],
            0.85,
            enamel(ENAMEL_RED),
        )),
        [0.0, eaves + 0.8, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        cylinder_tapered(0.05, 2.0, 8, 0.0, steel()),
        [0.0, eaves + 2.4, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        sphere(0.14, 3, glow(NEON_RED, 5.0)),
        [0.0, eaves + 3.45, 0.0],
        id_quat(),
    ));
    // The LOOKOUT board on the front legs: seven lit amber letter cells in
    // a hot red border just inside the backing's edge, on two brackets back
    // to the legs.
    let leg_z = -half_at(SIGN_Y);
    let face_z = leg_z - 0.5;
    let (w, h) = (5.4_f32, 1.3_f32);
    parts.extend(lit_sign(
        [0.0, SIGN_Y, face_z],
        [w, h],
        (7, 1),
        SIGN_AMBER,
        2.2,
    ));
    let border_z = face_z + 0.04;
    for dy in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([w - 0.04, 0.06, 0.06], 0.0, glow(NEON_RED, 6.0)),
            [0.0, SIGN_Y + dy * (h * 0.5 - 0.05), border_z],
            id_quat(),
        ));
    }
    for dx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.06, h - 0.16, 0.06], 0.0, glow(NEON_RED, 6.0)),
            [dx * (w * 0.5 - 0.05), SIGN_Y, border_z],
            id_quat(),
        ));
    }
    for dx in [-1.0_f32, 1.0] {
        let x = dx * (half_at(SIGN_Y) - 0.05);
        parts.push(prim(
            solid(cuboid_tapered([0.12, 0.12, 0.5], 0.0, steel())),
            [x, SIGN_Y, leg_z - 0.2],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cylinder_tapered(0.15, rise, 12, 0.0, steel())),
            [0.0, GRADE + rise * 0.5, 0.0],
            id_quat(),
        ),
        parts,
    )
}

/// The snack hut at the tower's foot: a cream box with a lit serving hatch
/// and counter on -Z under a red awning, a menu sign on its roof's edge.
fn snack_hut() -> Generator {
    let [cx, cz] = HUT;
    let [hx, hz] = HUT_HALF;
    let front = cz - hz;
    let mut parts = vec![
        // Roof slab, a little over the walls.
        prim(
            solid(cuboid_tapered(
                [hx * 2.0 + 0.3, 0.15, hz * 2.0 + 0.3],
                0.0,
                enamel(ENAMEL_RED),
            )),
            [cx, GRADE + HUT_H, cz],
            id_quat(),
        ),
        // The hatch's lit back, its counter, and the awning over it.
        prim(
            cuboid_tapered(
                [1.8, 1.0, 0.06],
                0.0,
                crate::catalogue::items::street::lit_from_dusk(glow(HATCH_WARM, 1.5)),
            ),
            [cx, GRADE + 1.65, front - 0.02],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([2.0, 0.08, 0.45], 0.0, enamel(ENAMEL_CREAM))),
            [cx, GRADE + 1.12, front - 0.2],
            id_quat(),
        ),
        prim(
            cuboid_tapered([2.4, 0.06, 1.0], 0.0, enamel(ENAMEL_RED)),
            [cx, GRADE + 2.35, front - 0.45],
            quat_x(-0.35),
        ),
    ];
    parts.extend(lit_sign(
        [cx, GRADE + HUT_H + 0.3, front - 0.1],
        [2.0, 0.5],
        (4, 1),
        NEON_RED,
        2.0,
    ));
    nest(
        prim(
            solid(cuboid_tapered(
                [hx * 2.0, HUT_H, hz * 2.0],
                0.0,
                enamel(ENAMEL_CREAM),
            )),
            [cx, GRADE + HUT_H * 0.5 - 0.05, cz],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let x0 = -FOOT_HALF - 0.5;
    let x1 = HUT[0] + HUT_HALF[0] + 0.2;
    let half_z = FOOT_HALF + 0.5;
    let mut root = footing(x1 - x0, half_z * 2.0, [(x0 + x1) * 0.5, 0.0], CLEARANCE);
    attach(&mut root, tower());
    attach(&mut root, snack_hut());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&OverlookTower.build(""), "overlook_tower");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let built = OverlookTower.build("");
        assert_no_z_fighting(&built, "overlook_tower");
        assert_nothing_floats(&built, "overlook_tower");
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = serde_json::to_vec(&OverlookTower.build(""))
            .expect("a built entry serialises")
            .len();
        assert!(bytes < 60_000, "overlook_tower serialises to {bytes} B");
    }
}
