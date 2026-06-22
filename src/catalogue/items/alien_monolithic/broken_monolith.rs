//! Broken monolith — the Alien-Monolithic *poor* landmark. A toppled, cracked
//! slab of dead grey stone, its glyph-grooves dark, a stump still standing on
//! a fractured base. The dormant counterpart to the
//! [`black_monolith`](super::black_monolith): same array, opposite end of the
//! prosperity axis (`Poor`), so a destitute alien room grows the dead,
//! lightless site instead of the active one.
//!
//! The toppled slab lies tipped with a [`quat_x`].

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, quat_z, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DEAD_STONE, stone};

pub struct BrokenMonolith;

impl CatalogueEntry for BrokenMonolith {
    fn slug(&self) -> &'static str {
        "broken_monolith"
    }
    fn name(&self) -> &'static str {
        "Broken Monolith"
    }
    fn description(&self) -> &'static str {
        "Toppled, cracked slab of dead grey stone, its glyph-grooves dark, a stump still standing."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::AlienMonolithic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MONOLITH_POOR
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
        // Fractured base disc — the root.
        prim(
            solid(cylinder_tapered(2.4, 0.3, 16, 0.0, stone(DEAD_STONE))),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Standing stump (lower part of the slab, snapped off short).
    prims.push(prim(
        solid(cuboid_tapered([2.2, 3.0, 0.8], 0.05, stone(DEAD_STONE))),
        [0.0, 1.6, 0.0],
        id_quat(),
    ));
    // Dark glyph groove down the stump's −Z face (no glow — light gone).
    prims.push(prim(
        cuboid_tapered([0.16, 2.0, 0.06], 0.0, stone([0.12, 0.12, 0.14])),
        [0.0, 1.6, -0.42],
        id_quat(),
    ));

    // Toppled upper slab fallen sideways across the ground to +X, so the topple
    // reads from the −Z hero front instead of hiding behind the stump.
    prims.push(prim(
        solid(cuboid_tapered([2.2, 5.5, 0.8], 0.05, stone(DEAD_STONE))),
        [3.3, 0.55, 0.4],
        quat_z(-1.46),
    ));

    // Shattered rubble chips around the fractured base.
    for (cx, cz, s, tilt) in [
        (-1.6_f32, 0.9_f32, 0.6_f32, 0.3_f32),
        (-1.0, -1.3, 0.5, 0.0),
        (1.2, -1.1, 0.45, 0.5),
    ] {
        prims.push(prim(
            solid(cuboid_tapered([s, s * 0.7, s], 0.3, stone(DEAD_STONE))),
            [cx, s * 0.35, cz],
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
        assert_sanitize_stable(&BrokenMonolith.build(""), "broken_monolith");
    }
}
