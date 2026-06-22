//! Fleshy spire — an Alien-Organic secondary. A writhing organic tower: a
//! lumpy S-curving trunk of fused flesh bulbs studded with biolume pods,
//! membrane frills fanning from its flanks and a glowing biolume crown at the
//! tip, keening eerily. Its lights are emissive trim the ruin pass can darken.
//!
//! The trunk is a stack of overlapping flesh spheres leaning alternately left
//! and right — the interpenetrating blobs fuse into one writhing mass (they
//! are round, so the overlap reads as flesh, not z-fight) and break the
//! smooth-column silhouette into something living.

use std::f32::consts::TAU;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, prim, quat_y, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BIOLUME_CYAN, BIOLUME_GREEN, FLESH_PINK, FLESH_RED, MEMBRANE_TEAL, flesh, fx, membrane,
};

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
    // Lumpy S-curving trunk: overlapping flesh bulbs, alternating lean.
    // (cx, y, cz, r) — centres close enough that adjacent radii overlap.
    let trunk = [
        (0.0_f32, 1.0_f32, 0.0_f32, 1.3_f32),
        (0.18, 2.0, 0.15, 1.12),
        (-0.28, 2.9, -0.1, 0.95),
        (0.3, 3.7, 0.18, 0.78),
        (-0.12, 4.5, 0.02, 0.6),
        (0.08, 5.15, 0.05, 0.46),
    ];
    let mut prims = vec![prim(
        solid(sphere(trunk[0].3, 5, flesh(FLESH_RED))),
        [trunk[0].0, trunk[0].1, trunk[0].2],
        id_quat(),
    )];
    for &(cx, y, cz, r) in &trunk[1..] {
        let mat = if y > 3.0 {
            flesh(FLESH_PINK)
        } else {
            flesh(FLESH_RED)
        };
        prims.push(prim(solid(sphere(r, 5, mat)), [cx, y, cz], id_quat()));
    }

    // Glowing biolume crown bulb at the tip — emissive.
    prims.push(prim(
        sphere(0.5, 5, glow(BIOLUME_CYAN, 2.0)),
        [0.1, 5.7, 0.05],
        id_quat(),
    ));

    // Biolume pods studding the trunk — deep cyan, proud on the flesh.
    for (y, ang, gr) in [
        (2.3_f32, 0.6_f32, 0.26_f32),
        (3.4, 3.4, 0.22),
        (4.3, 1.8, 0.18),
    ] {
        let r = 0.95 - (y - 2.0) * 0.12;
        prims.push(prim(
            sphere(gr, 4, glow(BIOLUME_CYAN, 2.0)),
            [ang.cos() * r, y, ang.sin() * r],
            id_quat(),
        ));
    }

    // Membrane frills fanning from the flanks: thin translucent sails, broad
    // face turned radially outward, with a glowing green rib up the outer edge
    // (emissive reads on the flat face — the steampunk lesson).
    for i in 0..3 {
        let a = i as f32 / 3.0 * TAU + 0.4;
        let y = 2.4 + i as f32 * 0.7;
        let reach = 0.85;
        prims.push(prim(
            cuboid_tapered([1.3, 1.5, 0.06], 0.55, membrane(MEMBRANE_TEAL)),
            [a.cos() * reach, y, a.sin() * reach],
            quat_y(-a),
        ));
        prims.push(prim(
            cuboid_tapered([0.09, 1.4, 0.1], 0.4, glow(BIOLUME_GREEN, 1.8)),
            [a.cos() * (reach + 0.5), y, a.sin() * (reach + 0.5)],
            quat_y(-a),
        ));
    }

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
