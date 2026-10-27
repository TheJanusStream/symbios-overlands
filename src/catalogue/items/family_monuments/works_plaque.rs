//! Owner Works Plaque - a family monument for worlds of mills and engines
//! (#1796): Industrial-Park and Steampunk.
//!
//! The plaque a works sets into its gate pier: a brick pier on a concrete
//! plinth, a stone string course and coping, and a cast-iron plate on its
//! face whose riveted frame holds the room owner's portrait above a brass
//! name band. A gas lamp on an iron standard burns on the coping.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::industrial_park::{BRICK_DARK, CONCRETE_GREY, brick, concrete};
use crate::catalogue::items::steampunk::{BRASS, IRON_DARK, LAMP_GAS, brass, iron};
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

const PANEL: f32 = 1.5;
const PANEL_Y: f32 = 2.8;
/// The pier's brick face.
const PIER_FRONT: f32 = -0.35;
/// The cast plate's face; the portrait and its frame stand on it.
const PLATE_FRONT: f32 = PIER_FRONT - 0.08;
/// The riveted frame's width round the portrait.
const FRAME: f32 = 0.16;
const CLEARANCE: f32 = 2.6;

pub struct WorksPlaqueMonument;

impl CatalogueEntry for WorksPlaqueMonument {
    fn slug(&self) -> &'static str {
        "works_plaque_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Works Plaque"
    }
    fn description(&self) -> &'static str {
        "Brick pier with a riveted cast-iron plaque framing the owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    // themes() stays empty: a family monument joins its themes' pools
    // through the kindred tables, never as a theme's own.
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let plinth = prim(
        solid(cuboid_tapered(
            [2.7, 0.3, 1.7],
            0.02,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );
    let pier = prim(
        solid(cuboid_tapered([2.1, 3.72, 0.9], 0.0, brick(BRICK_DARK))),
        [0.0, 2.15, PIER_FRONT + 0.45],
        id_quat(),
    );
    let coping = prim(
        solid(cuboid_tapered(
            [2.36, 0.22, 1.12],
            0.06,
            concrete(STONE_CAP),
        )),
        [0.0, 4.1, PIER_FRONT + 0.45],
        id_quat(),
    );
    nest(
        plinth,
        vec![
            nest(
                pier,
                vec![
                    // Stone string course across the pier below the plate.
                    prim(
                        solid(cuboid_tapered([2.18, 0.14, 0.98], 0.0, concrete(STONE_CAP))),
                        [0.0, 1.2, PIER_FRONT + 0.45],
                        id_quat(),
                    ),
                    nest(coping, vec![gas_lamp()]),
                    nest(plate(), plaque(did)),
                ],
            ),
            footing(2.7, 1.7, [0.0, 0.0], CLEARANCE),
        ],
    )
}

/// Pale dressed stone for the coping and string course.
const STONE_CAP: [f32; 3] = [0.66, 0.64, 0.60];

/// The cast plate bolted to the pier.
fn plate() -> Generator {
    prim(
        solid(cuboid_tapered([1.98, 2.4, 0.09], 0.0, iron(IRON_DARK))),
        [0.0, 2.7, PLATE_FRONT + 0.045],
        id_quat(),
    )
}

/// On the plate: the portrait, the riveted frame round it and the brass name
/// band under it.
fn plaque(did: &str) -> Vec<Generator> {
    let off = (PANEL + FRAME) * 0.5;
    let z = PLATE_FRONT - 0.04;
    let mut out = vec![
        pfp_panel(did, PANEL, [0.0, PANEL_Y, PLATE_FRONT - 0.03]),
        prim(
            solid(cuboid_tapered([1.4, 0.22, 0.05], 0.1, brass(BRASS))),
            [0.0, 1.68, PLATE_FRONT - 0.02],
            id_quat(),
        ),
    ];
    for s in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [FRAME, PANEL + FRAME * 2.0, 0.1],
                0.0,
                iron(IRON_DARK),
            )),
            [s * off, PANEL_Y, z - 0.005],
            id_quat(),
        ));
        out.push(prim(
            solid(cuboid_tapered([PANEL, FRAME, 0.085], 0.0, iron(IRON_DARK))),
            [0.0, PANEL_Y + s * off, z],
            id_quat(),
        ));
    }
    // Rivet heads down the middle of each frame bar, five a side.
    let front = z - 0.04;
    for k in 0..5 {
        let t = (k as f32 - 2.0) / 2.0 * (PANEL * 0.5 + FRAME * 0.5);
        for s in [-1.0_f32, 1.0] {
            out.push(rivet([s * off, PANEL_Y + t, front - 0.005]));
            if k != 0 && k != 4 {
                out.push(rivet([t, PANEL_Y + s * off, front]));
            }
        }
    }
    out
}

fn rivet(at: [f32; 3]) -> Generator {
    prim(solid(sphere(0.04, 3, iron(IRON_DARK))), at, id_quat())
}

/// A gas lamp on an iron standard in the middle of the coping - for the
/// brick, which goes flat at dusk; the portrait is unlit and reads on its own.
fn gas_lamp() -> Generator {
    let z = PIER_FRONT + 0.45;
    let standard = prim(
        solid(cylinder_tapered(0.07, 0.56, 10, 0.2, iron(IRON_DARK))),
        [0.0, 4.48, z],
        id_quat(),
    );
    let seat = prim(
        solid(cylinder_tapered(0.17, 0.07, 10, 0.0, iron(IRON_DARK))),
        [0.0, 4.78, z],
        id_quat(),
    );
    let cap = prim(
        solid(cylinder_tapered(0.21, 0.16, 10, 0.7, iron(IRON_DARK))),
        [0.0, 5.21, z],
        id_quat(),
    );
    nest(
        standard,
        vec![nest(
            seat,
            vec![
                prim(
                    cylinder_tapered(0.13, 0.36, 10, -0.2, glow(LAMP_GAS, 2.4)),
                    [0.0, 4.99, z],
                    id_quat(),
                ),
                nest(
                    cap,
                    vec![prim(
                        solid(sphere(0.05, 3, brass(BRASS))),
                        [0.0, 5.31, z],
                        id_quat(),
                    )],
                ),
                // Four slim glazing bars from seat to cap.
                lamp_bar(0.13, 0.0, z),
                lamp_bar(-0.13, 0.0, z),
                lamp_bar(0.0, 0.13, z),
                lamp_bar(0.0, -0.13, z),
            ],
        )],
    )
}

fn lamp_bar(x: f32, dz: f32, z: f32) -> Generator {
    prim(
        solid(cuboid_tapered([0.025, 0.4, 0.025], 0.0, iron(IRON_DARK))),
        [x, 4.99, z + dz],
        id_quat(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &WorksPlaqueMonument.build("did:plc:test"),
            "works_plaque_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&WorksPlaqueMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = WorksPlaqueMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "works_plaque_monument");
        assert_nothing_floats(&built, "works_plaque_monument");
    }
}
