//! Habitat dome — the Space-Outpost landmark and the kit's lit hero. A white
//! hull module under a glazed pressure dome, a lit viewport band around its
//! waist, an airlock on one side and a beacon-topped antenna mast. ~9 m
//! across, so it anchors the base and reads as the colony from across the home
//! region. Its viewports, interior glow and beacon are the trim escalation's
//! ruin pass snuffs to a cold, dead shell.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); authored in one
//! flat ground-relative frame via [`assemble`], which reparents every piece
//! under the pad.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, foundation_disc, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BEACON_RED, GLASS_CYAN, HULL_WHITE, INTERIOR_WARM, PAD_GREY, STEEL_DARK, VIEWPORT_LIT,
    concrete, fx, glass, hull, steel,
};

pub struct HabitatDome;

impl CatalogueEntry for HabitatDome {
    fn slug(&self) -> &'static str {
        "habitat_dome"
    }
    fn name(&self) -> &'static str {
        "Habitat Dome"
    }
    fn description(&self) -> &'static str {
        "White hull module under a glazed pressure dome with a lit viewport band and beacon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
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
    let pad_h = 0.6_f32;
    let module_h = 3.0_f32;
    let module_top = pad_h + module_h;

    let mut prims = vec![
        // Ceramic concrete pad — the root.
        prim(
            solid(cuboid_tapered([9.0, pad_h, 9.0], 0.0, concrete(PAD_GREY))),
            [0.0, pad_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    prims.push(foundation_disc(4.6, 1.0));

    // White hull module.
    prims.push(prim(
        solid(cylinder_tapered(4.0, module_h, 20, 0.0, hull(HULL_WHITE))),
        [0.0, pad_h + module_h * 0.5, 0.0],
        id_quat(),
    ));
    // Lit viewport band around the waist — emissive.
    prims.push(prim(
        cylinder_tapered(4.08, 1.0, 20, 0.0, glass(GLASS_CYAN, 1.4)),
        [0.0, pad_h + 1.5, 0.0],
        id_quat(),
    ));

    // Glazed pressure dome on top.
    prims.push(prim(
        solid(sphere(4.0, 3, glass(GLASS_CYAN, 1.2))),
        [0.0, module_top, 0.0],
        id_quat(),
    ));
    // Warm interior glow — emissive.
    prims.push(prim(
        sphere(3.0, 3, glow(INTERIOR_WARM, 1.5)),
        [0.0, module_top - 0.4, 0.0],
        id_quat(),
    ));

    // Airlock module protruding on the +Z face, with a lit hatch.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 2.4, 2.0], 0.0, hull(HULL_WHITE))),
        [0.0, pad_h + 1.2, 4.4],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([1.3, 1.4, 0.2], 0.0, glow(VIEWPORT_LIT, 1.8)),
        [0.0, pad_h + 1.1, 5.45],
        id_quat(),
    ));

    // Antenna mast topped by a red beacon — emissive.
    prims.push(prim(
        solid(cylinder_tapered(0.1, 3.0, 8, 0.0, steel(STEEL_DARK))),
        [0.0, module_top + 4.0 + 1.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        sphere(0.3, 3, glow(BEACON_RED, 3.0)),
        [0.0, module_top + 4.0 + 3.1, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the reactor hum and skating regolith dust.
    root.audio = fx::reactor_hum();
    root.children
        .push(fx::regolith_dust([0.0, pad_h + 0.3, 6.0], 0x5EA0_D03E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HabitatDome.build(""), "habitat_dome");
    }

    #[test]
    fn has_lit_viewports() {
        assert!(crate::catalogue::items::util::has_emissive(
            &HabitatDome.build("")
        ));
    }
}
