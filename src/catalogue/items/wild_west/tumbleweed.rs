//! Tumbleweed — a Wild-West *poor* prop. A dried tangle of brush bowling
//! across the empty street. The lonesome clutter of the bust town.
//!
//! A few twigs jut at angles via [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::canvas;

/// Dry, sun-bleached brush colour.
const BRUSH: [f32; 3] = [0.52, 0.44, 0.26];

pub struct Tumbleweed;

impl CatalogueEntry for Tumbleweed {
    fn slug(&self) -> &'static str {
        "tumbleweed"
    }
    fn name(&self) -> &'static str {
        "Tumbleweed"
    }
    fn description(&self) -> &'static str {
        "Dried tangle of brush bowling across the empty street."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::WildWest]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FRONTIER_POOR
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
        // Brush tangle ball — the root.
        prim(
            solid(sphere(0.6, 2, canvas(BRUSH))),
            [0.0, 0.6, 0.0],
            id_quat(),
        ),
    ];
    // A second, smaller clump fused on.
    prims.push(prim(
        solid(sphere(0.4, 2, canvas(BRUSH))),
        [0.4, 0.7, 0.2],
        id_quat(),
    ));
    // Stray twigs jutting at angles.
    for (i, tilt) in [0.7_f32, -0.7, 1.3, -1.3].into_iter().enumerate() {
        let z = if i % 2 == 0 { 0.5 } else { -0.5 };
        prims.push(prim(
            solid(cylinder_tapered(0.03, 0.8, 4, 0.0, canvas(BRUSH))),
            [0.0, 0.6, z],
            quat_x(tilt),
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
        assert_sanitize_stable(&Tumbleweed.build(""), "tumbleweed");
    }
}
