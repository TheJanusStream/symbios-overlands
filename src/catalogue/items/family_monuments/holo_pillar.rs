//! Owner Holo-Pillar - a family monument for worlds of machines and stars
//! (#1796): Space-Outpost, Cyberpunk and Alien-Monolithic.
//!
//! A slim dark pillar on a hazard-striped base, a cyan strip running up its
//! face, carries a projector head whose lit lens fans a sheet of light up
//! into a framed screen: the room owner's portrait, held in a dark bezel
//! edged in cyan on a mast behind it, with a red beacon on the mast head.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::cyberpunk::NEON_CYAN;
use crate::catalogue::items::space_outpost::{
    BEACON_RED, HAZARD_YELLOW, PAD_GREY, STEEL_DARK, concrete, painted, steel,
};
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, quat_z, solid,
    sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

const PANEL: f32 = 1.5;
const PANEL_Y: f32 = 3.7;
/// The screen's backing plate front; the portrait stands just proud of it.
const SCREEN_BACK: f32 = 0.0;
/// The bezel's width round the portrait.
const BEZEL: f32 = 0.12;
const CLEARANCE: f32 = 2.5;

pub struct HoloPillarMonument;

impl CatalogueEntry for HoloPillarMonument {
    fn slug(&self) -> &'static str {
        "holo_pillar_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Holo-Pillar"
    }
    fn description(&self) -> &'static str {
        "Slim pillar whose lit projector head throws the owner's portrait onto a framed screen."
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
    let pad = prim(
        solid(cuboid_tapered([2.4, 0.25, 1.9], 0.02, concrete(PAD_GREY))),
        [0.0, 0.125, 0.0],
        id_quat(),
    );
    let base = prim(
        solid(cuboid_tapered([1.0, 0.5, 0.8], 0.15, steel(STEEL_DARK))),
        [0.0, 0.5, 0.0],
        id_quat(),
    );
    let pillar = prim(
        solid(cuboid_tapered([0.4, 1.62, 0.34], 0.1, steel(STEEL_DARK))),
        [0.0, 1.54, 0.0],
        id_quat(),
    );
    let on_pillar = vec![
        // The lit strip up the pillar's face.
        prim(
            cuboid_tapered([0.05, 1.3, 0.03], 0.0, glow(NEON_CYAN, 6.0)),
            [0.0, 1.5, -0.168],
            id_quat(),
        ),
        projector(),
        nest(mast(), screen(did)),
    ];
    nest(
        pad,
        vec![
            nest(
                base,
                vec![
                    nest(pillar, on_pillar),
                    // Hazard-striped kick plate on the base's front.
                    prim(
                        solid(cuboid_tapered(
                            [0.84, 0.16, 0.03],
                            0.0,
                            painted(HAZARD_YELLOW),
                        )),
                        [0.0, 0.42, -0.385],
                        id_quat(),
                    ),
                ],
            ),
            footing(2.4, 1.9, [0.0, 0.0], CLEARANCE),
        ],
    )
}

/// The projector head on the pillar: its housing, the lit lens and the ring
/// round it, and the sheet of light it fans up into the screen.
fn projector() -> Generator {
    let head = prim(
        solid(cylinder_tapered(0.36, 0.3, 16, 0.0, steel(STEEL_DARK))),
        [0.0, 2.49, 0.0],
        id_quat(),
    );
    nest(
        head,
        vec![
            prim(
                cylinder_tapered(0.22, 0.04, 16, 0.0, glow(NEON_CYAN, 3.0)),
                [0.0, 2.66, 0.0],
                id_quat(),
            ),
            prim(
                torus(0.035, 0.29, glow(NEON_CYAN, 6.0)),
                [0.0, 2.66, 0.0],
                id_quat(),
            ),
            // The beam: a thin sheet narrow at the lens and wide at the
            // screen - a frustum turned over, since a taper pinches the top.
            prim(
                cuboid_tapered([1.5, 0.22, 0.03], 0.88, glow(NEON_CYAN, 2.0)),
                [0.0, 2.76, 0.0],
                quat_z(std::f32::consts::PI),
            ),
        ],
    )
}

/// The slim mast behind the screen, rising out of the pillar.
fn mast() -> Generator {
    prim(
        solid(cuboid_tapered([0.14, 2.56, 0.14], 0.0, steel(STEEL_DARK))),
        [0.0, 3.48, 0.12],
        id_quat(),
    )
}

/// The screen on the mast: its backing plate, the portrait, the dark bezel
/// round it edged in cyan, and the beacon on the mast head.
fn screen(did: &str) -> Vec<Generator> {
    let backing = prim(
        solid(cuboid_tapered(
            [PANEL + 0.22, PANEL + 0.22, 0.055],
            0.0,
            steel([0.08, 0.09, 0.11]),
        )),
        [0.0, PANEL_Y, SCREEN_BACK + 0.0275],
        id_quat(),
    );
    let off = (PANEL + BEZEL) * 0.5;
    let mut face = vec![pfp_panel(did, PANEL, [0.0, PANEL_Y, SCREEN_BACK - 0.03])];
    for s in [-1.0_f32, 1.0] {
        face.push(prim(
            solid(cuboid_tapered(
                [BEZEL, PANEL + BEZEL * 2.0, 0.07],
                0.0,
                steel(STEEL_DARK),
            )),
            [s * off, PANEL_Y, SCREEN_BACK - 0.02],
            id_quat(),
        ));
        face.push(prim(
            solid(cuboid_tapered([PANEL, BEZEL, 0.07], 0.0, steel(STEEL_DARK))),
            [0.0, PANEL_Y + s * off, SCREEN_BACK - 0.02],
            id_quat(),
        ));
        // Hot thin edges on the bezel's face; the uprights stand a
        // centimetre prouder than the rails so their corners never meet in
        // one plane.
        face.push(prim(
            cuboid_tapered([0.04, PANEL + BEZEL * 2.0, 0.03], 0.0, glow(NEON_CYAN, 6.5)),
            [s * off, PANEL_Y, SCREEN_BACK - 0.07],
            id_quat(),
        ));
        face.push(prim(
            cuboid_tapered([PANEL + BEZEL * 2.0, 0.04, 0.03], 0.0, glow(NEON_CYAN, 6.5)),
            [0.0, PANEL_Y + s * off, SCREEN_BACK - 0.06],
            id_quat(),
        ));
    }
    vec![
        nest(backing, face),
        prim(
            sphere(0.08, 4, glow(BEACON_RED, 4.0)),
            [0.0, 4.8, 0.12],
            id_quat(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &HoloPillarMonument.build("did:plc:test"),
            "holo_pillar_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&HoloPillarMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = HoloPillarMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "holo_pillar_monument");
        assert_nothing_floats(&built, "holo_pillar_monument");
    }
}
