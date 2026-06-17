//! Parking stack — a Cyberpunk secondary. An open multi-deck slab tower
//! on corner pillars, each deck edged with a neon band. The low, wide
//! counterpoint to the megatower's height.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{NEON_CYAN, concrete};

pub struct ParkingStack;

impl CatalogueEntry for ParkingStack {
    fn slug(&self) -> &'static str {
        "parking_stack"
    }
    fn name(&self) -> &'static str {
        "Parking Stack"
    }
    fn description(&self) -> &'static str {
        "Open multi-deck parking structure with neon-edged floors."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    // A parking structure is board-formed concrete, not glossy metal.
    let conc = [0.30_f32, 0.31, 0.34];
    let slab_h = 0.4;
    let (w, depth) = (11.0_f32, 8.0_f32);

    let mut root = prim(
        solid(cuboid_tapered([w, slab_h, depth], 0.0, concrete(conc))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(w, depth, [0.0, 0.0], 2.5);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Corner pillars.
    let total_h = 9.0;
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            root.children.push(prim(
                solid(cuboid_tapered([0.6, total_h, 0.6], 0.0, concrete(conc))),
                [
                    sx * (w * 0.5 - 0.6),
                    rel(slab_h + total_h * 0.5),
                    sz * (depth * 0.5 - 0.6),
                ],
                id_quat(),
            ));
        }
    }

    // Three decks, each with a neon edge band just beneath it.
    let decks = 3;
    for d in 0..decks {
        let dy = slab_h + total_h * (d as f32 + 1.0) / (decks as f32 + 0.5);
        root.children.push(prim(
            solid(cuboid_tapered(
                [w - 1.0, 0.3, depth - 1.0],
                0.0,
                concrete(conc),
            )),
            [0.0, rel(dy), 0.0],
            id_quat(),
        ));
        // Neon edge band (slightly wider, emissive) under the deck lip.
        root.children.push(prim(
            cuboid_tapered([w - 0.6, 0.18, depth - 0.6], 0.0, glow(NEON_CYAN, 5.0)),
            [0.0, rel(dy - 0.25), 0.0],
            id_quat(),
        ));
    }

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ParkingStack.build(""), "parking_stack");
    }

    #[test]
    fn has_neon() {
        assert!(super::super::has_emissive(&ParkingStack.build("")));
    }
}
