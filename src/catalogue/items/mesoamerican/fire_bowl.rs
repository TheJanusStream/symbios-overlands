//! Fire bowl — a Mesoamerican prop, and the kit's lit hero. A stone brazier
//! on a stepped pedestal holding a burning offering: leaping flame, lofted
//! embers, and a fire crackle. Its emissive core is the trim escalation's
//! ruin pass snuffs to a cold dead bowl.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cuboid_tapered, cylinder_tapered, glow, id_quat, prim, quat_y, solid, sphere,
    torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, OBSIDIAN_BLACK, STONE_GREY, STUCCO_CREAM, STUCCO_RED, cobble, fx, limestone,
    obsidian, painted,
};

pub struct FireBowl;

impl CatalogueEntry for FireBowl {
    fn slug(&self) -> &'static str {
        "fire_bowl"
    }
    fn name(&self) -> &'static str {
        "Fire Bowl"
    }
    fn description(&self) -> &'static str {
        "Stone brazier on a stepped pedestal burning an offering fire."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MESO_BAND
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
    let mut prims = vec![
        // Stepped pedestal base — the root.
        prim(
            solid(cuboid_tapered(
                [1.3, 0.4, 1.3],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, 0.2, 0.0],
            id_quat(),
        ),
        // Upper pedestal step.
        prim(
            solid(cuboid_tapered([1.0, 0.34, 1.0], 0.05, cobble(STONE_GREY))),
            [0.0, 0.57, 0.0],
            id_quat(),
        ),
    ];
    // Red moulding band straddling the pedestal head.
    prims.push(prim(
        solid(cuboid_tapered([1.06, 0.12, 1.06], 0.0, painted(STUCCO_RED))),
        [0.0, 0.74, 0.0],
        id_quat(),
    ));
    // Carved column shaft.
    let col_top = 0.74 + 0.85;
    prims.push(prim(
        solid(cylinder_tapered(0.32, 0.85, 12, 0.08, cobble(STONE_GREY))),
        [0.0, 0.74 + 0.425, 0.0],
        id_quat(),
    ));

    // A real concave stone bowl — a hemispherical basin (profile_cut keeps
    // the lower latitude band) cradling the offering fire, instead of a flat
    // disc with a ball balanced on top.
    let bowl_r = 0.72_f32;
    let bowl_y = col_top - 0.1 + bowl_r; // rim at the sphere equator
    prims.push(prim(
        solid(with_cut(
            sphere(bowl_r, 6, limestone(STUCCO_CREAM)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        )),
        [0.0, bowl_y, 0.0],
        id_quat(),
    ));
    // Obsidian rim ringing the bowl lip — a dark band against the pale basin.
    prims.push(prim(
        torus(0.07, bowl_r * 0.99, obsidian(OBSIDIAN_BLACK)),
        [0.0, bowl_y, 0.0],
        id_quat(),
    ));
    // Obsidian spikes ringing the rim — the Aztec brazier motif.
    let spikes = 8;
    for i in 0..spikes {
        let a = i as f32 / spikes as f32 * TAU;
        prims.push(prim(
            solid(cone(0.07, 0.3, 6, obsidian(OBSIDIAN_BLACK))),
            [
                a.cos() * bowl_r * 0.92,
                bowl_y + 0.12,
                a.sin() * bowl_r * 0.92,
            ],
            id_quat(),
        ));
    }

    // Charred offering logs laid across the basin.
    for (i, ang) in [0.5_f32, -0.7].into_iter().enumerate() {
        prims.push(prim(
            cuboid_tapered([0.95, 0.11, 0.13], 0.0, painted([0.11, 0.08, 0.06])),
            [0.0, bowl_y - 0.04 + i as f32 * 0.06, 0.0],
            quat_y(ang),
        ));
    }

    // Glowing coals mounded in the bowl — a dome of emissive embers, the
    // emissive heart escalation's ruin pass snuffs to a cold dead bowl.
    prims.push(prim(
        solid(with_cut(
            sphere(0.55, 6, glow(FIRE_ORANGE, 5.0)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, bowl_y - 0.22, 0.0],
        id_quat(),
    ));

    let flame_y = bowl_y + 0.3;
    let mut root = assemble(prims);
    // Signature life: leaping flame, lofted embers, and a fire crackle.
    let mut flame = fx::sacred_flame([0.0, flame_y, 0.0], 0xF1A0_B0E1);
    flame.audio = fx::fire_crackle();
    root.children.push(flame);
    root.children
        .push(fx::fire_embers([0.0, flame_y + 0.3, 0.0], 0xE3BE_B0E1));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FireBowl.build(""), "fire_bowl");
    }

    #[test]
    fn has_fire() {
        assert!(crate::catalogue::items::util::has_emissive(
            &FireBowl.build("")
        ));
    }
}
