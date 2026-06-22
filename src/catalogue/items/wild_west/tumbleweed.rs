//! Tumbleweed — a Wild-West *poor* prop. A dried tangle of brush bowling
//! across the empty street. The lonesome clutter of the bust town.
//!
//! A small core knot with many thin twigs radiating at scattered angles
//! ([`quat_mul`] of a [`quat_y`] azimuth and a [`quat_x`] tilt) — a spiky,
//! see-through tangle rather than a smooth ball.

use crate::catalogue::items::util::{
    assemble, cylinder_tapered, id_quat, prim, quat_mul, quat_x, quat_y, solid, sphere,
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
    let center = [0.0_f32, 0.6, 0.0];

    let mut prims = vec![
        // Small core knot of brush — the root.
        prim(solid(sphere(0.18, 3, canvas(BRUSH))), center, id_quat()),
    ];
    // Many thin twigs radiating in scattered directions — a diameter twig
    // through the core sticks out both ways, building a spiky tangle.
    for i in 0..14 {
        let az = i as f32 * 1.7;
        let tilt = -1.2 + (i % 6) as f32 * 0.45;
        let len = 0.74 + (i % 4) as f32 * 0.13;
        prims.push(prim(
            solid(cylinder_tapered(0.024, len, 4, 0.0, canvas(BRUSH))),
            center,
            quat_mul(quat_y(az), quat_x(tilt)),
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
