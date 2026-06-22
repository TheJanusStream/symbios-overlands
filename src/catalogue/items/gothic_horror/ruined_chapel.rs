//! Ruined chapel — the Gothic-Horror *poor* landmark. A roofless, crumbling
//! little chapel of broken mossy walls and a shattered arch, rubble strewn at
//! its foot and a leaning grave-cross. The forsaken counterpart to the
//! [`cathedral`](super::cathedral): same faith, opposite end of the prosperity
//! axis (`Poor`), so a destitute gothic room grows the abandoned ruin instead
//! of the consecrated cathedral.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the footing.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEADWOOD, STONE_MOSS, fx, mossy, pointed_arch, wood};

pub struct RuinedChapel;

impl CatalogueEntry for RuinedChapel {
    fn slug(&self) -> &'static str {
        "ruined_chapel"
    }
    fn name(&self) -> &'static str {
        "Ruined Chapel"
    }
    fn description(&self) -> &'static str {
        "Roofless crumbling chapel of broken mossy walls and a shattered arch."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 7.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let foot_h = 0.4_f32;
    let zf = -1.9_f32; // standing front gable-wall plane (-Z)
    let proud = |k: f32| zf - k;
    let ms = || mossy(STONE_MOSS);

    let mut prims = vec![
        // Stone footing — the root.
        prim(
            solid(cuboid_tapered([7.0, foot_h, 5.0], 0.0, ms())),
            [0.0, foot_h * 0.5, 0.0],
            id_quat(),
        ),
    ];

    // ---- Standing front gable-wall fragment (-Z) with a broken traceried
    //      pointed window. Built as a frame around the opening: tall intact
    //      left jamb, broken-short right jamb, low sill wall, pointed-arch head
    //      and open (glassless) tracery mullions. ----
    prims.push(prim(
        solid(cuboid_tapered([1.4, 5.0, 0.55], 0.0, ms())),
        [-1.85, foot_h + 2.5, zf],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.25, 3.4, 0.5], 0.04, ms())),
        [1.85, foot_h + 1.7, zf],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([2.6, 1.4, 0.55], 0.0, ms())),
        [0.0, foot_h + 0.7, zf],
        id_quat(),
    ));
    let win_sill = foot_h + 1.4;
    let win_half = 0.85_f32;
    let win_spring = win_sill + 1.9;
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.16, 1.9, 0.42], 0.0, ms())),
            [s * win_half, win_sill + 0.95, proud(0.03)],
            id_quat(),
        ));
    }
    prims.extend(pointed_arch(
        [0.0, win_spring, proud(0.03)],
        win_half,
        0.12,
        ms(),
    ));
    // Open tracery mullions against the empty sky.
    for x in [-0.42_f32, 0.42] {
        prims.push(prim(
            solid(cuboid_tapered([0.08, 2.5, 0.32], 0.0, ms())),
            [x, win_sill + 1.25, proud(0.01)],
            id_quat(),
        ));
    }
    // Broken gable-peak fragment teetering above the left jamb.
    prims.push(prim(
        solid(cuboid_tapered([1.3, 1.3, 0.5], 0.5, ms())),
        [-1.55, foot_h + 5.4, zf],
        quat_z(0.16),
    ));

    // ---- A half-fallen freestanding pointed arch (cloister fragment): only
    //      the right springer survives, reaching toward a lost apex. ----
    let arc_x = -3.0_f32;
    let arc_z = -0.2_f32;
    let arc_spring = foot_h + 1.9;
    prims.push(prim(
        solid(cuboid_tapered([0.26, 1.4, 0.5], 0.06, ms())),
        [arc_x - 0.7, foot_h + 0.7, arc_z],
        quat_z(-0.12),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.26, 1.9, 0.5], 0.0, ms())),
        [arc_x + 0.7, foot_h + 0.95, arc_z],
        id_quat(),
    ));
    let [right_arc, _fallen] = pointed_arch([arc_x, arc_spring, arc_z], 0.7, 0.14, ms());
    prims.push(right_arc);

    // ---- Collapsed side walls (running in Z) and low back wall. ----
    for (x, h, z) in [
        (-3.0_f32, 1.2_f32, 1.9_f32),
        (3.0, 1.9, 0.6),
        (3.0, 0.8, 2.1),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([0.5, h, 1.7], 0.0, ms())),
            [x, foot_h + h * 0.5, z],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([6.0, 1.5, 0.5], 0.0, ms())),
        [0.0, foot_h + 0.75, 2.3],
        id_quat(),
    ));

    // ---- Rubble, a fallen column drum, a leaning grave-cross. ----
    for (rx, rz, sc) in [
        (0.6_f32, 1.2_f32, 1.0_f32),
        (-1.4, 0.6, 0.8),
        (1.9, -0.5, 0.9),
        (2.3, 1.5, 0.7),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([0.9 * sc, 0.5 * sc, 0.9 * sc], 0.45, ms())),
            [rx, foot_h + 0.25 * sc, rz],
            quat_x(0.2),
        ));
    }
    prims.push(prim(
        solid(cylinder_tapered(0.34, 1.0, 12, 0.0, ms())),
        [1.3, foot_h + 0.34, -0.3],
        quat_z(FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.16, 1.6, 0.16], 0.0, wood(DEADWOOD))),
        [-2.4, foot_h + 0.8, -1.0],
        quat_x(0.22),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.16, 0.16], 0.0, wood(DEADWOOD))),
        [-2.4, foot_h + 1.3, -0.9],
        quat_x(0.22),
    ));

    let mut root = assemble(prims);
    // Signature life: graveyard mist creeping through the ruin.
    root.children
        .push(fx::ground_mist([0.0, 0.3, zf - 2.0], 0x60F0_C40E));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RuinedChapel.build(""), "ruined_chapel");
    }
}
