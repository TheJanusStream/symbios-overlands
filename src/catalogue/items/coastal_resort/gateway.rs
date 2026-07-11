//! Resort Gateway — the Coastal-Resort bespoke social gateway (#754),
//! replacing the neutral placeholder arch for a seaside-holiday room. Two
//! whitewashed stucco piers carry a sun-greyed plank lintel and a raised
//! amber name-board over a ~2.6 m walk-through, dressed with a striped
//! deck-chair awning slung over the approach and warm lantern orbs on the
//! piers. A deep-aqua glow strip under the lintel and low pool-aqua
//! footlights lining the passage echo the zone veil without blooming white.
//!
//! The functional element is the single [`GeneratorKind::Gateway`] zone
//! centred in the opening — walking into it opens the destination picker.
//! Everything else is set-dressing that frames the opening as a promenade
//! gate you pass through. Primitive-built; authored in one flat
//! ground-relative frame via [`assemble`], which reparents every piece
//! under the plinth. The render front is -Z, so the lit name-board and the
//! awning face the approach.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, DECK_WOOD, LAMP_WARM, POOL_AQUA, SAND_TAN, SIGN_AMBER, STUCCO_WHITE,
    canvas, concrete, fx, plank, sand, stucco,
};

/// Pale board-formed promenade concrete for the gateway plinth — the sunlit
/// seafront paving the strip's arches stand on.
const PROMENADE: [f32; 3] = [0.86, 0.83, 0.76];

pub struct CoastalResortGateway;

impl CatalogueEntry for CoastalResortGateway {
    fn slug(&self) -> &'static str {
        "coastal_resort_gateway"
    }
    fn name(&self) -> &'static str {
        "Resort Gateway"
    }
    fn description(&self) -> &'static str {
        "Whitewashed promenade arch with a striped awning and a lit name-board, opening onto travel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
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
    // Piers flank a walk-through opening a touch wider than the zone box.
    let px = 1.85_f32; // pier centre offset (X)
    let front = -0.85_f32; // -Z approach side (hero convention)

    // Promenade plinth — the flat-base root. Never tilt a root: assemble()
    // stamps its transform onto every child, so a spun root spins the gate.
    let mut prims = vec![prim(
        solid(cuboid_tapered([5.6, 0.3, 3.0], 0.0, concrete(PROMENADE))),
        [0.0, 0.15, 0.0],
        id_quat(),
    )];

    // A rippled sand runner bedded into the plinth across the threshold, so
    // the crossing reads as a beach entrance rather than bare paving.
    prims.push(prim(
        solid(cylinder_tapered(1.3, 0.1, 20, 0.0, sand(SAND_TAN))),
        [0.0, 0.31, 0.0],
        id_quat(),
    ));

    // Two whitewashed stucco piers, lightly tapered so they read as masonry
    // rather than posts. Bases at the plinth top (y = 0.30).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.8, 3.5, 0.8], 0.06, stucco(STUCCO_WHITE))),
            [sx * px, 2.05, 0.0],
            id_quat(),
        ));
    }

    // Sun-greyed plank lintel bridging the pier tops, ends overhanging.
    prims.push(prim(
        solid(cuboid_tapered([4.7, 0.5, 0.9], 0.0, plank(DECK_WOOD))),
        [0.0, 4.0, 0.0],
        id_quat(),
    ));

    // Raised name-board over the lintel: a stucco backing panel carrying a
    // deep-amber lit face on the -Z front — the sign that reads across the
    // strand at dusk. Broad lit face at low strength holds its hue instead of
    // blooming to a white blank.
    prims.push(prim(
        solid(cuboid_tapered([3.6, 0.9, 0.24], 0.0, stucco(STUCCO_WHITE))),
        [0.0, 4.65, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.2, 0.6, 0.08], 0.0, glow(SIGN_AMBER, 2.2)),
        [0.0, 4.65, -0.15],
        id_quat(),
    ));

    // Warm lantern orbs on the lintel above each pier, lighting the
    // threshold at dusk.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            sphere(0.17, 4, glow(LAMP_WARM, 2.6)),
            [sx * px, 4.45, 0.0],
            id_quat(),
        ));
    }

    // Striped deck-chair awning slung off the lintel front over the approach,
    // tilted so its leading edge drops toward the strand — the beach-kiosk
    // read that names the theme at a glance.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.6, 0.16, 1.1],
            0.05,
            canvas(AWNING_RED, AWNING_WHITE),
        )),
        [0.0, 3.9, front],
        quat_x(-0.32),
    ));

    // Deep-aqua glow strip under the lintel — a thin trim run can sit a touch
    // hot; the deep-saturated sea tone reads as an active threshold, echoing
    // the walk-in zone's veil without white bloom.
    prims.push(prim(
        cuboid_tapered([2.9, 0.12, 0.16], 0.0, glow(POOL_AQUA, 2.8)),
        [0.0, 3.62, 0.0],
        id_quat(),
    ));

    // Low pool-aqua footlights lining the inner pier faces, framing the
    // passage from the ground up.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            cuboid_tapered([0.08, 0.16, 1.6], 0.0, glow(POOL_AQUA, 2.4)),
            [sx * 1.4, 0.5, 0.0],
            id_quat(),
        ));
    }

    // The walk-in zone between the piers: floor at the plinth top, headroom
    // under the lintel. Bare kind — the gateway takes no material.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.95, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a soft sea breeze breathing over the promenade gate.
    root.audio = fx::sea_breeze();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CoastalResortGateway.build(""), "coastal_resort_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = CoastalResortGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
