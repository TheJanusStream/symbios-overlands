//! Ziggurat — four stepped sandstone tiers with a front stair ramp and
//! a glowing shrine at the summit. Reads as a desert temple in arid
//! regions, a jungle pyramid in lush ones, and an obsidian monument on
//! volcanic worlds (the landmark deriver only varies scale/yaw/seed,
//! so the biome palette around it does the recolouring work).
//!
//! Frame convention mirrors the lighthouse: the root is the base tier
//! with its base at the generator origin; upper tiers, ramp, and
//! shrine are children positioned relative to the base-tier centre.

use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, SovereignBrickConfig, SovereignMaterialSettings,
    SovereignTextureConfig,
};
use crate::seeded_defaults::{ProsperityBand, ProsperityTier, ThemeArchetype};

use crate::catalogue::items::util::{
    cuboid_tapered, foundation_block, glow, id_quat, prim, quat_x, solid,
};

pub struct Ziggurat;

impl CatalogueEntry for Ziggurat {
    fn slug(&self) -> &'static str {
        "ziggurat"
    }
    fn name(&self) -> &'static str {
        "Ziggurat"
    }
    fn description(&self) -> &'static str {
        "Four stepped sandstone tiers with a stair ramp and a glowing summit shrine."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn prosperity_band(&self) -> ProsperityBand {
        ProsperityBand::range(ProsperityTier::Modest, ProsperityTier::Rich)
    }

    fn themes(&self) -> &'static [ThemeArchetype] {
        &[
            ThemeArchetype::AncientClassical,
            ThemeArchetype::Mesoamerican,
        ]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 18.0,
            min_spawn_dist: 60.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn sandstone_mat() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.72, 0.58, 0.40]),
        roughness: Fp(0.9),
        uv_scale: Fp(3.0),
        texture: SovereignTextureConfig::Brick(SovereignBrickConfig {
            aspect_ratio: Fp64(4.0),
            color_brick: Fp3([0.68, 0.54, 0.36]),
            scale: Fp64(14.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn build_tree() -> Generator {
    let shrine_glow = [1.0, 0.75, 0.35];

    // Tier stack: (footprint, height) per level, slightly tapered so
    // each face leans inward like rammed earth.
    let tiers = [(16.0_f32, 2.2_f32), (12.0, 2.2), (8.5, 2.2), (5.5, 2.0)];

    let base_h = tiers[0].1;
    let mut root = prim(
        solid(cuboid_tapered(
            [tiers[0].0, base_h, tiers[0].0],
            0.08,
            sandstone_mat(),
        )),
        [0.0, base_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - base_h * 0.5;

    // Buried foundation, re-anchored from the entry ground frame into
    // the base-tier frame.
    let mut base = foundation_block(tiers[0].0 + 1.0, tiers[0].0 + 1.0, [0.0, 0.0], 3.0);
    base.transform.translation.0[1] -= base_h * 0.5;
    root.children.push(base);

    let mut y = base_h;
    for (size, height) in tiers.iter().skip(1) {
        root.children.push(prim(
            solid(cuboid_tapered(
                [*size, *height, *size],
                0.08,
                sandstone_mat(),
            )),
            [0.0, rel(y + height * 0.5), 0.0],
            id_quat(),
        ));
        y += height;
    }
    let summit = y; // ≈ 8.6 m

    // Summit shrine: a small tapered cell with a glowing doorway slab.
    let shrine_h = 2.4;
    let mut shrine = prim(
        solid(cuboid_tapered([3.2, shrine_h, 3.2], 0.18, sandstone_mat())),
        [0.0, rel(summit + shrine_h * 0.5), 0.0],
        id_quat(),
    );
    shrine.children.push(prim(
        cuboid_tapered([1.0, 1.6, 0.2], 0.0, glow(shrine_glow, 4.0)),
        [0.0, -shrine_h * 0.5 + 0.85, -1.55],
        id_quat(),
    ));
    root.children.push(shrine);

    // Front stair ramp: a long slab leaning against the south face,
    // running from the ground line to the summit edge.
    let ramp_len = 13.0;
    let ramp_angle = (summit / ramp_len).asin();
    root.children.push(prim(
        solid(cuboid_tapered([2.6, 0.6, ramp_len], 0.0, sandstone_mat())),
        [
            0.0,
            rel(summit * 0.5),
            -(tiers[0].0 * 0.5 + (ramp_len * ramp_angle.cos()) * 0.5 - 2.4),
        ],
        quat_x(-ramp_angle),
    ));

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&Ziggurat.build(""), "ziggurat");
    }

    #[test]
    fn shrine_doorway_glows() {
        fn any_emissive(g: &Generator) -> bool {
            let own = match &g.kind {
                crate::pds::GeneratorKind::Cuboid { material, .. } => {
                    material.emission_strength.0 > 1.0
                }
                _ => false,
            };
            own || g.children.iter().any(any_emissive)
        }
        assert!(any_emissive(&Ziggurat.build("")));
    }
}
