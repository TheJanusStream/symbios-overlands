//! Tendril — an Alien-Organic prop. A thick flesh tendril coiling up out of a
//! creep pad, lesser feelers branching off it, a lone light-node glowing where
//! they meet. Scatter clutter writhing across the colony floor.
//!
//! Rooted on a flat creep pad (`id_quat`); each tendril is a [`tendril`](fn@super::tendril)
//! subtree (its base segment carries a yaw, so it rides as a child — a rotated
//! `assemble` root would spin every sibling into its frame).

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, quat_z, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, FLESH_PINK, FLESH_RED, flesh, tendril};

pub struct Tendril;

impl CatalogueEntry for Tendril {
    fn slug(&self) -> &'static str {
        "tendril"
    }
    fn name(&self) -> &'static str {
        "Tendril"
    }
    fn description(&self) -> &'static str {
        "Thick flesh tendril coiling up from the creep, lesser feelers branching off."
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
        // Flat creep pad — the root (id_quat).
        prim(
            solid(cylinder_tapered(0.7, 0.18, 14, 0.3, flesh(FLESH_RED))),
            [0.0, 0.09, 0.0],
            id_quat(),
        ),
    ];

    // The main thick tendril: a chain of segments leaning ever further over
    // (`quat_z`, so it hooks sideways toward +X and the −Z camera reads the
    // coil in profile — a head-on coil foreshortens flat). Each segment is
    // hand-seated at the tip of the last so the curl is a clean hook, not the
    // gentle lean the generic helper gives over a short prop.
    let main = [
        (
            0.28_f32, 1.0_f32, 0.0_f32, 0.6_f32, 0.0_f32, -0.15_f32, FLESH_RED,
        ),
        (0.22, 0.85, 0.25, 1.47, 0.0, -0.45, FLESH_RED),
        (0.16, 0.7, 0.69, 2.08, 0.0, -0.85, FLESH_PINK),
        (0.11, 0.55, 1.22, 2.38, 0.0, -1.3, FLESH_PINK),
        (0.07, 0.4, 1.69, 2.41, 0.0, -1.75, FLESH_PINK),
    ];
    for (r, h, x, y, z, lean, col) in main {
        prims.push(prim(
            solid(cylinder_tapered(r, h, 6, 0.18, flesh(col))),
            [x, y, z],
            quat_z(lean),
        ));
    }

    // Two lesser feelers branching off, hooking the other ways (the generic
    // tendril helper — small writhing nubs, the curl is fine at this size).
    prims.push(tendril(
        [-0.45, 0.1, 0.2],
        0.5,
        0.12,
        0.42,
        4,
        0.66,
        flesh(FLESH_PINK),
    ));
    prims.push(tendril(
        [0.3, 0.1, -0.4],
        3.8,
        0.1,
        0.4,
        4,
        0.7,
        flesh(FLESH_PINK),
    ));

    // A lone light-node glowing where the feelers root.
    prims.push(prim(
        solid(sphere(0.16, 4, glow(BIOLUME_GREEN, 1.9))),
        [0.0, 0.5, 0.1],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Tendril.build(""), "tendril");
    }
}
