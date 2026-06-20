//! Biodome — the Solarpunk landmark and the kit's lit hero. A faceted glass
//! geodesic dome over a ring of planted soil, banded by white steel frame
//! rings and lit from within by a soft green glow. ~13 m across, so it
//! anchors the eco-quarter and reads as the conservatory from across the home
//! region. Its dome glass and interior glow are the trim escalation's ruin
//! pass snuffs to a dark, dead shell.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the concrete ring.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_disc, glow, id_quat, prim, solid,
    sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, DOME_GLOW, GLASS_CLEAN, LEAF_GREEN, STEEL_WHITE, concrete, foliage, fx, glass,
    steel,
};

pub struct Biodome;

impl CatalogueEntry for Biodome {
    fn slug(&self) -> &'static str {
        "biodome"
    }
    fn name(&self) -> &'static str {
        "Biodome"
    }
    fn description(&self) -> &'static str {
        "Faceted glass geodesic dome over planted soil, steel-banded and lit from within."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 50.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ring_h = 1.0_f32;
    let ring_top = ring_h;
    let radius = 5.5_f32; // dome sphere radius, centred on the ring top

    let mut prims = vec![
        // Concrete planter ring — the root.
        prim(
            solid(cuboid_tapered(
                [13.0, ring_h, 13.0],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, ring_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_disc(6.8, 1.2));

    // Planted soil inside the ring.
    prims.push(prim(
        solid(cylinder_tapered(6.0, 0.4, 24, 0.0, foliage(LEAF_GREEN))),
        [0.0, ring_top + 0.1, 0.0],
        id_quat(),
    ));

    // Faceted glass dome (low-poly sphere reads as geodesic).
    prims.push(prim(
        solid(sphere(radius, 3, glass(GLASS_CLEAN, 1.2))),
        [0.0, ring_top, 0.0],
        id_quat(),
    ));
    // Soft green interior glow — emissive.
    prims.push(prim(
        sphere(4.2, 3, glow(DOME_GLOW, 1.6)),
        [0.0, ring_top + 1.6, 0.0],
        id_quat(),
    ));

    // White steel latitude frame rings up the dome.
    for h in [2.5_f32, 4.0, 5.3] {
        let dy = h - ring_top;
        let r = (radius * radius - dy * dy).max(0.0).sqrt();
        prims.push(prim(
            solid(torus(0.1, r, steel(STEEL_WHITE))),
            [0.0, h, 0.0],
            id_quat(),
        ));
    }

    // Glass entrance on the +Z face with a steel frame.
    prims.push(prim(
        cuboid_tapered([2.0, 2.2, 0.2], 0.0, glass(GLASS_CLEAN, 1.2)),
        [0.0, ring_top + 1.1, 6.3],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.15, 2.4, 0.25], 0.0, steel(STEEL_WHITE))),
            [sx * 1.1, ring_top + 1.2, 6.3],
            id_quat(),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: clean-air breeze and drifting pollen.
    root.audio = fx::breeze_calm();
    root.children
        .push(fx::pollen_drift([0.0, ring_top + 2.0, 0.0], 0x501A_D03E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Biodome.build(""), "biodome");
    }

    #[test]
    fn has_glowing_dome() {
        assert!(crate::catalogue::items::util::has_emissive(
            &Biodome.build("")
        ));
    }
}
