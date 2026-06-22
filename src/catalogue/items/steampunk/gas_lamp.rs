//! Gas lamp — a Steampunk prop. A wrought-iron lamppost with brass bands,
//! scroll arms and a glowing gas mantle in a brass cage. Scatter clutter
//! lighting the works; its mantle is emissive trim the ruin pass can darken.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
    torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, IRON_DARK, LAMP_GAS, brass, glass, iron};

pub struct GasLamp;

impl CatalogueEntry for GasLamp {
    fn slug(&self) -> &'static str {
        "gas_lamp"
    }
    fn name(&self) -> &'static str {
        "Gas Lamp"
    }
    fn description(&self) -> &'static str {
        "Wrought-iron lamppost with brass bands and a glowing gas mantle in a cage."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.6,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Stepped iron base — the root.
        prim(
            solid(cuboid_tapered([0.7, 0.25, 0.7], 0.1, iron(IRON_DARK))),
            [0.0, 0.12, 0.0],
            id_quat(),
        ),
    ];
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.3, 0.5], 0.18, iron(IRON_DARK))),
        [0.0, 0.4, 0.0],
        id_quat(),
    ));

    // Fluted iron column with brass collar bands.
    prims.push(prim(
        solid(cylinder_tapered(0.1, 3.0, 8, 0.12, iron(IRON_DARK))),
        [0.0, 2.05, 0.0],
        id_quat(),
    ));
    for y in [1.0_f32, 3.0] {
        prims.push(prim(
            solid(torus(0.04, 0.13, brass(BRASS))),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Wrought-iron scroll bracket cross under the lantern, ring curls at each
    // arm tip — the lamppost's signature volutes.
    let bracket_y = 3.45_f32;
    prims.push(prim(
        solid(cuboid_tapered([0.72, 0.05, 0.06], 0.0, brass(BRASS))),
        [0.0, bracket_y, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.06, 0.05, 0.72], 0.0, brass(BRASS))),
        [0.0, bracket_y, 0.0],
        id_quat(),
    ));
    for (dx, dz) in [(0.34_f32, 0.0_f32), (-0.34, 0.0), (0.0, 0.34), (0.0, -0.34)] {
        prims.push(prim(
            solid(torus(0.025, 0.1, brass(BRASS))),
            [dx, bracket_y - 0.04, dz],
            quat_x(FRAC_PI_2),
        ));
    }

    // Glazed lantern: brass rings, iron corner posts, lit amber panes.
    let lamp_y = 3.95_f32;
    for dy in [-0.4_f32, 0.4] {
        prims.push(prim(
            solid(torus(0.035, 0.34, brass(BRASS))),
            [0.0, lamp_y + dy, 0.0],
            id_quat(),
        ));
    }
    for (dx, dz) in [
        (0.27_f32, 0.27_f32),
        (-0.27, 0.27),
        (0.27, -0.27),
        (-0.27, -0.27),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([0.07, 0.86, 0.07], 0.0, iron(IRON_DARK))),
            [dx, lamp_y, dz],
            id_quat(),
        ));
    }
    // Lit amber glass panes, inset so the iron corner posts stand proud and
    // the four glazed faces read as distinct panes, not one glowing cylinder.
    for (sx, sz, dx, dz) in [
        (0.4_f32, 0.04_f32, 0.0_f32, 0.22_f32),
        (0.4, 0.04, 0.0, -0.22),
        (0.04, 0.4, 0.22, 0.0),
        (0.04, 0.4, -0.22, 0.0),
    ] {
        prims.push(prim(
            cuboid_tapered([sx, 0.74, sz], 0.0, glass(LAMP_GAS, 1.7)),
            [dx, lamp_y, dz],
            id_quat(),
        ));
    }
    // Glowing gas mantle inside — emissive trim the ruin pass can darken.
    prims.push(prim(
        sphere(0.16, 3, glow(LAMP_GAS, 3.0)),
        [0.0, lamp_y, 0.0],
        id_quat(),
    ));

    // Peaked iron roof (four-sided pyramid) + brass finial.
    prims.push(prim(
        solid(cone(0.42, 0.5, 4, iron(IRON_DARK))),
        [0.0, lamp_y + 0.65, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.04, 0.32, 6, 0.5, brass(BRASS))),
        [0.0, lamp_y + 1.05, 0.0],
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
        assert_sanitize_stable(&GasLamp.build(""), "gas_lamp");
    }
}
