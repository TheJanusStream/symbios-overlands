//! Egg sac — an Alien-Organic prop. A swollen translucent sac aglow with the
//! life inside it, girdled by glowing veins and slung in a fleshy nest with
//! smaller satellite sacs budding alongside. Scatter clutter of the colony;
//! the sacs are emissive trim the ruin pass can darken.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, glow, id_quat, prim, prim_scaled, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, FLESH_PINK, FLESH_RED, SAC_GLOW, egg_pod, flesh};

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
    let mut prims = vec![
        // Fleshy nest collar — the root (id_quat).
        prim(
            solid(cylinder_tapered(0.72, 0.34, 12, 0.25, flesh(FLESH_RED))),
            [0.0, 0.17, 0.0],
            id_quat(),
        ),
        // Glowing egg sac — a tall translucent ovoid, emissive (deep magenta
        // driven bright: it blooms without washing to white).
        prim_scaled(
            solid(sphere(0.68, 6, glow(SAC_GLOW, 2.7))),
            [0.0, 1.05, 0.0],
            id_quat(),
            [1.0, 1.35, 1.0],
        ),
    ];

    // Glowing veins girdling the sac — thin rings at staggered latitudes
    // (staggered so no two share a plane), proud of the sac skin.
    for (y, major) in [(0.74_f32, 0.66_f32), (1.05, 0.72), (1.36, 0.62)] {
        prims.push(prim(
            torus(0.045, major, glow(BIOLUME_GREEN, 1.7)),
            [0.0, y, 0.0],
            id_quat(),
        ));
    }

    // Satellite sacs budding from the nest — one ripe, one not.
    prims.push(egg_pod(
        [-0.62, 0.0, 0.34],
        0.3,
        1.4,
        glow(SAC_GLOW, 2.5),
        flesh(FLESH_RED),
    ));
    prims.push(egg_pod(
        [0.6, 0.0, -0.28],
        0.26,
        1.4,
        flesh(FLESH_PINK),
        flesh(FLESH_RED),
    ));

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
