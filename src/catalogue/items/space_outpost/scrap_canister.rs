//! Scrap canister — a Space-Outpost *poor* prop. A clutch of dented, scorched
//! fuel canisters, one toppled on its side. The debris of the wreck site.
//!
//! The toppled canister is a cylinder tipped on its side with a [`quat_x`] of
//! π/2.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HULL_PANEL, SCORCH, STEEL_DARK, hull, steel};

pub struct ScrapCanister;

impl CatalogueEntry for ScrapCanister {
    fn slug(&self) -> &'static str {
        "scrap_canister"
    }
    fn name(&self) -> &'static str {
        "Scrap Canister"
    }
    fn description(&self) -> &'static str {
        "A clutch of dented, scorched fuel canisters, one toppled on its side."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One upright canister (body + two rib rings) returned for the assemble list.
fn canister(pos: [f32; 3], color: [f32; 3]) -> Generator {
    let mut body = prim(
        solid(cylinder_tapered(0.35, 1.1, 12, 0.0, hull(color))),
        pos,
        id_quat(),
    );
    for ring_y in [-0.3_f32, 0.3] {
        body.children.push(prim(
            torus(0.04, 0.35, steel(STEEL_DARK)),
            [0.0, ring_y, 0.0],
            id_quat(),
        ));
    }
    body
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // First upright canister — the root.
        canister([0.0, 0.55, 0.0], HULL_PANEL),
        canister([0.7, 0.55, 0.2], SCORCH),
    ];

    // A third canister toppled on its side.
    prims.push(prim(
        solid(cylinder_tapered(0.35, 1.1, 12, 0.0, hull(SCORCH))),
        [-0.5, 0.35, -0.4],
        quat_x(FRAC_PI_2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ScrapCanister.build(""), "scrap_canister");
    }
}
