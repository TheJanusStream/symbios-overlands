//! Campus gateway — the Civic/Campus bespoke social gate (#753). A
//! collegiate propylaeum: two dressed-stone piers on marble plinths flank
//! the walk-through, a marble architrave and pedimented gable span the top,
//! a verdigris copper wreath crest faces the quad and a warm lit nameplate
//! reads over the opening. Warm lantern globes crown the piers and a lit
//! sill lines the threshold. The neutral placeholder gate ([`super::super`]'s
//! `social_gateway`) is retired for this theme in its favour.
//!
//! The only functional element is the single [`GeneratorKind::Gateway`] zone
//! child between the piers — walking into it opens the destination picker.
//! Everything else is themed set-dressing framing that opening, authored in
//! one flat ground-relative frame via [`assemble`], which reparents every
//! piece under the base slab.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, foundation_block, glow, id_quat,
    prim, quat_x, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator, GeneratorKind};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    COPPER_VERDIGRIS, LAMP_WARM, MARBLE_WHITE, STONE_PALE, WINDOW_WARM, copper, fx, marble,
    painted, stone,
};

pub struct CivicCampusGateway;

impl CatalogueEntry for CivicCampusGateway {
    fn slug(&self) -> &'static str {
        "civic_campus_gateway"
    }
    fn name(&self) -> &'static str {
        "Campus Gateway"
    }
    fn description(&self) -> &'static str {
        "Neoclassical campus gate: two stone piers under a pedimented marble lintel, a copper crest and a warm lit nameplate over the walk-through."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Gateway
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    // No prosperity_band(): the gateway is the theme's per-theme fallback
    // matched by role, so it must place in a civic room of any prosperity —
    // the underfunded quad gets the same ceremonial gate near spawn.
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

/// One gate pier at `x`, standing on `base_y` (the slab top): a marble plinth
/// foot, a dressed stone ashlar shaft, a marble cornice cap and a warm copper
/// lantern crown — the solid support, echoing the town-hall portico's
/// stone-and-marble register. Its globe is emissive trim the ruin pass darkens.
fn gate_pier(x: f32, base_y: f32) -> Vec<Generator> {
    let foot_h = 0.4_f32;
    let shaft_h = 3.2_f32;
    let cap_h = 0.34_f32;
    let foot_top = base_y + foot_h;
    let shaft_top = foot_top + shaft_h;
    let cap_top = shaft_top + cap_h;
    vec![
        // Marble plinth foot, oversailing the shaft so its base is not flush.
        prim(
            solid(cuboid_tapered(
                [0.98, foot_h, 0.98],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [x, base_y + foot_h * 0.5, 0.0],
            id_quat(),
        ),
        // Dressed stone ashlar shaft, lightly tapered so it reads as masonry.
        prim(
            solid(cuboid_tapered(
                [0.72, shaft_h, 0.72],
                0.04,
                stone(STONE_PALE),
            )),
            [x, foot_top + shaft_h * 0.5, 0.0],
            id_quat(),
        ),
        // Marble cornice cap, proud of the shaft on every side.
        prim(
            solid(cuboid_tapered(
                [0.96, cap_h, 0.96],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [x, shaft_top + cap_h * 0.5, 0.0],
            id_quat(),
        ),
        // Verdigris copper lantern base seated on the cap.
        prim(
            solid(cuboid_tapered(
                [0.3, 0.32, 0.3],
                0.1,
                copper(COPPER_VERDIGRIS),
            )),
            [x, cap_top + 0.16, 0.0],
            id_quat(),
        ),
        // Warm lit globe — emissive trim.
        prim(
            sphere(0.16, 3, glow(LAMP_WARM, 3.0)),
            [x, cap_top + 0.44, 0.0],
            id_quat(),
        ),
        // Copper finial spike.
        prim(
            solid(cylinder_tapered(
                0.06,
                0.28,
                8,
                0.9,
                copper(COPPER_VERDIGRIS),
            )),
            [x, cap_top + 0.74, 0.0],
            id_quat(),
        ),
    ]
}

fn build_tree() -> Generator {
    let base_h = 0.3_f32;
    let base_y = base_h; // slab top; piers stand here.
    let pier_x = 1.9_f32;
    let cap_top = base_y + 0.4 + 3.2 + 0.34; // matches gate_pier internals.

    // Marble forecourt stylobate — the flat-base root. Never tilt a root:
    // every child would spin with it.
    let mut prims = vec![prim(
        solid(cuboid_tapered(
            [5.4, base_h, 2.4],
            0.0,
            marble(MARBLE_WHITE),
        )),
        [0.0, base_h * 0.5, 0.0],
        id_quat(),
    )];
    // Buried plinth so a terrain-snapped gate shows stone, not daylight, on a
    // downhill edge.
    prims.push(foundation_block(5.4, 2.4, [0.0, 0.0], 1.2));

    // Two piers flanking the ~2.6 m walk-through.
    for x in [-pier_x, pier_x] {
        prims.extend(gate_pier(x, base_y));
    }

    // Marble architrave beam spanning both cap tops.
    let arch_bot = cap_top;
    prims.push(prim(
        solid(cuboid_tapered([4.8, 0.5, 0.95], 0.0, marble(MARBLE_WHITE))),
        [0.0, arch_bot + 0.25, 0.0],
        id_quat(),
    ));
    // Triangular pediment gable over the architrave — pinch the front X width
    // to an apex ridge, keep the full depth (a pediment, not a hipped pyramid).
    let arch_top = arch_bot + 0.5;
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [4.8, 1.1, 0.95],
            [0.99, 0.0],
            marble(MARBLE_WHITE),
        )),
        [0.0, arch_top + 0.55, 0.0],
        id_quat(),
    ));

    // Copper wreath crest in the pediment tympanum, facing the -Z front — the
    // campus emblem, so the gate MEANS a gate and not a plain arch.
    let ped_front = -0.95 * 0.5 - 0.03;
    prims.push(prim(
        cylinder_tapered(0.28, 0.1, 16, 0.0, painted([0.66, 0.54, 0.26])),
        [0.0, arch_top + 0.36, ped_front + 0.02],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        torus(0.06, 0.4, copper(COPPER_VERDIGRIS)),
        [0.0, arch_top + 0.36, ped_front],
        quat_x(FRAC_PI_2),
    ));

    // Brass nameplate mounted under the architrave on the -Z front, standing
    // proud of the pier caps.
    let name_z = -0.5_f32;
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 0.44, 0.1],
            0.0,
            painted([0.66, 0.54, 0.26]),
        )),
        [0.0, arch_bot - 0.22, name_z],
        id_quat(),
    ));
    // Warm lit inscription channel proud of the brass plate — a thin lit strip
    // (low strength: it reads as warm lettering, not a white lightbox).
    prims.push(prim(
        cuboid_tapered([2.7, 0.18, 0.05], 0.0, glow(WINDOW_WARM, 2.0)),
        [0.0, arch_bot - 0.22, name_z - 0.06],
        id_quat(),
    ));

    // Lit threshold sill lining the front of the opening — a warm line the
    // visitor crosses stepping into the zone. Thin trim, so it can run warm.
    prims.push(prim(
        cuboid_tapered([2.6, 0.1, 0.16], 0.0, glow(LAMP_WARM, 2.4)),
        [0.0, base_y + 0.06, -0.3],
        id_quat(),
    ));

    // A lazy drift of seed-fluff out front — signature life on the quad.
    prims.push(fx::seed_drift([0.0, 1.4, -3.2], 0x0C1F_6A73));

    // The walk-in zone between the piers: floor at the slab top, headroom to
    // just under the architrave.
    prims.push(prim(
        GeneratorKind::Gateway {
            size: Fp3([2.6, 3.2, 1.4]),
        },
        [0.0, 1.9, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a calm airy quad bed at the gate.
    root.audio = fx::campus_calm();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CivicCampusGateway.build(""), "civic_campus_gateway");
    }

    /// The functional zone must survive assembly — a gateway without its
    /// `GeneratorKind::Gateway` child is furniture, not a gate.
    #[test]
    fn build_carries_exactly_one_gateway_zone() {
        let g = CivicCampusGateway.build("");
        fn count_zones(node: &Generator) -> usize {
            let own = matches!(node.kind, GeneratorKind::Gateway { .. }) as usize;
            own + node.children.iter().map(count_zones).sum::<usize>()
        }
        assert_eq!(count_zones(&g), 1);
    }
}
