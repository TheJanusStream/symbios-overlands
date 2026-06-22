//! Floodlight — an Industrial-Park prop, and the kit's lit hero. A steel mast
//! carrying a bank of four glaring floodlight heads on a crossbar, lighting
//! the yard. Its emissive lamps are the trim escalation's ruin pass kills.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    CONCRETE_GREY, FLOOD_WHITE, LAMP_AMBER, PIPE_GREY, concrete, gauge_plate, lattice_mast,
    tank_steel,
};

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
    let base_y = 0.45;
    let mast_h = 6.6;
    let top = base_y + mast_h;

    let mut prims = vec![
        // Cast concrete footing — the root (flat, id_quat).
        prim(
            solid(cuboid_tapered(
                [1.5, base_y, 1.5],
                0.08,
                concrete(CONCRETE_GREY),
            )),
            [0.0, base_y * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Braced steel lattice mast — plant steelwork, not a lamppost.
    prims.extend(lattice_mast(base_y, mast_h, 0.5, tank_steel(PIPE_GREY)));

    // Crossbar carrying the lamp bank.
    prims.push(prim(
        solid(cuboid_tapered([3.0, 0.2, 0.32], 0.0, tank_steel(PIPE_GREY))),
        [0.0, top + 0.12, 0.0],
        id_quat(),
    ));

    // Four floodlight heads, aimed down at the -Z yard — glare on the hero
    // front. Each is a rigid subtree (housing root + lens + hood children) so
    // the down-tilt keeps the lens and visor aligned to the housing.
    for hx in [-1.15_f32, -0.4, 0.4, 1.15] {
        let mut head = prim(
            solid(cuboid_tapered(
                [0.52, 0.42, 0.42],
                0.0,
                tank_steel([0.2, 0.2, 0.22]),
            )),
            [hx, top + 0.22, -0.12],
            quat_x(-0.42),
        );
        // Bright lit lens on the -Z (front) face.
        head.children.push(prim(
            cuboid_tapered([0.46, 0.34, 0.05], 0.0, glow(FLOOD_WHITE, 5.0)),
            [0.0, 0.0, -0.23],
            id_quat(),
        ));
        // Reflector hood shading the lens from above.
        head.children.push(prim(
            solid(cuboid_tapered(
                [0.58, 0.06, 0.26],
                0.0,
                tank_steel([0.15, 0.15, 0.17]),
            )),
            [0.0, 0.24, -0.16],
            id_quat(),
        ));
        prims.push(head);
    }

    // Control / junction box on the mast foot with a lit indicator.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.6, 0.9, 0.4],
            0.0,
            tank_steel([0.28, 0.3, 0.32]),
        )),
        [0.55, base_y + 0.6, -0.5],
        id_quat(),
    ));
    prims.extend(gauge_plate([0.55, base_y + 0.75, -0.72], 0.18, LAMP_AMBER));

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
        assert!(crate::catalogue::items::util::has_emissive(
            &Floodlight.build("")
        ));
    }
}
