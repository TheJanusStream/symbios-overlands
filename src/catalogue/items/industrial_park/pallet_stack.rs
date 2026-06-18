//! Pallet stack — an Industrial-Park prop. A stack of wooden pallets beside a
//! couple of shipping crates, waiting on the loading apron.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_y, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{cladding, timber};

/// Pale pallet wood.
const PALLET_WOOD: [f32; 3] = [0.60, 0.46, 0.28];
/// Crate wood.
const CRATE_WOOD: [f32; 3] = [0.48, 0.36, 0.22];

pub struct PalletStack;

impl CatalogueEntry for PalletStack {
    fn slug(&self) -> &'static str {
        "pallet_stack"
    }
    fn name(&self) -> &'static str {
        "Pallet Stack"
    }
    fn description(&self) -> &'static str {
        "Stack of wooden pallets beside a couple of shipping crates."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::IndustrialPark]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::INDUSTRIAL_BAND
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
    // One pallet = a top deck on three bearers.
    let pallet = |y: f32| -> Vec<Generator> {
        let mut v = vec![prim(
            solid(cuboid_tapered([1.2, 0.06, 1.0], 0.0, timber(PALLET_WOOD))),
            [0.0, y + 0.12, 0.0],
            id_quat(),
        )];
        for bz in [-0.45_f32, 0.0, 0.45] {
            v.push(prim(
                solid(cuboid_tapered(
                    [1.2, 0.1, 0.12],
                    0.0,
                    timber([0.5, 0.38, 0.22]),
                )),
                [0.0, y + 0.05, bz],
                id_quat(),
            ));
        }
        v
    };

    // Stack of pallets — the first pallet's deck is the root.
    let mut prims = Vec::new();
    for k in 0..5 {
        prims.extend(pallet(k as f32 * 0.18));
    }

    // A couple of crates alongside.
    prims.push(prim(
        solid(cuboid_tapered([1.1, 1.0, 1.1], 0.0, timber(CRATE_WOOD))),
        [1.5, 0.5, 0.2],
        quat_y(0.2),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.9, 0.8, 0.9],
            0.0,
            cladding([0.5, 0.5, 0.46]),
        )),
        [1.6, 1.4, 0.0],
        quat_y(-0.15),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&PalletStack.build(""), "pallet_stack");
    }
}
