//! Solar wreck — a Space-Outpost *poor* secondary. A collapsed solar array,
//! its steel frame buckled and panels cracked and toppled. The dead power
//! farm of the wreck site.

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp3, Generator};
use crate::seeded_defaults::ThemeArchetype;

use super::{PAD_GREY, PV_BLUE, SCORCH, STEEL_DARK, concrete, pv, pv_panel, steel};

pub struct SolarWreck;

impl CatalogueEntry for SolarWreck {
    fn slug(&self) -> &'static str {
        "solar_wreck"
    }
    fn name(&self) -> &'static str {
        "Solar Wreck"
    }
    fn description(&self) -> &'static str {
        "Collapsed solar array, its steel frame buckled and panels cracked and toppled."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::SpaceOutpost]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::OUTPOST_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 4.0,
            min_spawn_dist: 26.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Cracked concrete footing — the flat upright root (a leaning root
        // would spin every child into its frame).
        prim(
            solid(cuboid_tapered([5.0, 0.3, 2.0], 0.0, concrete(PAD_GREY))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Buckled torque tube, leaning (a child now → rotation-safe).
    prims.push(prim(
        solid(cuboid_tapered([4.6, 0.18, 0.18], 0.0, steel(STEEL_DARK))),
        [0.0, 1.1, -0.2],
        quat_x(0.22),
    ));
    // Snapped support posts at wild angles.
    prims.push(prim(
        solid(cuboid_tapered([0.16, 1.6, 0.16], 0.0, steel(STEEL_DARK))),
        [-1.8, 0.85, 0.0],
        quat_x(0.3),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.16, 1.1, 0.16], 0.0, steel(STEEL_DARK))),
        [1.5, 0.55, 0.1],
        quat_mul(quat_z(0.55), quat_x(0.2)),
    ));

    // A cracked panel still hanging off the frame at a steep angle.
    let mut hanging = pv_panel(2.4, 2.4, pv(PV_BLUE), steel(STEEL_DARK));
    hanging.transform.translation = Fp3([-0.7, 1.5, 0.3]);
    hanging.transform.rotation = quat_x(0.8);
    prims.push(hanging);
    // A dead panel toppled flat on the ground, scorched.
    let mut toppled = pv_panel(2.4, 2.4, pv(SCORCH), steel(STEEL_DARK));
    toppled.transform.translation = Fp3([2.1, 0.2, 0.55]);
    toppled.transform.rotation = quat_x(0.06);
    prims.push(toppled);

    // A dangling severed cable.
    prims.push(prim(
        solid(cylinder_tapered(0.045, 1.3, 4, 0.0, steel(STEEL_DARK))),
        [0.6, 0.7, -0.3],
        quat_mul(quat_z(0.3), quat_x(0.9)),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SolarWreck.build(""), "solar_wreck");
    }
}
