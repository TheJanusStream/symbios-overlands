//! Solar panel — a Solarpunk prop. A glossy gridded photovoltaic array tilted
//! to the sun on a steel A-frame over a concrete footing. Scatter clutter
//! powering the eco-quarter.
//!
//! The footing pad is the [`assemble`] root (flat, `id_quat`); the tilted PV
//! panel is a rotation-safe child (a tilted panel can never be the root, or
//! its rotation spins the legs out of place — the prior version did exactly
//! that). The panel's lit gridded face tilts toward the −Z hero camera.

use crate::catalogue::items::space_outpost::pv_panel;
use crate::catalogue::items::util::{assemble, cuboid_tapered, id_quat, prim, quat_x, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{CONCRETE_PALE, PV_BLUE, STEEL_GREY, concrete, pv, steel};

pub struct SolarPanel;

impl CatalogueEntry for SolarPanel {
    fn slug(&self) -> &'static str {
        "solar_panel"
    }
    fn name(&self) -> &'static str {
        "Solar Panel"
    }
    fn description(&self) -> &'static str {
        "Glossy gridded photovoltaic array tilted to the sun on a steel A-frame."
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
        // Concrete footing pad — the flat root.
        prim(
            solid(cuboid_tapered(
                [2.6, 0.12, 1.9],
                0.0,
                concrete(CONCRETE_PALE),
            )),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
    ];

    // Tall back legs (+Z) and short front legs (−Z) supporting the tilt.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.09, 1.5, 0.09], 0.0, steel(STEEL_GREY))),
            [sx * 1.05, 0.75, 0.6],
            id_quat(),
        ));
        prims.push(prim(
            solid(cuboid_tapered([0.09, 0.95, 0.09], 0.0, steel(STEEL_GREY))),
            [sx * 1.05, 0.48, -0.6],
            id_quat(),
        ));
    }
    // Cross-brace tying the leg pairs.
    for sz in [-0.6_f32, 0.6] {
        prims.push(prim(
            solid(cuboid_tapered([2.2, 0.08, 0.08], 0.0, steel(STEEL_GREY))),
            [0.0, if sz > 0.0 { 1.3 } else { 0.85 }, sz],
            id_quat(),
        ));
    }

    // Gridded PV panel — a rotation-safe child, lit gridded face tilted up and
    // toward the −Z camera (high edge at +Z back, low edge at −Z front).
    let mut panel = pv_panel(2.4, 1.7, pv(PV_BLUE), steel(STEEL_GREY));
    panel.transform.translation = Fp3([0.0, 1.18, 0.0]);
    panel.transform.rotation = quat_x(-0.42);
    prims.push(panel);

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SolarPanel.build(""), "solar_panel");
    }
}
