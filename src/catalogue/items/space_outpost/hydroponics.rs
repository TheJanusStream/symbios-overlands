//! Hydroponics — a Space-Outpost secondary. An open glazed barrel-vault grow
//! house over crop racks, lit pink by grow-lights. The food module of the
//! base; its grow-lights are emissive trim the ruin pass can darken.
//!
//! The vault is a round-up half-cylinder (`path_cut`) tipped along Z with a
//! [`quat_x`] of π/2, so the glass arches over the crops and the rows and
//! grow-lights read through and under it instead of being sealed in a tube.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, torus,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    GLASS_CYAN, GROW_PINK, HULL_PANEL, HULL_WHITE, STEEL_DARK, VIEWPORT_LIT, glass, hull, painted,
    steel,
};

/// Crop-row green inside the module.
const CROP: [f32; 3] = [0.30, 0.52, 0.22];
/// Leafy plant green for the crop tufts.
const LEAF: [f32; 3] = [0.36, 0.62, 0.26];

pub struct Hydroponics;

impl CatalogueEntry for Hydroponics {
    fn slug(&self) -> &'static str {
        "hydroponics"
    }
    fn name(&self) -> &'static str {
        "Hydroponics"
    }
    fn description(&self) -> &'static str {
        "Glazed barrel-vault grow module over crop racks, lit by pink grow-lights."
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
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let base_h = 0.4_f32;
    let vault_r = 1.95_f32;
    let vault_len = 5.4_f32;

    let mut prims = vec![
        // Hull base — the root.
        prim(
            solid(cuboid_tapered([6.0, base_h, 4.0], 0.0, hull(HULL_WHITE))),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Open glazed barrel vault (round side up) arching over the crops, laid
    // along Z. The flat cut side sits at the base so the interior shows.
    prims.push(prim(
        solid(with_cut(
            cylinder_tapered(vault_r, vault_len, 18, 0.0, glass(GLASS_CYAN, 0.9)),
            [0.5, 1.0],
            [0.0, 1.0],
            0.0,
        )),
        [0.0, base_h, 0.0],
        quat_x(FRAC_PI_2),
    ));
    // Steel arch ribs along the vault.
    for z in [-2.4_f32, 0.0, 2.4] {
        prims.push(prim(
            solid(with_cut(
                torus(0.07, vault_r, steel(STEEL_DARK)),
                [0.0, 0.5],
                [0.0, 1.0],
                0.0,
            )),
            [0.0, base_h, z],
            quat_x(-FRAC_PI_2),
        ));
    }
    // Hull gable end-walls (a low wall at +Z back, an open doorway frame at
    // the −Z front so the rows read).
    prims.push(prim(
        solid(cuboid_tapered([3.8, 1.0, 0.25], 0.0, hull(HULL_WHITE))),
        [0.0, base_h + 0.5, 2.7],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.3, 2.2, 0.25], 0.0, hull(HULL_PANEL))),
            [sx * 1.5, base_h + 1.1, -2.7],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([3.3, 0.3, 0.25], 0.0, hull(HULL_PANEL))),
        [0.0, base_h + 2.05, -2.7],
        id_quat(),
    ));

    // Crop racks with rows of leafy tufts + grow-light strips above (emissive).
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.7, 0.55, 4.6], 0.0, painted(CROP))),
            [sx * 0.85, base_h + 0.3, 0.0],
            id_quat(),
        ));
        // Leafy tufts in each tray.
        for k in 0..5 {
            let z = -1.8 + k as f32 * 0.9;
            prims.push(prim(
                cone(0.28, 0.5, 6, painted(LEAF)),
                [sx * 0.85, base_h + 0.85, z],
                id_quat(),
            ));
        }
        // Pink grow-light strip overhead.
        prims.push(prim(
            cuboid_tapered([0.55, 0.12, 4.6], 0.0, glow(GROW_PINK, 2.2)),
            [sx * 0.85, base_h + 1.9, 0.0],
            id_quat(),
        ));
    }
    // A small lit status panel beside the −Z doorway.
    prims.push(prim(
        cuboid_tapered([0.4, 0.5, 0.12], 0.0, glow(VIEWPORT_LIT, 2.2)),
        [1.5, base_h + 1.0, -2.84],
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
        assert_sanitize_stable(&Hydroponics.build(""), "hydroponics");
    }
}
