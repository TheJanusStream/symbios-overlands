//! Watermill - a Rural Farmland leader for the shore. A two-storey
//! fieldstone mill house with a big timber overshot wheel turning on its
//! -Z side, fed from above by a timber flume on trestles with a sluice gate
//! in it, the spent water running off down a walled tailrace; a boarded
//! grain store wing with a hoist at its gable, sacks at its door and a pair
//! of spare millstones lying by.
//!
//! The wheel turns: its hub is the pivot (see `util::turning`), and the
//! rims, spokes, sole and buckets hang off it.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, nest,
    plane, prim, quat_x, quat_z, solid, steady, strut, superellipsoid, torus, tube, turning,
    window_card, with_cut, with_face,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::generator::FaceKey;
use crate::pds::{Generator, GeneratorKind, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    HAY_GOLD, LAMP_WARM, ROOF_GREY, STONE_GREY, TRIM_WHITE, WOOD_GREY, barn_board, fx, shingle,
    stone, weathered,
};

pub struct Watermill;

/// Spacing circle: the footing's half diagonal.
const CLEARANCE: f32 = 12.6;
/// The mill house: centre, plan and wall height.
const MILL_X: f32 = 2.0;
const MILL_Z: f32 = 1.0;
const MILL_W: f32 = 9.0;
const MILL_D: f32 = 8.0;
const MILL_H: f32 = 7.0;
/// The wheel: radius, width, and its centre in front of the -Z wall.
const WHEEL_R: f32 = 2.6;
const WHEEL_W: f32 = 1.2;
const WHEEL_X: f32 = MILL_X;
const WHEEL_Y: f32 = 2.9;
const WHEEL_Z: f32 = MILL_Z - MILL_D * 0.5 - 0.9;
/// The flume's floor, above the wheel's crown.
const FLUME_FLOOR: f32 = WHEEL_Y + WHEEL_R + 0.12;
/// The mill race's water.
const RACE: [f32; 3] = [0.16, 0.38, 0.40];
/// Dark oak of the wheel and the flume.
const OAK: [f32; 3] = [0.34, 0.25, 0.16];

impl CatalogueEntry for Watermill {
    fn slug(&self) -> &'static str {
        "watermill"
    }
    fn name(&self) -> &'static str {
        "Watermill"
    }
    fn description(&self) -> &'static str {
        "Stone mill house with a turning overshot wheel fed by a flume on trestles, and a timber grain store."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1796).
    fn leads(&self) -> bool {
        true
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::RuralFarmland]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FARM_BAND
    }
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 35.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A roof prism whose two gable ends wear `gable` instead of the roofing.
fn gabled(
    kind: GeneratorKind,
    ends: [FaceKey; 2],
    gable: SovereignMaterialSettings,
) -> GeneratorKind {
    ends.into_iter()
        .fold(kind, |k, f| with_face(k, f, gable.clone()))
}

fn water() -> crate::pds::SovereignMaterialSettings {
    crate::catalogue::items::coastal_resort::water(RACE)
}

fn oak() -> crate::pds::SovereignMaterialSettings {
    weathered(OAK)
}

/// A lit window on the mill's -Z face (`zf`): the warm room behind a
/// white-framed card, under a stone lintel.
fn window(x: f32, sill: f32, zf: f32) -> Vec<Generator> {
    let (w, h) = (1.0, 1.3);
    vec![
        prim(
            solid(cuboid_tapered(
                [w, h, 0.1],
                0.0,
                lit_from_dusk(glow(LAMP_WARM, 1.8)),
            )),
            [x, sill + h * 0.5, zf - 0.03],
            id_quat(),
        ),
        prim(
            plane(
                [w - 0.04, h - 0.04],
                window_card(TRIM_WHITE, 2, 3, 0.35, 0.07),
            ),
            [x, sill + h * 0.5, zf - 0.1],
            quat_x(-FRAC_PI_2),
        ),
        prim(
            solid(cuboid_tapered([w + 0.3, 0.22, 0.2], 0.0, stone(TRIM_WHITE))),
            [x, sill + h + 0.11, zf - 0.08],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([w + 0.2, 0.1, 0.24], 0.0, stone(TRIM_WHITE))),
            [x, sill - 0.05, zf - 0.1],
            id_quat(),
        ),
    ]
}

/// The mill house: stone walls with a string course between its floors,
/// windows and a door to -Z, a shingled gable roof and a chimney with smoke.
fn mill_house() -> Generator {
    let zf = MILL_Z - MILL_D * 0.5;
    let mut parts = vec![prim(
        solid(cuboid_tapered(
            [MILL_W + 0.2, 0.12, MILL_D + 0.2],
            0.0,
            stone(TRIM_WHITE),
        )),
        [MILL_X, 3.6, MILL_Z],
        id_quat(),
    )];
    parts.extend(window(MILL_X + 3.4, 4.2, zf));
    parts.extend(window(MILL_X - 3.6, 4.2, zf));
    parts.extend(window(MILL_X - 3.6, 1.1, zf));
    // The door at the -Z face's +X end, oak under a stone lintel.
    let door_x = MILL_X + 3.4;
    parts.push(prim(
        solid(cuboid_tapered([1.2, 2.3, 0.12], 0.0, barn_board(OAK))),
        [door_x, 1.15, zf - 0.05],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered([1.6, 0.25, 0.22], 0.0, stone(TRIM_WHITE))),
        [door_x, 2.43, zf - 0.09],
        id_quat(),
    ));
    // Roof, ridge along X, and its chimney at the +X end.
    let rise = 3.4;
    // Its gable triangles are the roof prism's own ends, faced in stone.
    parts.push(prim(
        solid(gabled(
            cuboid_tapered_xz(
                [MILL_W + 0.1, rise, MILL_D + 1.0],
                [0.0, 0.94],
                shingle(ROOF_GREY),
            ),
            [FaceKey::SidePx, FaceKey::SideNx],
            stone(STONE_GREY),
        )),
        [MILL_X, MILL_H + rise * 0.5, MILL_Z],
        id_quat(),
    ));
    let chim = [MILL_X + MILL_W * 0.5 - 1.2, MILL_Z + 1.2];
    let chim_top = MILL_H + rise + 0.9;
    parts.push(prim(
        solid(cuboid_tapered(
            [0.8, chim_top - MILL_H, 0.8],
            0.0,
            stone(STONE_GREY),
        )),
        [chim[0], (MILL_H + chim_top) * 0.5, chim[1]],
        id_quat(),
    ));
    parts.push(fx::chimney_smoke(
        [chim[0], chim_top + 0.1, chim[1]],
        0x3A11,
    ));
    nest(
        prim(
            solid(cuboid_tapered(
                [MILL_W, MILL_H, MILL_D],
                0.0,
                stone(STONE_GREY),
            )),
            [MILL_X, MILL_H * 0.5, MILL_Z],
            id_quat(),
        ),
        parts,
    )
}

/// The overshot wheel, turning on its hub: two rims, the sole drum the
/// buckets are nailed to, sixteen buckets and eight spokes a side, on an
/// axle run back into the mill wall.
fn wheel() -> Generator {
    let (x, y, z) = (WHEEL_X, WHEEL_Y, WHEEL_Z);
    let sole_r = WHEEL_R - 0.5;
    let half = WHEEL_W * 0.5;
    let mut parts = Vec::new();
    for s in [-1.0_f32, 1.0] {
        let rz = z + s * (half - 0.05);
        parts.push(prim(
            torus(0.09, WHEEL_R, oak()),
            [x, y, rz],
            quat_x(FRAC_PI_2),
        ));
        for k in 0..4 {
            parts.push(prim(
                cuboid_tapered([WHEEL_R * 2.0, 0.14, 0.1], 0.0, oak()),
                [x, y, rz],
                quat_z(k as f32 * PI / 4.0 + PI / 8.0),
            ));
        }
    }
    parts.push(prim(
        tube(sole_r, sole_r - 0.12, WHEEL_W - 0.1, 32, oak()),
        [x, y, z],
        quat_x(FRAC_PI_2),
    ));
    for k in 0..16 {
        let a = k as f32 * PI / 8.0;
        let rb = (sole_r + WHEEL_R) * 0.5;
        // Each bucket board a little off radial, cupped to hold the water.
        parts.push(prim(
            cuboid_tapered([0.06, WHEEL_R - sole_r + 0.04, WHEEL_W - 0.12], 0.0, oak()),
            [x + a.cos() * rb, y + a.sin() * rb, z],
            quat_z(a - FRAC_PI_2 + 0.35),
        ));
    }
    turning(
        prim(
            cuboid_tapered([0.6, 0.6, WHEEL_W + 0.06], 0.0, oak()),
            [x, y, z],
            id_quat(),
        ),
        [0.0, 0.0, 1.0],
        steady(-12.0),
        parts,
    )
}

/// The wheel's axle and its bearing block on the mill wall - still while
/// the wheel turns on it.
fn axle() -> Generator {
    let zf = MILL_Z - MILL_D * 0.5;
    let len = zf - (WHEEL_Z - WHEEL_W * 0.5 - 0.2) + 0.1;
    nest(
        prim(
            solid(cuboid_tapered([0.8, 0.8, 0.3], 0.0, stone(TRIM_WHITE))),
            [WHEEL_X, WHEEL_Y, zf - 0.1],
            id_quat(),
        ),
        vec![prim(
            solid(cylinder_tapered(
                0.14,
                len,
                10,
                0.0,
                crate::catalogue::items::util::foundation_mat(),
            )),
            [WHEEL_X, WHEEL_Y, zf + 0.1 - len * 0.5],
            quat_x(FRAC_PI_2),
        )],
    )
}

/// The flume: a timber trough on three trestles bringing the water in from
/// -X over the wheel's crown, a sluice gate standing in it, and the spill
/// falling onto the buckets.
fn flume() -> Generator {
    let z = WHEEL_Z;
    let x_end = WHEEL_X + 0.35;
    let x_lo = -10.2;
    let len = x_end - x_lo;
    let cx = (x_lo + x_end) * 0.5;
    let floor_t = 0.1;
    let side_h = 0.6;
    let inner = 0.84;
    let mut parts = Vec::new();
    // Side boards hanging a little below the floor they are nailed to, and
    // an end board closing the trough's far end.
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([len + 0.04, side_h, 0.08], 0.0, oak())),
            [
                cx,
                FLUME_FLOOR - 0.03 + side_h * 0.5,
                z + s * (inner * 0.5 + 0.04),
            ],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([0.08, side_h - 0.1, inner], 0.0, oak())),
        [x_lo + 0.06, FLUME_FLOOR + floor_t + (side_h - 0.1) * 0.5, z],
        id_quat(),
    ));
    let (w_lo, w_hi) = (x_lo + 0.1, x_end - 0.03);
    parts.push(prim(
        cuboid_tapered([w_hi - w_lo, 0.32, inner], 0.0, water()),
        [(w_lo + w_hi) * 0.5, FLUME_FLOOR + floor_t * 0.5 + 0.16, z],
        id_quat(),
    ));
    // The spill off the end, onto the buckets past the crown.
    let crown = WHEEL_Y + WHEEL_R;
    parts.push(prim(
        cuboid_tapered(
            [0.28, FLUME_FLOOR + 0.3 - crown + 0.1, inner - 0.1],
            0.0,
            water(),
        ),
        [x_end + 0.12, (FLUME_FLOOR + 0.3 + crown - 0.1) * 0.5, z],
        id_quat(),
    ));
    // The sluice gate: a board across the trough between two posts, a
    // cross head and the lifting bar.
    let gx = x_end - 2.2;
    let post_h = 1.5;
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.14, post_h, 0.14], 0.0, oak())),
            [gx, FLUME_FLOOR + post_h * 0.5, z + s * (inner * 0.5 + 0.15)],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([0.18, 0.16, inner + 0.5], 0.0, oak())),
        [gx, FLUME_FLOOR + post_h + 0.08, z],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered([0.08, 0.7, inner], 0.0, barn_board(OAK))),
        [gx, FLUME_FLOOR + floor_t + 0.5, z],
        id_quat(),
    ));
    parts.push(prim(
        cylinder_tapered(0.03, 0.75, 6, 0.0, oak()),
        [gx, FLUME_FLOOR + post_h - 0.2, z],
        id_quat(),
    ));
    // Three trestles: splayed legs, a cap under the trough and a brace.
    let mut trestles = Vec::new();
    for tx in [-8.8_f32, -6.0, -3.2] {
        let cap_y = FLUME_FLOOR - 0.1;
        let legs: Vec<Generator> = [-1.0_f32, 1.0]
            .into_iter()
            .map(|s| {
                strut(
                    [tx, 0.1, z + s * 1.1],
                    [tx, cap_y, z + s * 0.4],
                    0.09,
                    6,
                    oak(),
                )
            })
            .collect();
        let mut t = vec![prim(
            solid(cuboid_tapered([0.2, 0.2, 1.3], 0.0, oak())),
            [tx, cap_y + 0.1, z],
            id_quat(),
        )];
        t.extend(legs);
        let mid = FLUME_FLOOR * 0.45;
        let at = |y: f32| 1.1 - 0.7 * (y - 0.1) / (cap_y - 0.1);
        t.push(strut(
            [tx, mid, z - at(mid)],
            [tx, mid, z + at(mid)],
            0.06,
            6,
            oak(),
        ));
        trestles.push(t);
    }
    // The trough's floor is the root; the trestles stand under it.
    let mut all = parts;
    for t in trestles {
        all.extend(t);
    }
    nest(
        prim(
            solid(cuboid_tapered([len, floor_t, inner + 0.12], 0.0, oak())),
            [cx, FLUME_FLOOR + floor_t * 0.5, z],
            id_quat(),
        ),
        all,
    )
}

/// The tailrace: the water under the wheel, walled on its open side, running
/// off along the mill's face.
fn tailrace() -> Generator {
    let zf = MILL_Z - MILL_D * 0.5;
    let wall_z = WHEEL_Z - WHEEL_W * 0.5 - 0.35;
    // Stops short of the mill door at the face's +X end.
    let (x_lo, x_hi) = (WHEEL_X - WHEEL_R - 0.8, WHEEL_X + WHEEL_R + 0.2);
    let len = x_hi - x_lo;
    let cx = (x_lo + x_hi) * 0.5;
    let wall = prim(
        solid(cuboid_tapered([len, 0.9, 0.3], 0.0, stone(STONE_GREY))),
        [cx, 0.45, wall_z],
        id_quat(),
    );
    let w_lo = wall_z + 0.15;
    nest(
        wall,
        vec![prim(
            cuboid_tapered([len, 0.2, zf - w_lo], 0.0, water()),
            [cx, 0.25, (w_lo + zf) * 0.5],
            id_quat(),
        )],
    )
}

/// The grain store wing at +X: boarded walls, its gable to the front with a
/// loft door and a hoist beam, double doors below with sacks before them.
fn grain_store() -> Generator {
    let x0 = MILL_X + MILL_W * 0.5;
    let (w, d, h) = (5.5, 6.0, 4.0);
    let cx = x0 + w * 0.5 - 0.1;
    let cz = MILL_Z - 0.6;
    let zf = cz - d * 0.5;
    let boards = barn_board(WOOD_GREY);
    let rise = 2.6;
    let mut parts = vec![
        // Its roof, ridge along Z so the gable faces the river.
        // Its gable triangles boarded like the walls below them.
        prim(
            solid(gabled(
                cuboid_tapered_xz([w + 0.6, rise, d + 0.1], [0.94, 0.0], shingle(ROOF_GREY)),
                [FaceKey::SidePz, FaceKey::SideNz],
                boards.clone(),
            )),
            [cx, h + rise * 0.5, cz],
            id_quat(),
        ),
    ];
    // Double doors, the loft door over them and the hoist beam.
    for s in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([1.1, 2.6, 0.1], 0.0, barn_board(OAK))),
            [cx + s * 0.57, 1.3, zf - 0.05],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([1.1, 1.2, 0.1], 0.0, barn_board(OAK))),
        [cx, h + 0.5, zf - 0.05],
        id_quat(),
    ));
    let beam_y = h + rise - 0.55;
    parts.push(prim(
        solid(cuboid_tapered([0.2, 0.2, 1.6], 0.0, oak())),
        [cx, beam_y, zf - 0.6],
        id_quat(),
    ));
    parts.push(prim(
        cylinder_tapered(0.02, 2.2, 5, 0.0, weathered([0.60, 0.50, 0.34])),
        [cx, beam_y - 1.2, zf - 1.3],
        id_quat(),
    ));
    parts.push(prim(
        solid(superellipsoid(
            [0.3, 0.38, 0.22],
            0.5,
            0.6,
            barn_board(HAY_GOLD),
        )),
        [cx, beam_y - 2.65, zf - 1.3],
        id_quat(),
    ));
    // Sacks of grain by the doors.
    for (dx, dz, up) in [
        (-1.6_f32, -0.6_f32, 0.0_f32),
        (-1.0, -0.8, 0.0),
        (-1.3, -0.7, 0.62),
    ] {
        parts.push(prim(
            solid(superellipsoid(
                [0.32, 0.31, 0.24],
                0.5,
                0.6,
                barn_board(HAY_GOLD),
            )),
            [cx + dx, 0.15 + 0.31 + up, zf + dz],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered([w, h, d], 0.0, boards)),
            [cx, h * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// Two spare millstones lying one on the other by the mill door.
fn millstones() -> Generator {
    let (x, z) = (MILL_X + 6.2, MILL_Z - MILL_D * 0.5 - 2.4);
    let t = 0.28;
    let stone_mat = stone([0.62, 0.60, 0.56]);
    nest(
        prim(
            solid(with_cut(
                cylinder_tapered(0.75, t, 20, 0.0, stone_mat.clone()),
                [0.0, 1.0],
                [0.0, 1.0],
                0.14,
            )),
            [x, 0.15 + t * 0.5, z],
            id_quat(),
        ),
        vec![prim(
            solid(with_cut(
                cylinder_tapered(0.7, t, 20, 0.0, stone_mat),
                [0.0, 1.0],
                [0.0, 1.0],
                0.15,
            )),
            [x + 0.15, 0.15 + t * 1.5, z - 0.1],
            id_quat(),
        )],
    )
}

fn build_tree() -> Generator {
    let mut root = footing(22.8, 11.6, [1.2, -0.4], CLEARANCE);
    attach(&mut root, mill_house());
    attach(&mut root, grain_store());
    attach(&mut root, tailrace());
    attach(&mut root, axle());
    attach(&mut root, wheel());
    attach(&mut root, flume());
    attach(&mut root, millstones());
    root.audio = crate::catalogue::items::fx::water_trickle();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Watermill.build(""), "watermill");
    }

    #[test]
    fn nothing_z_fights() {
        assert_no_z_fighting(&Watermill.build(""), "watermill");
    }

    #[test]
    fn nothing_floats() {
        assert_nothing_floats(&Watermill.build(""), "watermill");
    }

    #[test]
    fn its_windows_are_lit() {
        assert!(has_emissive(&Watermill.build("")));
    }

    #[test]
    fn record_stays_inside_the_budget() {
        let bytes = crate::pds::record_size::serialized_record_bytes(&Watermill.build(""))
            .expect("serialises");
        eprintln!("watermill record: {bytes} B");
        assert!(bytes < 80 * 1024, "{bytes} B");
    }
}
