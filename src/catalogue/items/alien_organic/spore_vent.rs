//! Spore vent — an Alien-Organic prop. A chitin crater with a dark recessed
//! basin and a glowing throat mounded at its centre, ringed by spine-nubs and
//! exhaling a haze of glowing spores. Scatter clutter venting across the
//! colony; the throat is emissive trim the ruin pass can darken.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, glow, id_quat, prim, prim_scaled, quat_z, solid, sphere,
    torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_GREEN, CHITIN_DARK, CHITIN_GREEN, chitin, fx};

pub struct SporeVent;

impl CatalogueEntry for SporeVent {
    fn slug(&self) -> &'static str {
        "spore_vent"
    }
    fn name(&self) -> &'static str {
        "Spore Vent"
    }
    fn description(&self) -> &'static str {
        "Chitin crater venting glowing spores from a lit throat."
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
        // Chitin crater body — the root (id_quat).
        prim(
            solid(cylinder_tapered(0.85, 0.55, 14, 0.12, chitin(CHITIN_GREEN))),
            [0.0, 0.27, 0.0],
            id_quat(),
        ),
    ];

    // Puckered lip ring round the rim.
    prims.push(prim(
        solid(torus(0.16, 0.82, chitin(CHITIN_GREEN))),
        [0.0, 0.55, 0.0],
        id_quat(),
    ));

    // Dark recessed basin — a flattened lower-hemisphere bowl set in the rim
    // (the meso fire_bowl recipe: reads as a solid concave socket).
    prims.push(prim_scaled(
        solid(with_cut(
            sphere(0.76, 6, chitin(CHITIN_DARK)),
            [0.0, 1.0],
            [0.0, 0.5],
            0.0,
        )),
        [0.0, 0.56, 0.0],
        id_quat(),
        [1.0, 0.55, 1.0],
    ));

    // Glowing throat mounded at the basin centre so it reads above the rim.
    prims.push(prim_scaled(
        solid(with_cut(
            sphere(0.42, 6, glow(BIOLUME_GREEN, 2.2)),
            [0.0, 1.0],
            [0.5, 1.0],
            0.0,
        )),
        [0.0, 0.58, 0.0],
        id_quat(),
        [1.0, 0.85, 1.0],
    ));

    // Spine-nubs ringing the rim, splayed out.
    for i in 0..7 {
        let a = i as f32 / 7.0 * TAU;
        prims.push(prim(
            solid(cone(0.1, 0.4, 5, chitin(CHITIN_GREEN))),
            [a.cos() * 0.85, 0.6, a.sin() * 0.85],
            quat_z(a.cos() * 0.5),
        ));
    }

    let mut root = assemble(prims);
    // Signature life: spores venting up out of the throat.
    root.children
        .push(fx::spore_drift([0.0, 1.1, 0.0], 0x0A11_5E12));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SporeVent.build(""), "spore_vent");
    }
}
