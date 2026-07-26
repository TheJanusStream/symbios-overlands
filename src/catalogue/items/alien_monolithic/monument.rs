//! Owner Glyph Slab — the Alien-Monolithic identity monument (#975).
//!
//! A black obsidian slab standing on a dead-stone dais, the room owner's
//! likeness inset behind a cyan glyph frame, with a column of violet glyphs
//! burning down each flank and a small counter-slab floating clear of the
//! ground beside it.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::util::{
    cuboid_tapered, glow, id_quat, nest, pfp_panel, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DEAD_STONE, ENERGY_BLUE, GLYPH_CYAN, GLYPH_VIOLET, OBSIDIAN, glyph_column, obsidian, stone,
};

const PANEL: f32 = 1.9;
const PANEL_Y: f32 = 3.4;

pub struct AlienMonolithicMonument;

impl CatalogueEntry for AlienMonolithicMonument {
    fn slug(&self) -> &'static str {
        "alien_monolithic_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Glyph Slab"
    }
    fn description(&self) -> &'static str {
        "Obsidian slab with the room owner's likeness inset behind a glyph frame."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.6,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let dais = prim(
        solid(cuboid_tapered([3.4, 0.36, 2.0], 0.2, stone(DEAD_STONE))),
        [0.0, 0.18, 0.0],
        id_quat(),
    );
    // The slab. Perfectly plain — the theme's whole grammar is unmarked mass
    // plus light, so any moulding here would read as the wrong civilisation.
    let slab = prim(
        solid(cuboid_tapered([2.6, 5.4, 0.5], 0.03, obsidian(OBSIDIAN))),
        [0.0, 3.04, 0.0],
        id_quat(),
    );

    nest(dais, vec![nest(slab, inset(did)), counter_slab(1.75)])
}

/// The inset: a recessed obsidian field, the likeness, the glyph frame, and a
/// column of glyphs down each flank.
fn inset(did: &str) -> Vec<Generator> {
    let z = -0.31;
    let fr = 0.09;
    let mut out = vec![
        // Recessed field — the backing the single-sided panel needs, and what
        // makes the likeness read as *inside* the slab.
        prim(
            solid(cuboid_tapered(
                [PANEL + 0.22, PANEL + 0.22, 0.09],
                0.0,
                obsidian([0.05, 0.05, 0.07]),
            )),
            [0.0, PANEL_Y, z + 0.06],
            id_quat(),
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, z]),
        // Energy seam under the inset, the theme's cold signature.
        prim(
            cuboid_tapered([PANEL + 0.3, 0.08, 0.07], 0.0, glow(ENERGY_BLUE, 1.3)),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.3, z - 0.04],
            id_quat(),
        ),
    ];
    // Cyan glyph frame — thin strips, so the light is an edge rather than a
    // wash. The likeness is unlit and reads on its own; this is what makes the
    // obsidian legible at all after dark.
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([fr, PANEL + fr * 2.0, 0.09], 0.0, glow(GLYPH_CYAN, 1.2)),
            [sx * (PANEL + fr) * 0.5, PANEL_Y, z - 0.03],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            cuboid_tapered([PANEL, fr, 0.09], 0.0, glow(GLYPH_CYAN, 1.2)),
            [0.0, PANEL_Y + sy * (PANEL + fr) * 0.5, z - 0.03],
            id_quat(),
        ));
    }
    // Glyph columns down the flanks, using the kit's own generator so this
    // monument speaks the same alphabet as its gateway.
    for sx in [-1.0_f32, 1.0] {
        out.extend(glyph_column(
            sx * 1.14,
            0.9,
            5.4,
            z - 0.02,
            &[0.2, 0.14, 0.24, 0.12, 0.18],
            glow(GLYPH_VIOLET, 1.1),
        ));
    }
    out
}

/// A smaller slab canted beside the main one — the theme's habit of leaving a
/// second stone at an angle nothing explains. Tilted, and a leaf, so the
/// rotation carries nothing.
fn counter_slab(x: f32) -> Generator {
    prim(
        solid(cuboid_tapered([0.8, 2.2, 0.3], 0.05, obsidian(OBSIDIAN))),
        [x, 1.2, -0.4],
        quat_z(0.12),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &AlienMonolithicMonument.build("did:plc:test"),
            "alien_monolithic_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&AlienMonolithicMonument, "did:plc:test");
    }
}
