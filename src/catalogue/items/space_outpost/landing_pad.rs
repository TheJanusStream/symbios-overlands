//! Landing pad — a Space-Outpost secondary. A wide ceramic pad ringed with
//! hazard paint and edge beacons, a touchdown cross at its centre. The
//! spaceport apron of the base; its beacons are emissive trim the ruin pass
//! can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the pad.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BEACON_RED, HAZARD_YELLOW, HULL_PANEL, PAD_GREY, STATUS_GREEN, STEEL_DARK, VIEWPORT_LIT,
    concrete, hull, painted, steel,
};

pub struct LandingPad;

impl CatalogueEntry for LandingPad {
    fn slug(&self) -> &'static str {
        "landing_pad"
    }
    fn name(&self) -> &'static str {
        "Landing Pad"
    }
    fn description(&self) -> &'static str {
        "Wide ceramic pad with hazard paint, edge beacons and a touchdown cross."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pad_h = 0.3_f32;
    let pad_top = pad_h;
    let radius = 6.0_f32;

    let mut prims = vec![
        // Ceramic pad — the root.
        prim(
            solid(cylinder_tapered(radius, pad_h, 28, 0.0, concrete(PAD_GREY))),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Raised steel perimeter curb.
    prims.push(prim(
        solid(torus(0.16, radius - 0.18, steel(STEEL_DARK))),
        [0.0, pad_top + 0.04, 0.0],
        id_quat(),
    ));
    // Painted hazard ring inboard of the curb.
    prims.push(prim(
        torus(0.12, radius - 1.0, painted(HAZARD_YELLOW)),
        [0.0, pad_top + 0.04, 0.0],
        id_quat(),
    ));
    // Touchdown circle.
    prims.push(prim(
        torus(0.1, 2.5, painted(HAZARD_YELLOW)),
        [0.0, pad_top + 0.05, 0.0],
        id_quat(),
    ));
    // Touchdown cross — the two bars sit at offset heights so the central
    // overlap does not leave coplanar top faces (the upper bar simply
    // occludes the lower one where they cross).
    prims.push(prim(
        cuboid_tapered([3.6, 0.06, 0.5], 0.0, painted(HAZARD_YELLOW)),
        [0.0, pad_top + 0.06, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.5, 0.06, 3.6], 0.0, painted(HAZARD_YELLOW)),
        [0.0, pad_top + 0.11, 0.0],
        id_quat(),
    ));

    // Edge beacon fixtures (post + lens) around the rim — emissive.
    for i in 0..8 {
        let a = i as f32 / 8.0 * TAU;
        let (px, pz) = (a.cos() * (radius - 0.45), a.sin() * (radius - 0.45));
        prims.push(prim(
            solid(cuboid_tapered([0.16, 0.5, 0.16], 0.0, steel(STEEL_DARK))),
            [px, pad_top + 0.25, pz],
            id_quat(),
        ));
        prims.push(prim(
            sphere(0.16, 4, glow(BEACON_RED, 2.6)),
            [px, pad_top + 0.62, pz],
            id_quat(),
        ));
    }

    // Lit white approach-light centreline on the −Z front half, guiding in.
    for j in 1..=3 {
        let z = -(2.6 + j as f32 * 1.0);
        prims.push(prim(
            solid(cuboid_tapered([0.55, 0.12, 0.18], 0.0, steel(STEEL_DARK))),
            [0.0, pad_top + 0.08, z],
            id_quat(),
        ));
        prims.push(prim(
            cuboid_tapered([0.45, 0.08, 0.1], 0.0, glow(VIEWPORT_LIT, 2.4)),
            [0.0, pad_top + 0.17, z],
            id_quat(),
        ));
    }

    // Service mast on the +X edge — floodlight, control box and status LED.
    prims.push(prim(
        solid(cylinder_tapered(0.12, 3.2, 8, 0.12, steel(STEEL_DARK))),
        [radius - 0.6, 1.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.7, 0.4], 0.0, hull(HULL_PANEL))),
        [radius - 0.6, 0.65, 0.65],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.09, 4, glow(STATUS_GREEN, 2.0)),
        [radius - 0.6, 0.95, 0.86],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.7, 0.32, 0.22], 0.0, glow(VIEWPORT_LIT, 2.6)),
        [radius - 0.6, 3.1, -0.28],
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
        assert_sanitize_stable(&LandingPad.build(""), "landing_pad");
    }
}
