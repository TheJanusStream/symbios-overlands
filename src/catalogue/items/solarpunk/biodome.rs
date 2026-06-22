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

use crate::catalogue::items::space_outpost::dome_ribs;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_disc, glow, id_quat, prim, solid,
    sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_PALE, CROP_GREEN, DOME_GLOW, GLASS_CLEAN, LEAF_GREEN, STEEL_WHITE, concrete,
    crop_tufts, foliage, fx, glass, steel,
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
    let drum_r = 6.2_f32; // round concrete planter drum
    let dome_r = 5.8_f32; // geodesic rib-cage radius
    let glass_r = 5.7_f32; // glass shell just inside the ribs so they stand proud

    let mut prims = vec![
        // Round concrete planter drum — the root (a real ring base, not a flat
        // square slab).
        prim(
            solid(cylinder_tapered(
                drum_r,
                ring_h,
                28,
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, ring_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_disc(drum_r - 0.4, 1.2));

    // Planted soil inside the drum + a leafy interior garden seen through the
    // glass.
    prims.push(prim(
        solid(cylinder_tapered(
            dome_r - 0.5,
            0.4,
            28,
            0.0,
            foliage(LEAF_GREEN),
        )),
        [0.0, ring_top + 0.15, 0.0],
        id_quat(),
    ));
    prims.extend(crop_tufts(
        [0.0, ring_top + 0.35, 0.0],
        [8.0, 8.0],
        5,
        5,
        1.0,
        foliage(CROP_GREEN),
    ));

    // Faceted glass dome — an upper hemisphere seated on the drum (not a
    // half-buried sphere), low-poly so it reads geodesic.
    prims.push(prim(
        solid(with_cut(
            sphere(glass_r, 6, glass(GLASS_CLEAN, 1.1)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, ring_top, 0.0],
        id_quat(),
    ));
    // Soft green interior glow — emissive (the lit hero's glow the ruin pass
    // snuffs).
    prims.push(prim(
        sphere(3.8, 5, glow(DOME_GLOW, 1.7)),
        [0.0, ring_top + 1.6, 0.0],
        id_quat(),
    ));
    // Steel base ring where the dome springs from the drum.
    prims.push(prim(
        solid(torus(0.12, dome_r, steel(STEEL_WHITE))),
        [0.0, ring_top, 0.0],
        id_quat(),
    ));
    // Geodesic steel rib cage standing proud of the glass — the paneled
    // habitat-dome read (reused from the space-outpost habitat dome).
    prims.extend(dome_ribs(
        [0.0, ring_top, 0.0],
        dome_r,
        8,
        steel(STEEL_WHITE),
    ));

    // Glazed entrance on the -Z hero front, steel-framed.
    let zf = -(drum_r + 0.02);
    prims.push(prim(
        cuboid_tapered([2.0, 2.2, 0.18], 0.0, glass(GLASS_CLEAN, 1.3)),
        [0.0, ring_top + 0.1, zf + 0.12],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.2, 2.5, 0.32], 0.0, steel(STEEL_WHITE))),
            [sx * 1.1, ring_top + 0.15, zf],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([2.5, 0.24, 0.32], 0.0, steel(STEEL_WHITE))),
        [0.0, ring_top + 1.4, zf],
        id_quat(),
    ));

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
