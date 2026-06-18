//! Rot patch — an Alien-Organic *poor* prop. A dark slick of necrotic sludge
//! pocked with sagging blisters. The decay clutter of the dying colony.

use crate::catalogue::items::util::{assemble, cylinder_tapered, id_quat, prim, solid, sphere};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::flesh;

/// Dark necrotic sludge colour.
const SLUDGE: [f32; 3] = [0.22, 0.20, 0.16];

pub struct RotPatch;

impl CatalogueEntry for RotPatch {
    fn slug(&self) -> &'static str {
        "rot_patch"
    }
    fn name(&self) -> &'static str {
        "Rot Patch"
    }
    fn description(&self) -> &'static str {
        "Dark slick of necrotic sludge pocked with sagging blisters."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Sludge slick — the root.
        prim(
            solid(cylinder_tapered(1.3, 0.12, 16, 0.0, flesh(SLUDGE))),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
    ];
    // Overlapping lobes.
    for (cx, cz, r) in [(0.8_f32, 0.3_f32, 0.7_f32), (-0.7, 0.5, 0.6)] {
        prims.push(prim(
            solid(cylinder_tapered(r, 0.1, 14, 0.0, flesh(SLUDGE))),
            [cx, 0.05, cz],
            id_quat(),
        ));
    }
    // Sagging blisters bulging from the rot.
    for (cx, cz) in [(0.2_f32, 0.1_f32), (-0.4, -0.3), (0.6, -0.4)] {
        prims.push(prim(
            solid(sphere(0.22, 3, flesh([0.30, 0.26, 0.20]))),
            [cx, 0.12, cz],
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
        assert_sanitize_stable(&RotPatch.build(""), "rot_patch");
    }
}
