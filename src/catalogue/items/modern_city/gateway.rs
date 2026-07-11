//! Transit Portal — the Modern-City social gateway (#762). A pair of board-
//! formed concrete pylons in steel collars carry a boxed steel span, a
//! cantilevered glass canopy, and a lit transit roundel facing the −Z render
//! front. Cool scanner light rakes the threshold so the walk-through reads as
//! a fare-gate onto the transit network. The functional element is the single
//! [`GeneratorKind::Gateway`] zone centred in the opening — walking into it
//! opens the destination picker; everything else is set-dressing that frames
//! it as a gate you pass through.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;
use std::f32::consts::FRAC_PI_2;

use super::{CONCRETE_GREY, GLASS_TEAL, STEEL_GREY, concrete, glass, steel};

/// Cool transit cyan — the network's wayfinding accent for scanner lines and
/// roundel bar.
const TRANSIT_CYAN: [f32; 3] = [0.32, 0.72, 0.98];
/// Deep signal blue for the broad roundel disc — deep-saturated so it holds
/// its colour at low emissive strength instead of blooming to white.
const TRANSIT_BLUE: [f32; 3] = [0.14, 0.34, 0.82];

pub struct ModernCityGateway;

impl CatalogueEntry for ModernCityGateway {
    fn slug(&self) -> &'static str {
        "modern_city_gateway"
    }
    fn name(&self) -> &'static str {
        "Transit Portal"
    }
    fn description(&self) -> &'static str {
        "Concrete-and-steel fare-gate under a glass canopy and a lit transit roundel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
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
    let deck_h = 0.3;
    let deck_top = deck_h;
    let pylon_h = 4.2;
    let pylon_top = deck_top + pylon_h;
    let span_y = pylon_top + 0.3; // boxed crossbeam centre

    // Concrete forecourt deck — the flat-base root (never tilt a root: every
    // child inherits its transform and would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.4, deck_h, 3.0],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, deck_top * 0.5, 0.0],
        id_quat(),
    )];

    // Twin pylons flanking a 3.0 m opening: a steel base collar, a tapered
    // concrete shaft, a steel cap, and a hot cyan light rail up the inner
    // corner that rakes the −Z front of the gap.
    for sx in [-1.0_f32, 1.0] {
        let x = sx * 1.9;
        // Cast steel base collar.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.98, 0.4, 0.98],
                0.1,
                steel([0.32, 0.33, 0.35]),
            )),
            [x, deck_top + 0.2, 0.0],
            id_quat(),
        ));
        // Board-formed concrete shaft, lightly tapered.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.8, pylon_h, 0.8],
                0.06,
                concrete(CONCRETE_GREY),
            )),
            [x, deck_top + pylon_h * 0.5, 0.0],
            id_quat(),
        ));
        // Steel capital under the span.
        prims.push(prim(
            solid(cuboid_tapered([0.92, 0.3, 0.92], 0.0, steel(STEEL_GREY))),
            [x, pylon_top + 0.15, 0.0],
            id_quat(),
        ));
        // Cyan light rail on the inner front corner — thin trim runs hot.
        prims.push(prim(
            cuboid_tapered([0.08, 3.5, 0.12], 0.0, glow(TRANSIT_CYAN, 5.5)),
            [sx * 1.46, deck_top + 1.85, -0.38],
            id_quat(),
        ));
    }

    // Boxed steel crossbeam spanning the pylons.
    prims.push(prim(
        solid(cuboid_tapered([4.6, 0.6, 0.7], 0.0, steel(STEEL_GREY))),
        [0.0, span_y, 0.0],
        id_quat(),
    ));
    // Dark steel fascia on the −Z front carrying the signage.
    prims.push(prim(
        solid(cuboid_tapered(
            [4.4, 0.7, 0.16],
            0.0,
            steel([0.16, 0.17, 0.2]),
        )),
        [0.0, span_y - 0.05, -0.42],
        id_quat(),
    ));
    // Cantilevered glass entrance canopy projecting over the threshold.
    prims.push(prim(
        solid(cuboid_tapered([4.6, 0.14, 0.24], 0.0, steel(STEEL_GREY))),
        [0.0, span_y + 0.42, -1.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([4.2, 0.08, 1.7], 0.0, glass(GLASS_TEAL, 0.8)),
        [0.0, span_y + 0.34, -1.05],
        id_quat(),
    ));

    // Transit roundel on the fascia front: a deep-blue disc crossed by a bright
    // cyan bar — the classic wayfinding mark, facing the −Z hero front. The
    // broad disc face runs low so it reads as lit colour; the thin bar runs hot.
    prims.push(prim(
        cylinder_tapered(0.62, 0.12, 20, 0.0, glow(TRANSIT_BLUE, 2.4)),
        [0.0, span_y, -0.56],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cuboid_tapered([1.5, 0.3, 0.14], 0.0, glow(TRANSIT_CYAN, 5.5)),
        [0.0, span_y, -0.64],
        id_quat(),
    ));

    // Lit destination marquee strip under the beam, facing −Z. Broad face, low
    // strength.
    prims.push(prim(
        cuboid_tapered([3.2, 0.34, 0.1], 0.0, glow(TRANSIT_CYAN, 2.0)),
        [0.0, pylon_top - 0.35, -0.5],
        id_quat(),
    ));

    // Scanner light bar inlaid across the deck at the threshold — a broad flat
    // top face, so it stays low.
    prims.push(prim(
        cuboid_tapered([2.6, 0.06, 0.4], 0.0, glow(TRANSIT_CYAN, 2.2)),
        [0.0, deck_top + 0.02, 0.0],
        id_quat(),
    ));

    // The walk-in zone between the pylons: floor at the deck top, headroom
    // under the crossbeam. This is the gate's only functional element.
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
        assert_sanitize_stable(&ModernCityGateway.build(""), "modern_city_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = ModernCityGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
