//! Cooling tower — an Industrial-Park secondary. A waisted concrete
//! hyperboloid shell billowing a fat white steam plume, hissing softly at the
//! rim. The unmistakable silhouette of a power or process plant.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, solid, torus};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, concrete, fx};

pub struct CoolingTower;

impl CatalogueEntry for CoolingTower {
    fn slug(&self) -> &'static str {
        "cooling_tower"
    }
    fn name(&self) -> &'static str {
        "Cooling Tower"
    }
    fn description(&self) -> &'static str {
        "Waisted concrete cooling tower billowing a white steam plume."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let conc = || concrete(CONCRETE_GREY);
    let base_h = 0.5;
    let inlet_h = 1.9;

    let mut prims = vec![
        // Ground ring — the flat root.
        prim(
            solid(cylinder_tapered(4.6, base_h, 28, 0.0, conc())),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Inlet colonnade: the cooling tower's signature open ring of columns,
    // lifting the shell off the apron so air can draw up through it.
    let col_r = 3.95;
    let ncol = 22;
    for i in 0..ncol {
        let a = i as f32 / ncol as f32 * TAU;
        prims.push(prim(
            solid(cylinder_tapered(0.17, inlet_h, 6, 0.0, conc())),
            [a.cos() * col_r, base_h + inlet_h * 0.5, a.sin() * col_r],
            id_quat(),
        ));
    }
    // Ring beam capping the colonnade.
    let y0 = base_h + inlet_h;
    prims.push(prim(
        solid(torus(0.24, col_r, conc())),
        [0.0, y0, 0.0],
        id_quat(),
    ));

    // Hyperboloid shell — many thin rings on a smooth waisted profile (the
    // old build used seven fat steps that read as a stack of cans).
    let rings = 15;
    let shell_h = 15.5_f32;
    let seg = shell_h / rings as f32;
    let waist = 2.8_f32;
    let mut rim_r = 0.0;
    for i in 0..rings {
        let t = i as f32 / (rings - 1) as f32;
        let d = (t - 0.5) / 0.52;
        let r = waist * (1.0 + d * d).sqrt() * (1.0 + 0.1 * (1.0 - t));
        rim_r = r;
        prims.push(prim(
            solid(cylinder_tapered(r, seg + 0.06, 28, 0.0, conc())),
            [0.0, y0 + seg * (i as f32 + 0.5), 0.0],
            id_quat(),
        ));
    }
    let rim = y0 + shell_h;
    // Rim cornice, and the dark throat seen down the open top.
    prims.push(prim(
        solid(torus(0.3, rim_r, concrete([0.5, 0.5, 0.51]))),
        [0.0, rim, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cylinder_tapered(rim_r - 0.35, 0.3, 28, 0.0, concrete([0.09, 0.09, 0.10])),
        [0.0, rim - 0.25, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: a fat steam plume off the rim, hissing.
    let mut steam = fx::cooling_steam([0.0, rim + 1.0, 0.0], 0xC001_5EE0);
    steam.audio = fx::steam_hiss();
    root.children.push(steam);
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CoolingTower.build(""), "cooling_tower");
    }
}
