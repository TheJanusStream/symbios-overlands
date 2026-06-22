//! Jail — a Wild-West secondary. A squat fieldstone lock-up with iron-barred
//! windows, a heavy iron door and a flat tin roof. The marshal's lock-up of
//! the boomtown.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the base.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{IRON_DARK, STONE_TAN, TIN_GREY, clapboard, iron, stone, tin};

pub struct Jail;

impl CatalogueEntry for Jail {
    fn slug(&self) -> &'static str {
        "jail"
    }
    fn name(&self) -> &'static str {
        "Jail"
    }
    fn description(&self) -> &'static str {
        "Squat fieldstone lock-up with iron-barred windows, a heavy door and a tin roof."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A fieldstone relieving arch humping over an opening — a half-torus seated
/// with its springers at the lintel ends. Decorative trim, so non-solid.
fn relieving_arch(center: [f32; 3], minor: f32, major: f32) -> Generator {
    prim(
        with_cut(
            torus(minor, major, stone(STONE_TAN)),
            [0.0, 0.5],
            [0.0, 1.0],
            0.0,
        ),
        center,
        quat_x(-FRAC_PI_2),
    )
}

fn build_tree() -> Generator {
    let body_w = 6.0_f32;
    let body_h = 3.2_f32;
    let body_d = 5.0_f32;
    // Render FRONT = −Z — barred windows, door and sign all face −Z.
    let front_z = -body_d * 0.5;

    let mut prims = vec![
        // Fieldstone body — the root.
        prim(
            solid(cuboid_tapered(
                [body_w, body_h, body_d],
                0.0,
                stone(STONE_TAN),
            )),
            [0.0, body_h * 0.5, 0.0],
            id_quat(),
        ),
    ];
    // Stone footing course, oversailed by the body.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.3, 0.5, body_d + 0.3],
            0.0,
            stone(STONE_TAN),
        )),
        [0.0, 0.25, 0.0],
        id_quat(),
    ));
    // Recessed tin roof ringed by a heavy stone parapet cap.
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w - 0.5, 0.3, body_d - 0.5],
            0.0,
            tin(TIN_GREY),
        )),
        [0.0, body_h + 0.1, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [body_w + 0.3, 0.55, body_d + 0.3],
            0.0,
            stone(STONE_TAN),
        )),
        [0.0, body_h + 0.28, 0.0],
        id_quat(),
    ));
    // Stone corner pilasters on the front, adding heft and a shadow line.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.5, body_h, 0.5], 0.0, stone(STONE_TAN))),
            [sx * (body_w * 0.5 - 0.1), body_h * 0.5, front_z + 0.05],
            id_quat(),
        ));
    }

    // Two iron-barred windows with stone sills, lintels and relieving arches.
    for sx in [-1.0_f32, 1.0] {
        let cx = sx * 1.7;
        // Dark recessed reveal.
        prims.push(prim(
            solid(cuboid_tapered(
                [0.9, 1.0, 0.2],
                0.0,
                stone([0.18, 0.16, 0.14]),
            )),
            [cx, 1.95, front_z + 0.06],
            id_quat(),
        ));
        // Iron bars in weathered steel — lighter than the near-black reveal so
        // they silhouette against it instead of vanishing.
        for bx in [-0.28_f32, 0.0, 0.28] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.06, 1.0, 0.08],
                    0.0,
                    iron([0.44, 0.44, 0.47]),
                )),
                [cx + bx, 1.95, front_z - 0.05],
                id_quat(),
            ));
        }
        // Stone sill + lintel.
        prims.push(prim(
            solid(cuboid_tapered([1.1, 0.16, 0.3], 0.0, stone(STONE_TAN))),
            [cx, 1.4, front_z - 0.06],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([1.1, 0.22, 0.3], 0.0, stone(STONE_TAN))),
            [cx, 2.55, front_z - 0.06],
            id_quat(),
        ));
        prims.push(relieving_arch([cx, 2.74, front_z - 0.06], 0.12, 0.5));
    }

    // Heavy iron-banded door, recessed under a relieving arch.
    prims.push(prim(
        solid(cuboid_tapered([1.2, 2.3, 0.2], 0.0, iron(IRON_DARK))),
        [0.0, 1.15, front_z + 0.0],
        id_quat(),
    ));
    for band_y in [0.7_f32, 1.6] {
        prims.push(prim(
            solid(cuboid_tapered(
                [1.24, 0.12, 0.06],
                0.0,
                iron([0.1, 0.1, 0.11]),
            )),
            [0.0, band_y, front_z - 0.11],
            id_quat(),
        ));
    }
    prims.push(relieving_arch([0.0, 2.5, front_z - 0.06], 0.16, 0.72));

    // "JAIL" sign board: a weathered cream plank on a dark backing so it reads
    // as a mounted sign rather than a pale patch against the stone.
    prims.push(prim(
        solid(cuboid_tapered([2.1, 0.7, 0.1], 0.0, iron([0.12, 0.1, 0.1]))),
        [0.0, body_h + 0.3, front_z - 0.16],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [1.8, 0.48, 0.12],
            0.0,
            clapboard([0.86, 0.8, 0.66]),
        )),
        [0.0, body_h + 0.3, front_z - 0.22],
        id_quat(),
    ));

    // Stone chimney with a short stovepipe at the back corner.
    prims.push(prim(
        solid(cuboid_tapered([0.7, 1.5, 0.7], 0.0, stone(STONE_TAN))),
        [2.1, body_h + 0.75, 1.6],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(0.14, 0.7, 8, 0.0, iron(IRON_DARK))),
        [2.1, body_h + 1.85, 1.6],
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
        assert_sanitize_stable(&Jail.build(""), "jail");
    }
}
