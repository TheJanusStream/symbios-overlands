//! Brazier — an AncientClassical prop and the kit's one firelit element. A
//! patinated bronze bowl on a footed stem holding glowing coals, with a low
//! altar flame, drifting embers, and a fire crackle. Its emissive coals are
//! the trim escalation's ruin pass snuffs to a cold dead bowl.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BRONZE_GREEN, EMBER_ORANGE, bronze, fx};

pub struct Brazier;

impl CatalogueEntry for Brazier {
    fn slug(&self) -> &'static str {
        "brazier"
    }
    fn name(&self) -> &'static str {
        "Brazier"
    }
    fn description(&self) -> &'static str {
        "Footed bronze bowl of glowing coals with a low altar flame."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AncientClassical]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ANCIENT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 1.2,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let bowl_y = 1.3;

    let mut prims = vec![
        // Bronze foot disc — the root.
        prim(
            solid(cylinder_tapered(0.42, 0.2, 12, 0.0, bronze(BRONZE_GREEN))),
            [0.0, 0.1, 0.0],
            id_quat(),
        ),
        // Stem.
        prim(
            solid(cylinder_tapered(0.16, 1.0, 10, 0.1, bronze(BRONZE_GREEN))),
            [0.0, 0.65, 0.0],
            id_quat(),
        ),
        // Bowl.
        prim(
            solid(cylinder_tapered(0.6, 0.4, 16, 0.18, bronze(BRONZE_GREEN))),
            [0.0, bowl_y, 0.0],
            id_quat(),
        ),
        // Rim ring.
        prim(
            torus(0.06, 0.6, bronze(BRONZE_GREEN)),
            [0.0, bowl_y + 0.2, 0.0],
            id_quat(),
        ),
    ];

    // Glowing coals heaped in the bowl — the emissive heart, crackling.
    let coals = [0.0, bowl_y + 0.22, 0.0];
    let mut fire = prim(
        solid(cylinder_tapered(
            0.5,
            0.22,
            12,
            0.4,
            glow(EMBER_ORANGE, 4.0),
        )),
        coals,
        id_quat(),
    );
    fire.audio = fx::fire_crackle();
    prims.push(fire);

    let mut root = assemble(prims);
    // Signature life: a low flame and drifting embers off the coals.
    root.children
        .push(fx::brazier_flame([0.0, bowl_y + 0.4, 0.0], 0x00F1_A3E0));
    root.children
        .push(fx::brazier_embers([0.0, bowl_y + 0.5, 0.0], 0x0E3B_E0A1));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Brazier.build(""), "brazier");
    }

    #[test]
    fn keeps_embers() {
        assert!(super::super::has_emissive(&Brazier.build("")));
    }
}
