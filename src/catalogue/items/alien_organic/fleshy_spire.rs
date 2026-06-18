//! Fleshy spire — an Alien-Organic secondary. A tall twisting tower of stacked
//! flesh segments tipped with a glowing biolume bulb, keening eerily. Its tip
//! is emissive trim the ruin pass can darken.
//!
//! Each segment leans slightly with a [`quat_x`] to give the spire its writhe.

use crate::catalogue::items::util::{
    assemble, cone, cylinder_tapered, glow, id_quat, prim, quat_x, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{BIOLUME_CYAN, FLESH_PINK, FLESH_RED, flesh, fx};

pub struct FleshySpire;

impl CatalogueEntry for FleshySpire {
    fn slug(&self) -> &'static str {
        "fleshy_spire"
    }
    fn name(&self) -> &'static str {
        "Fleshy Spire"
    }
    fn description(&self) -> &'static str {
        "Tall twisting tower of stacked flesh segments tipped with a glowing biolume bulb."
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
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let mut prims = vec![
        // Flesh base bulb — the root.
        prim(
            solid(sphere(1.4, 3, flesh(FLESH_RED))),
            [0.0, 1.0, 0.0],
            id_quat(),
        ),
    ];

    // Stacked tapering segments, each leaning to give a writhe.
    let mut y = 1.8_f32;
    for (k, tilt) in [0.12_f32, -0.12, 0.1, -0.08].into_iter().enumerate() {
        let r = 0.9 - k as f32 * 0.15;
        let h = 1.8;
        prims.push(prim(
            solid(cylinder_tapered(r, h, 8, 0.25, flesh(FLESH_PINK))),
            [0.0, y + h * 0.5, 0.0],
            quat_x(tilt),
        ));
        y += h;
    }
    // Tapered crown.
    prims.push(prim(
        solid(cone(0.5, 1.2, 8, flesh(FLESH_PINK))),
        [0.0, y + 0.4, 0.0],
        id_quat(),
    ));
    // Glowing biolume bulb at the tip — emissive.
    prims.push(prim(
        sphere(0.5, 3, glow(BIOLUME_CYAN, 2.8)),
        [0.0, y + 1.4, 0.0],
        id_quat(),
    ));

    let mut root = assemble(prims);
    // Signature life: the spire's eerie whine.
    root.audio = fx::eerie_whine();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&FleshySpire.build(""), "fleshy_spire");
    }
}
