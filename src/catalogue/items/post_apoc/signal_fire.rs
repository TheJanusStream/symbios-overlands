//! Signal fire — a Post-apocalyptic prop. A scrap brazier hoisted on a pole,
//! burning as a beacon. Scatter clutter marking the holdout; its fire is
//! emissive trim the ruin pass can darken.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, quat_mul, quat_x, quat_y, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{FIRE_ORANGE, RUST_BROWN, STEEL_GREY, fx, rusted};

pub struct SignalFire;

impl CatalogueEntry for SignalFire {
    fn slug(&self) -> &'static str {
        "signal_fire"
    }
    fn name(&self) -> &'static str {
        "Signal Fire"
    }
    fn description(&self) -> &'static str {
        "Scrap brazier hoisted on a pole, burning as a beacon."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::PostApoc]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::POSTAPOC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let pole_h = 2.6_f32;

    let mut prims = vec![
        // Scrap pole — the root.
        prim(
            solid(cylinder_tapered(0.12, pole_h, 6, 0.1, rusted(STEEL_GREY))),
            [0.0, pole_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // Three splayed scrap legs bracing the pole — so the beacon reads as a
    // planted tripod, not a stick balanced on the ground.
    for k in 0..3 {
        let a = k as f32 / 3.0 * TAU + 0.4;
        prims.push(prim(
            solid(cylinder_tapered(0.06, 1.5, 5, 0.0, rusted(STEEL_GREY))),
            [a.cos() * 0.45, 0.6, a.sin() * 0.45],
            quat_mul(quat_y(a), quat_x(0.45)),
        ));
    }

    // Brazier basket atop the pole.
    prims.push(prim(
        solid(cylinder_tapered(0.45, 0.5, 10, 0.3, rusted(RUST_BROWN))),
        [0.0, pole_h + 0.25, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        torus(0.06, 0.45, rusted(STEEL_GREY)),
        [0.0, pole_h + 0.45, 0.0],
        id_quat(),
    ));
    // Vertical cage bars around the basket so the flames lick through scrap.
    for k in 0..6 {
        let a = k as f32 / 6.0 * TAU;
        prims.push(prim(
            solid(cylinder_tapered(0.03, 0.6, 4, 0.0, rusted(STEEL_GREY))),
            [a.cos() * 0.42, pole_h + 0.5, a.sin() * 0.42],
            id_quat(),
        ));
    }
    // Glowing fire core — emissive, leaping proud of the cage rim. Held at a
    // moderate strength so bloom keeps it incandescent orange, not white-hot.
    let mut fire = prim(
        solid(cylinder_tapered(0.38, 0.62, 8, 0.0, glow(FIRE_ORANGE, 4.0))),
        [0.0, pole_h + 0.62, 0.0],
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);

    let mut root = assemble(prims);
    // Signature life: the beacon flame.
    root.children
        .push(fx::fire_flame([0.0, pole_h + 0.9, 0.0], 0x0A57_F2E2));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SignalFire.build(""), "signal_fire");
    }
}
