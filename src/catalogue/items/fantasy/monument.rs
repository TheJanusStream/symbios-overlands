//! Owner Scrying Frame — the Fantasy identity monument (#975).
//!
//! A mossy stone dais with two rough menhirs carrying a gilded arch: the room
//! owner's likeness hangs in the arch as a scrying pane, a crystal grows from
//! the dais at each side and a rune band burns gold along the lintel.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    cuboid_tapered, glow, id_quat, nest, pfp_panel, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CRYSTAL_CYAN, GOLD, MANA_TEAL, RUNE_GOLD, STONE_GREY, STONE_MOSS, crystal, gold, matte, mossy,
    stone,
};

const PANEL: f32 = 1.8;
const PANEL_Y: f32 = 3.05;
/// Blank tint — still scrying-water, the deep teal a pane shows when nothing
/// has been called into it.
const BLANK: [f32; 3] = [0.16, 0.28, 0.32];

pub struct FantasyMonument;

impl CatalogueEntry for FantasyMonument {
    fn slug(&self) -> &'static str {
        "fantasy_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Scrying Frame"
    }
    fn description(&self) -> &'static str {
        "Gilded menhir arch holding a scrying pane with the room owner's likeness."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.8,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    // Mossy dais — the root, and flat, so the leaning menhirs above cannot
    // spin what they carry.
    let dais = prim(
        solid(cuboid_tapered([3.8, 0.42, 2.2], 0.14, mossy(STONE_MOSS))),
        [0.0, 0.21, 0.0],
        id_quat(),
    );

    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(menhir(sx * 1.42, sx * -0.05));
        parts.push(crystal(
            [sx * 1.75, 0.42, -0.62],
            0.2,
            1.15,
            quat_z(sx * 0.18),
            glow(CRYSTAL_CYAN, 1.4),
        ));
    }
    parts.push(nest(lintel(), pane(did)));
    nest(dais, parts)
}

/// A rough menhir. The lean is on the stone itself, which carries nothing —
/// the lintel is a sibling, not a child, precisely so the tilt cannot
/// propagate into the pane.
fn menhir(x: f32, lean: f32) -> Generator {
    prim(
        solid(cuboid_tapered([0.62, 4.3, 0.55], 0.22, stone(STONE_GREY))),
        [x, 2.57, 0.0],
        quat_z(lean),
    )
}

/// The gilded lintel across the menhirs, and everything it carries.
fn lintel() -> Generator {
    prim(
        solid(cuboid_tapered([3.5, 0.4, 0.7], 0.06, gold(GOLD))),
        [0.0, 4.72, 0.0],
        id_quat(),
    )
}

/// The scrying pane: a dark slate backing, the likeness, a gold surround and
/// the rune band along the lintel.
fn pane(did: &str) -> Vec<Generator> {
    let z = -0.16;
    let fr = 0.14;
    let mut out = vec![
        // Slate backing — the pane is single-sided, and an arch you can see
        // straight through would not hold an image at all.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.24, PANEL + 0.24, 0.1],
                0.0,
                matte([0.18, 0.18, 0.22]),
            )),
            [0.0, PANEL_Y, z + 0.07],
            id_quat(),
        ),
        prim(
            pfp_panel(did, PANEL, BLANK),
            [0.0, PANEL_Y, z],
            quat_x(-FRAC_PI_2),
        ),
        // Rune band burning along the lintel's face. The pane is unlit and
        // reads on its own; this is what makes the stone read as enchanted
        // rather than merely old.
        prim(
            cuboid_tapered([3.0, 0.14, 0.08], 0.0, glow(RUNE_GOLD, 1.6)),
            [0.0, 4.72, -0.38],
            id_quat(),
        ),
        // Mana wisp under the pane, the theme's signature cold light.
        prim(
            cuboid_tapered([0.9, 0.08, 0.1], 0.0, glow(MANA_TEAL, 1.3)),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.3, z - 0.05],
            id_quat(),
        ),
    ];
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [fr, PANEL + fr * 2.0, 0.13],
                0.0,
                gold(GOLD),
            )),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([PANEL, fr, 0.13], 0.0, gold(GOLD))),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FantasyMonument.build("did:plc:test"), "fantasy_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&FantasyMonument, "did:plc:test");
    }
}
