//! Ring fort - a Nordic secondary that crowns a crest. One circular rampart
//! of turf on a dry-stone footing course, entered through a timber-lined
//! gate passage on -Z under a carved lintel with dragon heads at its ends,
//! and inside a turf-roofed longhouse and a lookout platform on posts.
//!
//! The rampart is a flattened torus and its stone course a tube, each
//! path-cut for the gate; a cut ring's gap sits at its own +X, so both are
//! turned a quarter about Y to bring it round to -Z, and neither is solid
//! (a ring's collider is the box round the whole of it). The bailey floor,
//! which is not turned, is the earthworks' root.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing_disc, glow, id_quat, nest,
    prim, prim_scaled, quat_x, quat_y, solid, strut, tile, tiles_per_metre, tint, torus, tube,
    upright_boards, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PrimCommon, SovereignGroundConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DRAGON_EYE, FIRE_ORANGE, IRON_DARK, NORDIC_BAND, STONE_GREY, TURF_GREEN, WOOD_DARK, WOOD_WARM,
    dragon_head, gable_roof, iron, rough_stone, timber, turf,
};

/// Spacing circle the settlement keeps round the fort.
const CLEARANCE: f32 = 16.0;

/// The rampart: centreline radius, half-width at the foot and the
/// flattening that sets its height (`minor * squash`). A squash of about
/// half is what makes the ring read as a low broad bank with a gentle slope
/// each side rather than as a tube lying on the ground.
const RING_R: f32 = 11.5;
const RING_MINOR: f32 = 4.0;
const RING_SQUASH: f32 = 0.55;

/// The gate passage: half its clear width, the half-thickness of its plank
/// linings, and the height of its turf bridge's underside.
const PASS_HALF: f32 = 1.5;
const LINING_T: f32 = 0.5;
const PASS_TOP: f32 = 2.25;

/// The bailey floor's top, just above the footing's 0.15 m reveal.
const FLOOR_TOP: f32 = 0.24;

pub struct RingFort;

impl CatalogueEntry for RingFort {
    fn slug(&self) -> &'static str {
        "nordic_ring_fort"
    }
    fn name(&self) -> &'static str {
        "Ring Fort"
    }
    fn description(&self) -> &'static str {
        "Round turf rampart on a stone course, a carved timber gate, a turf longhouse and a lookout within."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// High ground, where a ring fort watches the fjord from.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Nordic]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        NORDIC_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 44.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Cropped grass - the bailey floor.
fn sward() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.22, 0.30, 0.14]),
        roughness: Fp(0.95),
        uv_scale: tiles_per_metre(tile::GROUND),
        texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
            color_dry: Fp3([0.36, 0.40, 0.22]),
            color_moist: Fp3([0.20, 0.27, 0.13]),
            macro_scale: Fp64(4.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// The unlit dark of a doorway with a room behind it.
fn void() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.04, 0.035, 0.03]),
        roughness: Fp(1.0),
        ..Default::default()
    }
}

/// A ring path-cut to leave `gap` metres clear at radius `r`, as
/// [`with_cut`] takes it.
fn ring_cut(gap: f32, r: f32) -> [f32; 2] {
    let g = gap / (TAU * r) * 0.5;
    [g, 1.0 - g]
}

/// The earthworks as one level piece: the bailey floor, sunk deep enough to
/// count as footing for the ruin pass, carrying the turf rampart and its
/// stone course.
fn earthworks() -> Generator {
    let floor_depth = 1.4;
    let floor = prim(
        solid(cylinder_tapered(9.8, floor_depth, 48, 0.0, sward())),
        [0.0, FLOOR_TOP - floor_depth * 0.5, 0.0],
        id_quat(),
    );
    let bank = with_cut(
        GeneratorKind::Torus {
            minor_radius: Fp(RING_MINOR),
            major_radius: Fp(RING_R),
            minor_resolution: 12,
            major_resolution: 72,
            common: PrimCommon::with_material(turf(tint(TURF_GREEN, [0.8, 0.82, 0.75]))),
        },
        ring_cut(2.0 * (PASS_HALF + LINING_T) - 0.2, RING_R),
        [0.0, 1.0],
        0.0,
    );
    // The crest: a flatter, narrower ring of cropped sward whose top stands
    // 3 cm over the bank's, so the rough darker turf shows only on the
    // slopes below it. Its cut stands 5 cm back from the bank's at the gate,
    // so the two cut faces never share a plane.
    let (crest_r, crest_s) = (RING_MINOR * 0.85, RING_SQUASH * 0.75);
    let crest = with_cut(
        GeneratorKind::Torus {
            minor_radius: Fp(crest_r),
            major_radius: Fp(RING_R),
            minor_resolution: 12,
            major_resolution: 72,
            common: PrimCommon::with_material(sward()),
        },
        ring_cut(2.0 * (PASS_HALF + LINING_T) - 0.1, RING_R),
        [0.0, 1.0],
        0.0,
    );
    let crest_y = RING_MINOR * RING_SQUASH + 0.03 - crest_r * crest_s;
    // The dry-stone course the turf is laid on, showing at the bank's foot.
    let course_r = RING_R + RING_MINOR + 0.1;
    let course = with_cut(
        tube(course_r, course_r * 0.948, 1.6, 96, rough_stone(STONE_GREY)),
        ring_cut(2.0 * (PASS_HALF + LINING_T) + 0.6, course_r),
        [0.0, 1.0],
        0.0,
    );
    nest(
        floor,
        vec![
            prim_scaled(
                bank,
                [0.0, 0.0, 0.0],
                quat_y(FRAC_PI_2),
                [1.0, RING_SQUASH, 1.0],
            ),
            prim_scaled(
                crest,
                [0.0, crest_y, 0.0],
                quat_y(FRAC_PI_2),
                [1.0, crest_s, 1.0],
            ),
            prim(course, [0.0, 0.2, 0.0], quat_y(FRAC_PI_2)),
        ],
    )
}

/// The gate: plank linings either side of the passage through the rampart,
/// each running from foot to foot and battered at its ends as the bank is,
/// a turf bridge over it at the crest, the gate leaves swung in, and at the
/// front a carved lintel with an interlace of rings and a dragon head at
/// each end.
fn gate() -> Generator {
    let (z0, z1) = (-(RING_R - RING_MINOR) + 0.1, -RING_R - RING_MINOR - 0.35);
    // How much shorter a lining is along its top than along its foot.
    let batter = 0.7_f32;
    let zc = (z0 + z1) * 0.5;
    let len = z0 - z1;
    let lx = PASS_HALF + LINING_T * 0.5;
    let lining = |sx: f32| {
        prim(
            solid(cuboid_tapered_xz(
                [LINING_T, PASS_TOP + 0.1, len],
                [0.0, batter],
                upright_boards(timber(WOOD_WARM)),
            )),
            [sx * lx, (0.1 + PASS_TOP + 0.2) * 0.5, zc],
            id_quat(),
        )
    };
    let mut parts = vec![lining(1.0)];
    // The trodden way through the passage, out past the stone course.
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * PASS_HALF + 0.3, 0.3, len + 1.2],
            0.0,
            rough_stone(STONE_GREY),
        )),
        [0.0, 0.08, zc - 0.4],
        id_quat(),
    ));
    // Turf bridge carrying the rampart over the passage.
    parts.push(prim(
        solid(cuboid_tapered(
            [
                2.0 * (PASS_HALF + LINING_T) + 0.3,
                0.4,
                len * (1.0 - batter) + 0.1,
            ],
            0.0,
            turf(TURF_GREEN),
        )),
        [0.0, PASS_TOP + 0.2, zc],
        id_quat(),
    ));
    // Door posts and the carved lintel across the front.
    let front = z1 - 0.12;
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(
                0.26,
                PASS_TOP + 0.45,
                10,
                0.0,
                timber(WOOD_DARK),
            )),
            [sx * lx, (0.05 + PASS_TOP + 0.5) * 0.5, front],
            id_quat(),
        ));
    }
    let lintel_w = 2.0 * (PASS_HALF + LINING_T) + 1.0;
    let lintel_y = PASS_TOP + 0.3;
    parts.push(prim(
        solid(cuboid_tapered(
            [lintel_w, 0.6, 0.45],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, lintel_y, front],
        id_quat(),
    ));
    // Interlace carved on its face: a chain of rings proud of the beam.
    for k in -2..=2 {
        parts.push(prim(
            torus(0.045, 0.2, timber(WOOD_WARM)),
            [
                k as f32 * 0.34,
                lintel_y,
                front - 0.24 - (k as f32).abs() * 0.015,
            ],
            quat_x(FRAC_PI_2),
        ));
    }
    // A dragon head rising off each end of the lintel, glaring outward.
    for sx in [-1.0_f32, 1.0] {
        parts.push(dragon_head(
            [sx * (lintel_w * 0.5 - 0.25), lintel_y + 0.2, front],
            0.85,
            FRAC_PI_2,
            WOOD_WARM,
            DRAGON_EYE,
        ));
    }
    // Gate leaves, swung inward on hinges just behind the posts.
    let leaf_w = PASS_HALF - 0.05;
    let open = 1.2_f32;
    for sx in [-1.0_f32, 1.0] {
        let hinge = [sx * (PASS_HALF - 0.06), z1 + 0.35];
        let mut leaf = prim(
            solid(cuboid_tapered(
                [leaf_w, PASS_TOP - 0.3, 0.12],
                0.0,
                upright_boards(timber(WOOD_DARK)),
            )),
            [
                hinge[0] - sx * leaf_w * 0.5 * open.cos(),
                (0.15 + PASS_TOP - 0.15) * 0.5,
                hinge[1] + leaf_w * 0.5 * open.sin(),
            ],
            quat_y(sx * open),
        );
        leaf.children.push(prim(
            cuboid_tapered([leaf_w - 0.1, 0.1, 0.18], 0.0, iron(IRON_DARK)),
            [0.0, 0.5, 0.0],
            id_quat(),
        ));
        parts.push(leaf);
    }
    nest(lining(-1.0), parts)
}

/// The longhouse: stave walls under a turf gable, its gable ends boarded
/// and crowned by crossed bargeboards, a smoke louver over the hearth and
/// its door on the side that faces the gate.
fn longhouse() -> Generator {
    let (l, w) = (11.0_f32, 5.4_f32);
    let cz = 1.6_f32;
    let base = FLOOR_TOP - 0.1;
    let wall_top = base + 2.3;
    let rise = 2.9_f32;
    let (roof_l, span) = (l + 1.4, w + 1.5);
    let front = cz - w * 0.5;
    let mut parts = Vec::new();

    // The turf roof, bedded on birch bark: a low thick gable.
    parts.push(gable_roof(
        [roof_l, rise, span],
        [0.0, wall_top + rise * 0.5, cz],
        turf(TURF_GREEN),
    ));
    // Boarded gable triangles over the roof's ends, standing proud of the
    // turf.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered_xz(
                [0.12, rise * 0.96, span * 0.96],
                [0.0, 0.94],
                upright_boards(timber(WOOD_DARK)),
            )),
            [
                sx * (roof_l * 0.5 + 0.04),
                wall_top + 0.04 + rise * 0.48,
                cz,
            ],
            id_quat(),
        ));
    }
    // Crossed bargeboards proud of each roof end.
    let a = (rise / (span * 0.5)).atan();
    let board = (rise * rise + span * span * 0.25).sqrt() + 0.9;
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            // The two of a pair a little apart in X where they cross.
            let x = sx * (roof_l * 0.5 + 0.08 + if sz < 0.0 { 0.04 } else { 0.0 });
            // Centred on the slope, pushed up past the apex so the two cross.
            let along = 0.45;
            parts.push(prim(
                solid(cuboid_tapered([0.14, 0.3, board], 0.0, timber(WOOD_WARM))),
                [
                    x,
                    wall_top + rise * 0.5 + along * a.sin() + 0.12,
                    cz + sz * (span * 0.25 - along * a.cos()),
                ],
                quat_x(sz * a),
            ));
        }
    }
    // Smoke louver over the hearth, and its smoke.
    let ridge = wall_top + rise;
    parts.push(prim(
        solid(cuboid_tapered([1.2, 0.7, 0.9], 0.2, timber(WOOD_DARK))),
        [-2.5, ridge - 0.05, cz],
        id_quat(),
    ));
    parts.push(super::fx::hearth_smoke(
        [-2.5, ridge + 0.3, cz],
        0x0F02_71A6,
    ));
    // The door: a dark opening in a plank frame, a strip of the hearth's
    // light showing in it from dusk.
    parts.push(prim(
        cuboid_tapered([1.3, 2.0, 0.08], 0.0, void()),
        [0.0, base + 1.0, front - 0.02],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.5, 1.9, 0.06], 0.0, lit_from_dusk(glow(FIRE_ORANGE, 1.6))),
        [0.3, base + 0.98, front - 0.05],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.22, 2.3, 0.22], 0.0, timber(WOOD_DARK))),
            [sx * 0.78, base + 1.15, front - 0.06],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([1.9, 0.24, 0.24], 0.0, timber(WOOD_DARK))),
        [0.0, base + 2.2, front - 0.07],
        id_quat(),
    ));
    nest(
        prim(
            solid(cuboid_tapered(
                [l, wall_top + 0.1 - base, w],
                0.0,
                upright_boards(timber(WOOD_WARM)),
            )),
            [0.0, (base + wall_top + 0.1) * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// The lookout: a plank platform on four braced posts beside the gate, a
/// breastwork round it and a little roof over it, reached by a ladder.
fn lookout() -> Generator {
    let (cx, cz) = (-4.2_f32, -4.6_f32);
    let base = FLOOR_TOP - 0.1;
    let deck = 5.6_f32;
    let roof_at = deck + 2.4;
    let h = 1.0_f32;
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            if sx < 0.0 && sz < 0.0 {
                continue; // the sub-root
            }
            parts.push(post(cx + sx * h, cz + sz * h, base, roof_at + 0.2));
        }
    }
    // X-bracing on the two faces seen from the gate side, the second brace
    // of each pair set a little outward where the two cross.
    let o = 0.06;
    for (a, b) in [
        ([cx - h, base + 0.4, cz - h], [cx + h, deck - 0.5, cz - h]),
        (
            [cx + h, base + 0.4, cz - h - o],
            [cx - h, deck - 0.5, cz - h - o],
        ),
        ([cx - h, base + 0.4, cz - h], [cx - h, deck - 0.5, cz + h]),
        (
            [cx - h - o, base + 0.4, cz + h],
            [cx - h - o, deck - 0.5, cz - h],
        ),
    ] {
        parts.push(strut(a, b, 0.08, 6, timber(WOOD_DARK)));
    }
    // Deck and breastwork.
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * h + 0.6, 0.2, 2.0 * h + 0.6],
            0.0,
            timber(WOOD_WARM),
        )),
        [cx, deck - 0.1, cz],
        id_quat(),
    ));
    for (dx, dz, sx, sz) in [
        (0.0, -1.0, 2.0 * h + 0.6, 0.1),
        (0.0, 1.0, 2.0 * h + 0.6, 0.1),
        (-1.0, 0.0, 0.1, 2.0 * h + 0.4),
        (1.0, 0.0, 0.1, 2.0 * h + 0.4),
    ] {
        parts.push(prim(
            solid(cuboid_tapered(
                [sx, 0.9, sz],
                0.0,
                upright_boards(timber(WOOD_WARM)),
            )),
            [cx + dx * (h + 0.25), deck + 0.45, cz + dz * (h + 0.25)],
            id_quat(),
        ));
    }
    // Low pyramid roof of shakes over the platform.
    parts.push(prim(
        solid(cuboid_tapered(
            [2.0 * h + 1.2, 1.0, 2.0 * h + 1.2],
            0.9,
            timber(WOOD_DARK),
        )),
        [cx, roof_at + 0.5, cz],
        id_quat(),
    ));
    // Ladder up the back face, leant from the ground to the deck.
    parts.push(strut(
        [cx + 0.3, base, cz + h + 1.4],
        [cx + 0.3, deck, cz + h + 0.15],
        0.12,
        4,
        upright_boards(timber(WOOD_WARM)),
    ));
    nest(post(cx - h, cz - h, base, roof_at + 0.2), parts)
}

/// One lookout post from `bottom` to `top`.
fn post(x: f32, z: f32, bottom: f32, top: f32) -> Generator {
    prim(
        solid(cylinder_tapered(
            0.16,
            top - bottom,
            8,
            0.0,
            timber(WOOD_DARK),
        )),
        [x, (top + bottom) * 0.5, z],
        id_quat(),
    )
}

fn build_tree() -> Generator {
    // Round footing under the rampart: its rim stays inside the turf.
    let mut root = footing_disc(12.6, CLEARANCE);
    attach(&mut root, earthworks());
    attach(&mut root, gate());
    attach(&mut root, longhouse());
    attach(&mut root, lookout());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&RingFort.build(""), "nordic_ring_fort");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = RingFort.build("");
        assert_nothing_floats(&g, "nordic_ring_fort");
        assert_no_z_fighting(&g, "nordic_ring_fort");
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&RingFort.build("")).unwrap().len();
        eprintln!("nordic_ring_fort record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
