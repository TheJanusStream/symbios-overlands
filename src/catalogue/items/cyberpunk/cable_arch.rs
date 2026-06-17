//! Cable arch — a small Cyberpunk prop. Two posts and a top beam strung
//! with glowing power/data cables; a street-spanning piece of clutter
//! that frames walkways between the bigger structures.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, metal};

pub struct CableArch;

impl CatalogueEntry for CableArch {
    fn slug(&self) -> &'static str {
        "cable_arch"
    }
    fn name(&self) -> &'static str {
        "Cable Arch"
    }
    fn description(&self) -> &'static str {
        "Twin-post arch strung with glowing power cables."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.2;
    let span = 4.0_f32;

    let mut root = prim(
        solid(cuboid_tapered([span + 0.6, slab_h, 1.0], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(span + 0.6, 1.0, [0.0, 0.0], 1.5);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Two posts + a top beam.
    let post_h = 3.6;
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            solid(cuboid_tapered([0.35, post_h, 0.35], 0.0, metal(body))),
            [sx * span * 0.5, rel(slab_h + post_h * 0.5), 0.0],
            id_quat(),
        ));
    }
    root.children.push(prim(
        solid(cuboid_tapered([span + 0.5, 0.4, 0.4], 0.0, metal(body))),
        [0.0, rel(slab_h + post_h), 0.0],
        id_quat(),
    ));

    // Glowing cables hanging from the beam at a few points.
    let colors = [NEON_CYAN, NEON_LIME, NEON_CYAN];
    for (k, c) in colors.iter().enumerate() {
        let x = -span * 0.32 + span * 0.32 * k as f32;
        let drop = 1.0 + 0.4 * (k as f32 % 2.0);
        root.children.push(prim(
            cuboid_tapered([0.08, drop, 0.08], 0.0, glow(*c, 5.0)),
            [x, rel(slab_h + post_h - drop * 0.5), 0.0],
            id_quat(),
        ));
    }

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CableArch.build(""), "cable_arch");
    }

    #[test]
    fn has_neon() {
        assert!(super::super::has_emissive(&CableArch.build("")));
    }
}
