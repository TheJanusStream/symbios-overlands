//! Holo-billboard — a Cyberpunk secondary. Two dark-metal posts holding
//! a large emissive "holographic" advertising panel above street level,
//! with a thin neon frame. Reads as the settlement's advertising glow.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_MAGENTA, metal};

pub struct HoloBillboard;

impl CatalogueEntry for HoloBillboard {
    fn slug(&self) -> &'static str {
        "holo_billboard"
    }
    fn name(&self) -> &'static str {
        "Holo Billboard"
    }
    fn description(&self) -> &'static str {
        "Raised holographic advertising panel on twin posts."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 30.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.4;

    let mut root = prim(
        solid(cuboid_tapered([6.0, slab_h, 2.0], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(6.0, 2.0, [0.0, 0.0], 2.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Twin support posts.
    let post_h = 5.0;
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            solid(cuboid_tapered([0.4, post_h, 0.4], 0.0, metal(body))),
            [sx * 2.4, rel(slab_h + post_h * 0.5), 0.0],
            id_quat(),
        ));
    }

    // The panel — big emissive face, raised above the posts' midpoint.
    let panel_y = slab_h + post_h * 0.7;
    root.children.push(prim(
        cuboid_tapered([5.4, 3.6, 0.25], 0.0, glow(NEON_CYAN, 6.0)),
        [0.0, rel(panel_y + 1.6), 0.0],
        id_quat(),
    ));
    // Thin neon frame trim around the panel top.
    root.children.push(prim(
        cuboid_tapered([5.8, 0.3, 0.4], 0.0, glow(NEON_MAGENTA, 6.0)),
        [0.0, rel(panel_y + 3.5), 0.0],
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
        assert_sanitize_stable(&HoloBillboard.build(""), "holo_billboard");
    }

    #[test]
    fn has_neon() {
        assert!(super::super::has_emissive(&HoloBillboard.build("")));
    }
}
