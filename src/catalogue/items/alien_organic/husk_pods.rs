//! Husk pods — an Alien-Organic *poor* secondary. A cluster of burst, dried
//! egg husks gaping open on dead stalks, their shells splayed back like spent
//! petals around a hollow dark cavity. The spent brood of the necrotic colony.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{HUSK, NECROTIC, flesh};

/// Dark hollow of a spent husk.
const HOLLOW: [f32; 3] = [0.17, 0.15, 0.12];

/// One burst husk (dead stalk + hollow cavity + splayed shell petals) for the
/// assemble list. The stalk is the subtree root with `id_quat`, so the first
/// husk can be `prims[0]` safely.
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
    let cup_y = stalk_h * 0.5 + 0.28 * scale;
    // Dark hollow cavity nested where the pod burst.
    stalk.children.push(prim(
        solid(sphere(0.34 * scale, 5, flesh(HOLLOW))),
        [0.0, cup_y, 0.0],
        id_quat(),
    ));
    // Three dried shell petals splayed back radially around the cavity.
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU + 0.4;
        stalk.children.push(prim(
            solid(cone(0.4 * scale, 0.85 * scale, 6, flesh(HUSK))),
            [
                a.cos() * 0.22 * scale,
                cup_y + 0.05 * scale,
                a.sin() * 0.22 * scale,
            ],
            quat_mul(quat_y(a), quat_x(0.85)),
        ));
    }
    stalk
}

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

fn build_tree() -> Generator {
    let prims = vec![
        // Largest husk — the root (stalk is id_quat).
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
