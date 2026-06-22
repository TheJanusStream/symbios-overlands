//! Withered hive — the Alien-Organic *poor* landmark. A collapsed, necrotic
//! hive: cracked grey-green chitin bulbs slumped and caved over dead tissue,
//! its biolume long gone, its maw a dark empty socket and its tendrils
//! shrivelled and flopped on the ground. The necrotic counterpart to the
//! [`chitinous_hive`](super::chitinous_hive): same organism, opposite end of
//! the prosperity axis (`Poor`), so a destitute alien room grows the dying
//! colony instead of the thriving one.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the slumped base (the
//! root, `id_quat`).

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::util::{
    assemble, cone, id_quat, prim, prim_scaled, quat_x, solid, sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{CHITIN_DARK, CHITIN_GREEN, NECROTIC, chitin, flesh, tendril};

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
    // Slumped, squashed base bulb — the root (id_quat). The shell stays one
    // dark grey-green chitin so the stack reads as the dead twin of the hive;
    // necrotic beige is used only for the tissue exposed where it has broken
    // open.
    let mut prims = vec![prim_scaled(
        solid(sphere(2.8, 5, chitin(CHITIN_GREEN))),
        [0.0, 2.0, 0.0],
        id_quat(),
        [1.12, 0.86, 1.0],
    )];

    // A caved-in mid bulb, slumped and leaning off-axis (child — safe).
    prims.push(prim_scaled(
        solid(sphere(2.0, 5, chitin(CHITIN_GREEN))),
        [0.35, 3.9, -0.2],
        quat_x(0.22),
        [1.1, 0.82, 1.0],
    ));
    // A broken, snapped-off crown stub with exposed dead tissue at the break.
    prims.push(prim(
        solid(cone(1.0, 1.5, 8, chitin(CHITIN_GREEN))),
        [0.7, 5.2, -0.35],
        quat_x(0.4),
    ));
    prims.push(prim(
        solid(sphere(0.6, 5, flesh(NECROTIC))),
        [1.0, 5.9, -0.55],
        id_quat(),
    ));

    // The cave-in: a big dark concave socket bitten out of the front, exposing
    // dead tissue inside — the headline "collapsed" read, facing the −Z hero.
    prims.push(prim_scaled(
        solid(with_cut(
            sphere(1.7, 6, flesh(NECROTIC)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        )),
        [-0.2, 3.0, -2.0],
        quat_x(-1.4),
        [1.0, 0.85, 1.0],
    ));

    // Broken rib arcs girdling the slump — partial, not full rings.
    for (y, major, arc) in [
        (2.0_f32, 2.95_f32, [0.0_f32, 0.34_f32]),
        (3.4, 2.2, [0.45, 0.85]),
    ] {
        prims.push(prim(
            solid(with_cut(
                torus(0.22, major, chitin(CHITIN_GREEN)),
                arc,
                [0.0, 1.0],
                0.0,
            )),
            [0.0, y, 0.0],
            quat_x(FRAC_PI_2),
        ));
    }

    // Dead maw on the −Z front: a dark lip ring round an empty black socket,
    // no glow (the biolume is gone).
    prims.push(prim(
        solid(torus(0.3, 0.95, chitin(CHITIN_GREEN))),
        [0.0, 1.5, -2.6],
        quat_x(FRAC_PI_2),
    ));
    prims.push(prim_scaled(
        solid(with_cut(
            sphere(0.85, 6, chitin(CHITIN_DARK)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        )),
        [0.0, 1.5, -2.4],
        quat_x(-FRAC_PI_2),
        [1.0, 0.7, 1.0],
    ));

    // Shrivelled dead tendrils flopped on the ground, curling hard over.
    for i in 0..6 {
        let a = i as f32 / 6.0 * TAU + 0.3;
        prims.push(tendril(
            [a.cos() * 2.5, 0.0, a.sin() * 2.5],
            a,
            0.24,
            0.55,
            4,
            0.68,
            flesh(NECROTIC),
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
