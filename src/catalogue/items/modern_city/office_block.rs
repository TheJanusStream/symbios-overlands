//! Office block — a Modern-City secondary. A mid-rise box with a lit glass
//! curtain wall on its street face, concrete flanks, an entrance canopy, and
//! a parapet roof with a humming rooftop unit. The everyday downtown
//! building that rings the landmark tower.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, GLASS_TEAL, LAMP_WARM, STEEL_GREY, concrete, curtain_wall, fx, glass, steel,
};

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

    let body_cy = base_h + body_h * 0.5;
    let front_z = -d * 0.5; // the −Z render front is the glazed street face

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
        // Concrete core box — the flanks and back stay solid masonry.
        prim(
            solid(cuboid_tapered([w, body_h, d], 0.0, concrete(CONCRETE_GREY))),
            [0.0, body_cy, 0.0],
            id_quat(),
        ),
    ];

    // Lit glass curtain wall gridded by steel mullions across the street face.
    prims.extend(curtain_wall(
        [0.0, body_cy + 0.6, front_z],
        [w - 1.0, body_h - 2.4],
        (4, 5),
        -0.34,
        glass(GLASS_TEAL, 2.0),
        steel(STEEL_GREY),
    ));

    // Glazed ground-floor lobby + revolving-door portal under a canopy.
    prims.push(prim(
        cuboid_tapered([w - 1.0, 2.4, 0.5], 0.0, glass(GLASS_TEAL, 1.6)),
        [0.0, base_h + 1.3, front_z - 0.18],
        id_quat(),
    ));
    // Dark entrance portal recess + glass doors.
    prims.push(prim(
        solid(cuboid_tapered(
            [3.0, 2.5, 0.4],
            0.0,
            steel([0.16, 0.17, 0.2]),
        )),
        [0.0, base_h + 1.25, front_z - 0.36],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([2.4, 2.1, 0.2], 0.0, glass([0.14, 0.18, 0.2], 1.2)),
        [0.0, base_h + 1.05, front_z - 0.5],
        id_quat(),
    ));
    // Steel entrance canopy cantilevered over the doors.
    prims.push(prim(
        solid(cuboid_tapered([5.4, 0.3, 2.2], 0.0, steel(STEEL_GREY))),
        [0.0, base_h + 3.0, front_z - 1.0],
        id_quat(),
    ));
    // Warm lit address band above the canopy.
    prims.push(prim(
        cuboid_tapered([4.2, 0.55, 0.18], 0.0, glow(LAMP_WARM, 1.8)),
        [0.0, base_h + 3.7, front_z - 0.3],
        id_quat(),
    ));

    // Parapet coping ringing the roof, held proud of the body.
    prims.push(prim(
        solid(cuboid_tapered(
            [w + 0.5, 0.7, d + 0.5],
            0.0,
            concrete([0.6, 0.6, 0.61]),
        )),
        [0.0, base_h + body_h + 0.35, 0.0],
        id_quat(),
    ));
    // Rooftop air-handling unit, set toward the back.
    prims.push(prim(
        solid(cuboid_tapered([2.4, 1.2, 2.0], 0.0, steel(STEEL_GREY))),
        [-2.5, base_h + body_h + 1.2, 1.6],
        id_quat(),
    ));
    // A vent stack beside it.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 1.6, 0.5],
            0.0,
            steel([0.45, 0.46, 0.48]),
        )),
        [1.8, base_h + body_h + 1.4, 1.6],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the rooftop unit steaming with a steady hum.
    root.children.push(fx::vent_steam(
        [-2.5, base_h + body_h + 2.4, 1.6],
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
