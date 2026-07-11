//! Civic Gateway — the cross-theme fallback gate (#752). A dignified formal
//! portal in the classical civic idiom: two fluted marble columns on stepped
//! stone bases carry an architrave, a cornice and a low triangular pediment,
//! with a gilt civic seal set in the tympanum and warm sconce-lit threshold.
//!
//! `themes()` is left empty on purpose. The seeded wiring reaches a bespoke
//! per-theme gateway via the `entries_for(theme, Gateway)` query and falls
//! back to this neutral colonnade for any room whose theme has no gate of its
//! own, so it must sit comfortably in every biome — hence plain stone and
//! marble rather than a theme-specific palette.
//!
//! The one functional element is the [`GeneratorKind::Gateway`] zone child
//! centred in the opening; everything else frames it so it reads as a gate you
//! walk through. The gate front is `-Z` (hero convention): the seal faces the
//! render front.

use crate::catalogue::items::util::{
    cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, foundation_mat, glow, id_quat, prim,
    quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};

use super::{BRONZE, GOLD, LANTERN_WARM, MARBLE, STONE, bronze, marble, stone};

pub struct CivicGateway;

impl CatalogueEntry for CivicGateway {
    fn slug(&self) -> &'static str {
        "civic_gateway"
    }
    fn name(&self) -> &'static str {
        "Civic Gateway"
    }
    fn description(&self) -> &'static str {
        "A columned civic portal crowned by a pediment and a gilt seal."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    // themes() stays empty: this is the cross-theme fallback gate, reached by
    // slug when a room's theme ships no bespoke gateway of its own.
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
    use std::f32::consts::FRAC_PI_2;

    // Columns flank a ~2.85 m gap; the walk-through opening sits between them.
    let col_x = 1.9;

    // Forecourt slab — the flat-base root. Never tilt the root: `assemble`
    // rebases every child under it, so a rotated root would spin the whole gate.
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.6, 0.3, 2.6], 0.0, foundation_mat())),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // Polished threshold inlay across the opening — a marble band, set proud of
    // the slab top so its face never sits coplanar with the slab (z-fight).
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.1, 1.3], 0.0, marble(MARBLE))),
        [0.0, 0.34, 0.0],
        id_quat(),
    ));

    for sx in [-1.0_f32, 1.0] {
        let x = sx * col_x;
        // Stepped stone base under each column.
        prims.push(prim(
            solid(cuboid_tapered([0.95, 0.3, 0.95], 0.0, stone(STONE))),
            [x, 0.45, 0.0],
            id_quat(),
        ));
        // Fluted marble shaft with a slight entasis taper toward the top.
        prims.push(prim(
            solid(cylinder_tapered(0.36, 3.4, 20, 0.08, marble(MARBLE))),
            [x, 2.3, 0.0],
            id_quat(),
        ));
        // Square abacus capital, oversailing the shaft.
        prims.push(prim(
            solid(cuboid_tapered([0.85, 0.3, 0.85], 0.0, stone(STONE))),
            [x, 4.15, 0.0],
            id_quat(),
        ));
    }

    // Architrave spanning the capitals — the lintel of the gate.
    prims.push(prim(
        solid(cuboid_tapered([4.95, 0.5, 1.0], 0.0, marble(MARBLE))),
        [0.0, 4.55, 0.0],
        id_quat(),
    ));
    // Cornice, wider than the architrave so no two faces sit flush.
    prims.push(prim(
        solid(cuboid_tapered([5.2, 0.28, 1.16], 0.0, stone(STONE))),
        [0.0, 4.94, 0.0],
        id_quat(),
    ));
    // Low pediment — the X taper pinches the top to a ridge, giving the front
    // (-Z) face its triangular gable silhouette over a full-depth prism.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [4.75, 0.85, 1.0],
            [0.99, 0.0],
            stone(STONE),
        )),
        [0.0, 5.505, 0.0],
        id_quat(),
    ));

    // Gilt civic seal set in the tympanum, facing the -Z front: a ring around a
    // raised disc, both rotated a quarter-turn so their faces point forward.
    prims.push(prim(
        torus(0.06, 0.34, bronze(GOLD)),
        [0.0, 5.3, -0.56],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cylinder_tapered(0.28, 0.08, 20, 0.0, bronze(GOLD)),
        [0.0, 5.3, -0.54],
        quat_x(FRAC_PI_2),
    ));

    // Warm threshold strip under the architrave — a broad lit face at low
    // strength so it reads as lamplight on the lintel, not a white lightbox.
    prims.push(prim(
        cuboid_tapered([2.6, 0.14, 0.16], 0.0, glow(LANTERN_WARM, 2.6)),
        [0.0, 4.2, -0.2],
        id_quat(),
    ));

    // Sconce lanterns on the inner faces of the columns, lighting the passage.
    for sx in [-1.0_f32, 1.0] {
        // Bronze mounting bracket against the column.
        prims.push(prim(
            cuboid_tapered([0.16, 0.16, 0.2], 0.0, bronze(BRONZE)),
            [sx * 1.48, 2.6, -0.12],
            id_quat(),
        ));
        // Small warm orb — compact enough to run a touch hotter than the strip.
        prims.push(prim(
            sphere(0.14, 3, glow(LANTERN_WARM, 4.2)),
            [sx * 1.34, 2.6, -0.12],
            id_quat(),
        ));
    }

    // The walk-in zone: bottom at the slab top, headroom up under the lintel.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    super::assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CivicGateway.build(""), "civic_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = CivicGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
