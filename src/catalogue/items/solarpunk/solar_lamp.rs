//! Solar lamp — a Solarpunk prop. A path bollard with a small PV cap and a
//! warm glowing light. Scatter clutter lighting the garden paths; its lamp is
//! emissive trim the ruin pass can darken.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
    torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{LAMP_WARM, PV_BLUE, STEEL_GREY, STEEL_WHITE, pv, steel};

pub struct SolarLamp;

impl CatalogueEntry for SolarLamp {
    fn slug(&self) -> &'static str {
        "solar_lamp"
    }
    fn name(&self) -> &'static str {
        "Solar Lamp"
    }
    fn description(&self) -> &'static str {
        "Path bollard with a small PV cap and a warm glowing light."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.5,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let head_y = 1.6_f32;
    let mut prims = vec![
        // Steel bollard post — the root.
        prim(
            solid(cylinder_tapered(0.1, 1.6, 8, 0.05, steel(STEEL_GREY))),
            [0.0, 0.8, 0.0],
            id_quat(),
        ),
    ];

    // Lantern head: a glowing lens enclosed in a little steel cage under a
    // hood — a fixture, not a bare glow cube.
    // Collar ring where the head meets the post.
    prims.push(prim(
        solid(torus(0.035, 0.2, steel(STEEL_WHITE))),
        [0.0, head_y - 0.02, 0.0],
        id_quat(),
    ));
    // Warm glowing lens — emissive trim the ruin pass can darken.
    prims.push(prim(
        sphere(0.15, 5, glow(LAMP_WARM, 2.2)),
        [0.0, head_y + 0.12, 0.0],
        id_quat(),
    ));
    // Cage bars round the lens.
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU;
        prims.push(prim(
            solid(cylinder_tapered(0.014, 0.32, 6, 0.0, steel(STEEL_WHITE))),
            [a.cos() * 0.16, head_y + 0.12, a.sin() * 0.16],
            id_quat(),
        ));
    }
    // Conical hood capping the lantern.
    prims.push(prim(
        solid(cone(0.24, 0.16, 10, steel(STEEL_WHITE))),
        [0.0, head_y + 0.36, 0.0],
        id_quat(),
    ));
    // Small tilted PV cap on top, soaking the sun.
    prims.push(prim(
        solid(cuboid_tapered([0.46, 0.05, 0.46], 0.0, pv(PV_BLUE))),
        [0.0, head_y + 0.5, 0.0],
        quat_x(0.25),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SolarLamp.build(""), "solar_lamp");
    }
}
