//! Cooling tower — an Industrial-Park secondary. A waisted concrete
//! hyperboloid shell billowing a fat white steam plume, hissing softly at the
//! rim. The unmistakable silhouette of a power or process plant.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, solid,
};
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
    let base_h = 0.6;

    let mut prims = vec![
        // Concrete base ring — the root.
        prim(
            solid(cylinder_tapered(
                4.6,
                base_h,
                24,
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Hyperboloid shell approximated by stacked straight rings whose radii
    // pinch to a waist then flare to the rim.
    let radii = [4.2_f32, 3.5, 3.05, 2.9, 3.0, 3.4, 3.9];
    let seg_h = 2.4_f32;
    let mut y = base_h;
    for r in radii {
        prims.push(prim(
            solid(cylinder_tapered(
                r,
                seg_h + 0.1,
                24,
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, y + seg_h * 0.5, 0.0],
            id_quat(),
        ));
        y += seg_h;
    }
    let rim = y;
    // Thin rim lip.
    prims.push(prim(
        cuboid_tapered([8.0, 0.3, 8.0], 0.0, concrete([0.5, 0.5, 0.51])),
        [0.0, rim, 0.0],
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
