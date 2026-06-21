//! Neon megatower — the Cyberpunk landmark. Four stacked, slightly tapered
//! dark-metal tiers with setback ledges, each ringed with an emissive neon
//! band and lit window rows, crowned by a round lit observation drum inside
//! hollow neon halo rings and topped by an antenna cluster and beacon.
//! ~55 m tall, so it anchors the settlement and reads as a glowing spire
//! across the home region.
//!
//! Primitive-built (see [`crate::catalogue::items::util`]); the root is a thin
//! podium slab whose base sits at the generator origin (= terrain-snapped
//! height), and every child measures its Y from the slab centre via `rel`.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, foundation_block, glow, id_quat, prim, solid, sphere, torus,
    tube,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{DARK_METAL, NEON_CYAN, NEON_LIME, NEON_MAGENTA, fx, metal, window_wall};

pub struct NeonMegatower;

impl CatalogueEntry for NeonMegatower {
    fn slug(&self) -> &'static str {
        "neon_megatower"
    }
    fn name(&self) -> &'static str {
        "Neon Megatower"
    }
    fn description(&self) -> &'static str {
        "Towering tiered megastructure banded in neon, crowned by an observation drum."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Cyberpunk]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CYBER_BAND
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

    let mut base = foundation_block(14.0, 14.0, [0.0, 0.0], 3.0);
    base.transform.translation.0[1] -= slab_h * 0.5;
    root.children.push(base);

    // Stacked tiers shrinking upward, each with a setback ledge, a neon base
    // band, vertical strips, and lit window rows.
    let tiers = [(12.0_f32, 14.0_f32), (9.0, 12.0), (6.5, 10.0), (4.5, 8.0)];
    let neon = [NEON_CYAN, NEON_MAGENTA, NEON_CYAN, NEON_MAGENTA];
    let taper = 0.12;

    let mut y = slab_h;
    for (i, (w, h)) in tiers.iter().enumerate() {
        let (w, h) = (*w, *h);
        // Setback ledge — a wide thin overhang slab at the tier base.
        root.children.push(prim(
            solid(cuboid_tapered([w + 0.8, 0.3, w + 0.8], 0.0, metal(body))),
            [0.0, rel(y + 0.15), 0.0],
            id_quat(),
        ));
        // Tier body.
        root.children.push(prim(
            solid(cuboid_tapered([w, h, w], taper, metal(body))),
            [0.0, rel(y + h * 0.5), 0.0],
            id_quat(),
        ));
        // Neon band ring at the base seam.
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
        // Lit window-grid bands climbing the front + back faces.
        let rows = 3;
        for r in 0..rows {
            let wy = y + h * (0.25 + 0.5 * r as f32 / (rows - 1) as f32);
            for sz in [-1.0_f32, 1.0] {
                root.children.push(prim(
                    cuboid_tapered([w * 0.78, 0.45, 0.15], 0.0, glow(neon[i], 3.5)),
                    [0.0, rel(wy), sz * w * 0.5],
                    id_quat(),
                ));
            }
        }
        y += h;
    }
    let top = y;

    // ---- Crown: a round lit observation drum inside neon halo rings -------
    let drum_r = 2.2_f32;
    let drum_h = 3.8_f32;
    root.children.push(prim(
        solid(cylinder_tapered(
            drum_r,
            drum_h,
            24,
            0.0,
            window_wall([0.15, 0.7, 0.82], 2.6),
        )),
        [0.0, rel(top + drum_h * 0.5), 0.0],
        id_quat(),
    ));
    // Dark roof cap.
    root.children.push(prim(
        solid(cylinder_tapered(drum_r + 0.2, 0.4, 24, 0.0, metal(body))),
        [0.0, rel(top + drum_h + 0.2), 0.0],
        id_quat(),
    ));
    // Thin glowing rings sandwiching the lit deck.
    for ry in [top + 0.1, top + drum_h - 0.1] {
        root.children.push(prim(
            tube(drum_r + 0.18, drum_r + 0.02, 0.22, 28, glow(NEON_LIME, 6.0)),
            [0.0, rel(ry), 0.0],
            id_quat(),
        ));
    }
    // A big hollow holo halo ring floating around the drum.
    root.children.push(prim(
        tube(drum_r + 1.6, drum_r + 1.4, 0.3, 32, glow(NEON_CYAN, 5.0)),
        [0.0, rel(top + drum_h * 0.5), 0.0],
        id_quat(),
    ));
    // Crown glow ring on top of the cap.
    let crown = top + drum_h + 0.4;
    root.children.push(prim(
        torus(0.28, drum_r * 0.7, glow(NEON_MAGENTA, 7.0)),
        [0.0, rel(crown + 0.1), 0.0],
        id_quat(),
    ));

    // ---- Antenna cluster + beacon ----------------------------------------
    // Side masts with red aviation lights.
    for (mx, mz, mh) in [(0.9_f32, 0.6_f32, 5.0_f32), (-0.7, -0.5, 6.2)] {
        root.children.push(prim(
            solid(cylinder_tapered(0.14, mh, 6, 0.3, metal(body))),
            [mx, rel(crown + mh * 0.5), mz],
            id_quat(),
        ));
        root.children.push(prim(
            sphere(0.13, 2, glow([1.0, 0.12, 0.08], 6.0)),
            [mx, rel(crown + mh + 0.12), mz],
            id_quat(),
        ));
    }
    // Central beacon mast + orb.
    let mast_h = 8.0_f32;
    root.children.push(prim(
        solid(cylinder_tapered(0.22, mast_h, 8, 0.3, metal(body))),
        [0.0, rel(crown + mast_h * 0.5), 0.0],
        id_quat(),
    ));
    root.children.push(prim(
        sphere(0.7, 3, glow(NEON_MAGENTA, 10.0)),
        [0.0, rel(crown + mast_h + 0.4), 0.0],
        id_quat(),
    ));

    // Signature life: a coolant vent breathing steam at the podium edge and a
    // deep transformer hum at the tower base.
    root.children
        .push(fx::steam_vent([5.5, rel(slab_h + 0.4), 4.0], 0x6E60_57A1));
    root.audio = fx::transformer_hum();

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
            crate::catalogue::items::util::has_emissive(&NeonMegatower.build("")),
            "neon megatower lost its emissive trim"
        );
    }
}
