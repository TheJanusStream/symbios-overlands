//! Foundry — a Steampunk secondary. A sooty brick hall with two banded brick
//! chimneys belching smoke, copper pipes up the wall and a glowing furnace
//! door. The roaring heart of the works.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the hall.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, glow, id_quat, prim, quat_x,
    solid, torus, tube, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRASS, BRICK_SOOT, COPPER_ORANGE, IRON_DARK, brass, brick, cog, copper, fx, iron};

pub struct Foundry;

impl CatalogueEntry for Foundry {
    fn slug(&self) -> &'static str {
        "foundry"
    }
    fn name(&self) -> &'static str {
        "Foundry"
    }
    fn description(&self) -> &'static str {
        "Sooty brick hall with smoking chimneys, copper pipes and a glowing furnace door."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Steampunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::STEAM_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 42.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let hall_h = 5.0_f32;
    // Hero (−Z) wall front face sits at z = -4.0; detail rides proud of it.
    let wall = -4.0_f32;

    let mut prims = vec![
        // Sooty brick hall — the root.
        prim(
            solid(cuboid_tapered([12.0, hall_h, 8.0], 0.0, brick(BRICK_SOOT))),
            [0.0, hall_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Brass cornice band where the wall meets the roof.
    prims.push(prim(
        solid(cuboid_tapered([12.2, 0.3, 8.2], 0.0, brass(BRASS))),
        [0.0, hall_h, 0.0],
        id_quat(),
    ));
    // Pitched iron gable roof — ridge running along X.
    prims.push(prim(
        solid(cuboid_tapered_xz(
            [12.6, 1.9, 8.6],
            [0.0, 0.92],
            iron(IRON_DARK),
        )),
        [0.0, hall_h + 0.95, 0.0],
        id_quat(),
    ));

    // Arched glowing furnace door on the −Z hero wall, offset to one side.
    // Deep red-orange at moderate strength: over-bright orange clips to pale
    // cream after tonemapping, so a saturated colour reads hotter than raw power.
    let fire = [1.0_f32, 0.36, 0.09];
    let door_x = -2.8_f32;
    prims.push(prim(
        cuboid_tapered([2.6, 2.6, 0.3], 0.0, glow(fire, 2.8)),
        [door_x, 1.5, wall - 0.1],
        id_quat(),
    ));
    // Semicircle iron arch hood springing from the door head.
    prims.push(prim(
        solid(with_cut(
            torus(0.24, 1.5, iron(IRON_DARK)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        )),
        [door_x, 2.8, wall - 0.1],
        quat_x(-FRAC_PI_2),
    ));
    // Glowing ember sill spilling from the door foot.
    prims.push(prim(
        cuboid_tapered([2.8, 0.3, 0.6], 0.0, glow(fire, 2.4)),
        [door_x, 0.2, wall - 0.25],
        id_quat(),
    ));

    // A great exposed gear train on the −Z wall beside the door; the small
    // pinion sits inboard of the corner pipes so it isn't occluded.
    prims.push(cog(
        [3.1, 3.0, wall - 0.15],
        quat_x(-FRAC_PI_2),
        1.7,
        0.32,
        16,
        brass(BRASS),
        iron(IRON_DARK),
    ));
    prims.push(cog(
        [4.5, 1.55, wall - 0.15],
        quat_x(-FRAC_PI_2),
        0.8,
        0.28,
        12,
        iron(IRON_DARK),
        brass(BRASS),
    ));

    // Hollow copper pipes climbing the −Z wall corners.
    for sx in [-5.7_f32, 5.7] {
        prims.push(prim(
            solid(tube(0.22, 0.14, hall_h - 0.4, 10, copper(COPPER_ORANGE))),
            [sx, (hall_h - 0.4) * 0.5, wall - 0.15],
            id_quat(),
        ));
    }

    // Two banded brick chimneys with hollow flared iron pots, smoking.
    let mut smoky = None;
    for (i, sx) in [-1.0_f32, 1.0].into_iter().enumerate() {
        let cx = sx * 3.8;
        let chimney_h = 8.0;
        let top = hall_h + chimney_h;
        prims.push(prim(
            solid(cylinder_tapered(
                0.9,
                chimney_h,
                12,
                0.18,
                brick(BRICK_SOOT),
            )),
            [cx, hall_h + chimney_h * 0.5, -2.2],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.12, 0.76, brass(BRASS))),
            [cx, top - 1.0, -2.2],
            id_quat(),
        ));
        // Hollow flared chimney pot.
        prims.push(prim(
            solid(tube(0.56, 0.4, 0.9, 12, iron(IRON_DARK))),
            [cx, top + 0.3, -2.2],
            id_quat(),
        ));
        prims.push(prim(
            solid(torus(0.1, 0.62, iron(IRON_DARK))),
            [cx, top + 0.75, -2.2],
            id_quat(),
        ));
        if i == 0 {
            smoky = Some([cx, top + 1.2, -2.2]);
        }
    }

    let mut root = assemble(prims);
    // Signature life: smoke off a chimney, the boiler hiss.
    root.audio = fx::boiler_hiss();
    if let Some(p) = smoky {
        root.children.push(fx::furnace_smoke(p, 0x500F_F0D1));
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Foundry.build(""), "foundry");
    }
}
