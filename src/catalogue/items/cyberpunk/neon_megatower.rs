//! Neon megatower — the Cyberpunk landmark. Four stacked, slightly
//! tapered dark-metal tiers, each ringed with an emissive neon band at
//! its base seam and flanked by vertical neon strips, crowned by a glow
//! ring, an antenna mast, and a beacon orb. ~50 m tall, so it anchors
//! the settlement and reads as a glowing spire across the home region.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); frame
//! convention follows the lighthouse — the root is a thin podium slab
//! whose base sits at the generator origin (= terrain-snapped height),
//! and every child measures its Y from the slab centre via `rel`.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid, sphere, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, metal};

pub struct NeonMegatower;

impl CatalogueEntry for NeonMegatower {
    fn slug(&self) -> &'static str {
        "neon_megatower"
    }
    fn name(&self) -> &'static str {
        "Neon Megatower"
    }
    fn description(&self) -> &'static str {
        "Towering tiered megastructure banded in neon, topped by a beacon mast."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 16.0,
            min_spawn_dist: 70.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let body = DARK_METAL;
    let slab_h = 0.6;

    // Podium slab — the root. Its base sits at the generator origin.
    let mut root = prim(
        solid(cuboid_tapered([14.0, slab_h, 14.0], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    // Buried foundation, re-anchored into the slab-root frame.
    let mut base = foundation_block(14.0, 14.0, [0.0, 0.0], 3.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Stacked tiers: (footprint width, height), shrinking upward. Each
    // gets a neon band at its base seam and a pair of vertical strips.
    let tiers = [(12.0_f32, 14.0_f32), (9.0, 12.0), (6.5, 10.0), (4.5, 8.0)];
    let neon = [NEON_CYAN, NEON_MAGENTA, NEON_CYAN, NEON_MAGENTA];
    let taper = 0.12;

    let mut y = slab_h;
    for (i, (w, h)) in tiers.iter().enumerate() {
        let (w, h) = (*w, *h);
        // Tier body.
        root.children.push(prim(
            solid(cuboid_tapered([w, h, w], taper, metal(body))),
            [0.0, rel(y + h * 0.5), 0.0],
            id_quat(),
        ));
        // Neon band ring at the base seam (a thin emissive collar
        // slightly wider than the tier).
        root.children.push(prim(
            cuboid_tapered([w + 0.5, 0.5, w + 0.5], 0.0, glow(neon[i], 7.0)),
            [0.0, rel(y + 0.25), 0.0],
            id_quat(),
        ));
        // Vertical neon strips on two opposite faces.
        let strip_h = h * 0.8;
        for sx in [-1.0_f32, 1.0] {
            root.children.push(prim(
                cuboid_tapered([0.3, strip_h, 0.3], 0.0, glow(neon[i], 6.0)),
                [sx * w * 0.5, rel(y + h * 0.5), 0.0],
                id_quat(),
            ));
        }
        y += h;
    }
    let top = y;

    // Crown glow ring.
    root.children.push(prim(
        torus(0.3, 3.0, glow(NEON_LIME, 7.0)),
        [0.0, rel(top + 0.2), 0.0],
        id_quat(),
    ));

    // Antenna mast + beacon orb.
    let mast_h = 8.0;
    root.children.push(prim(
        solid(cylinder_tapered(0.25, mast_h, 8, 0.3, metal(body))),
        [0.0, rel(top + mast_h * 0.5), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.7, 3, glow(NEON_MAGENTA, 10.0)),
        [0.0, rel(top + mast_h + 0.4), 0.0],
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
        assert_sanitize_stable(&NeonMegatower.build(""), "neon_megatower");
    }

    #[test]
    fn has_neon() {
        assert!(
            super::super::has_emissive(&NeonMegatower.build("")),
            "neon megatower lost its emissive trim"
        );
    }
}
