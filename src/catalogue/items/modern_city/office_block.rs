//! Office block — a Modern-City secondary. A mid-rise box with a lit glass
//! curtain wall on its street face, concrete flanks, an entrance canopy, and
//! a parapet roof with a humming rooftop unit. The everyday downtown
//! building that rings the landmark tower.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, GLASS_TEAL, STEEL_GREY, concrete, fx, glass, steel};

pub struct OfficeBlock;

impl CatalogueEntry for OfficeBlock {
    fn slug(&self) -> &'static str {
        "office_block"
    }
    fn name(&self) -> &'static str {
        "Office Block"
    }
    fn description(&self) -> &'static str {
        "Mid-rise office with a glass street facade, concrete flanks, and a roof unit."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::ModernCity]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CITY_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 32.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let w = 14.0_f32;
    let d = 10.0_f32;
    let base_h = 0.5;
    let body_h = 16.0;

    let mut prims = vec![
        // Concrete base — the root.
        prim(
            solid(cuboid_tapered(
                [w + 1.0, base_h, d + 1.0],
                0.0,
                concrete(CONCRETE_GREY),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Concrete core box.
        prim(
            solid(cuboid_tapered([w, body_h, d], 0.0, concrete(CONCRETE_GREY))),
            [0.0, base_h + body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Lit glass curtain wall across the street face (+Z), with spandrels.
    prims.push(prim(
        cuboid_tapered([w - 1.0, body_h - 1.0, 0.4], 0.0, glass(GLASS_TEAL, 2.2)),
        [0.0, base_h + body_h * 0.5, d * 0.5],
        id_quat(),
    ));
    let floors = 5;
    for k in 1..floors {
        let y = base_h + body_h * (k as f32 / floors as f32);
        prims.push(prim(
            cuboid_tapered([w - 0.8, 0.3, 0.5], 0.0, steel(STEEL_GREY)),
            [0.0, y, d * 0.5],
            id_quat(),
        ));
    }

    // Parapet roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.4, 0.7, d + 0.4],
            0.0,
            concrete(CONCRETE_GREY),
        )),
        [0.0, base_h + body_h + 0.35, 0.0],
        id_quat(),
    ));
    // Rooftop unit.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 1.2, 2.0], 0.0, steel(STEEL_GREY))),
        [-2.5, base_h + body_h + 0.6 + 0.6, 1.0],
        id_quat(),
    ));

    // Entrance canopy over the ground-floor door.
    prims.push(prim(
        solid(cuboid_tapered([5.0, 0.3, 2.2], 0.0, steel(STEEL_GREY))),
        [0.0, 3.2, d * 0.5 + 1.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the rooftop unit steaming with a steady hum.
    root.children.push(fx::vent_steam(
        [-2.5, base_h + body_h + 1.8, 1.0],
        0x0FF1_CE10,
    ));
    root.audio = fx::ac_hum();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&OfficeBlock.build(""), "office_block");
    }
}
