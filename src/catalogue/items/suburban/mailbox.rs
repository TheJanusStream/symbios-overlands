//! Mailbox — a Suburban prop. A roadside post-mounted mailbox with a rounded
//! lid and a little red flag.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{WOOD_BROWN, enamel, wood};

/// Mailbox-flag red.
const FLAG_RED: [f32; 3] = [0.66, 0.14, 0.12];

pub struct Mailbox;

impl CatalogueEntry for Mailbox {
    fn slug(&self) -> &'static str {
        "mailbox"
    }
    fn name(&self) -> &'static str {
        "Mailbox"
    }
    fn description(&self) -> &'static str {
        "Post-mounted roadside mailbox with a rounded lid and a red flag."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.8,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let post_h = 1.1;
    let box_y = post_h + 0.25;

    let mut prims = vec![
        // Timber post — the root.
        prim(
            solid(cuboid_tapered([0.12, post_h, 0.12], 0.0, wood(WOOD_BROWN))),
            [0.0, post_h * 0.5, 0.0],
            id_quat(),
        ),
        // Mailbox body.
        prim(
            solid(cuboid_tapered(
                [0.5, 0.34, 0.7],
                0.0,
                enamel([0.5, 0.5, 0.55]),
            )),
            [0.0, box_y, 0.0],
            id_quat(),
        ),
        // Rounded lid.
        prim(
            solid(cylinder_tapered(
                0.26,
                0.7,
                10,
                0.0,
                enamel([0.55, 0.55, 0.6]),
            )),
            [0.0, box_y + 0.17, 0.0],
            quat_x(FRAC_PI_2),
        ),
    ];

    // Little red flag on the side.
    prims.push(prim(
        solid(cuboid_tapered([0.05, 0.3, 0.04], 0.0, enamel(FLAG_RED))),
        [0.28, box_y + 0.05, 0.2],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.04, 0.16, 0.18], 0.0, enamel(FLAG_RED))),
        [0.3, box_y + 0.15, 0.27],
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
        assert_sanitize_stable(&Mailbox.build(""), "mailbox");
    }
}
