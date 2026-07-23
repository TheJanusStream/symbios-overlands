//! Hedge hut — the High-Fantasy *poor* landmark. A hedge-witch's daub-and-
//! timber hut under a shaggy thatch roof, a crooked chimney and a single
//! softly-glowing window, charms hung at the door. The hedge-magic
//! counterpart to the [`wizard_tower`](super::wizard_tower): same craft,
//! opposite end of the prosperity axis (`Poor`), so a destitute fantasy room
//! grows the witch's holding instead of the mage's seat.
//!
//! Primitive-built; authored in one flat ground-relative frame via
//! [`assemble`], which reparents every piece under the earthen floor. The hut
//! is a hollow daub shell — rear, side and punched front walls around a floor
//! and ceiling — so the one window is a cut pane you see *into* a warm hearth-
//! lit room through, not a glowing panel stuck on a solid wall (#949).

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, glow, id_quat, plane, prim, quat_x, solid, tube, window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    ARCANE_GLASS, STONE_MOSS, THATCH_STRAW, TIMBER_DARK, fx, matte, mossy, thatch, timber,
};

/// Pale daub plaster of the hut walls.
const DAUB: [f32; 3] = [0.74, 0.70, 0.58];
/// Dried-herb bundle colour.
const HERB: [f32; 3] = [0.42, 0.5, 0.3];
/// Warm hearth glow spilling through the cut window.
const HEARTH: [f32; 3] = [1.0, 0.62, 0.34];

pub struct HedgeHut;

impl CatalogueEntry for HedgeHut {
    fn slug(&self) -> &'static str {
        "hedge_hut"
    }
    fn name(&self) -> &'static str {
        "Hedge Hut"
    }
    fn description(&self) -> &'static str {
        "Hedge-witch's daub-and-timber hut under shaggy thatch with a glowing window."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Fantasy]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::FANTASY_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 6.0,
            min_spawn_dist: 34.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let wall_h = 2.6_f32;
    let wall_top = wall_h;
    let hw = 2.25_f32; // half width  (±X outer face)
    let hd = 1.9_f32; // half depth  (±Z outer face; front = −Z)
    let zf = -hd + 0.09; // centre of the 0.18 m front wall (outer face at −hd)

    let mut prims = vec![
        // Earthen floor — the root of the hollow shell.
        prim(
            solid(cuboid_tapered(
                [4.5, 0.12, 3.8],
                0.0,
                matte([0.34, 0.29, 0.22]),
            )),
            [0.0, 0.06, 0.0],
            id_quat(),
        ),
    ];
    // Rear + side daub walls and a flat ceiling close the shell so the window
    // looks into a lit room, not straight out the far side.
    prims.push(prim(
        solid(cuboid_tapered([4.5, wall_h, 0.18], 0.0, matte(DAUB))),
        [0.0, wall_h * 0.5, hd - 0.09],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered([0.18, wall_h, 3.8], 0.0, matte(DAUB))),
            [sx * (hw - 0.09), wall_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([4.5, 0.12, 3.8], 0.0, matte(DAUB))),
        [0.0, wall_top - 0.06, 0.0],
        id_quat(),
    ));

    // Front daub wall (−Z), solid but for one punched window opening at x≈1.2,
    // y 1.1–1.9: a full-width header, wall to each side, and a sill beneath.
    let (win_cx, win_hw, win_sill, win_head) = (1.2_f32, 0.4_f32, 1.1_f32, 1.9_f32);
    prims.push(prim(
        solid(cuboid_tapered(
            [4.5, wall_h - win_head, 0.18],
            0.0,
            matte(DAUB),
        )),
        [0.0, (win_head + wall_h) * 0.5, zf],
        id_quat(),
    ));
    // Wall left of the window (spans the door side) and the short strip right.
    let left_r = win_cx - win_hw; // 0.8
    prims.push(prim(
        solid(cuboid_tapered(
            [left_r + hw, win_head, 0.18],
            0.0,
            matte(DAUB),
        )),
        [(-hw + left_r) * 0.5, win_head * 0.5, zf],
        id_quat(),
    ));
    let right_l = win_cx + win_hw; // 1.6
    prims.push(prim(
        solid(cuboid_tapered(
            [hw - right_l, win_head, 0.18],
            0.0,
            matte(DAUB),
        )),
        [(right_l + hw) * 0.5, win_head * 0.5, zf],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [win_hw * 2.0, win_sill, 0.18],
            0.0,
            matte(DAUB),
        )),
        [win_cx, win_sill * 0.5, zf],
        id_quat(),
    ));

    // Timber corner frame.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.2, wall_h, 0.2], 0.0, timber(TIMBER_DARK))),
                [sx * 2.2, wall_h * 0.5, sz * 1.85],
                id_quat(),
            ));
        }
    }

    // Warm hearth glow filling the room against the rear wall, plus a low green
    // cauldron ember — the witch-light the cut window shows off.
    prims.push(prim(
        cuboid_tapered([1.6, 1.5, 0.4], 0.0, glow(HEARTH, 2.4)),
        [0.9, 0.95, hd - 0.5],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.5, 0.5],
            0.1,
            glow([0.46, 0.82, 0.5], 1.8),
        )),
        [1.1, 0.4, 0.3],
        id_quat(),
    ));

    // Shaggy steep A-frame thatch roof (ridge ‖ X over the long walls).
    let ridge_y = wall_top + 2.1;
    prims.push(gable_roof(
        [5.4, 2.1, 4.6],
        [0.0, wall_top + 1.05, 0.0],
        thatch(THATCH_STRAW),
    ));
    // Ridge beam seated proud above the apex (never grazing it).
    prims.push(prim(
        solid(cuboid_tapered([5.5, 0.14, 0.14], 0.0, timber(TIMBER_DARK))),
        [0.0, ridge_y + 0.06, 0.0],
        id_quat(),
    ));

    // Timber door on the −Z (front) face, and the softly-glowing window glazed
    // with a cut arcane pane on a plane, spanning its opening over the hearth.
    prims.push(prim(
        solid(cuboid_tapered([0.9, 1.9, 0.2], 0.0, timber(TIMBER_DARK))),
        [-1.0, 0.95, -1.95],
        id_quat(),
    ));
    prims.push(prim(
        plane(
            [win_hw * 2.0, win_head - win_sill],
            window_card(ARCANE_GLASS, 2, 2, 0.35, 0.07),
        ),
        [win_cx, (win_sill + win_head) * 0.5, -hd - 0.02],
        quat_x(-std::f32::consts::FRAC_PI_2),
    ));

    // Crooked hollow mossy-stone chimney at the gable end — a round flue open
    // through the top, poking up past the ridge, with wood-smoke curling out.
    prims.push(prim(
        tube(0.34, 0.2, 3.0, 12, mossy(STONE_MOSS)),
        [1.8, wall_top + 1.0, -0.9],
        quat_x(0.08),
    ));
    prims.push(fx::chimney_smoke([1.8, 5.2, -0.78], 0x0A5C_5304));

    // Dried-herb bundles hung beside the door, tapering to a tied tip.
    for (cy, len) in [(1.7_f32, 0.5_f32), (1.4, 0.42), (1.15, 0.46)] {
        prims.push(prim(
            solid(cuboid_tapered([0.14, len, 0.14], 0.7, matte(HERB))),
            [-1.9, cy, -1.92],
            quat_x(std::f32::consts::PI),
        ));
    }

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::{GeneratorKind, SovereignTextureConfig};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HedgeHut.build(""), "hedge_hut");
    }

    #[test]
    fn has_lit_window() {
        assert!(crate::catalogue::items::util::has_emissive(
            &HedgeHut.build("")
        ));
    }

    /// #949: the window card sits on a `Plane` at `uv_scale` 1.0 (spans once,
    /// not tiled), and — being a landmark that gets embedded in room records —
    /// the built tree survives a serde round-trip.
    #[test]
    fn glazing_is_planes_and_round_trips() {
        use crate::pds::material_finish::node_materials_mut;
        fn walk(g: &mut Generator) {
            let tag = g.kind.kind_tag();
            let is_plane = matches!(g.kind, GeneratorKind::Plane { .. });
            for m in node_materials_mut(&mut g.kind) {
                if matches!(m.texture, SovereignTextureConfig::Window(_)) {
                    assert!(is_plane, "Window card must sit on a Plane, found {tag}");
                    assert_eq!(m.uv_scale.0, 1.0, "Window cards must stay at uv_scale 1.0");
                }
            }
            for c in &mut g.children {
                walk(c);
            }
        }
        let mut g = HedgeHut.build("");
        walk(&mut g);
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "hedge hut must survive a serde round-trip"
        );
    }
}
