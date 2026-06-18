//! Spore vent — an Alien-Organic prop. A chitin crater venting a haze of
//! glowing spores from a lit throat. Scatter clutter exhaling across the
//! colony; the throat is emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{assemble, cylinder_tapered, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, CHITIN_GREEN, chitin, fx};

pub struct SporeVent;

impl CatalogueEntry for SporeVent {
    fn slug(&self) -> &'static str {
        "spore_vent"
    }
    fn name(&self) -> &'static str {
        "Spore Vent"
    }
    fn description(&self) -> &'static str {
        "Chitin crater venting glowing spores from a lit throat."
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
            clearance: 1.0,
            min_spawn_dist: 18.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Chitin crater rim — the root, a flaring cup.
        prim(
            solid(cylinder_tapered(0.9, 0.9, 12, 0.0, chitin(CHITIN_GREEN))),
            [0.0, 0.45, 0.0],
            id_quat(),
        ),
    ];
    // Outer flare.
    prims.push(prim(
        solid(cylinder_tapered(0.6, 0.4, 12, 0.5, chitin(CHITIN_GREEN))),
        [0.0, 0.9, 0.0],
        id_quat(),
    ));
    // Glowing throat — emissive.
    prims.push(prim(
        solid(cylinder_tapered(
            0.5,
            0.2,
            12,
            0.0,
            glow(BIOLUME_GREEN, 2.6),
        )),
        [0.0, 0.92, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: spores venting up out of the throat.
    root.children
        .push(fx::spore_drift([0.0, 1.1, 0.0], 0x0A11_5E12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SporeVent.build(""), "spore_vent");
    }
}
