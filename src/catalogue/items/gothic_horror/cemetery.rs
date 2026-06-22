//! Cemetery — a Gothic-Horror secondary. A mossy grave plot of leaning
//! headstones behind an iron railing, a stone cross at its heart, mist
//! pooling between the rows. The burial ground of the necropolis.
//!
//! Leaning stones tilt with a single [`quat_x`].

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_BLACK, STONE_MOSS, fx, iron, mossy, pointed_arch};

pub struct Cemetery;

impl CatalogueEntry for Cemetery {
    fn slug(&self) -> &'static str {
        "cemetery"
    }
    fn name(&self) -> &'static str {
        "Cemetery"
    }
    fn description(&self) -> &'static str {
        "Mossy grave plot of leaning headstones behind an iron railing, a stone cross at its heart."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::GothicHorror]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::GOTHIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let ms = || mossy(STONE_MOSS);
    let ir = || iron(IRON_BLACK);
    let mut prims = vec![
        // Mossy grave plot — the root.
        prim(
            solid(cuboid_tapered([8.0, 0.2, 6.0], 0.0, ms())),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
    ];

    // Rows of leaning headstones in mixed Gothic styles.
    let mut k = 0usize;
    for gx in [-2.6_f32, -0.9, 0.8, 2.5] {
        for gz in [-1.4_f32, 0.5, 1.9] {
            let tilt = ((k % 3) as f32 - 1.0) * 0.1;
            let h = 0.8 + (k % 3) as f32 * 0.16;
            let depth = 0.16_f32;
            let by = 0.2 + h * 0.5;
            let top = 0.2 + h;
            // Slab body.
            prims.push(prim(
                solid(cuboid_tapered([0.6, h, depth], 0.05, ms())),
                [gx, by, gz],
                quat_x(tilt),
            ));
            match k % 4 {
                0 => {
                    // Round-topped tablet (half-cylinder cap, round side up).
                    prims.push(prim(
                        solid(with_cut(
                            cylinder_tapered(0.3, depth, 12, 0.0, ms()),
                            [0.5, 1.0],
                            [0.0, 1.0],
                            0.0,
                        )),
                        [gx, top, gz],
                        quat_x(tilt + FRAC_PI_2),
                    ));
                }
                1 => {
                    // Cross-topped.
                    prims.push(prim(
                        solid(cuboid_tapered([0.12, 0.5, depth], 0.0, ms())),
                        [gx, top + 0.22, gz],
                        quat_x(tilt),
                    ));
                    prims.push(prim(
                        solid(cuboid_tapered([0.42, 0.12, depth], 0.0, ms())),
                        [gx, top + 0.26, gz],
                        quat_x(tilt),
                    ));
                }
                2 => {
                    // Pointed (gabled) cap — a four-sided cap reading as a peak.
                    prims.push(prim(
                        solid(cone(0.33, 0.42, 4, ms())),
                        [gx, top + 0.18, gz],
                        quat_x(tilt),
                    ));
                }
                _ => {} // weathered flat-top slab
            }
            k += 1;
        }
    }

    // Central ringed cross monument on a stepped plinth.
    prims.push(prim(
        solid(cuboid_tapered([1.0, 0.3, 1.0], 0.0, ms())),
        [0.0, 0.35, -0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.7, 0.3, 0.7], 0.0, ms())),
        [0.0, 0.65, -0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.32, 2.4, 0.32], 0.06, ms())),
        [0.0, 1.95, -0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.25, 0.32, 0.3], 0.0, ms())),
        [0.0, 2.85, -0.3],
        id_quat(),
    ));
    prims.push(prim(
        solid(torus(0.1, 0.44, ms())),
        [0.0, 2.85, -0.3],
        id_quat(),
    ));

    // ---- Iron cemetery gate + railing on the -Z front. ----
    let gz_f = -3.0_f32;
    // Two stout gateposts with urn finials, flanking the entrance.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.18, 1.8, 0.18], 0.0, ir())),
            [s * 0.95, 0.9, gz_f],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.13, 0.34, 6, ir())),
            [s * 0.95, 1.95, gz_f],
            id_quat(),
        ));
    }
    // Pointed iron arch over the gate.
    prims.extend(pointed_arch([0.0, 1.5, gz_f], 0.85, 0.05, ir()));
    // Gate leaves: vertical bars with spear tips.
    for i in 0..6 {
        let x = -0.7 + i as f32 * 0.28;
        prims.push(prim(
            solid(cylinder_tapered(0.035, 1.4, 6, 0.0, ir())),
            [x, 0.85, gz_f],
            id_quat(),
        ));
        prims.push(prim(
            solid(cone(0.06, 0.2, 6, ir())),
            [x, 1.6, gz_f],
            id_quat(),
        ));
    }
    // Front railing flanking the gate, and a low rail with finials each side.
    for s in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([2.6, 0.08, 0.07], 0.0, ir())),
            [s * 2.55, 0.95, gz_f],
            id_quat(),
        ));
        for i in 0..4 {
            let x = s * (1.4 + i as f32 * 0.75);
            prims.push(prim(
                solid(cylinder_tapered(0.035, 1.1, 6, 0.0, ir())),
                [x, 0.65, gz_f],
                id_quat(),
            ));
            prims.push(prim(
                solid(cone(0.06, 0.2, 6, ir())),
                [x, 1.25, gz_f],
                id_quat(),
            ));
        }
    }

    let mut root = assemble(prims);
    // Signature life: mist pooling between the rows.
    root.children
        .push(fx::ground_mist([0.0, 0.3, 0.0], 0x60F0_CE12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Cemetery.build(""), "cemetery");
    }
}
