//! Holo-billboard — a Cyberpunk secondary. Two dark-metal posts holding a
//! large advertising screen above street level: a recessed housing filled
//! with a mosaic of lit ad-tiles behind scanlines and a hot neon frame.
//! Reads as the settlement's advertising glow.

use crate::catalogue::items::util::{cuboid_tapered, foundation_block, glow, id_quat, prim, solid};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal};

pub struct HoloBillboard;

impl CatalogueEntry for HoloBillboard {
    fn slug(&self) -> &'static str {
        "holo_billboard"
    }
    fn name(&self) -> &'static str {
        "Holo Billboard"
    }
    fn description(&self) -> &'static str {
        "Raised advertising screen of lit ad-tiles on twin posts."
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

    // The screen is mounted on the *front* of the posts (front face at z=+0.2)
    // so nothing skewers through it. Layered front-to-back: dark housing →
    // lit ad-tile mosaic → scanlines → hot neon frame.
    let panel_y = slab_h + post_h * 0.7;
    let cy = rel(panel_y + 1.6);

    // Recessed dark housing behind the tiles.
    root.children.push(prim(
        cuboid_tapered([5.4, 3.6, 0.25], 0.0, metal(shade(body))),
        [0.0, cy, 0.4],
        id_quat(),
    ));

    // Ad-tile mosaic — a grid of lit panels in mixed neon hues at *moderate*
    // glow (a broad face blows out to white well before a thin tube does, so
    // these stay 1.5–2.3 and keep their hue), with the odd dark "off" pixel.
    let (cols, rows) = (5usize, 3usize);
    let (cell_w, cell_h) = (1.0_f32, 1.05_f32);
    let palette = [NEON_CYAN, NEON_MAGENTA, NEON_LIME];
    for r in 0..rows {
        for c in 0..cols {
            let idx = r * cols + c;
            let x = (c as f32 - (cols as f32 - 1.0) * 0.5) * cell_w;
            let y = cy + (r as f32 - (rows as f32 - 1.0) * 0.5) * cell_h;
            let (col, strength) = if (idx * 7 + 3) % 5 == 0 {
                ([0.03, 0.04, 0.06], 0.0)
            } else {
                (palette[(c + r) % 3], 1.5 + (idx % 6) as f32 * 0.14)
            };
            root.children.push(prim(
                cuboid_tapered(
                    [cell_w * 0.88, cell_h * 0.84, 0.06],
                    0.0,
                    glow(col, strength),
                ),
                [x, y, 0.58],
                id_quat(),
            ));
        }
    }

    // Scanlines — thin dark strips across the screen face.
    for k in 0..5 {
        let y = cy + (k as f32 - 2.0) * 0.72;
        root.children.push(prim(
            cuboid_tapered([5.2, 0.05, 0.04], 0.0, metal(shade(body))),
            [0.0, y, 0.63],
            id_quat(),
        ));
    }

    // Hot magenta neon frame — a crisp lit border reads the broad face as a
    // framed sign rather than a floating slab.
    let (half_w, half_h, bar) = (2.85_f32, 1.95_f32, 0.22_f32);
    for sy in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([5.7, bar, 0.5], 0.0, glow(NEON_MAGENTA, 5.0)),
            [0.0, cy + sy * half_h, 0.45],
            id_quat(),
        ));
    }
    for sx in [-1.0_f32, 1.0] {
        root.children.push(prim(
            cuboid_tapered([bar, 3.9, 0.5], 0.0, glow(NEON_MAGENTA, 5.0)),
            [sx * half_w, cy, 0.45],
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

/// A darker shade of a body colour — for recessed housings and scanlines.
fn shade(c: [f32; 3]) -> [f32; 3] {
    [c[0] * 0.6, c[1] * 0.6, c[2] * 0.6]
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
        assert!(crate::catalogue::items::util::has_emissive(
            &HoloBillboard.build("")
        ));
    }
}
