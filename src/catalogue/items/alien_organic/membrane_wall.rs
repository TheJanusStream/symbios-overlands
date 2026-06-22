//! Membrane wall — an Alien-Organic secondary. A living rampart: translucent
//! membrane skin stretched between knuckled chitin ribs, threaded with a
//! branching web of glowing veins on its face, spined along the top and rooted
//! in a creep sill. Its veins are emissive trim the ruin pass can darken.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the first rib (the root,
//! `id_quat`).

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BIOLUME_GREEN, CHITIN_DARK, FLESH_RED, MEMBRANE_TEAL, chitin, flesh, glow_veins, membrane,
};

pub struct MembraneWall;

impl CatalogueEntry for MembraneWall {
    fn slug(&self) -> &'static str {
        "membrane_wall"
    }
    fn name(&self) -> &'static str {
        "Membrane Wall"
    }
    fn description(&self) -> &'static str {
        "Translucent membrane stretched between chitin ribs, threaded with glowing veins."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 5.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// One knuckled chitin rib (shaft + mid-bulge + spine finial) for the
/// assemble list.
fn rib(x: f32) -> Vec<Generator> {
    vec![
        prim(
            solid(cylinder_tapered(0.32, 4.2, 8, 0.35, chitin(CHITIN_DARK))),
            [x, 2.1, 0.0],
            id_quat(),
        ),
        // Mid-rib knuckle so the rib reads as a living strut, not a dowel.
        prim(
            solid(sphere(0.42, 5, chitin(CHITIN_DARK))),
            [x, 2.3, 0.0],
            id_quat(),
        ),
        // Spine finial.
        prim(
            solid(cone(0.22, 0.7, 6, chitin(CHITIN_DARK))),
            [x, 4.4, 0.0],
            id_quat(),
        ),
    ]
}

fn build_tree() -> Generator {
    // Three knuckled ribs — the first shaft is the root (id_quat).
    let mut prims = rib(-3.0);
    prims.extend(rib(0.0));
    prims.extend(rib(3.0));

    // Creep sill tying the ribs together at the foot.
    prims.push(prim(
        solid(cuboid_tapered([6.6, 0.5, 0.7], 0.1, flesh(FLESH_RED))),
        [0.0, 0.25, 0.0],
        id_quat(),
    ));

    // Stretched membrane bays + a branching glowing vein web on the −Z hero
    // face of each (render FRONT = −Z; the veins were on +Z before).
    for x in [-1.5_f32, 1.5] {
        prims.push(prim(
            cuboid_tapered([2.8, 3.4, 0.1], 0.0, membrane(MEMBRANE_TEAL)),
            [x, 2.1, 0.0],
            id_quat(),
        ));
        for v in glow_veins([x, 2.1, 0.0], -0.1, 2.6, glow(BIOLUME_GREEN, 1.8)) {
            prims.push(v);
        }
    }

    // Biolume pods nestled where the ribs meet the sill.
    for x in [-3.0_f32, 0.0, 3.0] {
        prims.push(prim(
            solid(sphere(0.26, 4, glow(BIOLUME_GREEN, 1.8))),
            [x, 0.6, -0.45],
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
        assert_sanitize_stable(&MembraneWall.build(""), "membrane_wall");
    }
}
