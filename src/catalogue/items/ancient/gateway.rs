//! Classical Propylaea — the AncientClassical bespoke social gateway (#751),
//! replacing the neutral placeholder arch for this theme. A monumental Doric
//! porch: a stepped sandstone stylobate carrying two pairs of fluted marble
//! columns, a full entablature, and a triangular pediment gable facing the
//! approach, with a bronze victory wreath on the tympanum and warm votive
//! fire framing the passage you walk through.
//!
//! The only functional element is the single [`GeneratorKind::Gateway`] zone
//! centred in the intercolumniation — walking into it opens the destination
//! picker. Everything else frames that opening so it reads as a gate: two
//! flanking column pairs, an entablature span across the top, and threshold
//! firelight. The gate front is `-Z` (hero convention): the pediment, the
//! wreath emblem, and the gilded frieze inscription all face `-Z`.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, prim, quat_x,
    solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{BRONZE_GREEN, EMBER_ORANGE, MARBLE_WHITE, SANDSTONE_GOLD, bronze, marble, sandstone};

pub struct AncientGateway;

impl CatalogueEntry for AncientGateway {
    fn slug(&self) -> &'static str {
        "ancient_gateway"
    }
    fn name(&self) -> &'static str {
        "Classical Propylaea"
    }
    fn description(&self) -> &'static str {
        "Columned marble porch under a pedimented gable, its threshold lit by votive fire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
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
    // Stepped stylobate — the flat-base root (never tilt a root: children
    // would spin with it). Bottom step first, then the top tread.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.6, 0.3, 3.0],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];
    prims.push(prim(
        solid(cuboid_tapered(
            [5.1, 0.3, 2.6],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [0.0, 0.45, 0.0],
        id_quat(),
    ));
    let syl_top = 0.6;
    let shaft_h = 3.0;

    // Two column pairs flanking a ~2.8 m intercolumniation — a front and a
    // back row so the porch reads as a walk-through propylaea, not a facade.
    // Each column: base drum, tapered fluted shaft, square Doric capital.
    let cap_top = syl_top + 0.3 + shaft_h + 0.35;
    for x in [-1.8, 1.8] {
        for z in [-0.65, 0.65] {
            prims.push(prim(
                solid(cylinder_tapered(0.46, 0.3, 16, 0.0, marble(MARBLE_WHITE))),
                [x, syl_top + 0.15, z],
                id_quat(),
            ));
            prims.push(prim(
                solid(cylinder_tapered(
                    0.4,
                    shaft_h,
                    16,
                    0.12,
                    marble(MARBLE_WHITE),
                )),
                [x, syl_top + 0.3 + shaft_h * 0.5, z],
                id_quat(),
            ));
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.85, 0.35, 0.85],
                    0.0,
                    marble(MARBLE_WHITE),
                )),
                [x, cap_top - 0.175, z],
                id_quat(),
            ));
        }
    }

    // Entablature spanning all four capitals: sandstone architrave beam +
    // oversailing marble cornice.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.5, 0.55, 2.0],
            0.0,
            sandstone(SANDSTONE_GOLD),
        )),
        [0.0, cap_top + 0.275, 0.0],
        id_quat(),
    ));
    let corn_y = cap_top + 0.55 + 0.175;
    prims.push(prim(
        solid(cuboid_tapered([4.9, 0.35, 2.2], 0.0, marble(MARBLE_WHITE))),
        [0.0, corn_y, 0.0],
        id_quat(),
    ));
    let entab_top = cap_top + 0.9;

    // Triangular pediment gable atop the cornice — taper pinches the Z sides
    // to a ridge line along X, so the tympanum triangles face front (-Z) and
    // back (+Z), the classic Doric gable silhouette.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [4.8, 1.1, 2.0],
            [0.0, 0.99],
            marble(MARBLE_WHITE),
        )),
        [0.0, entab_top + 0.55, 0.0],
        id_quat(),
    ));

    // Bronze victory wreath on the tympanum front face (-Z) — the gate's
    // emblem, stood vertical to face the approach.
    prims.push(prim(
        torus(0.07, 0.52, bronze(BRONZE_GREEN)),
        [0.0, entab_top + 0.5, -1.02],
        quat_x(std::f32::consts::FRAC_PI_2),
    ));

    // Gilded inscription band across the frieze front (-Z) — a broad flat
    // strip, so LOW emissive strength to read as gilt lettering not white glow.
    prims.push(prim(
        cuboid_tapered([2.6, 0.12, 0.04], 0.0, glow([1.0, 0.74, 0.34], 2.0)),
        [0.0, cap_top + 0.3, -1.02],
        id_quat(),
    ));

    // Warm votive underglow along the top of the opening — a thin ember trim
    // under the architrave, run hot because it is a slim edge, not a face.
    prims.push(prim(
        cuboid_tapered([2.8, 0.1, 0.14], 0.0, glow(EMBER_ORANGE, 5.0)),
        [0.0, cap_top - 0.1, -0.65],
        id_quat(),
    ));
    // Two votive fire orbs at the front column bases, framing the threshold
    // at foot level and lighting the passage floor.
    for x in [-1.4, 1.4] {
        prims.push(prim(
            sphere(0.13, 6, glow(EMBER_ORANGE, 4.0)),
            [x, syl_top + 0.22, -0.65],
            id_quat(),
        ));
    }

    // The single walk-in zone in the intercolumniation: bottom at the
    // stylobate tread, headroom up under the architrave.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.8, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&AncientGateway.build(""), "ancient_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is a porch, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = AncientGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
