//! Airlock — a Space-Outpost prop. A standalone pressure-lock chamber with a
//! lit hatch port and hazard banding. Scatter clutter linking the modules.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    HAZARD_YELLOW, HULL_PANEL, HULL_WHITE, STEEL_DARK, VIEWPORT_LIT, hull, painted, pressure_hatch,
    steel,
};

pub struct Airlock;

impl CatalogueEntry for Airlock {
    fn slug(&self) -> &'static str {
        "airlock"
    }
    fn name(&self) -> &'static str {
        "Airlock"
    }
    fn description(&self) -> &'static str {
        "Standalone pressure-lock chamber with a lit hatch port and hazard banding."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_BAND
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
    let chamber_r = 1.2_f32;
    let mut prims = vec![
        // Hull chamber — the root.
        prim(
            solid(cylinder_tapered(chamber_r, 2.2, 16, 0.0, hull(HULL_WHITE))),
            [0.0, 1.1, 0.0],
            id_quat(),
        ),
    ];

    // Hazard band around the chamber (proud of the hull).
    prims.push(prim(
        solid(cylinder_tapered(
            chamber_r + 0.05,
            0.3,
            16,
            0.0,
            painted(HAZARD_YELLOW),
        )),
        [0.0, 1.85, 0.0],
        id_quat(),
    ));
    // Domed roof cap + a pressure-relief vent stack.
    prims.push(prim(
        solid(cylinder_tapered(chamber_r, 0.3, 16, 0.5, hull(HULL_PANEL))),
        [0.0, 2.3, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(tube(0.18, 0.1, 0.6, 10, steel(STEEL_DARK))),
        [0.55, 2.6, 0.0],
        id_quat(),
    ));

    // Round pressure hatch on the −Z hero face.
    for piece in pressure_hatch(
        [0.0, 1.0, -chamber_r],
        0.78,
        -1.0,
        hull(HULL_PANEL),
        steel(STEEL_DARK),
        glow(VIEWPORT_LIT, 2.0),
    ) {
        prims.push(piece);
    }

    // Threshold step + side conduit running to the next module.
    prims.push(prim(
        solid(cuboid_tapered([1.4, 0.18, 0.5], 0.0, hull(HULL_PANEL))),
        [0.0, 0.09, -1.45],
        id_quat(),
    ));
    prims.push(prim(
        solid(tube(0.12, 0.07, 1.6, 10, steel(STEEL_DARK))),
        [1.15, 0.4, 0.0],
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
        assert_sanitize_stable(&Airlock.build(""), "airlock");
    }
}
