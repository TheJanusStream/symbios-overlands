//! Neon kiosk — a small Cyberpunk prop. A waist-to-head-height
//! dark-metal vending box with a glowing screen panel and a thin neon
//! canopy strip; scattered through the settlement as street clutter.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_LIME, NEON_MAGENTA, metal};

pub struct NeonKiosk;

impl CatalogueEntry for NeonKiosk {
    fn slug(&self) -> &'static str {
        "neon_kiosk"
    }
    fn name(&self) -> &'static str {
        "Neon Kiosk"
    }
    fn description(&self) -> &'static str {
        "Small vending kiosk with a glowing screen and neon canopy."
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
            clearance: 1.5,
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

    let mut root = prim(
        solid(cuboid_tapered([1.6, slab_h, 1.2], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(1.6, 1.2, [0.0, 0.0], 1.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Vending body.
    let box_h = 2.2;
    root.children.push(prim(
        solid(cuboid_tapered([1.4, box_h, 1.0], 0.0, metal(body))),
        [0.0, rel(slab_h + box_h * 0.5), 0.0],
        id_quat(),
    ));

    // Glowing screen on the front face.
    root.children.push(prim(
        cuboid_tapered([0.1, 1.2, 0.8], 0.0, glow(NEON_LIME, 6.0)),
        [0.72, rel(slab_h + 1.3), 0.0],
        id_quat(),
    ));

    // Neon canopy strip across the top.
    root.children.push(prim(
        cuboid_tapered([1.7, 0.18, 1.2], 0.0, glow(NEON_MAGENTA, 5.0)),
        [0.0, rel(slab_h + box_h + 0.1), 0.0],
        id_quat(),
    ));

    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&NeonKiosk.build(""), "neon_kiosk");
    }

    #[test]
    fn has_neon() {
        assert!(super::super::has_emissive(&NeonKiosk.build("")));
    }
}
