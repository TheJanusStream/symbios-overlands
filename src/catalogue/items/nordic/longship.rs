//! Longship — a Nordic prop, and the steading's proudest one. A beached
//! clinker-hulled raider with a carved dragon prow, a furled striped sail
//! on its mast, and a row of painted shields slung along each gunwale.
//! Larger than the usual scatter clutter, it reads as the crew's ship drawn
//! up on the shingle.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    IRON_DARK, SHIELD_BLUE, SHIELD_CREAM, SHIELD_GOLD, SHIELD_RED, WOOD_DARK, WOOD_WARM, cloth,
    round_shield, timber,
};

pub struct Longship;

impl CatalogueEntry for Longship {
    fn slug(&self) -> &'static str {
        "longship"
    }
    fn name(&self) -> &'static str {
        "Longship"
    }
    fn description(&self) -> &'static str {
        "Beached clinker-built longship with a dragon prow and shielded gunwales."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 24.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let hull_y = 0.85;
    let half_w = 0.9;

    let mut prims = vec![
        // Hull mid-section (clinker planking) — the root.
        prim(
            solid(cuboid_tapered(
                [9.0, 1.3, 2.0 * half_w],
                0.15,
                timber(WOOD_WARM),
            )),
            [0.0, hull_y, 0.0],
            id_quat(),
        ),
        // Keel strip beneath.
        prim(
            solid(cuboid_tapered([10.0, 0.3, 0.4], 0.0, timber(WOOD_DARK))),
            [0.0, hull_y - 0.7, 0.0],
            id_quat(),
        ),
    ];

    // Curved-up prow (+X) and stern (-X).
    prims.push(prim(
        solid(cuboid_tapered([1.6, 2.8, 1.5], 0.55, timber(WOOD_WARM))),
        [4.8, hull_y + 0.9, 0.0],
        quat_x(0.0),
    ));
    prims.push(prim(
        solid(cuboid_tapered([1.4, 2.2, 1.4], 0.55, timber(WOOD_WARM))),
        [-4.8, hull_y + 0.7, 0.0],
        quat_x(0.0),
    ));
    // Dragon head crowning the prow.
    prims.push(prim(
        solid(cuboid_tapered([0.6, 0.8, 0.45], 0.3, timber(WOOD_DARK))),
        [5.5, hull_y + 2.4, 0.0],
        quat_x(0.0),
    ));
    prims.push(prim(
        cone(0.2, 0.8, 7, timber(WOOD_DARK)),
        [6.1, hull_y + 2.6, 0.0],
        quat_x(FRAC_PI_2),
    ));

    // Gunwale top rails.
    for sz in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([9.2, 0.18, 0.2], 0.0, timber(WOOD_DARK))),
            [0.0, hull_y + 0.65, sz * half_w],
            id_quat(),
        ));
    }

    // Painted shields slung along each gunwale, facing outward.
    let palette = [
        SHIELD_RED,
        SHIELD_BLUE,
        SHIELD_GOLD,
        SHIELD_RED,
        SHIELD_BLUE,
    ];
    for (i, face) in palette.iter().enumerate() {
        let x = -3.4 + i as f32 * 1.7;
        prims.push(round_shield(
            [x, hull_y + 0.2, half_w + 0.08],
            quat_x(FRAC_PI_2),
            *face,
            IRON_DARK,
        ));
        prims.push(round_shield(
            [x, hull_y + 0.2, -(half_w + 0.08)],
            quat_x(-FRAC_PI_2),
            palette[(i + 2) % palette.len()],
            IRON_DARK,
        ));
    }

    // Mast with a furled striped sail on its yard.
    let mast_h = 5.2;
    prims.push(prim(
        solid(cylinder_tapered(0.16, mast_h, 8, 0.1, timber(WOOD_WARM))),
        [0.0, hull_y + mast_h * 0.5, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([4.4, 0.18, 0.18], 0.0, timber(WOOD_DARK))),
        [0.0, hull_y + mast_h - 0.6, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.8, 0.55, 0.45], 0.0, cloth(SHIELD_RED, SHIELD_CREAM)),
        [0.0, hull_y + mast_h - 0.95, 0.0],
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
        assert_sanitize_stable(&Longship.build(""), "longship");
    }
}
