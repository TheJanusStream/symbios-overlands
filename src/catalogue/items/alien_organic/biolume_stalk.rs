//! Biolume stalk — an Alien-Organic prop. A slender flesh stalk tipped with a
//! glowing bulb and beaded with light-nodes. Scatter clutter lighting the
//! colony; the glow is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, FLESH_RED, flesh};

pub struct BiolumeStalk;

impl CatalogueEntry for BiolumeStalk {
    fn slug(&self) -> &'static str {
        "biolume_stalk"
    }
    fn name(&self) -> &'static str {
        "Biolume Stalk"
    }
    fn description(&self) -> &'static str {
        "Slender flesh stalk tipped with a glowing bulb and beaded with light-nodes."
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
            clearance: 0.8,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Flesh stalk — the root, curving up.
        prim(
            solid(cylinder_tapered(0.16, 2.2, 6, 0.4, flesh(FLESH_RED))),
            [0.0, 1.1, 0.0],
            quat_x(0.12),
        ),
    ];

    // Glowing bulb at the tip — emissive.
    prims.push(prim(
        solid(sphere(0.35, 3, glow(BIOLUME_GREEN, 2.6))),
        [0.0, 2.4, 0.2],
        id_quat(),
    ));
    // A couple of smaller light-nodes down the stalk.
    for (y, z) in [(1.4_f32, 0.05_f32), (0.9, 0.0)] {
        prims.push(prim(
            solid(sphere(0.14, 3, glow(BIOLUME_GREEN, 2.0))),
            [0.0, y, z],
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
        assert_sanitize_stable(&BiolumeStalk.build(""), "biolume_stalk");
    }
}
