//! Light Ring Gateway - a family gate for the far future (#1796), stood by
//! space outpost, monolith and neon-city worlds beside their own. A tall
//! hoop of dark housing with a ring of light run round it, sunk in a tech
//! plinth: conduits run across the plinth into its feet, a field frame of
//! two emitter posts and a header stands in it round the walk-in field, and
//! two marker pylons stand at the plinth's front corners.
//!
//! `themes()` stays empty, as `civic_gateway`'s does: the gate reaches the
//! space outpost, alien monolithic and cyberpunk pools through
//! `kindred_gates`.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::space_outpost::{
    BEACON_RED, HAZARD_YELLOW, STEEL_DARK, hull, painted, steel,
};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, footing, glow, id_quat, prim, quat_x, solid, sphere, spine, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

pub struct LightRingGateway;

impl CatalogueEntry for LightRingGateway {
    fn slug(&self) -> &'static str {
        "light_ring_gateway"
    }
    fn name(&self) -> &'static str {
        "Light Ring Gateway"
    }
    fn description(&self) -> &'static str {
        "A tall ring of light sunk in a tech plinth, fed by conduits and flanked by marker pylons."
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

/// The ring's light: a deep cyan that reads as energy in any of the three
/// worlds the gate stands in.
const FIELD_CYAN: [f32; 3] = [0.12, 0.72, 1.0];
/// Half the field frame's opening (m).
const HALF_OPENING: f32 = 1.3;
/// Top of the plinth.
const PLINTH_TOP: f32 = 0.6;
/// Underside of the field frame's header, the head of the opening.
const HEADER_UNDER: f32 = 4.0;
/// The ring's centre height, its radius and its housing's tube radius.
const RING_Y: f32 = 2.6;
const RING_R: f32 = 3.0;
const HOUSING_R: f32 = 0.3;

fn build_tree() -> Generator {
    let gunmetal = [0.22, 0.23, 0.26];
    let mut prims = vec![footing(6.0, 2.6, [0.0, 0.0], 3.5)];
    // The plinth, and a step up to it across the front.
    prims.push(prim(
        solid(cuboid_tapered([6.0, PLINTH_TOP, 2.6], 0.0, hull(gunmetal))),
        [0.0, PLINTH_TOP * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [3.4, PLINTH_TOP * 0.5 + 0.02, 0.5],
            0.0,
            hull(gunmetal),
        )),
        [0.0, PLINTH_TOP * 0.25 - 0.01, -1.5],
        id_quat(),
    ));
    // Hazard banding along the plinth's front edge.
    prims.push(prim(
        cuboid_tapered([5.9, 0.08, 0.04], 0.0, painted(HAZARD_YELLOW)),
        [0.0, PLINTH_TOP - 0.1, -1.31],
        id_quat(),
    ));

    // The hoop: dark housing sunk in the plinth, a ring of light round each
    // face of it and a thin lit rim round its inside edge.
    prims.push(prim(
        solid(hoop(HOUSING_R, RING_R, steel(STEEL_DARK))),
        [0.0, RING_Y, 0.0],
        quat_x(FRAC_PI_2),
    ));
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            hoop(0.1, RING_R, glow(FIELD_CYAN, 5.0)),
            [0.0, RING_Y, sz * (HOUSING_R - 0.06)],
            quat_x(FRAC_PI_2),
        ));
    }
    prims.push(prim(
        hoop(0.05, RING_R - HOUSING_R + 0.02, glow(FIELD_CYAN, 6.5)),
        [0.0, RING_Y, 0.0],
        quat_x(FRAC_PI_2),
    ));

    // Clamps where the hoop's feet enter the plinth, and the conduits run
    // across the plinth into them from the back.
    let foot_x = 2.2;
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([1.0, 0.42, 0.9], 0.2, steel(STEEL_DARK))),
            [sx * foot_x, PLINTH_TOP + 0.2, 0.0],
            id_quat(),
        ));
        for (k, dz) in [0.0_f32, 0.22].into_iter().enumerate() {
            let r = 0.07 - 0.015 * k as f32;
            let x0 = sx * (2.75 - 0.18 * k as f32);
            prims.push(prim(
                spine(
                    &[
                        ([x0, PLINTH_TOP + r - 0.01, 1.25], r),
                        ([x0, PLINTH_TOP + r - 0.01, 0.7 + dz], r),
                        ([sx * (foot_x + 0.25), PLINTH_TOP + 0.12, 0.3 + dz * 0.5], r),
                    ],
                    8,
                    4,
                    steel([0.12, 0.12, 0.14]),
                ),
                [0.0, 0.0, 0.0],
                id_quat(),
            ));
        }
        prims.extend(pylon([sx * 2.6, PLINTH_TOP, -0.95], sx));
    }

    // The field frame: two emitter posts and a header, the header's ends
    // bedded in the hoop's housing, a lit strip down each post's inner face.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.3, HEADER_UNDER - PLINTH_TOP + 0.04, 0.6],
                0.0,
                steel(STEEL_DARK),
            )),
            [
                sx * (HALF_OPENING + 0.15),
                (HEADER_UNDER + PLINTH_TOP - 0.02) * 0.5,
                0.0,
            ],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.04, 3.0, 0.1], 0.0, glow(FIELD_CYAN, 3.0)),
            [
                sx * (HALF_OPENING - 0.01),
                (HEADER_UNDER + PLINTH_TOP) * 0.5,
                -0.2,
            ],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([4.3, 0.36, 0.64], 0.0, steel(STEEL_DARK))),
        [0.0, HEADER_UNDER + 0.18, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.5, 0.06, 0.1], 0.0, glow(FIELD_CYAN, 3.0)),
        [0.0, HEADER_UNDER - 0.02, -0.2],
        id_quat(),
    ));

    // The walk-in field, its edges bedded 4 cm in the posts, the header and
    // the plinth (#1006).
    let veil_h = HEADER_UNDER - PLINTH_TOP + 0.08;
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.0 * HALF_OPENING + 0.08, veil_h, 0.5]),
        },
        [0.0, PLINTH_TOP - 0.04 + veil_h * 0.5, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

/// A torus fine enough round its length to read as a true circle at the
/// hoop's three metres.
fn hoop(minor: f32, major: f32, mat: crate::pds::SovereignMaterialSettings) -> GeneratorKind {
    let mut kind = torus(minor, major, mat);
    if let GeneratorKind::Torus {
        major_resolution, ..
    } = &mut kind
    {
        *major_resolution = 64;
    }
    kind
}

/// A marker pylon standing on the plinth at `foot`, on side `sx`: a
/// tapered post with a lit strip down its front and a beacon on its head.
fn pylon(foot: [f32; 3], sx: f32) -> Vec<Generator> {
    let [x, y, z] = foot;
    let h = 2.2;
    let beacon = if sx < 0.0 { BEACON_RED } else { FIELD_CYAN };
    vec![
        prim(
            solid(cuboid_tapered([0.34, h, 0.34], 0.25, steel(STEEL_DARK))),
            [x, y + h * 0.5 - 0.01, z],
            id_quat(),
        ),
        prim(
            cuboid_tapered([0.06, h * 0.7, 0.04], 0.0, glow(FIELD_CYAN, 3.0)),
            [x, y + h * 0.45, z - 0.165],
            id_quat(),
        ),
        prim(
            sphere(0.1, 3, glow(beacon, 6.0)),
            [x, y + h + 0.06, z],
            id_quat(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_is_a_family_gate() {
        super::super::assert_family_gate(&LightRingGateway);
    }
}
