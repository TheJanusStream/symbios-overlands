//! Vending machine — a Roadside prop. A lit enamel drinks machine with a
//! glowing selection panel and a chrome dispenser slot. Scatter clutter
//! standing against the store or motel wall.

use crate::catalogue::items::util::{assemble, cuboid_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHROME_BRIGHT, ENAMEL_RED, chrome, enamel};

/// Cool lit blue-white of the machine's display panel.
const PANEL_LIT: [f32; 3] = [0.45, 0.85, 1.0];

pub struct VendingMachine;

impl CatalogueEntry for VendingMachine {
    fn slug(&self) -> &'static str {
        "vending_machine"
    }
    fn name(&self) -> &'static str {
        "Vending Machine"
    }
    fn description(&self) -> &'static str {
        "Lit enamel drinks machine with a glowing selection panel."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Roadside]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ROADSIDE_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 0.7,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let prims = vec![
        // Enamel body — the root.
        prim(
            solid(cuboid_tapered([0.9, 1.9, 0.8], 0.0, enamel(ENAMEL_RED))),
            [0.0, 0.95, 0.0],
            id_quat(),
        ),
        // Lit selection panel on the front.
        prim(
            cuboid_tapered([0.7, 1.4, 0.1], 0.0, glow(PANEL_LIT, 2.0)),
            [0.0, 1.1, 0.41],
            id_quat(),
        ),
        // Chrome dispenser slot at the bottom.
        prim(
            solid(cuboid_tapered([0.6, 0.2, 0.1], 0.0, chrome(CHROME_BRIGHT))),
            [0.0, 0.4, 0.42],
            id_quat(),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&VendingMachine.build(""), "vending_machine");
    }
}
