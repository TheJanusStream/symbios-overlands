//! Drowned abbey - a Gothic-Horror leader for the shore. A great abbey the
//! sea has taken back: a long roofless nave whose walls stand as tall
//! pointed windows open to the sky, a broken bell tower with its bell fallen
//! on the quay, masonry tumbled across a flooded floor, and at the water end
//! (-Z) the sea door, still lit by its lanterns.
//!
//! Where the ruined chapel is a village chapel's last gable, this is a
//! monastery's whole plan: the nave, the west front, the east window and the
//! tower all still stand, tide-stained to the sill.
//!
//! The pointed openings are framed, not cut: each wall is piers, haunch
//! blocks and a ragged top course built round pairs of voussoir arcs (see
//! [`pointed_wall`]).

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::coastal_resort::water;
use crate::catalogue::items::pirate::lantern;
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, id_quat, nest, prim,
    quat_mul, quat_x, quat_y, quat_z, solid, sphere, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp4, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, IRON_BLACK, STONE_DARK, fx, iron, mossy, pointed_arch, stone, wood};

pub struct DrownedAbbey;

/// Spacing circle: the footing's half diagonal.
const CLEARANCE: f32 = 15.0;
/// Centre line of the nave (x).
const NX: f32 = -2.2;
/// Half the nave's width, wall centre to wall centre.
const NAVE_HALF: f32 = 5.0;
/// West front (the sea front) and east wall centres (z), and their depth.
const WEST_Z: f32 = -9.0;
const EAST_Z: f32 = 12.0;
const END_DEPTH: f32 = 1.4;
/// Side walls' depth.
const SIDE_DEPTH: f32 = 1.1;
/// How far an arc stands proud of the wall built round it.
const PROUD: f32 = 0.04;
/// Tide-stained green of the lowest courses.
const TIDE_GREEN: [f32; 3] = [0.24, 0.31, 0.26];
/// Ribs of the arches, a shade paler than the walls.
const RIB_GREY: [f32; 3] = [0.50, 0.50, 0.52];
/// Brackish water standing on the nave floor.
const BRACKISH: [f32; 3] = [0.10, 0.17, 0.16];

impl CatalogueEntry for DrownedAbbey {
    fn slug(&self) -> &'static str {
        "drowned_abbey"
    }
    fn name(&self) -> &'static str {
        "Drowned Abbey"
    }
    fn description(&self) -> &'static str {
        "Roofless abbey at the water's edge: a nave of pointed arches, a broken bell tower and a lantern-lit sea door."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1796).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
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

fn wall_mat() -> SovereignMaterialSettings {
    stone(STONE_DARK)
}

/// One wall pierced by pointed openings, in its own frame: running along X
/// from `x_lo` to `x_hi`, its faces to -Z and +Z, standing on `base`.
struct PointedWall<'a> {
    x_lo: f32,
    x_hi: f32,
    /// Centres of the openings, left to right.
    openings: &'a [f32],
    /// Half span of every opening.
    hs: f32,
    /// Arc thickness and the wall's depth (the arcs' own; the wall is
    /// [`PROUD`] shy of them each side).
    t: f32,
    depth: f32,
    base: f32,
    spring: f32,
    /// The ragged top course's height over each pier, `openings.len() + 1`.
    tops: &'a [f32],
}

/// A [`PointedWall`]'s parts. Each opening is two arcs of the equilateral
/// pointed arch, each struck from the opposite springer, with a keystone
/// filling the notch where their cut ends meet. Between openings a pier
/// rises past the springing and haunch blocks climb beside the arcs, each
/// block kept inside the arc's band at the heights it spans - for a block
/// from `h0` to `h1` above the springing that holds while
/// `h1^2 - h0^2 <= Ro^2 - Ri^2` - so no block reaches into an opening and
/// each corner hides behind the proud arc. Above the apex the top course
/// stands to a different height over each pier.
fn pointed_wall(w: &PointedWall) -> Vec<Generator> {
    let wall = w.depth - 2.0 * PROUD;
    let ri = 2.0 * w.hs;
    let ro = ri + w.t;
    let apex = w.hs * 3.0_f32.sqrt();
    let step2 = (ro * ro - ri * ri) * 0.95;
    let mut hs = vec![0.0_f32];
    while *hs.last().expect("starts at zero") < apex {
        let h = *hs.last().expect("non-empty");
        hs.push((h * h + step2).sqrt());
    }
    let band_from = w.spring + *hs.last().expect("non-empty");
    let n = w.openings.len();
    // An opening's left edge as it rises: the arc struck from its right
    // springer, and its right edge, the arc struck from its left.
    let left_edge = |cx: f32, r: f32, h: f32| {
        if h >= apex {
            cx
        } else {
            (cx + w.hs - (r * r - h * h).max(0.0).sqrt()).min(cx)
        }
    };
    let right_edge = |cx: f32, r: f32, h: f32| {
        if h >= apex {
            cx
        } else {
            (cx - w.hs + (r * r - h * h).max(0.0).sqrt()).max(cx)
        }
    };

    let mut parts = Vec::new();
    for j in 0..=n {
        let a = if j == 0 {
            w.x_lo
        } else {
            w.openings[j - 1] + w.hs
        };
        let b = if j == n { w.x_hi } else { w.openings[j] - w.hs };
        let pier_top = w.spring + hs[1];
        let pier = prim(
            solid(cuboid_tapered(
                [b - a, pier_top - w.base, wall],
                0.0,
                wall_mat(),
            )),
            [(a + b) * 0.5, (w.base + pier_top) * 0.5, 0.0],
            id_quat(),
        );
        let mut haunches = Vec::new();
        for s in hs.windows(2).skip(1) {
            let (h0, h1) = (s[0], s[1]);
            let lo = if j == 0 {
                w.x_lo
            } else {
                let cx = w.openings[j - 1];
                (right_edge(cx, ro, h1) - 0.02).max(right_edge(cx, ri, h0) + 0.005)
            };
            let hi = if j == n {
                w.x_hi
            } else {
                let cx = w.openings[j];
                (left_edge(cx, ro, h1) + 0.02).min(left_edge(cx, ri, h0) - 0.005)
            };
            haunches.push(prim(
                solid(cuboid_tapered([hi - lo, h1 - h0, wall], 0.0, wall_mat())),
                [(lo + hi) * 0.5, w.spring + (h0 + h1) * 0.5, 0.0],
                id_quat(),
            ));
        }
        // The top course over this pier, from the middle of the opening on
        // its left to the middle of the one on its right.
        let ta = if j == 0 { w.x_lo } else { w.openings[j - 1] };
        let tb = if j == n { w.x_hi } else { w.openings[j] };
        let top = w.tops[j];
        // Every other stretch crumbles to a narrower top.
        let crumble = if j % 2 == 1 { 0.28 } else { 0.0 };
        haunches.push(prim(
            solid(cuboid_tapered_xz(
                [tb - ta, top - band_from, wall],
                [crumble, 0.0],
                wall_mat(),
            )),
            [(ta + tb) * 0.5, (band_from + top) * 0.5, 0.0],
            id_quat(),
        ));
        parts.push(nest(pier, haunches));
    }
    for &cx in w.openings {
        let rib = stone(RIB_GREY);
        parts.push(prim(
            solid(with_cut(
                tube(ro, ri, w.depth, 32, rib.clone()),
                [0.0, 1.0 / 6.0],
                [0.0, 1.0],
                0.0,
            )),
            [cx - w.hs, w.spring, 0.0],
            quat_x(-FRAC_PI_2),
        ));
        // The left arc a shade deeper, so where the two overlap at the apex
        // their faces are not in one plane.
        parts.push(prim(
            solid(with_cut(
                tube(ro, ri, w.depth + 0.03, 32, rib.clone()),
                [1.0 / 3.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [cx + w.hs, w.spring, 0.0],
            quat_x(-FRAC_PI_2),
        ));
        let ks_lo = w.spring + apex;
        let ks_hi = band_from.max(ks_lo + w.t * 0.9) + 0.05;
        parts.push(prim(
            solid(cuboid_tapered(
                [w.t, ks_hi - ks_lo, w.depth + 0.015],
                0.0,
                rib,
            )),
            [cx, (ks_lo + ks_hi) * 0.5, 0.0],
            id_quat(),
        ));
    }
    parts
}

/// A tide-stained course along the foot of a wall segment, proud of it on
/// every side, its ends included.
fn tide_course(x_lo: f32, x_hi: f32, h: f32, depth: f32) -> Generator {
    prim(
        solid(cuboid_tapered(
            [x_hi - x_lo + 0.12, h, depth - 2.0 * PROUD + 0.12],
            0.0,
            mossy(TIDE_GREEN),
        )),
        [(x_lo + x_hi) * 0.5, h * 0.5, 0.0],
        id_quat(),
    )
}

/// Stand a wall built in its own frame (`parts`, the first its root) at
/// `at`, turned by `yaw` about its frame's origin.
fn stand(mut parts: Vec<Generator>, at: [f32; 3], yaw: f32) -> Generator {
    let root = parts.remove(0);
    let mut g = nest(root, parts);
    let [x, y, z] = g.transform.translation.0;
    let (s, c) = yaw.sin_cos();
    g.transform.translation.0 = [at[0] + x * c + z * s, at[1] + y, at[2] - x * s + z * c];
    g.transform.rotation = quat_y(yaw);
    g
}

/// A side wall of the nave: a tide course the length of it as the sill, and
/// five tall pointed windows above.
fn side_wall(x: f32, tops: &[f32]) -> Generator {
    let (z_lo, z_hi) = (
        WEST_Z + END_DEPTH * 0.5 - 0.12,
        EAST_Z - END_DEPTH * 0.5 + 0.12,
    );
    let half = (z_hi - z_lo) * 0.5;
    let pier = 1.1;
    let bay = (2.0 * half - pier) / 5.0;
    let openings: Vec<f32> = (0..5)
        .map(|k| -half + pier * 0.5 + bay * (k as f32 + 0.5))
        .collect();
    let sill = 1.0;
    let mut parts = vec![tide_course(-half, half, sill, SIDE_DEPTH)];
    parts.extend(pointed_wall(&PointedWall {
        x_lo: -half,
        x_hi: half,
        openings: &openings,
        hs: (bay - pier) * 0.5,
        t: 0.45,
        depth: SIDE_DEPTH,
        base: sill,
        spring: 5.2,
        tops,
    }));
    // Turned so the wall runs along Z; its own +X is the world's +Z.
    stand(parts, [x, 0.0, (z_lo + z_hi) * 0.5], -FRAC_PI_2)
}

/// An iron bracket off a wall face at `at` (the face point), reaching to -Z,
/// with a lantern hung from its end.
fn bracket_lantern(at: [f32; 3], seed: u32) -> Vec<Generator> {
    let [x, y, z] = at;
    let reach = 0.75;
    let arm = prim(
        solid(cuboid_tapered(
            [0.08, 0.08, reach + 0.05],
            0.0,
            iron(IRON_BLACK),
        )),
        [x, y, z - reach * 0.5 + 0.025],
        id_quat(),
    );
    let h = 0.6;
    let lamp_y = y - 0.6;
    let ring_top = lamp_y + h * 0.5 + 0.015;
    let chain_top = y - 0.04 + 0.02;
    let chain = prim(
        cylinder_tapered(0.02, chain_top - ring_top + 0.02, 6, 0.0, iron(IRON_BLACK)),
        [x, (chain_top + ring_top) * 0.5, z - reach + 0.04],
        id_quat(),
    );
    vec![arm, chain, lantern([x, lamp_y, z - reach + 0.04], h, seed)]
}

/// The west front, to the sea: a tide course, the great sea door with
/// lanterns either side and one hung in its arch, a pointed window above
/// with its broken tracery, a truncated gable, and one door leaf hanging
/// open.
fn west_front() -> Generator {
    let half = 5.8;
    let door_hs = 1.6;
    let mut parts = vec![tide_course(-half, -door_hs + 0.06, 1.0, END_DEPTH)];
    parts.push(tide_course(door_hs - 0.06, half, 1.0, END_DEPTH));
    let lower_top = 7.0;
    let door_spring = 3.2;
    parts.extend(pointed_wall(&PointedWall {
        x_lo: -half,
        x_hi: half,
        openings: &[0.0],
        hs: door_hs,
        t: 0.5,
        depth: END_DEPTH,
        base: 0.0,
        spring: door_spring,
        tops: &[lower_top, lower_top],
    }));
    let win_hs = 1.1;
    let win_spring = lower_top + 1.2;
    let upper_top = 11.4;
    parts.extend(pointed_wall(&PointedWall {
        x_lo: -half,
        x_hi: half,
        openings: &[0.0],
        hs: win_hs,
        t: 0.4,
        depth: END_DEPTH,
        base: lower_top,
        spring: win_spring,
        tops: &[upper_top, upper_top],
    }));
    // The gable, broken off flat.
    parts.push(prim(
        solid(cuboid_tapered_xz(
            [2.0 * half, 3.2, END_DEPTH - 2.0 * PROUD],
            [0.58, 0.0],
            wall_mat(),
        )),
        [0.0, upper_top + 1.6, 0.0],
        id_quat(),
    ));
    // Broken tracery: a mullion carrying a ring in the west window.
    let ring_y = win_spring + 0.6;
    let ring_r = 0.4;
    let mullion_top = ring_y - ring_r - 0.04;
    parts.push(prim(
        solid(cuboid_tapered(
            [0.18, mullion_top - lower_top, 0.3],
            0.0,
            stone(RIB_GREY),
        )),
        [0.0, (lower_top + mullion_top) * 0.5, 0.0],
        id_quat(),
    ));
    parts.push(prim(
        torus(0.07, ring_r, stone(RIB_GREY)),
        [0.0, ring_y, 0.0],
        quat_x(-FRAC_PI_2),
    ));
    // Lanterns either side of the sea door, and one in its arch.
    let face = -END_DEPTH * 0.5 + PROUD;
    for (k, s) in [-1.0_f32, 1.0].into_iter().enumerate() {
        parts.extend(bracket_lantern(
            [s * (door_hs + 0.85), 3.9, face],
            0x5EA0 + k as u32,
        ));
    }
    let door_apex = door_spring + door_hs * 3.0_f32.sqrt();
    let hang_lamp = door_apex - 1.35;
    parts.push(prim(
        cylinder_tapered(
            0.02,
            door_apex + 0.05 - (hang_lamp + 0.33),
            6,
            0.0,
            iron(IRON_BLACK),
        ),
        [0.0, (door_apex + 0.05 + hang_lamp + 0.33) * 0.5, -0.3],
        id_quat(),
    ));
    parts.push(lantern([0.0, hang_lamp, -0.3], 0.6, 0x5EA2));
    // One leaf of the sea door, hung from the left jamb and swung in.
    let swing = 1.0_f32;
    let (s, c) = swing.sin_cos();
    let hinge = [-door_hs - 0.02, 0.1];
    parts.push(prim(
        solid(cuboid_tapered([1.5, 3.0, 0.12], 0.0, wood(DEADWOOD))),
        [hinge[0] + 0.75 * c, 0.17 + 1.5, hinge[1] + 0.75 * s],
        quat_y(-swing),
    ));
    stand(parts, [NX, 0.0, WEST_Z], 0.0)
}

/// The east wall: a blind lower wall, then the great east window with two
/// mullions and a ring of tracery, its top broken unevenly.
fn east_wall() -> Generator {
    let half = 5.8;
    let sill = 3.2;
    let mut parts = vec![tide_course(-half, half, 1.0, END_DEPTH)];
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * half, sill, END_DEPTH - 2.0 * PROUD],
            0.0,
            wall_mat(),
        )),
        [0.0, sill * 0.5, 0.0],
        id_quat(),
    ));
    let spring = sill + 3.0;
    parts.extend(pointed_wall(&PointedWall {
        x_lo: -half,
        x_hi: half,
        openings: &[0.0],
        hs: 1.7,
        t: 0.45,
        depth: END_DEPTH,
        base: sill,
        spring,
        tops: &[10.6, 9.7],
    }));
    let ring_y = 8.3;
    let ring_r = 0.6;
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.16, ring_y - sill, 0.3],
                0.0,
                stone(RIB_GREY),
            )),
            [s * ring_r, (sill + ring_y) * 0.5, 0.0],
            id_quat(),
        ));
    }
    parts.push(prim(
        torus(0.08, ring_r, stone(RIB_GREY)),
        [0.0, ring_y, 0.0],
        quat_x(-FRAC_PI_2),
    ));
    stand(parts, [NX, 0.0, EAST_Z], 0.0)
}

/// A pointed belfry arch on a wall facing +X: [`pointed_arch`]'s two arcs
/// turned a quarter about Y, struck along Z.
fn pointed_arch_x(spring: [f32; 3], half_span: f32, thick: f32) -> [Generator; 2] {
    let [x, y, z] = spring;
    let r = 2.0 * half_span;
    let turn: Fp4 = quat_mul(quat_y(-FRAC_PI_2), quat_x(-FRAC_PI_2));
    [
        prim(
            with_cut(
                torus(thick, r, stone(RIB_GREY)),
                [0.0, 1.0 / 6.0],
                [0.0, 1.0],
                0.0,
            ),
            [x, y, z - half_span],
            turn,
        ),
        prim(
            with_cut(
                torus(thick, r, stone(RIB_GREY)),
                [1.0 / 3.0, 0.5],
                [0.0, 1.0],
                0.0,
            ),
            [x, y, z + half_span],
            turn,
        ),
    ]
}

/// The bell tower at the sea front's +X corner, broken off at the belfry:
/// two corner stubs and a stretch of parapet still stand above it.
fn bell_tower() -> Generator {
    let x0 = NX + NAVE_HALF + 0.5;
    let (z0, z1) = (-9.9, -4.9);
    let w = 5.0;
    let cx = x0 + w * 0.5;
    let cz = (z0 + z1) * 0.5;
    let body_h = 13.0;
    let mut parts = vec![prim(
        solid(cuboid_tapered([w, body_h, w], 0.0, wall_mat())),
        [cx, body_h * 0.5, cz],
        id_quat(),
    )];
    for y in [5.0_f32, 9.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [w + 0.2, 0.25, w + 0.2],
                0.0,
                stone(RIB_GREY),
            )),
            [cx, y, cz],
            id_quat(),
        ));
    }
    // Broken top: two corner stubs and a piece of parapet.
    for (size, at) in [
        ([1.3_f32, 3.3, 1.3], [x0 + w - 0.65, z0 + 0.65]),
        ([1.3, 1.6, 2.4], [x0 + 0.65, z1 - 1.2]),
        ([1.8, 0.9, 0.5], [cx - 0.2, z0 + 0.25]),
    ] {
        parts.push(prim(
            solid(cuboid_tapered(size, 0.0, wall_mat())),
            [at[0], body_h + size[1] * 0.5, at[1]],
            id_quat(),
        ));
    }
    // Belfry openings on the sea face and the +X face: a dark void, louvres,
    // and a pointed arch over each.
    let void = super::matte([0.03, 0.03, 0.04]);
    parts.push(prim(
        cuboid_tapered([1.4, 1.6, 0.1], 0.0, void.clone()),
        [cx, 10.8, z0 - 0.03],
        id_quat(),
    ));
    parts.extend(pointed_arch(
        [cx, 11.6, z0 - 0.06],
        0.7,
        0.09,
        stone(RIB_GREY),
    ));
    parts.push(prim(
        cuboid_tapered([0.1, 1.6, 1.4], 0.0, void),
        [x0 + w + 0.03, 10.8, cz],
        id_quat(),
    ));
    parts.extend(pointed_arch_x([x0 + w + 0.06, 11.6, cz], 0.7, 0.09));
    for k in 0..4 {
        let y = 10.2 + k as f32 * 0.38;
        parts.push(prim(
            cuboid_tapered([1.3, 0.06, 0.08], 0.0, wood(DEADWOOD)),
            [cx, y, z0 - 0.1],
            id_quat(),
        ));
        parts.push(prim(
            cuboid_tapered([0.08, 0.06, 1.3], 0.0, wood(DEADWOOD)),
            [x0 + w + 0.1, y, cz],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered(
                [w + 0.24, 1.2, w + 0.24],
                0.0,
                mossy(TIDE_GREEN),
            )),
            [cx, 0.6, cz],
            id_quat(),
        ),
        parts,
    )
}

/// The nave floor, flagged and flooded in pools, with the masonry that
/// fell on it.
fn nave_floor() -> Generator {
    let half_x = NAVE_HALF - (SIDE_DEPTH - 2.0 * PROUD) * 0.5;
    let (z_lo, z_hi) = (
        WEST_Z + (END_DEPTH - 2.0 * PROUD) * 0.5,
        EAST_Z - (END_DEPTH - 2.0 * PROUD) * 0.5,
    );
    let floor_top = 0.2;
    let mut parts = Vec::new();
    for (x, z, r) in [
        (-1.5_f32, 2.0_f32, 1.6_f32),
        (-4.0, 7.0, 1.0),
        (0.5, -5.0, 1.2),
    ] {
        parts.push(prim(
            cylinder_tapered(r, 0.04, 20, 0.0, water(BRACKISH)),
            [x, floor_top + 0.02, z],
            id_quat(),
        ));
    }
    // Fallen blocks, lying where they landed.
    for (size, at, yaw) in [
        ([1.2_f32, 0.6, 0.8], [-4.8_f32, 3.0_f32], 0.4_f32),
        ([0.9, 0.5, 0.7], [-0.2, 6.5], 1.1),
        ([1.6, 0.4, 0.9], [-3.0, -3.5], -0.3),
        ([0.7, 0.45, 0.6], [1.2, 9.6], 0.8),
    ] {
        parts.push(prim(
            solid(cuboid_tapered(size, 0.0, wall_mat())),
            [at[0], floor_top + size[1] * 0.5, at[1]],
            quat_y(yaw),
        ));
    }
    // A column drum on its side, and a length of fallen arch lying flat.
    parts.push(prim(
        solid(cylinder_tapered(0.45, 1.2, 14, 0.0, stone(RIB_GREY))),
        [0.8, floor_top + 0.43, 0.5],
        quat_z(FRAC_PI_2),
    ));
    parts.push(prim(
        solid(with_cut(
            tube(3.0, 2.55, 0.45, 24, stone(RIB_GREY)),
            [0.0, 0.12],
            [0.0, 1.0],
            0.0,
        )),
        [-5.0, floor_top + 0.225, 7.6],
        id_quat(),
    ));
    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * half_x, floor_top, z_hi - z_lo],
                0.0,
                mossy([0.34, 0.36, 0.33]),
            )),
            [NX, floor_top * 0.5, (z_lo + z_hi) * 0.5],
            id_quat(),
        ),
        parts,
    )
}

/// The landing quay before the sea door: two mooring posts, a tumbled
/// block and the tower's bell, fallen and lying on its side.
fn quay() -> Generator {
    let top = 0.3;
    let (z_lo, z_hi) = (-12.2, WEST_Z - END_DEPTH * 0.5 + 0.06);
    let (x_lo, x_hi) = (NX - 5.5, NX + NAVE_HALF + 0.5);
    let mut parts = Vec::new();
    for x in [NX - 4.5, NX + 3.2] {
        parts.push(prim(
            solid(cylinder_tapered(0.18, 0.9, 10, 0.1, wood(DEADWOOD))),
            [x, top + 0.45, -11.75],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([0.9, 0.5, 0.6], 0.0, wall_mat())),
        [NX - 2.4, top + 0.25, -11.2],
        quat_y(0.7),
    ));
    // The bell: a flared waist, its crown and the lip of its mouth, lying
    // mouth to +X on the quay.
    let bell = iron([0.30, 0.27, 0.20]);
    let (bx, bz) = (NX + 4.4, -11.0);
    let mouth = 0.7;
    let by = top + mouth - 0.03;
    parts.push(prim(
        solid(cylinder_tapered(mouth, 1.1, 18, 0.45, bell.clone())),
        [bx, by, bz],
        quat_z(FRAC_PI_2),
    ));
    parts.push(prim(
        sphere(mouth * 0.55, 4, bell.clone()),
        [bx - 0.55, by, bz],
        id_quat(),
    ));
    parts.push(prim(
        torus(0.07, mouth - 0.02, bell),
        [bx + 0.55, by, bz],
        quat_z(FRAC_PI_2),
    ));
    nest(
        prim(
            solid(cuboid_tapered(
                [x_hi - x_lo, top, z_hi - z_lo],
                0.0,
                mossy(TIDE_GREEN),
            )),
            [(x_lo + x_hi) * 0.5, top * 0.5, (z_lo + z_hi) * 0.5],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let mut root = footing(16.8, 25.2, [0.2, 0.2], CLEARANCE);
    attach(&mut root, quay());
    attach(&mut root, nave_floor());
    attach(&mut root, west_front());
    attach(&mut root, east_wall());
    attach(
        &mut root,
        side_wall(NX - NAVE_HALF, &[10.2, 8.8, 10.8, 8.5, 9.6, 11.0]),
    );
    attach(
        &mut root,
        side_wall(NX + NAVE_HALF, &[10.6, 9.6, 8.6, 9.2, 10.8, 9.8]),
    );
    attach(&mut root, bell_tower());
    attach(&mut root, fx::ground_mist([NX, 0.6, 1.5], 0xAB_BE70));
    root.audio = fx::cold_wind();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&DrownedAbbey.build(""), "drowned_abbey");
    }

    #[test]
    fn nothing_z_fights() {
        assert_no_z_fighting(&DrownedAbbey.build(""), "drowned_abbey");
    }

    #[test]
    fn nothing_floats() {
        assert_nothing_floats(&DrownedAbbey.build(""), "drowned_abbey");
    }

    #[test]
    fn its_sea_door_is_lit() {
        assert!(has_emissive(&DrownedAbbey.build("")));
    }

    #[test]
    fn record_stays_inside_the_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&DrownedAbbey.build(""))
            .expect("serialises");
        eprintln!("drowned_abbey record: {bytes} B");
        assert!(bytes < 80 * 1024, "{bytes} B");
    }
}
