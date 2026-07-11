//! Social gateway — the theme-agnostic placeholder gate (#747). Every
//! seeded room places one gateway near spawn; the seeded wiring prefers a
//! per-theme bespoke gateway entry (`StructureRole::Gateway` tagged with
//! the room's theme) and falls back to this neutral arch until that
//! theme's bespoke pass lands (#749-#772). Owners can also place it from
//! the catalogue in customised rooms.
//!
//! The functional element is the [`GeneratorKind::Gateway`] zone child —
//! walking into it opens the destination picker listing the room owner's
//! mutual follows. The frame around it is deliberately quiet: grey stone
//! pillars, a lintel, and a soft cool glow strip that echoes the zone
//! veil without blooming white.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, foundation_mat, glow, id_quat, prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, GeneratorKind, SovereignMaterialSettings};

pub struct SocialGateway;

impl CatalogueEntry for SocialGateway {
    fn slug(&self) -> &'static str {
        "social_gateway"
    }
    fn name(&self) -> &'static str {
        "Social Gateway"
    }
    fn description(&self) -> &'static str {
        "Gate that lists the room owner's mutual follows as travel destinations."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    // themes() stays empty on purpose: the seeded wiring reaches this
    // entry by slug as the explicit fallback, so a bespoke per-theme
    // gateway automatically wins the `entries_for(theme, Gateway)` query
    // the moment it registers.
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

/// Weathered grey masonry for the pillars and lintel — neutral enough to
/// sit in any biome palette until the themed passes replace it.
fn masonry() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.52, 0.53, 0.57]),
        roughness: Fp(0.85),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    // Forecourt slab — the flat-base root (never tilt a root: children
    // would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.4, 0.3, 3.0], 0.0, foundation_mat())),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];
    // Pillars, lightly tapered so they read as masonry rather than posts.
    for x in [-1.9, 1.9] {
        prims.push(prim(
            solid(cuboid_tapered([0.7, 3.6, 0.7], 0.08, masonry())),
            [x, 2.1, 0.0],
            id_quat(),
        ));
    }
    // Lintel bridging the pillars.
    prims.push(prim(
        solid(cuboid_tapered([4.6, 0.5, 0.9], 0.0, masonry())),
        [0.0, 4.15, 0.0],
        id_quat(),
    ));
    // Soft glow strip under the lintel — deep-saturated cool tone at low
    // strength so it reads as an active threshold without white bloom.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.2, 0.12, 0.16],
            0.0,
            glow([0.35, 0.65, 1.0], 2.5),
        )),
        [0.0, 3.82, 0.0],
        id_quat(),
    ));
    // The walk-in zone between the pillars: bottom at the slab top,
    // headroom under the glow strip.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.95, 0.0],
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
        assert_sanitize_stable(&SocialGateway.build(""), "social_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = SocialGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
