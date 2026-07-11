//! Neighborhood Arch — the Suburban bespoke social gateway (#771), replacing
//! the neutral placeholder arch for a residential-street room. Two tan brick
//! piers with white coping caps carry a white-painted timber lintel and a
//! raised name-board under a little shingled gable — the subdivision-entrance
//! monument you drive past turning onto the street. Black coach-lamp lanterns
//! crown the piers, a warm amber sign reads over the walk-through, a low warm
//! glow strip lines the threshold and clipped hedges flank the approach.
//!
//! The functional element is the single [`GeneratorKind::Gateway`] zone centred
//! in the ~2.6 m opening — walking into it opens the destination picker listing
//! the room owner's mutual follows. Everything else is themed set-dressing that
//! frames the opening as a gate you pass through. Primitive-built; authored in
//! one flat ground-relative frame via [`assemble`], which reparents every piece
//! under the base slab. The render front is -Z, so the lit name-board faces the
//! approach.

use crate::catalogue::items::roadside::sign_board;
use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, HEDGE_GREEN, PORCH_WARM, RENDER_WHITE, ROOF_GREY, SIGN_GLOW, WOOD_WHITE, brick,
    enamel, fx, render, shingle, wood,
};

pub struct SuburbanGateway;

impl CatalogueEntry for SuburbanGateway {
    fn slug(&self) -> &'static str {
        "suburban_gateway"
    }
    fn name(&self) -> &'static str {
        "Neighborhood Arch"
    }
    fn description(&self) -> &'static str {
        "Brick-pier subdivision arch with coach lamps and a lit name-board under a shingled gable, opening onto travel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    // No prosperity_band(): the gateway is the theme's per-theme fallback
    // matched by role, so it must place in a suburban room of any prosperity —
    // even the trailer-lot end gets its neighborhood arch near spawn.
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
    let px = 1.85_f32; // pier centre offset (X)
    let base_h = 0.3_f32; // slab thickness; its top (y = 0.30) is the piers' floor

    // Concrete apron — the flat-base root. Never tilt a root: assemble() stamps
    // its transform onto every child, so a spun root spins the whole gate.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.6, base_h, 3.0],
            0.0,
            render([0.62, 0.61, 0.59]),
        )),
        [0.0, base_h * 0.5, 0.0],
        id_quat(),
    )];

    // A brick paver runner bedded into the apron across the threshold, so the
    // crossing reads as a swept neighborhood entry walk, not bare concrete.
    prims.push(prim(
        solid(cuboid_tapered([2.6, 0.1, 1.4], 0.0, brick(BRICK_TAN))),
        [0.0, 0.31, 0.0],
        id_quat(),
    ));

    // Two tan brick piers, lightly tapered so they read as masonry. Bases at
    // the apron top (y = 0.30); tops at y = 3.60.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.8, 3.3, 0.8], 0.05, brick(BRICK_TAN))),
            [sx * px, 1.95, 0.0],
            id_quat(),
        ));
    }

    // White-painted timber lintel bridging the pier tops, ends overhanging.
    prims.push(prim(
        solid(cuboid_tapered([4.7, 0.5, 0.9], 0.0, wood(WOOD_WHITE))),
        [0.0, 3.85, 0.0],
        id_quat(),
    ));

    // A white render coping cap over each pier, a coach-lamp housing on the cap
    // and a warm lit globe — the crown of each support. The globe is emissive
    // trim (a small orb, so it can run a touch warm) that the ruin pass darkens.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.9, 0.28, 0.9], 0.2, render(RENDER_WHITE))),
            [sx * px, 4.24, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [0.24, 0.26, 0.24],
                0.1,
                enamel([0.1, 0.1, 0.12]),
            )),
            [sx * px, 4.51, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.14, 4, glow(PORCH_WARM, 2.6)),
            [sx * px, 4.77, 0.0],
            id_quat(),
        ));
    }

    // Raised name-board over the lintel: a white render backing panel carrying a
    // deep-amber lit sign on the -Z front — the street name that reads at dusk.
    // The sign is segmented via sign_board (dark cell gaps + low strength) so a
    // broad lit face holds its warm hue instead of blooming to a white blank.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.85, 0.22], 0.0, render(RENDER_WHITE))),
        [0.0, 4.55, 0.0],
        id_quat(),
    ));
    prims.extend(sign_board(
        [0.0, 4.55, -0.12],
        [2.6, 0.58],
        (4, 1),
        SIGN_GLOW,
        2.2,
        -1.0,
    ));

    // A little shingled gable over the name-board — the neighborhood roofline
    // echoed in miniature, so the gate MEANS a home street and not a plain
    // beam. Pinch the front X width to an apex ridge, keep the full depth.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [3.4, 0.7, 0.95],
            [0.98, 0.0],
            shingle(ROOF_GREY),
        )),
        [0.0, 5.33, 0.0],
        id_quat(),
    ));

    // Warm glow strip tucked under the lintel — a thin trim run at low strength,
    // an active threshold line echoing the walk-in zone's veil without bloom.
    prims.push(prim(
        cuboid_tapered([2.9, 0.12, 0.16], 0.0, glow(PORCH_WARM, 2.4)),
        [0.0, 3.5, 0.0],
        id_quat(),
    ));

    // Low warm footlights lining the inner pier faces, framing the passage from
    // the ground up.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.08, 0.16, 1.5], 0.0, glow(PORCH_WARM, 2.2)),
            [sx * 1.4, 0.55, 0.0],
            id_quat(),
        ));
    }

    // Clipped hedges flanking the approach — leafy clumps, the manicured planting
    // that names a subdivision entrance.
    for sx in [-1.0_f32, 1.0] {
        prims.extend(crop_tufts(
            [sx * 2.9, base_h, 0.0],
            [1.4, 2.0],
            3,
            2,
            0.85,
            foliage(HEDGE_GREEN),
        ));
    }

    // The walk-in zone between the piers: floor at the apron top, headroom under
    // the lintel. Bare kind — the gateway takes no material.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: birdsong drifting over the street at the gate.
    root.audio = fx::birdsong();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SuburbanGateway.build(""), "suburban_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = SuburbanGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
