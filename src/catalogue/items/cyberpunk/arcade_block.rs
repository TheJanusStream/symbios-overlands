//! Arcade block — a wide, low Cyberpunk secondary. A dark-metal entertainment
//! box with lit window bands, a neon-framed entrance under a wedge marquee,
//! and a big content-tile sign board on the roof; the street-level
//! counterpoint to the megatower's height.

use std::f32::consts::PI;

use crate::catalogue::items::util::{
    cuboid_tapered, foundation_block, glow, id_quat, prim, quat_y, solid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal, window_wall};

pub struct ArcadeBlock;

impl CatalogueEntry for ArcadeBlock {
    fn slug(&self) -> &'static str {
        "arcade_block"
    }
    fn name(&self) -> &'static str {
        "Arcade Block"
    }
    fn description(&self) -> &'static str {
        "Low neon entertainment block with a marquee entrance and roof sign."
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

fn shade(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6]
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

    // Main dark glossy block. Front face is -Z (z = -3.0).
    let block_h = 5.0_f32;
    let zf = -3.0_f32;
    root.children.push(prim(
        solid(cuboid_tapered([9.0, block_h, 6.0], 0.0, metal(body))),
        [0.0, rel(slab_h + block_h * 0.5), 0.0],
        id_quat(),
    ));

    // Lit window-grid bands on the back (+Z) and end (±X) faces — the dark
    // block reads as a glowing arcade interior, not a black slab.
    for r in 0..2 {
        let wy = slab_h + block_h * (0.35 + 0.36 * r as f32);
        root.children.push(prim(
            cuboid_tapered([7.2, 0.7, 0.12], 0.0, window_wall([0.12, 0.52, 0.62], 2.0)),
            [0.0, rel(wy), 3.05],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.12, 0.7, 4.4], 0.0, window_wall([0.12, 0.52, 0.62], 2.0)),
            [sx * 4.55, rel(slab_h + block_h * 0.55), 0.0],
            id_quat(),
        ));
    }

    // Neon roofline trim (a thin emissive collar around the block top).
    let roof_y = slab_h + block_h;
    root.children.push(prim(
        cuboid_tapered([9.4, 0.35, 6.4], 0.0, glow(NEON_MAGENTA, 6.0)),
        [0.0, rel(roof_y), 0.0],
        id_quat(),
    ));

    // Vertical neon accent strips down the four corners.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            root.children.push(prim(
                cuboid_tapered([0.12, block_h * 0.92, 0.12], 0.0, glow(NEON_CYAN, 4.5)),
                [sx * 4.5, rel(slab_h + block_h * 0.5), sz * 3.0],
                id_quat(),
            ));
        }
    }

    // ---- Entrance on the front (-Z) face ---------------------------------
    let door_y = slab_h + 1.5;
    // Recessed interior with a warm inner glow.
    root.children.push(prim(
        cuboid_tapered([3.8, 2.8, 0.4], 0.0, metal(shade(body))),
        [0.0, rel(door_y), zf + 0.15],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([3.4, 2.4, 0.1], 0.0, glow([1.0, 0.72, 0.32], 1.3)),
        [0.0, rel(door_y), zf + 0.05],
        id_quat(),
    ));
    // Hot magenta neon door frame.
    for sy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([4.4, 0.22, 0.45], 0.0, glow(NEON_MAGENTA, 5.0)),
            [0.0, rel(door_y + sy * 1.5), zf - 0.05],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.22, 3.2, 0.45], 0.0, glow(NEON_MAGENTA, 5.0)),
            [sx * 2.1, rel(door_y), zf - 0.05],
            id_quat(),
        ));
    }
    // Wedge marquee canopy projecting out over the door (quat_y(PI) puts the
    // thick edge against the building, sloping down to a thin front lip) with
    // a hot neon lip strip.
    root.children.push(prim(
        wedge([5.2, 0.5, 1.3], metal(body)),
        [0.0, rel(slab_h + 3.2), zf - 0.65],
        quat_y(PI),
    ));
    root.children.push(prim(
        cuboid_tapered([5.2, 0.09, 0.1], 0.0, glow(NEON_CYAN, 5.0)),
        [0.0, rel(slab_h + 2.97), zf - 1.28],
        id_quat(),
    ));

    // ---- Rooftop sign board (content tiles + hot frame) ------------------
    let sign_y = roof_y + 2.4;
    let sign_z = -1.2_f32;
    // Two support legs.
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            solid(cuboid_tapered([0.25, 1.4, 0.25], 0.0, metal(body))),
            [sx * 2.4, rel(roof_y + 0.7), sign_z],
            id_quat(),
        ));
    }
    // Dark backing.
    let mut backing = prim(
        cuboid_tapered([6.4, 2.8, 0.25], 0.0, metal(shade(body))),
        [0.0, rel(sign_y), sign_z],
        id_quat(),
    );
    backing.audio = fx::neon_buzz();
    root.children.push(backing);
    // Lit content tiles across the face (front = -Z).
    let tiles = [NEON_CYAN, NEON_MAGENTA, NEON_LIME, NEON_CYAN];
    for (i, c) in tiles.into_iter().enumerate() {
        let x = -2.25 + 1.5 * i as f32;
        root.children.push(prim(
            cuboid_tapered([1.3, 2.0, 0.06], 0.0, glow(c, 2.0 + 0.1 * i as f32)),
            [x, rel(sign_y), sign_z - 0.13],
            id_quat(),
        ));
    }
    // Hot magenta frame around the sign.
    for sy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([6.7, 0.2, 0.4], 0.0, glow(NEON_MAGENTA, 5.0)),
            [0.0, rel(sign_y + sy * 1.5), sign_z - 0.05],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.2, 3.2, 0.4], 0.0, glow(NEON_MAGENTA, 5.0)),
            [sx * 3.35, rel(sign_y), sign_z - 0.05],
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
        assert_sanitize_stable(&ArcadeBlock.build(""), "arcade_block");
    }

    #[test]
    fn has_neon() {
        assert!(crate::catalogue::items::util::has_emissive(
            &ArcadeBlock.build("")
        ));
    }
}
