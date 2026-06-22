//! Biolume stalk — an Alien-Organic prop. A slender flesh stalk curving up
//! from a creep pad, tipped with a glowing bulb and beaded with light-nodes.
//! Scatter clutter lighting the colony; the glow is emissive trim the ruin
//! pass can darken.
//!
//! Rooted on a flat creep pad (`id_quat`) so the curving stalk and its bulb
//! ride as children — a rotated `assemble` root would spin every sibling into
//! its frame (the rotated-root gotcha).

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, FLESH_PINK, FLESH_RED, flesh};

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
    let prims = vec![
        // Flat creep pad — the root (id_quat), so the curving stalk is a child.
        prim(
            solid(cylinder_tapered(0.5, 0.16, 12, 0.3, flesh(FLESH_RED))),
            [0.0, 0.08, 0.0],
            id_quat(),
        ),
        // Lower stalk segment, leaning toward the −Z front.
        prim(
            solid(cylinder_tapered(0.17, 1.4, 6, 0.35, flesh(FLESH_RED))),
            [0.0, 0.85, -0.1],
            quat_x(-0.18),
        ),
        // Upper segment curling over further.
        prim(
            solid(cylinder_tapered(0.11, 1.1, 6, 0.45, flesh(FLESH_PINK))),
            [0.0, 1.75, -0.5],
            quat_x(-0.5),
        ),
        // Glowing bulb at the tip — emissive, deep green.
        prim(
            solid(sphere(0.34, 5, glow(BIOLUME_GREEN, 2.1))),
            [0.0, 2.25, -0.95],
            id_quat(),
        ),
        // Light-nodes beaded down the stalk.
        prim(
            solid(sphere(0.13, 4, glow(BIOLUME_GREEN, 1.9))),
            [0.0, 1.5, -0.32],
            id_quat(),
        ),
        prim(
            solid(sphere(0.1, 4, glow(BIOLUME_GREEN, 1.9))),
            [0.0, 0.95, -0.08],
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
        assert_sanitize_stable(&BiolumeStalk.build(""), "biolume_stalk");
    }
}
