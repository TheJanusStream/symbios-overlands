//! Airlock Gateway — the Space-Outpost bespoke social gate (#768). Replaces the
//! neutral placeholder arch for this theme: two hull-plated door jambs on steel
//! footings frame a pressure-lock threshold, bridged by a hull header with a
//! conduit truss and a pair of relief-vent stacks. Status-green light strips
//! line the opening, a cyan lit port reads the portal as cycling, and red
//! caution lamps flank the lintel. The single functional element is the
//! [`GeneratorKind::Gateway`] zone standing in the opening — walking into it
//! opens the destination picker; everything else is airlock set-dressing.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, quat_z, solid, sphere,
    torus, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, PAD_GREY, STATUS_GREEN, STEEL_DARK,
    VIEWPORT_LIT, concrete, hull, painted, steel,
};

pub struct SpaceOutpostGateway;

impl CatalogueEntry for SpaceOutpostGateway {
    fn slug(&self) -> &'static str {
        "space_outpost_gateway"
    }
    fn name(&self) -> &'static str {
        "Airlock Gateway"
    }
    fn description(&self) -> &'static str {
        "Pressure-lock portal whose cycling threshold lists the room owner's mutual follows."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
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
    let pad_top = 0.28_f32;
    let jamb_h = 3.7_f32;
    let jamb_top = pad_top + jamb_h; // 3.98
    let header_y = 4.15_f32;

    // Ceramic landing-pad plinth — the flat-base root (never tilt a root:
    // every child would spin with it).
    let mut prims = vec![prim(
        solid(cuboid_tapered([4.8, pad_top, 2.6], 0.0, concrete(PAD_GREY))),
        [0.0, pad_top * 0.5, 0.0],
        id_quat(),
    )];

    // Hazard floor marking across the threshold — the airlock's caution band.
    prims.push(prim(
        cuboid_tapered([2.6, 0.05, 1.0], 0.0, painted(HAZARD_YELLOW)),
        [0.0, pad_top + 0.02, 0.0],
        id_quat(),
    ));

    // Door jambs: hull-plated stanchions on steel footings, lightly tapered so
    // they read as structural pylons rather than posts. A hazard band girds
    // each near the threshold.
    for x in [-1.75_f32, 1.75] {
        prims.push(prim(
            solid(cuboid_tapered([0.62, jamb_h, 0.7], 0.06, hull(HULL_WHITE))),
            [x, pad_top + jamb_h * 0.5, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.8, 0.36, 0.86], 0.0, steel(STEEL_DARK))),
            [x, pad_top + 0.18, 0.0],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered(
                [0.66, 0.3, 0.74],
                0.0,
                painted(HAZARD_YELLOW),
            )),
            [x, pad_top + 0.9, 0.0],
            id_quat(),
        ));
    }

    // Status-green light strips lining the inner face of each jamb — deep-
    // saturated so bloom holds the true green instead of washing to mint.
    for x in [-1.42_f32, 1.42] {
        prims.push(prim(
            cuboid_tapered([0.08, 2.4, 0.16], 0.0, glow(STATUS_GREEN, 3.0)),
            [x, 2.0, 0.0],
            id_quat(),
        ));
    }

    // Hull header bridging the jambs — the lintel span.
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.5, 0.85], 0.0, hull(HULL_PANEL))),
        [0.0, header_y, 0.0],
        id_quat(),
    ));
    // Conduit truss running across the header front (Y-axis pipe laid onto X).
    prims.push(prim(
        solid(cylinder_tapered(0.1, 4.2, 8, 0.0, steel(STEEL_DARK))),
        [0.0, header_y - 0.33, -0.36],
        quat_z(FRAC_PI_2),
    ));
    // Pressure-relief vent stacks standing on the header.
    for x in [-1.2_f32, 1.2] {
        prims.push(prim(
            solid(tube(0.16, 0.09, 0.6, 10, steel(STEEL_DARK))),
            [x, header_y + 0.55, 0.0],
            id_quat(),
        ));
    }

    // Threshold glow bar under the lintel — a broad cyan strip at low strength
    // so it reads as a lit sill, not a white lightbox.
    prims.push(prim(
        cuboid_tapered([3.0, 0.12, 0.16], 0.0, glow(VIEWPORT_LIT, 2.2)),
        [0.0, jamb_top - 0.2, -0.36],
        id_quat(),
    ));

    // Cycling-status emblem on the −Z hero face: a lit round port in a bolted
    // steel rim, so the gate reads as an airlock mid-cycle.
    prims.push(prim(
        torus(0.05, 0.32, steel(STEEL_DARK)),
        [0.0, header_y, -0.45],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        cylinder_tapered(0.28, 0.08, 16, 0.0, glow(VIEWPORT_LIT, 2.8)),
        [0.0, header_y, -0.47],
        quat_x(FRAC_PI_2),
    ));
    // Red caution lamps flanking the lintel — small hot orbs the ruin pass can
    // snuff.
    for x in [-1.75_f32, 1.75] {
        prims.push(prim(
            sphere(0.12, 4, glow(BEACON_RED, 5.5)),
            [x, header_y - 0.2, -0.47],
            id_quat(),
        ));
    }

    // The walk-in zone standing in the opening: floor at the pad top, headroom
    // clearing the glow bar under the lintel.
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
        assert_sanitize_stable(&SpaceOutpostGateway.build(""), "space_outpost_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = SpaceOutpostGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
