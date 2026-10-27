//! Works Gateway - a family gate for the industrial age (#1796), stood by
//! industrial park and steampunk worlds beside their own. The gate of a
//! works yard: two brick piers with gas globes on their caps carry an iron
//! girder, an iron arch over it holds the works' name board, lit at every
//! hour as a sign is; a striped lifting barrier stands raised before one
//! pier and a gatekeeper's hut against the other, its window lit from dusk.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! industrial park and steampunk pools through `kindred_gates`.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::coastal_resort::enamel;
use crate::catalogue::items::industrial_park::{
    BRICK_DARK, CONCRETE_GREY, STEEL_BLUE, WINDOW_LIT, brick, cladding, concrete,
};
use crate::catalogue::items::pirate::SIGN_AMBER;
use crate::catalogue::items::steampunk::{IRON_DARK, LAMP_GAS, iron};
use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, plane, prim, quat_x,
    quat_z, solid, sphere, strut, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct WorksGateway;

impl CatalogueEntry for WorksGateway {
    fn slug(&self) -> &'static str {
        "works_gateway"
    }
    fn name(&self) -> &'static str {
        "Works Gateway"
    }
    fn description(&self) -> &'static str {
        "Brick piers under an iron arch and a lit works name board, a lifting barrier and a gatekeeper's hut."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn footprint(&self) -> Footprint {
        super::gate_footprint()
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Half the opening between the piers' inner faces (m).
const HALF_OPENING: f32 = 1.45;
/// Top of the concrete apron.
const APRON_TOP: f32 = 0.25;
/// Underside of the iron girder, the head of the opening.
const GIRDER_UNDER: f32 = 3.7;
/// Top of the girder, where the arch springs.
const GIRDER_TOP: f32 = 4.05;
/// Top of the piers, under their caps.
const PIER_TOP: f32 = 4.45;

fn build_tree() -> Generator {
    let pier_w = 0.8;
    let pier_x = HALF_OPENING + pier_w * 0.5;
    let mut prims = vec![footing(7.0, 2.6, [0.0, 0.0], 3.5)];
    prims.push(prim(
        solid(cuboid_tapered(
            [7.0, APRON_TOP, 2.6],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, APRON_TOP * 0.5, 0.0],
        id_quat(),
    ));

    for sx in [-1.0_f32, 1.0] {
        let x = sx * pier_x;
        prims.push(prim(
            solid(cuboid_tapered(
                [pier_w, PIER_TOP - APRON_TOP + 0.02, pier_w],
                0.0,
                brick(BRICK_DARK),
            )),
            [x, (PIER_TOP + APRON_TOP - 0.02) * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [pier_w + 0.15, 0.2, pier_w + 0.15],
                0.0,
                concrete([0.66, 0.64, 0.60]),
            )),
            [x, PIER_TOP + 0.1, 0.0],
            id_quat(),
        ));
        // A gas globe on an iron column on each cap.
        prims.push(prim(
            solid(cylinder_tapered(0.06, 0.4, 8, 0.0, iron(IRON_DARK))),
            [x, PIER_TOP + 0.19 + 0.2, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.17, 3, glow(LAMP_GAS, 1.6)),
            [x, PIER_TOP + 0.74, 0.0],
            id_quat(),
        ));
    }

    // The iron girder across the opening, its ends in the piers.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.0 * HALF_OPENING + 0.2, GIRDER_TOP - GIRDER_UNDER, 0.5],
            0.0,
            iron(IRON_DARK),
        )),
        [0.0, (GIRDER_TOP + GIRDER_UNDER) * 0.5, 0.0],
        id_quat(),
    ));
    // Rivet heads along its face.
    for k in 0..9 {
        prims.push(prim(
            sphere(0.035, 2, iron(IRON_DARK)),
            [
                (k as f32 - 4.0) * 0.34,
                (GIRDER_TOP + GIRDER_UNDER) * 0.5,
                -0.25,
            ],
            id_quat(),
        ));
    }

    // The iron arch over the girder: eight chords, spokes up to it from the
    // girder, and the name board hung in it.
    let arch_w = HALF_OPENING + 0.15;
    let arch_h = 1.25;
    let arc = |a: f32| [arch_w * a.cos(), GIRDER_TOP - 0.02 + arch_h * a.sin()];
    for i in 0..8 {
        let [x0, y0] = arc(PI * i as f32 / 8.0);
        let [x1, y1] = arc(PI * (i + 1) as f32 / 8.0);
        prims.push(strut(
            [x0, y0, 0.0],
            [x1, y1, 0.0],
            0.055,
            8,
            iron(IRON_DARK),
        ));
    }
    for x in [-1.05_f32, -0.5, 0.5, 1.05] {
        let top = GIRDER_TOP - 0.02 + arch_h * (1.0 - (x / arch_w).powi(2)).sqrt();
        prims.push(strut(
            [x, GIRDER_TOP - 0.02, 0.06],
            [x, top - 0.02, 0.06],
            0.025,
            6,
            iron(IRON_DARK),
        ));
    }
    prims.extend(name_board([0.0, 4.5, 0.0]));

    prims.extend(barrier([-2.45, APRON_TOP, -0.85]));
    prims.extend(hut());

    // The walk-in zone between the piers, its edges bedded 4 cm in the
    // piers, the girder and the apron (#1006).
    let veil_h = GIRDER_UNDER - APRON_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.4]),
        },
        [0.0, APRON_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// The works' name board, centred at `at`: a dark iron-framed board with
/// nine lit letter cells along its face - lit at every hour, as a sign is,
/// and segmented so the lit area stays small enough not to bloom.
fn name_board(at: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = at;
    let (w, h) = (2.6, 0.56);
    let mut parts = vec![
        prim(
            solid(cuboid_tapered(
                [w, h, 0.12],
                0.0,
                enamel([0.07, 0.07, 0.08]),
            )),
            [x, y, z],
            id_quat(),
        ),
        prim(
            cuboid_tapered([w + 0.04, 0.06, 0.16], 0.0, iron(IRON_DARK)),
            [x, y + h * 0.5 + 0.02, z],
            id_quat(),
        ),
        prim(
            cuboid_tapered([w + 0.04, 0.06, 0.16], 0.0, iron(IRON_DARK)),
            [x, y - h * 0.5 - 0.02, z],
            id_quat(),
        ),
    ];
    let cells = 9;
    let pitch = w / cells as f32;
    for k in 0..cells {
        parts.push(prim(
            cuboid_tapered([pitch * 0.7, h * 0.62, 0.05], 0.0, glow(SIGN_AMBER, 2.4)),
            [x - w * 0.5 + pitch * (k as f32 + 0.5), y, z - 0.075],
            id_quat(),
        ));
    }
    // Hangers from the girder up to the board's foot.
    for hx in [-0.9_f32, 0.9] {
        parts.push(strut(
            [x + hx, GIRDER_TOP - 0.02, z],
            [x + hx, y - h * 0.5, z],
            0.02,
            6,
            iron(IRON_DARK),
        ));
    }
    parts
}

/// The lifting barrier, its post standing on the apron at `foot`: a
/// housing, and a red-and-white arm raised nearly upright on its pivot with
/// a counterweight on the short end.
fn barrier(foot: [f32; 3]) -> Vec<Generator> {
    let [x, y, z] = foot;
    let pivot = [x, y + 0.85, z - 0.23];
    let lift = 78.0_f32.to_radians();
    let dir = [lift.cos(), lift.sin()];
    let tilt = quat_z(lift - FRAC_PI_2);
    let mut parts = vec![
        prim(
            solid(cuboid_tapered(
                [0.4, 1.0, 0.4],
                0.0,
                enamel([0.80, 0.62, 0.10]),
            )),
            [x, y + 0.49, z],
            id_quat(),
        ),
        // The pivot boss on the housing's face.
        prim(
            cylinder_tapered(0.09, 0.08, 12, 0.0, iron(IRON_DARK)),
            [pivot[0], pivot[1], z - 0.22],
            quat_x(FRAC_PI_2),
        ),
    ];
    // Six striped segments of the arm, abutting end to end.
    let seg = 0.56;
    for i in 0..6 {
        let d = seg * (i as f32 + 0.5);
        let colour = if i % 2 == 0 {
            [0.70, 0.10, 0.08]
        } else {
            [0.90, 0.90, 0.88]
        };
        parts.push(prim(
            cuboid_tapered([0.1, seg, 0.08], 0.0, enamel(colour)),
            [
                pivot[0] + dir[0] * d,
                pivot[1] + dir[1] * d,
                pivot[2] - 0.06,
            ],
            tilt,
        ));
    }
    parts.push(prim(
        cuboid_tapered([0.22, 0.3, 0.12], 0.0, iron(IRON_DARK)),
        [
            pivot[0] - dir[0] * 0.2,
            pivot[1] - dir[1] * 0.2,
            pivot[2] - 0.06,
        ],
        tilt,
    ));
    parts
}

/// The gatekeeper's hut against the `+X` pier: clad walls, a window in its
/// front with the keeper's room lit behind it from dusk to dawn, and a flat
/// roof oversailing it.
fn hut() -> Vec<Generator> {
    let (x0, x1) = (2.27, 3.45);
    let (z0, z1) = (-1.1, 0.5);
    let top = 2.6;
    let wall = 0.1;
    let cx = (x0 + x1) * 0.5;
    let clad = || cladding(STEEL_BLUE);
    let y0 = APRON_TOP - 0.02;
    let mid = |a: f32, b: f32| (a + b) * 0.5;
    let (win_x0, win_x1) = (x0 + 0.25, x1 - 0.25);
    let (sill, head) = (1.15, 2.0);
    let mut parts = vec![
        // Back and side walls.
        prim(
            solid(cuboid_tapered([x1 - x0, top - y0, wall], 0.0, clad())),
            [cx, mid(y0, top), z1 - wall * 0.5],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [wall, top - y0, z1 - z0 - 2.0 * wall],
                0.0,
                clad(),
            )),
            [x0 + wall * 0.5, mid(y0, top), mid(z0, z1)],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                [wall, top - y0, z1 - z0 - 2.0 * wall],
                0.0,
                clad(),
            )),
            [x1 - wall * 0.5, mid(y0, top), mid(z0, z1)],
            id_quat(),
        ),
        // The front wall round its window: under the sill, over the head,
        // and a jamb each side.
        prim(
            solid(cuboid_tapered([x1 - x0, sill - y0, wall], 0.0, clad())),
            [cx, mid(y0, sill), z0 + wall * 0.5],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([x1 - x0, top - head, wall], 0.0, clad())),
            [cx, mid(head, top), z0 + wall * 0.5],
            id_quat(),
        ),
    ];
    for (a, b) in [(x0, win_x0), (win_x1, x1)] {
        parts.push(prim(
            solid(cuboid_tapered([b - a, head - sill, wall], 0.0, clad())),
            [mid(a, b), mid(sill, head), z0 + wall * 0.5],
            id_quat(),
        ));
    }
    // The pane in the window, and the keeper's lamplit room behind it.
    parts.push(prim(
        plane(
            [win_x1 - win_x0, head - sill],
            window_card([0.2, 0.22, 0.24], 2, 2, 0.3, 0.06),
        ),
        [mid(win_x0, win_x1), mid(sill, head), z0 + wall * 0.5],
        quat_x(-FRAC_PI_2),
    ));
    parts.push(prim(
        cuboid_tapered(
            [x1 - x0 - 2.0 * wall, 1.5, 0.02],
            0.0,
            lit_from_dusk(glow(WINDOW_LIT, 1.2)),
        ),
        [cx, 1.6, z1 - wall - 0.01],
        id_quat(),
    ));
    // Flat roof oversailing the walls.
    parts.push(prim(
        solid(cuboid_tapered(
            [x1 - x0 + 0.3, 0.12, z1 - z0 + 0.3],
            0.0,
            iron([0.18, 0.18, 0.19]),
        )),
        [cx, top + 0.06, mid(z0, z1)],
        id_quat(),
    ));
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&WorksGateway);
    }
}
