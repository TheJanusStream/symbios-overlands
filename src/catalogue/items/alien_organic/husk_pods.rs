//! Husk pods — an Alien-Organic *poor* secondary. A cluster of burst, dried
//! egg husks gaping open on dead stalks. The spent brood of the necrotic
//! colony.
//!
//! The split husk shells are cones tipped open with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, id_quat, prim, quat_x, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HUSK, NECROTIC, flesh};

pub struct HuskPods;

impl CatalogueEntry for HuskPods {
    fn slug(&self) -> &'static str {
        "husk_pods"
    }
    fn name(&self) -> &'static str {
        "Husk Pods"
    }
    fn description(&self) -> &'static str {
        "Cluster of burst, dried egg husks gaping open on dead stalks."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_POOR
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

/// One burst husk (stalk + gaping shell halves) for the assemble list.
fn husk(pos: [f32; 3], scale: f32) -> Generator {
    let stalk_h = 0.7 * scale;
    let mut stalk = prim(
        solid(cylinder_tapered(
            0.18 * scale,
            stalk_h,
            6,
            0.2,
            flesh(NECROTIC),
        )),
        pos,
        id_quat(),
    );
    // Two split shell halves splayed open at the top.
    for sx in [-1.0_f32, 1.0] {
        stalk.children.push(prim(
            solid(cone(0.45 * scale, 0.9 * scale, 6, flesh(HUSK))),
            [sx * 0.2 * scale, stalk_h * 0.5 + 0.3 * scale, 0.0],
            quat_x(sx * 0.6),
        ));
    }
    stalk
}

fn build_tree() -> Generator {
    let prims = vec![
        // Largest husk — the root.
        husk([0.0, 0.35, 0.0], 1.3),
        husk([1.1, 0.3, 0.3], 1.0),
        husk([-0.9, 0.28, -0.4], 0.9),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HuskPods.build(""), "husk_pods");
    }
}
