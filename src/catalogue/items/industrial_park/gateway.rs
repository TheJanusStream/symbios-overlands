//! Works Gate — the Industrial-Park bespoke social gateway (#759). A steel
//! portal-frame gantry straddling the yard entrance: two braced lattice pylons
//! on concrete footings, a clad box-girder span trussed to a lower pipe chord,
//! and an amber-lit sign board over the walk-through. The functional element is
//! the single [`GeneratorKind::Gateway`] zone hung in the opening — walking into
//! it opens the destination picker of the room owner's mutual follows. The
//! frame replaces the neutral placeholder arch in an industrial idiom: plant
//! steelwork, sodium-lit signage, and a hazard-lit threshold beam, all facing
//! the `-Z` hero front.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, LAMP_AMBER, PIPE_GREY, STEEL_BLUE, cladding, concrete, gauge_plate,
    lattice_mast, tank_steel,
};

pub struct IndustrialParkGateway;

impl CatalogueEntry for IndustrialParkGateway {
    fn slug(&self) -> &'static str {
        "industrial_park_gateway"
    }
    fn name(&self) -> &'static str {
        "Works Gate"
    }
    fn description(&self) -> &'static str {
        "Steel gantry gate of lattice pylons and a signed pipe truss over an amber-lit threshold."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
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
    // Portal dimensions. The opening sits between the two pylons; the gantry
    // span rides their tops.
    let base_y = 0.3; // top of the concrete apron — the pylon feet.
    let mast_h = 4.4;
    let top = base_y + mast_h; // 4.7 — the pylon crowns / girder seat.
    let half = 0.35; // pylon half-width at foot.
    let cx = 2.0; // pylon centres flank a ~3.3 m clear opening.
    let girder_y = top + 0.28; // box-girder centre, seated on the pylons.
    let front = -0.45; // -Z hero face of the span (signage rides here).

    // Concrete apron — the flat-base root (never tilt a root: every child
    // rides the root's transform, so a tilt would spin the whole gate).
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [6.0, base_y, 2.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, base_y * 0.5, 0.0],
        id_quat(),
    )];

    // Two braced steel lattice pylons — plant steelwork, not fenceposts. The
    // helper builds a mast centred on the origin; shift each flat list out to
    // its bay so the legs, rings, and zig-zag diagonals all move together.
    let shift_x = |mut v: Vec<Generator>, dx: f32| -> Vec<Generator> {
        for g in &mut v {
            g.transform.translation.0[0] += dx;
        }
        v
    };
    for sx in [-1.0_f32, 1.0] {
        prims.extend(shift_x(
            lattice_mast(base_y, mast_h, half, tank_steel(PIPE_GREY)),
            sx * cx,
        ));
        // Cast footing grounding each pylon foot.
        prims.push(prim(
            solid(cuboid_tapered(
                [1.1, 0.5, 1.1],
                0.06,
                concrete(CONCRETE_GREY),
            )),
            [sx * cx, base_y + 0.1, 0.0],
            id_quat(),
        ));
    }

    // Top chord — a clad box girder spanning both pylon crowns.
    prims.push(prim(
        solid(cuboid_tapered([5.0, 0.56, 0.8], 0.0, cladding(STEEL_BLUE))),
        [0.0, girder_y, 0.0],
        id_quat(),
    ));
    // Bottom chord — a steel pipe laid along X below the girder.
    let bottom_y = top - 0.5;
    prims.push(prim(
        solid(cylinder_tapered(0.16, 4.6, 12, 0.0, tank_steel(PIPE_GREY))),
        [0.0, bottom_y, 0.0],
        quat_z(FRAC_PI_2),
    ));
    // Vertical web posts tying the chords into a gantry truss.
    for wx in [-1.6_f32, -0.55, 0.55, 1.6] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.1, girder_y - bottom_y, 0.1],
                0.0,
                tank_steel(PIPE_GREY),
            )),
            [wx, (girder_y + bottom_y) * 0.5, 0.0],
            id_quat(),
        ));
    }

    // Sign board on the girder's -Z hero face: a dark steel plate carrying a
    // broad sodium-amber panel at LOW strength (reads as a lit works sign, not
    // a blown-out white box).
    prims.push(prim(
        solid(cuboid_tapered(
            [3.6, 0.5, 0.08],
            0.0,
            tank_steel([0.16, 0.16, 0.18]),
        )),
        [0.0, girder_y, front + 0.05],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.1, 0.34, 0.04], 0.0, glow(LAMP_AMBER, 2.2)),
        [0.0, girder_y, front],
        id_quat(),
    ));

    // Threshold beam under the truss — a thin amber tube runs hot across the
    // opening so the gate reads as an active passage from the front.
    prims.push(prim(
        cuboid_tapered([3.2, 0.1, 0.14], 0.0, glow(LAMP_AMBER, 6.0)),
        [0.0, bottom_y - 0.3, front + 0.15],
        id_quat(),
    ));

    // Lit indicator gauges on the inner face of each pylon, facing the hero
    // front — the "gate live" tell-tales.
    for sx in [-1.0_f32, 1.0] {
        prims.extend(gauge_plate([sx * cx, 2.4, front + 0.2], 0.2, LAMP_AMBER));
    }

    // The walk-in zone, centred in the opening: floor at the apron top, head
    // clearing the truss. This is the gate's only functional element.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
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
        assert_sanitize_stable(&IndustrialParkGateway.build(""), "industrial_park_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is set-dressing, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = IndustrialParkGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
