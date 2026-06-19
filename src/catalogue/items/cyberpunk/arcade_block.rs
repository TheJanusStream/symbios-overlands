//! Arcade block — a wide, low Cyberpunk secondary. A dark-metal box
//! with neon trim along its roofline and a big standing neon sign
//! board on top; the street-level entertainment counterpoint to the
//! megatower's height.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_MAGENTA, fx, metal, window_wall};

pub struct ArcadeBlock;

impl CatalogueEntry for ArcadeBlock {
    fn slug(&self) -> &'static str {
        "arcade_block"
    }
    fn name(&self) -> &'static str {
        "Arcade Block"
    }
    fn description(&self) -> &'static str {
        "Low neon-trimmed entertainment block with a rooftop sign board."
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
            clearance: 6.5,
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
        solid(cuboid_tapered([10.0, slab_h, 7.0], 0.0, metal(body))),
        [0.0, slab_h * 0.5, 0.0],
        id_quat(),
    );
    let rel = |ground_y: f32| ground_y - slab_h * 0.5;

    let mut base = foundation_block(10.0, 7.0, [0.0, 0.0], 2.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Main block — a dark glossy body, like the rest of the neon kit. (The
    // old full-height `window_wall` facade read as a *pale* mass that broke
    // the kit's dark-metal-plus-neon cohesion — and contradicted this
    // entry's own "dark-metal box" billing.)
    let block_h = 5.0;
    root.children.push(prim(
        solid(cuboid_tapered([9.0, block_h, 6.0], 0.0, metal(body))),
        [0.0, rel(slab_h + block_h * 0.5), 0.0],
        id_quat(),
    ));

    // Two rows of lit window-grid bands up the long (±Z) faces, so the dark
    // block still reads as a glowing arcade interior rather than a black
    // slab — dim, like rows of windows, not a floodlight. Set slightly proud
    // of the face so the band can't z-fight the body plane.
    for r in 0..2 {
        let wy = slab_h + block_h * (0.33 + 0.34 * r as f32);
        for sz in [-1.0_f32, 1.0] {
            root.children.push(prim(
                cuboid_tapered([7.2, 0.7, 0.12], 0.0, window_wall([0.12, 0.52, 0.62], 2.0)),
                [0.0, rel(wy), sz * 3.05],
                id_quat(),
            ));
        }
    }

    // Neon roofline trim (a thin emissive collar around the block top).
    let roof_y = slab_h + block_h;
    root.children.push(prim(
        cuboid_tapered([9.4, 0.35, 6.4], 0.0, glow(NEON_MAGENTA, 6.0)),
        [0.0, rel(roof_y), 0.0],
        id_quat(),
    ));

    // Standing rooftop sign board, set toward the front edge — its tubes
    // buzz with a signature electrical hum. A broad 4×5 m face, so it runs
    // at the moderated face strength (see `mod.rs`): at the tube strength it
    // would clip to a white slab instead of reading as a lit cyan sign.
    let mut sign = prim(
        cuboid_tapered([0.3, 4.0, 5.0], 0.0, glow(NEON_CYAN, 2.5)),
        [3.6, rel(roof_y + 2.0), 0.0],
        id_quat(),
    );
    sign.audio = fx::neon_buzz();
    root.children.push(sign);

    // Glowing doorway band at street level on the front face — a smaller
    // lit face, held just below the blow-out point.
    root.children.push(prim(
        cuboid_tapered([0.2, 2.4, 3.0], 0.0, glow(NEON_CYAN, 3.0)),
        [4.5, rel(slab_h + 1.2), 0.0],
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
        assert_sanitize_stable(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &ArcadeBlock.build("")
        ));
    }
}
