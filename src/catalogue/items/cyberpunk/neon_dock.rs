//! Neon dock - a Cyberpunk secondary that leads a settlement on its shore
//! (#1796). A harbour night market on a concrete pier: shipping containers
//! stacked two high and opened into lit stalls, a gallery along the upper
//! row, framed neon signs on the roofs, a floating pontoon edge on the water
//! (-Z) side, and a 20 m lattice crane whose mast carries a tall blade sign
//! and whose jib hangs a sign out over the water.
//!
//! Primitive-built on a footing; each stack, the gallery and the crane is a
//! subtree rooted where it stands, so the ruin pass leans each whole.

use crate::catalogue::items::util::{
    BALUSTER_PITCH, attach, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim,
    quat_x, quat_y, railing, solid, sphere, strut, torus, tube, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONTAINER_BLUE, CONTAINER_RUST, DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, concrete,
    corrugated, fx, metal,
};

/// The spacing circle the settlement keeps round the dock: the pier's half
/// diagonal.
const CLEARANCE: f32 = 13.0;
/// The pier: its width along the shore and its depth inland, the main deck's
/// top, and where the main deck gives way to the pontoon.
const PIER_W: f32 = 22.0;
const PIER_D: f32 = 14.0;
const DECK_TOP: f32 = 0.5;
const PONTOON_TOP: f32 = 0.3;
const DECK_FRONT: f32 = -4.0;
/// A shipping container's height and depth; its length is per stack.
const BOX_H: f32 = 2.6;
const BOX_D: f32 = 2.44;
/// The front face of the stall row.
const STALL_FRONT: f32 = 0.8;

/// Deep red and dark green container paint beside the kit's blue and rust.
const CONTAINER_RED: [f32; 3] = [0.40, 0.11, 0.10];
const CONTAINER_GREEN: [f32; 3] = [0.16, 0.28, 0.20];
/// The warm light inside an open stall.
const STALL_LIGHT: [f32; 3] = [1.0, 0.52, 0.24];

pub struct NeonDock;

impl CatalogueEntry for NeonDock {
    fn slug(&self) -> &'static str {
        "neon_dock"
    }
    fn name(&self) -> &'static str {
        "Neon Dock"
    }
    fn description(&self) -> &'static str {
        "Harbour night market of stacked container stalls under neon, with a crane sign mast."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    /// A pier: it stands where the land meets the water.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
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

fn build_tree() -> Generator {
    let mut root = footing(PIER_W, PIER_D, [0.0, 0.0], CLEARANCE);
    attach(&mut root, deck());
    attach(&mut root, pontoon());

    let row_z = STALL_FRONT + BOX_D * 0.5;
    let up = DECK_TOP + BOX_H - 0.02;
    // The left stack: an open stall with a sign on its roof.
    let left = nest(
        container([-7.0, DECK_TOP, row_z], 6.1, CONTAINER_BLUE, true),
        vec![sign(
            [-8.4, up + 0.9, STALL_FRONT + 0.3],
            [3.0, 1.3],
            NEON_CYAN,
            NEON_MAGENTA,
            true,
        )],
    );
    attach(&mut root, left);
    // The middle stack, two high and offset: two stalls below, two above.
    for (x, below, above) in [
        (0.0_f32, CONTAINER_RUST, CONTAINER_RED),
        (7.0, CONTAINER_GREEN, CONTAINER_BLUE),
    ] {
        let top_x = if x < 1.0 { -3.5 } else { 3.5 };
        let upper = nest(
            // Stood 5 cm back so its face clears the stall's below.
            container([top_x, up, row_z + 0.05], 6.1, above, true),
            vec![sign(
                [top_x, up + BOX_H + 0.86, STALL_FRONT + 0.3],
                [4.2, 1.5],
                if x < 1.0 { NEON_MAGENTA } else { NEON_LIME },
                NEON_CYAN,
                true,
            )],
        );
        attach(
            &mut root,
            nest(
                container([x, DECK_TOP, row_z], 6.1, below, true),
                vec![upper],
            ),
        );
    }
    // Two closed containers behind, the market's stores.
    for (x, colour) in [(-6.4_f32, CONTAINER_RUST), (1.2, CONTAINER_BLUE)] {
        attach(&mut root, container([x, DECK_TOP, 5.4], 6.1, colour, false));
    }
    attach(&mut root, gallery());
    attach(&mut root, crane());
    // Signature life: the neon's buzz over the market.
    root.audio = fx::neon_buzz();
    root
}

/// The pier's main deck, a cyan strip along the step down to the pontoon.
fn deck() -> Generator {
    let d = PIER_D * 0.5 - DECK_FRONT;
    let slab = prim(
        solid(cuboid_tapered(
            [PIER_W, DECK_TOP, d],
            0.0,
            concrete([0.30, 0.30, 0.32]),
        )),
        [0.0, DECK_TOP * 0.5, DECK_FRONT + d * 0.5],
        id_quat(),
    );
    let strip = prim(
        cuboid_tapered([PIER_W - 0.6, 0.06, 0.06], 0.0, glow(NEON_CYAN, 6.0)),
        [0.0, DECK_TOP - 0.08, DECK_FRONT - 0.02],
        id_quat(),
    );
    nest(slab, vec![strip])
}

/// The floating platform along the water side: a dark grated pontoon, its
/// edge picked out in magenta neon, with tyre fenders on its face and
/// mooring bollards along its lip.
fn pontoon() -> Generator {
    let front = -PIER_D * 0.5;
    let d = DECK_FRONT - front + 0.1;
    let slab = prim(
        solid(cuboid_tapered(
            [PIER_W - 0.1, PONTOON_TOP, d],
            0.0,
            metal(DARK_METAL),
        )),
        [0.0, PONTOON_TOP * 0.5, front + d * 0.5],
        id_quat(),
    );
    let mut parts = vec![prim(
        cuboid_tapered([PIER_W - 0.4, 0.06, 0.06], 0.0, glow(NEON_MAGENTA, 6.0)),
        [0.0, PONTOON_TOP - 0.05, front - 0.02],
        id_quat(),
    )];
    for (k, x) in [-8.0_f32, -2.7, 2.7, 8.0].into_iter().enumerate() {
        parts.push(prim(
            torus(0.12, 0.26, metal([0.04, 0.04, 0.05])),
            [x, 0.2 + k as f32 * 0.01, front - 0.1],
            quat_x(std::f32::consts::FRAC_PI_2),
        ));
        parts.push(prim(
            solid(cylinder_tapered(
                0.16,
                0.5,
                10,
                0.0,
                metal([0.38, 0.36, 0.30]),
            )),
            [x + 1.3, PONTOON_TOP + 0.24, front + 0.4],
            id_quat(),
        ));
    }
    nest(slab, parts)
}

/// A shipping container standing on `base` (its floor's centre), `len`
/// long along X, corrugated in `colour`, with proud corner posts. An `open`
/// one is a stall: its -Z side rolled up on a lit interior over a counter.
fn container(base: [f32; 3], len: f32, colour: [f32; 3], open: bool) -> Generator {
    let [x, y, z] = base;
    let body = prim(
        solid(cuboid_tapered([len, BOX_H, BOX_D], 0.0, corrugated(colour))),
        [x, y + BOX_H * 0.5, z],
        id_quat(),
    );
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(prim(
                cuboid_tapered([0.18, BOX_H + 0.02, 0.18], 0.0, metal(DARK_METAL)),
                [
                    x + sx * (len * 0.5 - 0.07),
                    y + BOX_H * 0.5,
                    z + sz * (BOX_D * 0.5 - 0.07),
                ],
                id_quat(),
            ));
        }
    }
    if open {
        let front = z - BOX_D * 0.5;
        parts.push(prim(
            cuboid_tapered([len - 0.5, 1.7, 0.06], 0.0, glow(STALL_LIGHT, 1.1)),
            [x, y + 0.95, front - 0.02],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered(
                [len - 0.4, 0.62, 0.12],
                0.0,
                corrugated(crate::catalogue::items::util::tint(colour, [0.7, 0.7, 0.7])),
            ),
            [x, y + 2.15, front - 0.05],
            id_quat(),
        ));
        parts.push(prim(
            solid(cuboid_tapered(
                [len - 0.7, 1.0, 0.5],
                0.0,
                metal([0.12, 0.12, 0.14]),
            )),
            [x, y + 0.5, front - 0.3],
            id_quat(),
        ));
    }
    nest(body, parts)
}

/// A framed neon sign: a dark backing board, a lit face and a hot tube
/// border, the uprights a centimetre prouder than the rails. One that
/// `stands` has two legs down to the roof behind it.
fn sign(
    center: [f32; 3],
    size: [f32; 2],
    face: [f32; 3],
    tube_col: [f32; 3],
    stands: bool,
) -> Generator {
    let [x, y, z] = center;
    let [w, h] = size;
    let board = prim(
        solid(cuboid_tapered([w, h, 0.2], 0.0, metal(DARK_METAL))),
        [x, y, z],
        id_quat(),
    );
    let mut parts = vec![prim(
        cuboid_tapered([w - 0.3, h - 0.3, 0.04], 0.0, glow(face, 2.0)),
        [x, y, z - 0.11],
        id_quat(),
    )];
    // The tubes ride the board's edges, half proud of them.
    for sy in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([w + 0.06, 0.08, 0.08], 0.0, glow(tube_col, 6.0)),
            [x, y + sy * h * 0.5, z - 0.13],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.08, h + 0.06, 0.08], 0.0, glow(tube_col, 6.0)),
            [x + sx * (w * 0.5 + 0.01), y, z - 0.14],
            id_quat(),
        ));
    }
    // Two legs down to the roof behind the board.
    for sx in [-1.0_f32, 1.0].into_iter().filter(|_| stands) {
        parts.push(prim(
            cuboid_tapered([0.12, 0.9, 0.12], 0.0, metal(DARK_METAL)),
            [x + sx * w * 0.3, y - h * 0.5 - 0.4, z + 0.05],
            id_quat(),
        ));
    }
    nest(board, parts)
}

/// The gallery along the upper stalls: a grated walk on posts, railed, with
/// a ramp up from the market floor at its left end.
fn gallery() -> Generator {
    let level = DECK_TOP + BOX_H - 0.02;
    let (x0, x1) = (-8.8_f32, 6.6_f32);
    let (z0, z1) = (-0.7_f32, STALL_FRONT + 0.06);
    // The posts and the ramp stop 4 cm short of the walk's top.
    let post_h = level - DECK_TOP - 0.04;
    let first_post = prim(
        solid(cuboid_tapered([0.2, post_h, 0.2], 0.0, metal(DARK_METAL))),
        [x0 + 0.2, DECK_TOP + post_h * 0.5, z0 + 0.12],
        id_quat(),
    );
    let mut parts = vec![prim(
        solid(cuboid_tapered(
            [x1 - x0, 0.16, z1 - z0],
            0.0,
            metal([0.18, 0.18, 0.20]),
        )),
        [(x0 + x1) * 0.5, level - 0.08, (z0 + z1) * 0.5],
        id_quat(),
    )];
    for px in [-4.0_f32, 0.0, 3.6, x1 - 0.2] {
        parts.push(prim(
            solid(cuboid_tapered([0.2, post_h, 0.2], 0.0, metal(DARK_METAL))),
            [px, DECK_TOP + post_h * 0.5, z0 + 0.12],
            id_quat(),
        ));
    }
    parts.extend(railing(
        [x0 + 2.0, level, z0 + 0.04],
        [x1 - 0.05, level, z0 + 0.04],
        1.0,
        BALUSTER_PITCH * 4.0,
        metal([0.22, 0.22, 0.24]),
    ));
    // A lime rail light along the gallery's lip.
    parts.push(prim(
        cuboid_tapered([x1 - x0 - 0.4, 0.05, 0.05], 0.0, glow(NEON_LIME, 6.0)),
        [(x0 + x1) * 0.5 + 0.2, level - 0.12, z0 - 0.02],
        id_quat(),
    ));
    // The ramp: a wedge turned to climb toward +Z onto the walk's left end.
    let ramp_len = 3.4_f32;
    parts.push(prim(
        solid(wedge([1.6, post_h, ramp_len], metal([0.16, 0.16, 0.18]))),
        [
            x0 + 0.85,
            DECK_TOP + post_h * 0.5,
            z0 - ramp_len * 0.5 + 0.3,
        ],
        quat_y(std::f32::consts::PI),
    ));
    nest(first_post, parts)
}

/// The crane sign mast at the pier's water corner: a 20 m lattice of four
/// legs, square collars and front braces; a tall blade sign up its -Z face;
/// a jib out over the water hanging a sign; a red aviation light on top.
fn crane() -> Generator {
    let (x, z) = (9.0_f32, -2.4_f32);
    let half = 0.65_f32;
    let top = 20.0_f32;
    let iron = metal([0.14, 0.14, 0.16]);
    let plate = prim(
        solid(cuboid_tapered([2.0, 0.3, 2.0], 0.0, metal(DARK_METAL))),
        [x, DECK_TOP + 0.13, z],
        id_quat(),
    );
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.2, top - DECK_TOP, 0.2],
                    0.0,
                    iron.clone(),
                )),
                [x + sx * half, (top + DECK_TOP) * 0.5, z + sz * half],
                id_quat(),
            ));
        }
    }
    // Square collars every few metres: a four-sided tube turned square on.
    let collars = [4.0_f32, 8.0, 12.0, 16.0, top - 0.1];
    for (k, cy) in collars.into_iter().enumerate() {
        parts.push(prim(
            tube(
                half * std::f32::consts::SQRT_2 + 0.12,
                half * std::f32::consts::SQRT_2 - 0.06,
                0.2,
                4,
                iron.clone(),
            ),
            [x, cy + k as f32 * 0.003, z],
            quat_y(std::f32::consts::FRAC_PI_4),
        ));
    }
    // Zig-zag braces on the +Z (back) face, clear of the blade sign.
    for (k, pair) in collars.windows(2).enumerate() {
        let (y0, y1) = (pair[0], pair[1]);
        let s = if k % 2 == 0 { 1.0 } else { -1.0 };
        parts.push(strut(
            [x - s * half, y0, z + half + 0.02],
            [x + s * half, y1, z + half + 0.02],
            0.06,
            6,
            iron.clone(),
        ));
    }
    // The tall blade sign on the -Z face: four glyph tiles in a hot frame.
    let (bw, b0, b1) = (1.7_f32, 5.6_f32, 15.4_f32);
    let bz = z - half - 0.2;
    parts.push(prim(
        solid(cuboid_tapered([bw, b1 - b0, 0.25], 0.0, metal(DARK_METAL))),
        [x, (b0 + b1) * 0.5, bz],
        id_quat(),
    ));
    let glyphs = [NEON_MAGENTA, NEON_CYAN, NEON_MAGENTA, NEON_LIME];
    let tile_h = (b1 - b0 - 0.6) / 4.0;
    for (k, c) in glyphs.into_iter().enumerate() {
        parts.push(prim(
            cuboid_tapered([bw - 0.5, tile_h - 0.35, 0.04], 0.0, glow(c, 2.2)),
            [x, b0 + 0.3 + tile_h * (k as f32 + 0.5), bz - 0.14],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered([0.08, b1 - b0 + 0.06, 0.08], 0.0, glow(NEON_CYAN, 7.0)),
            [x + sx * (bw * 0.5 + 0.01), (b0 + b1) * 0.5, bz - 0.17],
            id_quat(),
        ));
    }
    for y in [b0, b1] {
        parts.push(prim(
            cuboid_tapered([bw + 0.06, 0.08, 0.08], 0.0, glow(NEON_CYAN, 7.0)),
            [x, y, bz - 0.16],
            id_quat(),
        ));
    }
    // The jib: a boom over the collar at the top, out past the pontoon,
    // a counterweight at its back end and a peak with two ties.
    let jib_y = top + 0.22;
    let (jib_front, jib_back) = (-9.6_f32, 1.8_f32);
    let jib_len = jib_back - jib_front;
    let jib = prim(
        solid(cuboid_tapered([0.5, 0.5, jib_len], 0.0, iron.clone())),
        [x, jib_y, (jib_front + jib_back) * 0.5],
        id_quat(),
    );
    let peak_y = jib_y + 3.2;
    let mut jib_parts = vec![
        prim(
            solid(cuboid_tapered(
                [1.2, 1.1, 1.4],
                0.0,
                concrete([0.36, 0.36, 0.38]),
            )),
            [x, jib_y - 0.75, jib_back - 0.8],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.36, 3.2, 0.36], 0.0, iron.clone())),
            [x, jib_y + 1.6, z],
            id_quat(),
        ),
        strut(
            [x, peak_y, z],
            [x, jib_y + 0.2, jib_front + 0.6],
            0.04,
            6,
            iron.clone(),
        ),
        strut(
            [x, peak_y, z],
            [x, jib_y + 0.2, jib_back - 0.3],
            0.04,
            6,
            iron.clone(),
        ),
        prim(
            sphere(0.26, 4, glow([1.0, 0.09, 0.07], 2.6)),
            [x, peak_y + 0.2, z],
            id_quat(),
        ),
    ];
    // The hanging sign out over the water, on two cables from a spreader
    // under the jib.
    let sign_y = jib_y - 3.6;
    let sign_z = jib_front + 1.4;
    jib_parts.push(prim(
        cuboid_tapered([2.2, 0.15, 0.2], 0.0, iron.clone()),
        [x, jib_y - 0.3, sign_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        jib_parts.push(prim(
            cylinder_tapered(0.03, 2.0, 6, 0.0, iron.clone()),
            [x + sx * 0.9, jib_y - 1.2, sign_z],
            id_quat(),
        ));
    }
    jib_parts.push(sign(
        [x, sign_y, sign_z],
        [2.6, 3.2],
        NEON_CYAN,
        NEON_MAGENTA,
        false,
    ));
    parts.push(nest(jib, jib_parts));
    nest(plate, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&NeonDock.build(""), "neon_dock");
    }

    #[test]
    fn nothing_z_fights_and_nothing_floats() {
        let built = NeonDock.build("");
        assert_no_z_fighting(&built, "neon_dock");
        assert_nothing_floats(&built, "neon_dock");
    }

    #[test]
    fn has_neon() {
        assert!(has_emissive(&NeonDock.build("")));
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&NeonDock.build(""))
            .expect("serialises");
        assert!(bytes < 70_000, "{bytes} B");
    }

    /// The crane stands tall: its jib tops 20 m.
    #[test]
    fn the_crane_stands_tall() {
        let top = crate::catalogue::items::measure::solids(&NeonDock.build(""))
            .iter()
            .map(|s| s.bounds.max.y)
            .fold(0.0_f32, f32::max);
        assert!(top > 20.0, "the tallest solid tops {top} m");
    }
}
