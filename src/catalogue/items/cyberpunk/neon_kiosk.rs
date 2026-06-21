//! Neon kiosk — a small Cyberpunk prop. A waist-to-head-height dark-metal
//! vending terminal: a framed menu screen under a neon-lipped awning, a lit
//! header sign, a dispense slot, and side accent strips. Scattered through
//! the settlement as street clutter.

use crate::catalogue::items::util::{
    cuboid_tapered, foundation_block, glow, id_quat, prim, solid, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal};

pub struct NeonKiosk;

impl CatalogueEntry for NeonKiosk {
    fn slug(&self) -> &'static str {
        "neon_kiosk"
    }
    fn name(&self) -> &'static str {
        "Neon Kiosk"
    }
    fn description(&self) -> &'static str {
        "Small vending terminal with a framed menu screen and neon awning."
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

    // Vending body — hums with the signature low buzz of a live machine. Its
    // front face sits at z = +0.5; everything below mounts onto it.
    let box_h = 2.0_f32;
    let mut vending = prim(
        solid(cuboid_tapered([1.4, box_h, 1.0], 0.0, metal(body))),
        [0.0, rel(slab_h + box_h * 0.5), 0.0],
        id_quat(),
    );
    vending.audio = fx::neon_buzz();
    root.children.push(vending);

    // Framed menu screen on the front face: dark housing → 2×2 lit menu tiles
    // → hot magenta frame. A framed, *content*-filled face, not a flat slab.
    let scr_y = rel(slab_h + 1.32);
    root.children.push(prim(
        cuboid_tapered([0.94, 1.04, 0.05], 0.0, metal(shade(body))),
        [0.0, scr_y, 0.5],
        id_quat(),
    ));
    let tiles = [NEON_LIME, NEON_CYAN, NEON_CYAN, NEON_LIME];
    for (i, c) in tiles.into_iter().enumerate() {
        let tx = if i % 2 == 0 { -0.22 } else { 0.22 };
        let ty = if i < 2 { 0.26 } else { -0.26 };
        root.children.push(prim(
            cuboid_tapered([0.38, 0.44, 0.05], 0.0, glow(c, 1.9 + 0.1 * i as f32)),
            [tx, scr_y + ty, 0.55],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([1.06, 0.1, 0.16], 0.0, glow(NEON_MAGENTA, 5.0)),
            [0.0, scr_y + sy * 0.56, 0.5],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.1, 1.18, 0.16], 0.0, glow(NEON_MAGENTA, 5.0)),
            [sx * 0.52, scr_y, 0.5],
            id_quat(),
        ));
    }

    // Lit header nameplate above the screen.
    root.children.push(prim(
        cuboid_tapered([1.0, 0.26, 0.06], 0.0, glow(NEON_CYAN, 2.2)),
        [0.0, rel(slab_h + 1.78), 0.52],
        id_quat(),
    ));

    // Dispense slot + a lit tray line near the bottom.
    root.children.push(prim(
        cuboid_tapered([0.6, 0.28, 0.06], 0.0, metal(shade(body))),
        [0.0, rel(slab_h + 0.48), 0.5],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([0.6, 0.04, 0.07], 0.0, glow(NEON_LIME, 3.0)),
        [0.0, rel(slab_h + 0.32), 0.52],
        id_quat(),
    ));

    // Side accent strips down the front corners.
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([0.05, 1.5, 0.05], 0.0, glow(NEON_CYAN, 4.0)),
            [sx * 0.68, rel(slab_h + box_h * 0.5), 0.5],
            id_quat(),
        ));
    }

    // A wedge awning projecting over the front, thick at the back (against the
    // box) sloping to a thin front lip, with a hot neon lip strip.
    root.children.push(prim(
        wedge([1.5, 0.35, 0.55], metal(body)),
        [0.0, rel(slab_h + box_h + 0.16), 0.55],
        id_quat(),
    ));
    root.children.push(prim(
        cuboid_tapered([1.5, 0.07, 0.08], 0.0, glow(NEON_MAGENTA, 5.0)),
        [0.0, rel(slab_h + box_h + 0.02), 0.8],
        id_quat(),
    ));

    root
}

/// A darker shade of a body colour — recessed housings / dispense slots.
fn shade(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6]
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
        assert!(crate::catalogue::items::util::has_emissive(
            &NeonKiosk.build("")
        ));
    }
}
