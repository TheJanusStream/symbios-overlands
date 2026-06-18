//! Swing set — a Suburban prop. A galvanised A-frame swing set with two
//! chain-hung seats: the centrepiece of a back yard.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::enamel;

/// Galvanised steel frame.
const FRAME: [f32; 3] = [0.60, 0.62, 0.64];
/// Dark chain / seat.
const SEAT: [f32; 3] = [0.14, 0.16, 0.18];

pub struct SwingSet;

impl CatalogueEntry for SwingSet {
    fn slug(&self) -> &'static str {
        "swing_set"
    }
    fn name(&self) -> &'static str {
        "Swing Set"
    }
    fn description(&self) -> &'static str {
        "Galvanised A-frame swing set with two chain-hung seats."
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
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let bar_y = 2.4_f32;
    let leg_len = 2.6_f32;
    let splay = 0.358_f32;

    // Top bar — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([4.0, 0.12, 0.12], 0.0, enamel(FRAME))),
        [0.0, bar_y, 0.0],
        id_quat(),
    )];

    // A-frame legs at each end, splayed fore and aft.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.1, leg_len, 0.1], 0.0, enamel(FRAME))),
                [sx * 2.0, bar_y - leg_len * 0.5 + 0.05, sz * 0.45],
                quat_x(sz * splay),
            ));
        }
    }

    // Two chain-hung seats.
    for sx in [-0.85_f32, 0.85] {
        for cz in [-0.12_f32, 0.12] {
            prims.push(prim(
                solid(cuboid_tapered([0.03, 1.45, 0.03], 0.0, enamel(SEAT))),
                [sx, bar_y - 0.75, cz],
                id_quat(),
            ));
        }
        prims.push(prim(
            solid(cuboid_tapered([0.5, 0.08, 0.26], 0.0, enamel(SEAT))),
            [sx, bar_y - 1.5, 0.0],
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
        assert_sanitize_stable(&SwingSet.build(""), "swing_set");
    }
}
