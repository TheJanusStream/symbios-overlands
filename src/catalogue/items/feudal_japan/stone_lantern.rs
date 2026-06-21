//! Stone lantern — a Feudal-Japan prop, and the kit's lit hero. A stacked
//! granite ishidōrō: a footed base, a column, a platform, a glowing light
//! box (hibukuro), a pyramidal cap, and an onion finial. Its emissive light
//! box is the trim escalation's ruin pass snuffs to cold dead stone.

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{LANTERN_GLOW, STONE_GREY, stone};

pub struct StoneLantern;

impl CatalogueEntry for StoneLantern {
    fn slug(&self) -> &'static str {
        "stone_lantern"
    }
    fn name(&self) -> &'static str {
        "Stone Lantern"
    }
    fn description(&self) -> &'static str {
        "Stacked granite lantern with a glowing light box under a pyramidal cap."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::FeudalJapan]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FEUDAL_BAND
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
    use std::f32::consts::TAU;
    let s = || stone(STONE_GREY);

    let mut prims = vec![
        // Hexagonal footed base (kiso) — the root.
        prim(
            solid(cylinder_tapered(0.55, 0.4, 6, 0.25, s())),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Round shaft (sao) — the round contrast to the hexagonal courses.
        prim(
            solid(cylinder_tapered(0.17, 1.6, 10, 0.06, s())),
            [0.0, 1.2, 0.0],
            id_quat(),
        ),
        // Hexagonal platform (chūdai) flaring out under the light box.
        prim(
            solid(cylinder_tapered(0.5, 0.22, 6, 0.18, s())),
            [0.0, 2.05, 0.0],
            id_quat(),
        ),
    ];

    // Light box (hibukuro): a glowing hexagonal core framed by six stone
    // mullions between top and bottom frame rings — the kit's emissive hero.
    let box_y = 2.62;
    let box_h = 0.6;
    let box_r = 0.34;
    prims.push(prim(
        cylinder_tapered(box_r, box_h, 6, 0.0, glow(LANTERN_GLOW, 3.0)),
        [0.0, box_y, 0.0],
        id_quat(),
    ));
    for k in 0..6 {
        let a = k as f32 / 6.0 * TAU;
        prims.push(prim(
            solid(cuboid_tapered([0.08, box_h + 0.04, 0.08], 0.0, s())),
            [a.cos() * (box_r + 0.01), box_y, a.sin() * (box_r + 0.01)],
            id_quat(),
        ));
    }
    for ry in [box_y - box_h * 0.5, box_y + box_h * 0.5] {
        prims.push(prim(
            solid(cylinder_tapered(box_r + 0.06, 0.08, 6, 0.0, s())),
            [0.0, ry, 0.0],
            id_quat(),
        ));
    }

    // Hexagonal pyramidal cap (kasa) on a flared eave ring, crowned by the
    // onion finial (hōju).
    let cap_base = box_y + box_h * 0.5 + 0.04;
    prims.push(prim(
        solid(cylinder_tapered(0.72, 0.1, 6, 0.0, s())),
        [0.0, cap_base + 0.05, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cone(0.7, 0.5, 6, s())),
        [0.0, cap_base + 0.35, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.16, 3, s())),
        [0.0, cap_base + 0.72, 0.0],
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
        assert_sanitize_stable(&StoneLantern.build(""), "stone_lantern");
    }

    #[test]
    fn has_light() {
        assert!(crate::catalogue::items::util::has_emissive(
            &StoneLantern.build("")
        ));
    }
}
