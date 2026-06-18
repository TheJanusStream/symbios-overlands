//! Creep patch — an Alien-Organic prop. A spreading mat of fleshy creep with
//! a few glowing nodules. Scatter clutter carpeting the colony floor; the
//! nodules are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, FLESH_RED, flesh};

pub struct CreepPatch;

impl CatalogueEntry for CreepPatch {
    fn slug(&self) -> &'static str {
        "creep_patch"
    }
    fn name(&self) -> &'static str {
        "Creep Patch"
    }
    fn description(&self) -> &'static str {
        "Spreading mat of fleshy creep with a few glowing nodules."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
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
        // Main creep mat — the root.
        prim(
            solid(cylinder_tapered(1.3, 0.16, 16, 0.0, flesh(FLESH_RED))),
            [0.0, 0.08, 0.0],
            id_quat(),
        ),
    ];
    // Overlapping lobes of creep.
    for (cx, cz, r) in [
        (0.9_f32, 0.3_f32, 0.7_f32),
        (-0.7, 0.6, 0.6),
        (0.2, -0.9, 0.65),
    ] {
        prims.push(prim(
            solid(cylinder_tapered(r, 0.12, 14, 0.0, flesh(FLESH_RED))),
            [cx, 0.06, cz],
            id_quat(),
        ));
    }
    // Glowing nodules budding from the creep — emissive.
    for (cx, cz) in [(0.3_f32, 0.2_f32), (-0.5, -0.3), (0.7, -0.5)] {
        prims.push(prim(
            solid(sphere(0.18, 3, glow(BIOLUME_GREEN, 2.2))),
            [cx, 0.2, cz],
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
        assert_sanitize_stable(&CreepPatch.build(""), "creep_patch");
    }
}
