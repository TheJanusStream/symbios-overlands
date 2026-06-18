//! Withered hive — the Alien-Organic *poor* landmark. A collapsed, necrotic
//! hive: cracked grey chitin slumped over dead tissue, its biolume long gone,
//! shrivelled tendrils splayed. The necrotic counterpart to the
//! [`chitinous_hive`](super::chitinous_hive): same organism, opposite end of
//! the prosperity axis (`Poor`), so a destitute alien room grows the dying
//! colony instead of the thriving one.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slumped base.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHITIN_GREEN, NECROTIC, chitin, flesh};

pub struct WitheredHive;

impl CatalogueEntry for WitheredHive {
    fn slug(&self) -> &'static str {
        "withered_hive"
    }
    fn name(&self) -> &'static str {
        "Withered Hive"
    }
    fn description(&self) -> &'static str {
        "Collapsed necrotic hive of cracked grey chitin over dead tissue, biolume gone."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienOrganic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::ORGANIC_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 36.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Slumped base bulb — the root.
        prim(
            solid(sphere(3.4, 3, chitin(CHITIN_GREEN))),
            [0.0, 2.2, 0.0],
            id_quat(),
        ),
    ];

    // A caved-in mid bulb, leaning.
    prims.push(prim(
        solid(sphere(2.2, 3, flesh(NECROTIC))),
        [0.6, 4.6, -0.3],
        quat_x(0.25),
    ));
    // A broken, snapped-off crown stub.
    prims.push(prim(
        solid(cone(1.3, 2.0, 8, chitin(CHITIN_GREEN))),
        [0.8, 6.0, -0.5],
        quat_x(0.4),
    ));

    // Shrivelled dead tendrils splayed on the ground.
    for i in 0..4 {
        let a = i as f32 / 4.0 * TAU + 0.3;
        prims.push(prim(
            solid(cylinder_tapered(0.22, 2.2, 6, 0.8, flesh(NECROTIC))),
            [a.cos() * 3.0, 0.5, a.sin() * 3.0],
            quat_x(1.2),
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
        assert_sanitize_stable(&WitheredHive.build(""), "withered_hive");
    }
}
