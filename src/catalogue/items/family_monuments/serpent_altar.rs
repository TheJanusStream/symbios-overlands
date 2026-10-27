//! Owner Serpent Altar - a family monument for Mesoamerican worlds (#1796).
//!
//! A low limestone altar on a two-step platform, a feathered-serpent head at
//! each end with its jaws open and its jade ruff spread behind it, and a
//! copal bowl smouldering on the altar top. Behind it stands a carved panel
//! whose jade border holds the room owner's portrait, a gold glyph band
//! beneath and a crest of jade plumes along its head.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::mesoamerican::{
    BONE_WHITE, CLAY_TERRACOTTA, FIRE_ORANGE, GOLD_WARM, JADE_GREEN, LIMESTONE_PALE,
    OBSIDIAN_BLACK, STONE_GREY, STUCCO_RED, gold, jade, limestone, obsidian, painted,
};
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel, prim, quat_z, solid,
    sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

const PANEL: f32 = 1.6;
const PANEL_Y: f32 = 2.8;
/// The carved panel's front face; the portrait stands just proud of it.
const STELA_FRONT: f32 = 0.28;
/// The top of the platform, which the altar, the panel and the heads stand on.
const DECK: f32 = 0.55;
/// The altar's and the heads' centre line, in front of the panel.
const ALTAR_Z: f32 = -0.45;
/// The serpent heads' scale over their unit draft.
const HEAD: f32 = 1.4;
const CLEARANCE: f32 = 2.9;

pub struct SerpentAltarMonument;

impl CatalogueEntry for SerpentAltarMonument {
    fn slug(&self) -> &'static str {
        "serpent_altar_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Serpent Altar"
    }
    fn description(&self) -> &'static str {
        "Stone altar between two feathered-serpent heads, the owner's portrait carved behind."
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
    let step0 = prim(
        solid(cuboid_tapered([5.1, 0.3, 2.7], 0.0, limestone(STONE_GREY))),
        [0.0, 0.15, 0.0],
        id_quat(),
    );
    let step1 = prim(
        solid(cuboid_tapered(
            [4.6, 0.25, 2.4],
            0.0,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, 0.425, -0.05],
        id_quat(),
    );
    nest(
        step0,
        vec![
            nest(
                step1,
                vec![stela(did), altar(), serpent_head(-1.75), serpent_head(1.75)],
            ),
            footing(5.1, 2.7, [0.0, 0.0], CLEARANCE),
        ],
    )
}

/// The carved panel behind the altar: the portrait in a jade border, the
/// gold glyph band under it, the cap and the plume crest along its head.
fn stela(did: &str) -> Generator {
    let h = 3.8;
    let slab = prim(
        solid(cuboid_tapered(
            [2.4, h, 0.56],
            0.04,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, DECK + h * 0.5, STELA_FRONT + 0.28],
        id_quat(),
    );
    let bar = 0.16;
    let z = STELA_FRONT - 0.03;
    let mut face = vec![
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        prim(
            solid(cuboid_tapered([1.8, 0.26, 0.08], 0.0, gold(GOLD_WARM))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.36, STELA_FRONT - 0.02],
            id_quat(),
        ),
        nest(
            prim(
                solid(cuboid_tapered(
                    [2.62, 0.3, 0.74],
                    0.08,
                    limestone(LIMESTONE_PALE),
                )),
                [0.0, DECK + h + 0.13, STELA_FRONT + 0.28],
                id_quat(),
            ),
            crest(DECK + h + 0.28),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        face.push(prim(
            solid(cuboid_tapered(
                [bar, PANEL + bar * 2.0, 0.1],
                0.0,
                jade(JADE_GREEN),
            )),
            [sx * (PANEL + bar) * 0.5, PANEL_Y, STELA_FRONT - 0.04],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        face.push(prim(
            solid(cuboid_tapered([PANEL, bar, 0.1], 0.0, jade(JADE_GREEN))),
            [0.0, PANEL_Y + sy * (PANEL + bar) * 0.5, STELA_FRONT - 0.04],
            id_quat(),
        ));
    }
    nest(slab, face)
}

/// A fan of jade plumes rising from the stela's cap at `base`, each a leaf
/// turned about its own centre.
fn crest(base: f32) -> Vec<Generator> {
    (0..7)
        .map(|k| {
            let a = (k as f32 - 3.0) * 0.26;
            let len = if k % 2 == 0 { 1.1 } else { 0.9 };
            let r = len * 0.5 - 0.06;
            prim(
                solid(cuboid_tapered(
                    [0.24, len, 0.07 + 0.04 * (k % 2) as f32],
                    0.55,
                    jade(JADE_GREEN),
                )),
                [-a.sin() * r, base + a.cos() * r, STELA_FRONT + 0.28],
                quat_z(a),
            )
        })
        .collect()
}

/// The altar block, its painted band and mensa, and the copal bowl on it.
fn altar() -> Generator {
    let z = ALTAR_Z;
    let block = prim(
        solid(cuboid_tapered(
            [2.6, 0.8, 1.0],
            0.0,
            limestone(LIMESTONE_PALE),
        )),
        [0.0, DECK + 0.4, z],
        id_quat(),
    );
    let bowl = prim(
        solid(cylinder_tapered(
            0.26,
            0.18,
            12,
            -0.3,
            painted(CLAY_TERRACOTTA),
        )),
        [0.0, DECK + 1.01, z],
        id_quat(),
    );
    nest(
        block,
        vec![
            // Painted band across the front.
            prim(
                solid(cuboid_tapered([2.1, 0.34, 0.04], 0.0, painted(STUCCO_RED))),
                [0.0, DECK + 0.4, z - 0.52],
                id_quat(),
            ),
            nest(
                prim(
                    solid(cuboid_tapered(
                        [2.84, 0.12, 1.16],
                        0.0,
                        limestone(STONE_GREY),
                    )),
                    [0.0, DECK + 0.86, z],
                    id_quat(),
                ),
                vec![nest(
                    bowl,
                    vec![prim(
                        cuboid_tapered([0.34, 0.1, 0.34], 0.4, glow(FIRE_ORANGE, 2.4)),
                        [0.0, DECK + 1.1, z],
                        id_quat(),
                    )],
                )],
            ),
        ],
    )
}

/// A feathered-serpent head at `x`, jaws open toward the approach (-Z): the
/// lower jaw on the deck carries the mouth, the skull, its snout, brows,
/// eyes and fangs, and the jade ruff spread behind it. Authored at a unit
/// head and set out at [`HEAD`] times that, so its parts keep their
/// proportions to each other.
fn serpent_head(x: f32) -> Generator {
    let s = HEAD;
    let z = ALTAR_Z;
    let at = |dx: f32, dy: f32, dz: f32| [x + dx * s, DECK + dy * s, z + dz * s];
    let size = |a: f32, b: f32, c: f32| [a * s, b * s, c * s];
    let jaw = prim(
        solid(cuboid_tapered(
            size(0.78, 0.22, 1.0),
            0.05,
            limestone(LIMESTONE_PALE),
        )),
        at(0.0, 0.11, 0.0),
        id_quat(),
    );
    let mouth = prim(
        solid(cuboid_tapered(
            size(0.6, 0.26, 0.8),
            0.0,
            painted(STUCCO_RED),
        )),
        at(0.0, 0.33, 0.06),
        id_quat(),
    );
    let skull = prim(
        solid(cuboid_tapered(
            size(0.84, 0.56, 0.9),
            0.18,
            limestone(LIMESTONE_PALE),
        )),
        at(0.0, 0.72, 0.08),
        id_quat(),
    );
    let mut on_skull = vec![
        // The snout, jutting past the jaw over the open mouth.
        prim(
            solid(cuboid_tapered(
                size(0.7, 0.3, 0.42),
                0.2,
                limestone(LIMESTONE_PALE),
            )),
            at(0.0, 0.6, -0.46),
            id_quat(),
        ),
        // A heavy brow over the eyes.
        prim(
            solid(cuboid_tapered(
                size(0.9, 0.14, 0.34),
                0.1,
                painted(STUCCO_RED),
            )),
            at(0.0, 1.04, -0.12),
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        on_skull.push(prim(
            solid(sphere(0.1 * s, 6, obsidian(OBSIDIAN_BLACK))),
            at(sx * 0.28, 0.9, -0.3),
            id_quat(),
        ));
        // A fang down from each corner of the snout.
        on_skull.push(prim(
            solid(cuboid_tapered(
                size(0.09, 0.22, 0.09),
                0.95,
                painted(BONE_WHITE),
            )),
            at(sx * 0.26, 0.4, -0.56),
            quat_z(std::f32::consts::PI),
        ));
    }
    // The ruff: plumes fanned round the back of the head, facing the
    // approach; alternate ones are deeper, so no two fronts share a plane.
    for k in 0..5 {
        let a = (k as f32 - 2.0) * 0.32;
        on_skull.push(prim(
            solid(cuboid_tapered(
                size(0.22, 0.8, 0.06 + 0.04 * (k % 2) as f32),
                0.5,
                jade(JADE_GREEN),
            )),
            at(-a.sin() * 0.5, 0.72 + a.cos() * 0.5, 0.45),
            quat_z(a),
        ));
    }
    nest(jaw, vec![nest(mouth, vec![nest(skull, on_skull)])])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &SerpentAltarMonument.build("did:plc:test"),
            "serpent_altar_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&SerpentAltarMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = SerpentAltarMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "serpent_altar_monument");
        assert_nothing_floats(&built, "serpent_altar_monument");
    }
}
