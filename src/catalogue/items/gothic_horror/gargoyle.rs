//! Gargoyle — a Gothic-Horror prop. A crouched stone grotesque on a plinth,
//! wings half-spread, snout jutting. Scatter clutter watching from the
//! necropolis.
//!
//! The wings tilt with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, glow, id_quat, prim, quat_mul, quat_x, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{STONE_DARK, stone};

pub struct Gargoyle;

impl CatalogueEntry for Gargoyle {
    fn slug(&self) -> &'static str {
        "gargoyle"
    }
    fn name(&self) -> &'static str {
        "Gargoyle"
    }
    fn description(&self) -> &'static str {
        "Crouched stone grotesque on a plinth, wings half-spread, snout jutting."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // The grotesque crouches facing the -Z hero front, its fanged snout and lit
    // eyes jutting toward the camera, bat wings half-spread behind.
    let st = || stone(STONE_DARK);
    let mut prims = vec![
        // Corbel plinth — the root.
        prim(
            solid(cuboid_tapered([0.95, 1.1, 0.9], 0.08, st())),
            [0.0, 0.55, 0.0],
            id_quat(),
        ),
    ];

    // Crouched haunches.
    prims.push(prim(
        solid(cuboid_tapered([0.72, 0.5, 0.85], 0.12, st())),
        [0.0, 1.32, -0.02],
        id_quat(),
    ));
    // Chest leaning forward over the corbel edge.
    prims.push(prim(
        solid(cuboid_tapered([0.62, 0.6, 0.5], 0.14, st())),
        [0.0, 1.75, -0.2],
        quat_x(-0.22),
    ));
    // Clawed feet gripping the front edge.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.18, 0.18, 0.3], 0.0, st())),
            [s * 0.26, 1.12, -0.42],
            id_quat(),
        ));
        for c in [-1.0_f32, 0.0, 1.0] {
            prims.push(prim(
                solid(cone(0.05, 0.22, 5, st())),
                [s * 0.26 + c * 0.07, 1.06, -0.56],
                quat_x(-1.7),
            ));
        }
    }
    // Forelimbs reaching down to grip, with claws.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.13, 0.55, 0.13], 0.2, st())),
            [s * 0.32, 1.5, -0.34],
            quat_x(0.35),
        ));
        prims.push(prim(
            solid(cone(0.06, 0.2, 5, st())),
            [s * 0.32, 1.2, -0.5],
            quat_x(-1.4),
        ));
    }

    // Beastly head.
    prims.push(prim(
        solid(cuboid_tapered([0.46, 0.42, 0.46], 0.1, st())),
        [0.0, 2.12, -0.28],
        id_quat(),
    ));
    // Heavy brow ridge.
    prims.push(prim(
        solid(cuboid_tapered([0.5, 0.12, 0.16], 0.0, st())),
        [0.0, 2.24, -0.52],
        quat_x(0.2),
    ));
    // Jutting fanged jaw / waterspout snout.
    prims.push(prim(
        solid(cuboid_tapered([0.36, 0.2, 0.34], 0.15, st())),
        [0.0, 1.98, -0.56],
        quat_x(-0.15),
    ));
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cone(0.045, 0.16, 5, st())),
            [s * 0.09, 1.86, -0.66],
            quat_x(-2.9),
        ));
    }
    // Lit eyes glaring from beneath the brow.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(sphere(0.06, 6, glow([1.0, 0.5, 0.14], 2.2))),
            [s * 0.12, 2.12, -0.5],
            id_quat(),
        ));
    }
    // Curled horns sweeping back.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cone(0.07, 0.4, 6, st())),
            [s * 0.16, 2.36, -0.18],
            quat_x(0.7),
        ));
    }
    // Pointed ears.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cone(0.06, 0.22, 5, st())),
            [s * 0.24, 2.3, -0.32],
            quat_z(s * 0.4),
        ));
    }

    // Bat wings, half-spread behind.
    for s in [-1.0_f32, 1.0] {
        // Membrane.
        prims.push(prim(
            solid(cuboid_tapered([0.06, 1.0, 0.85], 0.5, st())),
            [s * 0.42, 1.85, 0.12],
            quat_mul(quat_z(s * 0.55), quat_x(-0.4)),
        ));
        // Leading-edge spar.
        prims.push(prim(
            solid(cone(0.05, 1.05, 5, st())),
            [s * 0.5, 1.95, 0.12],
            quat_mul(quat_z(s * 0.75), quat_x(-0.4)),
        ));
    }
    // Coiled tail flicking up behind.
    prims.push(prim(
        solid(cone(0.1, 0.7, 6, st())),
        [0.0, 1.4, 0.42],
        quat_x(1.9),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Gargoyle.build(""), "gargoyle");
    }
}
