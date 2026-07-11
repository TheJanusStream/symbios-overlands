//! Monolith Gateway — the Alien-Monolithic bespoke social gateway (#749). Two
//! black obsidian monolith pylons flank a walk-through gap, bridged by a
//! glyph-lit obsidian lintel, humming over a charged threshold. Replaces the
//! neutral placeholder gate for this theme.
//!
//! The one functional element is the [`GeneratorKind::Gateway`] zone child
//! centred in the opening — walking into it opens the destination picker.
//! Everything else is set-dressing that frames the zone as a gate you pass
//! through: the pylons, the lintel span, and the emissive threshold trim, with
//! the walk-through opening kept clear in the middle. Its glyphs, collars and
//! threshold line are emissive trim escalation's ruin pass can snuff.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the forecourt plinth.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{ENERGY_BLUE, GLYPH_CYAN, GLYPH_VIOLET, OBSIDIAN, fx, glyph_column, obsidian};

pub struct AlienMonolithicGateway;

impl CatalogueEntry for AlienMonolithicGateway {
    fn slug(&self) -> &'static str {
        "alien_monolithic_gateway"
    }
    fn name(&self) -> &'static str {
        "Monolith Gateway"
    }
    fn description(&self) -> &'static str {
        "Two black obsidian monoliths bridged by a glyph-lit lintel, humming over a charged threshold."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.5,
            min_spawn_dist: 8.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.3_f32;
    let base_top = base_h; // 0.3
    let pylon_h = 3.3_f32;
    let pylon_x = 1.8_f32; // pylon centre; inner faces ≈ ±1.35 → ~2.7 m gap
    let pylon_cy = base_top + pylon_h * 0.5; // 1.95
    let pylon_top = base_top + pylon_h; // 3.6
    let lintel_h = 0.55_f32;
    let lintel_cy = pylon_top + lintel_h * 0.5; // 3.875
    let zf_pylon = -(0.45 + 0.04); // proud of a pylon's −Z hero face
    let zf_lintel = -(0.5 + 0.04); // proud of the lintel's −Z hero face

    let mut prims = vec![
        // Obsidian forecourt plinth — the flat-base root. Never tilt a root:
        // `assemble` applies its transform to every child, so a rotated root
        // would spin the whole gate.
        prim(
            solid(cuboid_tapered([5.2, base_h, 2.6], 0.0, obsidian(OBSIDIAN))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Two flanking obsidian monolith pylons, lightly tapered so they read as
    // standing slabs rather than plain posts.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.9, pylon_h, 0.9], 0.1, obsidian(OBSIDIAN))),
            [sx * pylon_x, pylon_cy, 0.0],
            id_quat(),
        ));
    }

    // Obsidian lintel monolith bridging the pylon tops — the span across the
    // gate, laid like a monolith on its side.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.6, lintel_h, 1.0],
            0.05,
            obsidian(OBSIDIAN),
        )),
        [0.0, lintel_cy, 0.0],
        id_quat(),
    ));

    // Glowing energy collars ringing each pylon — emissive halo rings standing
    // proud of the shaft.
    for sx in [-1.0_f32, 1.0] {
        for (k, major) in [0.6_f32, 0.54].into_iter().enumerate() {
            let y = base_top + 0.9 + k as f32 * 1.5;
            prims.push(prim(
                torus(0.07, major, glow(GLYPH_CYAN, 2.4)),
                [sx * pylon_x, y, 0.0],
                id_quat(),
            ));
        }
    }

    // Inscribed glyph columns down the −Z hero front of each pylon — asymmetric
    // alien script, emissive, varied stroke heights so the column doesn't read
    // as one stamp repeated.
    for sx in [-1.0_f32, 1.0] {
        for g in glyph_column(
            sx * pylon_x,
            base_top + 0.6,
            pylon_top - 0.5,
            zf_pylon,
            &[0.6, 0.8, 0.55, 0.7],
            glow(GLYPH_CYAN, 2.0),
        ) {
            prims.push(g);
        }
    }

    // Glowing threshold bar just under the lintel spanning the opening — a thin
    // luminous line, run at the theme's warm-but-safe strength so it reads as a
    // charged lintel seam without blooming white.
    prims.push(prim(
        cuboid_tapered([3.4, 0.12, 0.16], 0.0, glow(GLYPH_CYAN, 2.6)),
        [0.0, pylon_top - 0.12, -0.28],
        id_quat(),
    ));

    // Keystone destination sigil at the lintel front centre — a single deep
    // violet glyph facing the −Z hero side.
    for g in glyph_column(
        0.0,
        lintel_cy,
        lintel_cy,
        zf_lintel,
        &[0.85],
        glow(GLYPH_VIOLET, 2.2),
    ) {
        prims.push(g);
    }

    // Lit threshold line inlaid in the plinth, running the walk axis through the
    // opening — a broad face-up strip held at low strength so it stays lit blue,
    // not a white lightbox.
    prims.push(prim(
        cuboid_tapered([0.6, 0.06, 2.4], 0.0, glow(ENERGY_BLUE, 1.6)),
        [0.0, base_top + 0.04, 0.0],
        id_quat(),
    ));

    // The walk-in zone between the pylons: floor at the plinth top, headroom
    // under the lintel seam. The one functional element.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.8, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the gate hums with the monolith array's voice, energy
    // motes rising through the charged threshold.
    root.audio = fx::monolith_hum();
    root.children
        .push(fx::energy_motes([0.0, 0.5, 0.0], 0x0A30_9A71));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &AlienMonolithicGateway.build(""),
            "alien_monolithic_gateway",
        );
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = AlienMonolithicGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
