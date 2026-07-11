//! Hive Maw Gateway — the Alien-Organic bespoke social gateway (#750). Two
//! chitin mandible piers flank a walk-through gap, bridged by a bulbous chitin
//! brow that fangs hang from and a saturated biolume throat lighting the
//! opening: a living maw that swallows travellers through to distant rooms.
//! It replaces the neutral placeholder arch for the theme; the seeded wiring
//! prefers this `StructureRole::Gateway` entry the moment it registers.
//!
//! The one functional element is the [`GeneratorKind::Gateway`] zone child —
//! walking into it opens the destination picker. Everything else frames that
//! zone so it reads as a maw you pass through.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the creep forecourt pad (the root, `id_quat` — a tilted root would
//! spin the whole maw).

use std::f32::consts::PI;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, prim_scaled, quat_x,
    quat_z, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_CYAN, BIOLUME_GREEN, CHITIN_DARK, FLESH_RED, chitin, flesh, fx, glow_veins};

pub struct AlienOrganicGateway;

impl CatalogueEntry for AlienOrganicGateway {
    fn slug(&self) -> &'static str {
        "alien_organic_gateway"
    }
    fn name(&self) -> &'static str {
        "Hive Maw Gateway"
    }
    fn description(&self) -> &'static str {
        "Living chitin maw whose glowing throat swallows travellers through to distant rooms."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
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
    // Creep forecourt pad — the flat-base root (id_quat); a tilted root would
    // spin every mandible and fang into its frame.
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.0, 0.3, 2.2], 0.05, flesh(FLESH_RED))),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // Two chitin mandible piers flanking the ~2.6 m walk-through gap, plus a
    // biolume pod glowing through each shell on the −Z hero face and an
    // outward-leaning tusk at its foot.
    for x in [-1.7_f32, 1.7] {
        // Main rib shaft, lightly tapered so it reads as living chitin.
        prims.push(prim(
            solid(cylinder_tapered(0.44, 3.6, 8, 0.28, chitin(CHITIN_DARK))),
            [x, 1.8, 0.0],
            id_quat(),
        ));
        // Mid knuckle bulge so the pier is a strut, not a dowel.
        prims.push(prim(
            solid(sphere(0.5, 5, chitin(CHITIN_DARK))),
            [x, 1.95, 0.0],
            id_quat(),
        ));
        // Biolume pod glowing through the shell on the −Z front.
        prims.push(prim(
            solid(sphere(0.24, 4, glow(BIOLUME_CYAN, 2.0))),
            [x, 2.5, -0.4],
            id_quat(),
        ));
        // Foot tusk flanking the entrance, leaning outward away from the gap.
        prims.push(prim(
            solid(cone(0.22, 1.2, 6, chitin(CHITIN_DARK))),
            [x + x.signum() * 0.4, 0.85, 0.1],
            quat_z(-x.signum() * 0.4),
        ));
    }

    // Upper jaw — a bulbous flattened chitin brow arching across both piers
    // (a swelling bulb, not a flat lintel), girdled by a carapace rib band.
    prims.push(prim_scaled(
        solid(sphere(2.1, 5, chitin(CHITIN_DARK))),
        [0.0, 3.7, 0.0],
        id_quat(),
        [1.0, 0.42, 0.7],
    ));
    prims.push(prim(
        solid(torus(0.18, 1.9, chitin(CHITIN_DARK))),
        [0.0, 3.7, 0.0],
        id_quat(),
    ));

    // Upper fang row hanging from the brow over the opening — the maw's teeth,
    // on the −Z hero front.
    for i in 0..4 {
        let fang_x = -1.05 + i as f32 * 0.7;
        prims.push(prim(
            solid(cone(0.13, 0.6, 6, chitin(CHITIN_DARK))),
            [fang_x, 3.0, -0.55],
            quat_x(PI),
        ));
    }

    // Threshold biolume framing the opening on three sides — a saturated cyan
    // strip under the brow and a strip lining each pier's inner face. Deep
    // hue at low strength so it reads as lit tissue, never white bloom.
    prims.push(prim(
        solid(cuboid_tapered(
            [2.6, 0.12, 0.14],
            0.0,
            glow(BIOLUME_CYAN, 2.2),
        )),
        [0.0, 2.75, -0.45],
        id_quat(),
    ));
    for x in [-1.3_f32, 1.3] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.1, 2.4, 0.14],
                0.0,
                glow(BIOLUME_CYAN, 2.0),
            )),
            [x, 1.55, -0.4],
            id_quat(),
        ));
    }

    // Glowing vein emblem blazoned on the −Z brow face — the maw's living mark
    // that names the gate from the front.
    for v in glow_veins([0.0, 3.7, -1.42], -0.12, 1.1, glow(BIOLUME_GREEN, 1.9)) {
        prims.push(v);
    }

    // The walk-through maw: the one functional zone, centred in the opening
    // from the creep floor up under the brow.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    // The maw breathes and exhales glowing spores through the threshold.
    prims.push(fx::spore_drift([0.0, 1.6, 0.0], 0x0A11_6A6E));

    let mut root = assemble(prims);
    root.audio = fx::bio_pulse();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&AlienOrganicGateway.build(""), "alien_organic_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = AlienOrganicGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
