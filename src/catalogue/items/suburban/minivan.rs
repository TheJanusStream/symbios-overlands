//! Minivan — a Suburban prop. The family hauler: a tall boxy body with a
//! glazed greenhouse and dark wheels, parked at the kerb.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, enamel, glass};

/// Minivan body colour.
const VAN_BODY: [f32; 3] = [0.36, 0.40, 0.46];
/// Tyre black.
const TIRE: [f32; 3] = [0.06, 0.06, 0.07];

pub struct Minivan;

impl CatalogueEntry for Minivan {
    fn slug(&self) -> &'static str {
        "minivan"
    }
    fn name(&self) -> &'static str {
        "Minivan"
    }
    fn description(&self) -> &'static str {
        "Tall boxy family minivan with a glazed greenhouse, parked at the kerb."
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
            clearance: 1.6,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Lower body — the root.
        prim(
            solid(cuboid_tapered([4.6, 1.0, 2.0], 0.05, enamel(VAN_BODY))),
            [0.0, 0.7, 0.0],
            id_quat(),
        ),
        // Tall cabin.
        prim(
            solid(cuboid_tapered([3.8, 1.1, 1.9], 0.08, enamel(VAN_BODY))),
            [-0.1, 1.65, 0.0],
            id_quat(),
        ),
        // Glazed greenhouse.
        prim(
            cuboid_tapered([3.6, 0.85, 1.95], 0.08, glass(GLASS_TINT, 0.0)),
            [-0.1, 1.6, 0.0],
            id_quat(),
        ),
    ];

    // Four wheels (dark blocks, read as tyres from the side).
    for (sx, sz) in [(-1.0_f32, -1.0_f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered([0.35, 0.7, 0.7], 0.0, enamel(TIRE))),
            [sx * 1.5, 0.35, sz * 1.0],
            id_quat(),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Minivan.build(""), "minivan");
    }
}
