//! Floodlight — an Industrial-Park prop, and the kit's lit hero. A steel mast
//! carrying a bank of four glaring floodlight heads on a crossbar, lighting
//! the yard. Its emissive lamps are the trim escalation's ruin pass kills.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_GREY, FLOOD_WHITE, PIPE_GREY, concrete, tank_steel};

pub struct Floodlight;

impl CatalogueEntry for Floodlight {
    fn slug(&self) -> &'static str {
        "floodlight"
    }
    fn name(&self) -> &'static str {
        "Floodlight"
    }
    fn description(&self) -> &'static str {
        "Steel mast with a bank of four glaring floodlight heads on a crossbar."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mast_h = 6.5;

    let mut prims = vec![
        // Concrete footing — the root.
        prim(
            solid(cuboid_tapered(
                [0.8, 0.4, 0.8],
                0.1,
                concrete(CONCRETE_GREY),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Steel mast.
        prim(
            solid(cylinder_tapered(
                0.16,
                mast_h,
                8,
                0.2,
                tank_steel(PIPE_GREY),
            )),
            [0.0, mast_h * 0.5, 0.0],
            id_quat(),
        ),
        // Crossbar.
        prim(
            solid(cuboid_tapered(
                [2.8, 0.16, 0.16],
                0.0,
                tank_steel(PIPE_GREY),
            )),
            [0.0, mast_h, 0.0],
            id_quat(),
        ),
    ];

    // Four floodlight heads, tilted down to light the yard.
    for hx in [-1.05_f32, -0.35, 0.35, 1.05] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.5, 0.4, 0.35],
                0.0,
                tank_steel([0.25, 0.25, 0.27]),
            )),
            [hx, mast_h - 0.3, 0.0],
            quat_x(0.4),
        ));
        prims.push(prim(
            cuboid_tapered([0.42, 0.32, 0.08], 0.0, glow(FLOOD_WHITE, 5.0)),
            [hx, mast_h - 0.35, 0.18],
            quat_x(0.4),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Floodlight.build(""), "floodlight");
    }

    #[test]
    fn has_lamps() {
        assert!(super::super::has_emissive(&Floodlight.build("")));
    }
}
