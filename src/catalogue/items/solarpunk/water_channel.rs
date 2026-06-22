//! Water channel — a Solarpunk prop. A timber rill carrying a ribbon of
//! water between low banks, past a little stone weir, fringed with reeds.
//! Scatter clutter irrigating the gardens.

use crate::catalogue::items::util::{assemble, cone, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_PALE, CROP_GREEN, TIMBER_WARM, WATER_BLUE, concrete, foliage, timber, water};

pub struct WaterChannel;

impl CatalogueEntry for WaterChannel {
    fn slug(&self) -> &'static str {
        "water_channel"
    }
    fn name(&self) -> &'static str {
        "Water Channel"
    }
    fn description(&self) -> &'static str {
        "Timber rill carrying a ribbon of water past a little stone weir."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Solarpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SOLAR_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let len = 3.6_f32;

    let mut prims = vec![
        // Timber channel floor — the root, running along Z.
        prim(
            solid(cuboid_tapered([0.84, 0.12, len], 0.0, timber(TIMBER_WARM))),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
    ];

    // Two low timber banks forming the U-channel.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, 0.4, len], 0.0, timber(TIMBER_WARM))),
            [sx * 0.35, 0.26, 0.0],
            id_quat(),
        ));
    }
    // Recessed water ribbon between the banks (top below the bank rim).
    prims.push(prim(
        cuboid_tapered([0.52, 0.18, len - 0.1], 0.0, water(WATER_BLUE)),
        [0.0, 0.21, 0.0],
        id_quat(),
    ));

    // A little stone weir across the rill, with a notch of water spilling over.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.86, 0.5, 0.2],
            0.0,
            concrete(CONCRETE_PALE),
        )),
        [0.0, 0.25, 0.45],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.26, 0.16, 0.26], 0.0, water(WATER_BLUE)),
        [0.0, 0.33, 0.45],
        id_quat(),
    ));

    // Reed clumps fringing both banks.
    for sx in [-1.0_f32, 1.0] {
        for (i, z) in [-1.4_f32, -0.6, 0.9, 1.5].iter().enumerate() {
            let lean = 0.06 * (i as f32 - 1.5);
            for (j, dz) in [-0.08_f32, 0.06, 0.0].iter().enumerate() {
                let h = 0.46 + ((i + j) % 3) as f32 * 0.1;
                prims.push(prim(
                    solid(cone(0.04, h, 5, foliage(CROP_GREEN))),
                    [sx * (0.46 + j as f32 * 0.05), 0.4, z + dz],
                    quat_x(lean),
                ));
            }
        }
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&WaterChannel.build(""), "water_channel");
    }
}
