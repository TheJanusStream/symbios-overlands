//! Holo-billboard — a Cyberpunk secondary. Two dark-metal posts holding
//! a large emissive "holographic" advertising panel above street level,
//! with a thin neon frame. Reads as the settlement's advertising glow.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_MAGENTA, fx, metal};

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

    // The panel — a large emissive face, kept at a *moderate* glow. A broad
    // face pushes every colour channel past 1.0 long before a thin neon
    // tube does, so at the tube's strength this slab would clip to a
    // featureless white lightbox; held low it reads as a lit cyan screen
    // that keeps its hue (see the emissive-strength note in `mod.rs`).
    let panel_y = slab_h + post_h * 0.7;
    let cy = rel(panel_y + 1.6);
    // Mount the panel + frame on the *front* of the posts rather than
    // skewered through them: the posts' front face sits at z = +0.2, so a
    // 0.45 m offset clears the deepest frame bar (back face at 0.25) past
    // it. This both removes the posts bleeding through the screen and keeps
    // the magenta frame from sharing a face plane with a post (z-fighting).
    let z_front = 0.45_f32;
    root.children.push(prim(
        cuboid_tapered([5.4, 3.6, 0.25], 0.0, glow(NEON_CYAN, 1.6)),
        [0.0, cy, z_front],
        id_quat(),
    ));
    // Hot magenta neon frame around the panel edge. The thin tube *can*
    // run hot, and a crisp lit border reads the broad face as a framed sign
    // rather than a floating slab.
    let (half_w, half_h, bar) = (2.85_f32, 1.95_f32, 0.22_f32);
    for sy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([5.7, bar, 0.4], 0.0, glow(NEON_MAGENTA, 5.0)),
            [0.0, cy + sy * half_h, z_front],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([bar, 3.9, 0.4], 0.0, glow(NEON_MAGENTA, 5.0)),
            [sx * half_w, cy, z_front],
            id_quat(),
        ));
    }

    // Signature life: holographic shimmer drifting off the panel face.
    root.children.push(fx::rising_motes(
        [0.0, rel(panel_y + 1.6), 0.7],
        NEON_MAGENTA,
        0x4010_B0FE,
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
