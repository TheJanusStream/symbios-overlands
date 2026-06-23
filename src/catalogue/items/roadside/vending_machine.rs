//! Vending machine — a Roadside prop. A lit enamel drinks machine with a
//! glowing selection panel and a chrome dispenser slot. Scatter clutter
//! standing against the store or motel wall.

use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHROME_BRIGHT, ENAMEL_RED, GLASS_TINT, SIGN_AMBER, chrome, enamel, glass, sign_board};

/// Cool lit blue of the machine's selection panel — deep-saturated so the lit
/// cells read as a blue selector rather than washing to white.
const PANEL_LIT: [f32; 3] = [0.16, 0.62, 1.0];

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
    let front = -0.4_f32; // −Z camera-facing wall plane

    let mut prims = vec![
        // Enamel body — the root.
        prim(
            solid(cuboid_tapered([0.9, 1.9, 0.8], 0.0, enamel(ENAMEL_RED))),
            [0.0, 0.95, 0.0],
            id_quat(),
        ),
    ];

    // Lit brand header strip across the top of the front (deep-sat amber).
    for g in sign_board(
        [0.0, 1.7, front - 0.02],
        [0.78, 0.28],
        (3, 1),
        SIGN_AMBER,
        2.0,
        -1.0,
    ) {
        prims.push(g);
    }

    // Lit glazed product window on the left half.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.42, 0.95, 0.08],
            0.0,
            chrome(CHROME_BRIGHT),
        )),
        [-0.21, 1.05, front + 0.02],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.34, 0.86, 0.08], 0.0, glass(GLASS_TINT, 1.5)),
        [-0.21, 1.05, front - 0.04],
        id_quat(),
    ));

    // Segmented blue selection panel (2×3 buttons) on the right half.
    for g in sign_board(
        [0.22, 1.1, front - 0.02],
        [0.36, 0.86],
        (2, 3),
        PANEL_LIT,
        2.0,
        -1.0,
    ) {
        prims.push(g);
    }
    // Chrome coin / card slot beside the selector.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.12, 0.24, 0.06],
            0.0,
            chrome(CHROME_BRIGHT),
        )),
        [0.22, 0.55, front - 0.05],
        id_quat(),
    ));
    // Chrome dispenser tray at the bottom.
    prims.push(prim(
        solid(cuboid_tapered([0.66, 0.2, 0.1], 0.0, chrome(CHROME_BRIGHT))),
        [0.0, 0.36, front - 0.04],
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
        assert_sanitize_stable(&VendingMachine.build(""), "vending_machine");
    }
}
