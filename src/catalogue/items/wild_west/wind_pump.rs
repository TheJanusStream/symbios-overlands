//! Wind pump — a Wild-West prop. A homestead windmill: a tapered timber mast
//! topped by a multi-blade rotor and a tin tail vane, pumping the well below.
//! Scatter clutter of the frontier.
//!
//! The rotor turns in the Y-Z plane (its axle along X), so its many blades
//! radiate from the hub via [`quat_x`] alone — no Z-axis rotation needed.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, TIN_GREY, WOOD_RAW, clapboard, fx, iron, tin};

pub struct WindPump;

impl CatalogueEntry for WindPump {
    fn slug(&self) -> &'static str {
        "wind_pump"
    }
    fn name(&self) -> &'static str {
        "Wind Pump"
    }
    fn description(&self) -> &'static str {
        "Homestead windmill: a timber mast topped by a multi-blade rotor and tin tail vane."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.5,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mast_h = 6.0_f32;
    let hub = [0.0_f32, mast_h, 0.6_f32];

    let mut prims = vec![
        // Tapered timber mast — the root.
        prim(
            solid(cuboid_tapered(
                [0.7, mast_h, 0.7],
                0.55,
                clapboard(WOOD_RAW),
            )),
            [0.0, mast_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Iron hub at the head.
    prims.push(prim(
        solid(sphere(0.22, 3, iron(IRON_DARK))),
        hub,
        id_quat(),
    ));
    // Multi-blade rotor: many blades radiating around the X axis.
    let blade_len = 1.4_f32;
    for i in 0..10 {
        let a = i as f32 / 10.0 * TAU;
        let c = blade_len * 0.5 + 0.2;
        prims.push(prim(
            solid(cuboid_tapered([0.06, blade_len, 0.22], 0.0, tin(TIN_GREY))),
            [hub[0], hub[1] + a.cos() * c, hub[2] + a.sin() * c],
            quat_x(a),
        ));
    }

    // Tin tail vane behind the hub.
    prims.push(prim(
        solid(cuboid_tapered([0.1, 0.9, 1.2], 0.0, tin(TIN_GREY))),
        [0.0, mast_h, -1.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the slow groan of the turning vane.
    root.audio = fx::windmill_creak();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WindPump.build(""), "wind_pump");
    }
}
