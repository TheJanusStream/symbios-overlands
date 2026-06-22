//! Wind pump — a Wild-West prop. A homestead windmill: a four-legged steel
//! lattice tower topped by a multi-blade fan wheel and a directional tail vane,
//! a pump rod running down to a wellhead. Scatter clutter of the frontier.
//!
//! The fan wheel faces −Z (the render FRONT), its axle along Z, so the many
//! paddles radiate in the X-Y plane via [`quat_z`]; the tail boom streams back
//! along +Z.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, TIN_GREY, fx, iron, tin};

pub struct WindPump;

impl CatalogueEntry for WindPump {
    fn slug(&self) -> &'static str {
        "wind_pump"
    }
    fn name(&self) -> &'static str {
        "Wind Pump"
    }
    fn description(&self) -> &'static str {
        "Homestead windmill: a steel lattice tower topped by a multi-blade fan wheel and tail vane."
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
    let tower_h = 6.0_f32;
    let lw = 0.7_f32; // leg half-spread

    let mut prims = vec![
        // First tower leg — the root.
        prim(
            solid(cuboid_tapered([0.1, tower_h, 0.1], 0.0, iron(IRON_DARK))),
            [-lw, tower_h * 0.5, -lw],
            id_quat(),
        ),
    ];
    for (sx, sz) in [(1.0_f32, -1.0_f32), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.1, tower_h, 0.1], 0.0, iron(IRON_DARK))),
            [sx * lw, tower_h * 0.5, sz * lw],
            id_quat(),
        ));
    }
    // Horizontal girts at three levels.
    for y in [1.6_f32, 3.4, 5.2] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([2.0 * lw, 0.07, 0.07], 0.0, iron(IRON_DARK))),
                [0.0, y, sz * lw],
                id_quat(),
            ));
        }
        for sx in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.07, 0.07, 2.0 * lw], 0.0, iron(IRON_DARK))),
                [sx * lw, y, 0.0],
                id_quat(),
            ));
        }
    }
    // Full-height X cross-braces on all four faces — the lattice look.
    let span = tower_h - 0.4;
    let theta = span.atan2(2.0 * lw);
    let dlen = (span * span + 4.0 * lw * lw).sqrt();
    for sz in [-1.0_f32, 1.0] {
        for s in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([dlen, 0.06, 0.06], 0.0, iron(IRON_DARK))),
                [0.0, tower_h * 0.5, sz * lw],
                quat_z(s * theta),
            ));
        }
    }
    for sx in [-1.0_f32, 1.0] {
        for s in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.06, 0.06, dlen], 0.0, iron(IRON_DARK))),
                [sx * lw, tower_h * 0.5, 0.0],
                quat_x(s * theta),
            ));
        }
    }

    // Fan wheel mounted at the top, facing −Z (the render FRONT).
    let hub = [0.0_f32, tower_h + 0.35, -lw - 0.3];
    prims.push(prim(
        solid(cylinder_tapered(0.18, 0.5, 12, 0.0, iron(IRON_DARK))),
        hub,
        quat_x(FRAC_PI_2),
    ));
    // Multi-blade fan: narrow tin paddles radiating in the X-Y plane.
    let blades = 18;
    let wheel_r = 1.3_f32;
    let rc = 0.82_f32;
    let blade_len = 1.15_f32;
    for i in 0..blades {
        let a = i as f32 / blades as f32 * TAU;
        prims.push(prim(
            solid(cuboid_tapered([0.16, blade_len, 0.03], 0.0, tin(TIN_GREY))),
            [hub[0] + a.cos() * rc, hub[1] + a.sin() * rc, hub[2] + 0.04],
            quat_z(a - FRAC_PI_2),
        ));
    }
    // Outer rim + inner ring binding the blades (rings in the X-Y plane).
    prims.push(prim(
        solid(torus(0.045, wheel_r, iron(IRON_DARK))),
        [hub[0], hub[1], hub[2] + 0.04],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(torus(0.04, wheel_r * 0.34, iron(IRON_DARK))),
        [hub[0], hub[1], hub[2] + 0.04],
        quat_x(FRAC_PI_2),
    ));

    // Tail vane on a boom behind the hub, streaming back along +Z.
    prims.push(prim(
        solid(cuboid_tapered([0.06, 0.06, 1.8], 0.0, iron(IRON_DARK))),
        [0.0, hub[1], hub[2] + 1.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.04, 0.95, 1.3], 0.0, tin(TIN_GREY))),
        [0.0, hub[1], hub[2] + 2.2],
        id_quat(),
    ));

    // Pump rod down the tower centre to a small iron wellhead.
    prims.push(prim(
        solid(cylinder_tapered(
            0.04,
            tower_h + 0.3,
            6,
            0.0,
            iron(IRON_DARK),
        )),
        [0.0, (tower_h + 0.3) * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.55, 0.5, 0.55], 0.0, iron(IRON_DARK))),
        [0.0, 0.25, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.07, 0.7, 8, 0.0, iron(IRON_DARK))),
        [0.0, 0.35, -0.5],
        quat_x(-0.5),
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
