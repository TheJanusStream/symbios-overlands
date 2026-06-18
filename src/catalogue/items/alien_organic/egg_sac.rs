//! Egg sac — an Alien-Organic prop. A swollen translucent sac aglow with the
//! life inside it, slung on a flesh base. Scatter clutter of the colony; the
//! sac is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{FLESH_RED, SAC_GLOW, flesh};

pub struct EggSac;

impl CatalogueEntry for EggSac {
    fn slug(&self) -> &'static str {
        "egg_sac"
    }
    fn name(&self) -> &'static str {
        "Egg Sac"
    }
    fn description(&self) -> &'static str {
        "Swollen translucent sac aglow with the life inside it, on a flesh base."
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
    let prims = vec![
        // Flesh base ring — the root.
        prim(
            solid(cylinder_tapered(0.6, 0.3, 12, 0.2, flesh(FLESH_RED))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
        // Glowing egg sac — emissive.
        prim(
            solid(sphere(0.7, 3, glow(SAC_GLOW, 2.4))),
            [0.0, 0.9, 0.0],
            id_quat(),
        ),
    ];

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&EggSac.build(""), "egg_sac");
    }
}
