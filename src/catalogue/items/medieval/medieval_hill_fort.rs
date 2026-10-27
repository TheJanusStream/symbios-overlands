//! Hill fort - a Medieval secondary that crowns a crest. Two concentric
//! turf ramparts with a ditch between them, a sharpened-log palisade along
//! the inner bank, a timber gatehouse with a fighting walkway over its gate
//! where both banks open to the -Z front, and a thatched great hall inside.
//!
//! Where the [castle](super::medieval_castle) is dressed stone, this is the
//! older earth-and-timber way of holding high ground: the silhouette is the
//! rings of bank and stockade, read from below the hill.
//!
//! The banks are flattened tori and the palisade a tube, each path-cut to
//! leave the gate gap; a cut ring's gap sits at its own +X, so every ring is
//! turned a quarter about Y to bring it round to -Z. None of them is solid:
//! a ring's collider is the box round the whole of it, which would wall the
//! bailey in.

use std::f32::consts::{FRAC_PI_2, TAU};

use crate::catalogue::items::nordic::gable_roof;
use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    attach, cone, cuboid_tapered, cylinder_tapered, footing_disc, glow, id_quat, nest, prim,
    prim_scaled, quat_y, railing, solid, sphere, tile, tiles_per_metre, tube, upright_boards,
    with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{
    Fp, Fp3, Fp64, Generator, GeneratorKind, PrimCommon, SovereignGroundConfig,
    SovereignMaterialSettings, SovereignTextureConfig,
};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    DAUB_CREAM, HERALD_GOLD, HERALD_RED, IRON_DARK, SLATE_GREY, THATCH_STRAW, WOOD_DARK, WOOD_OAK,
    cloth, daub, iron, shingle, thatch, timber,
};

/// Spacing circle the settlement keeps round the fort, and the reach of its
/// outer bank.
const CLEARANCE: f32 = 18.0;

/// Outer rampart: centreline radius, half-width at the foot, and the
/// flattening that sets its height (`minor * squash`). A squash of about
/// half is what makes a ring read as a low broad bank with a gentle slope
/// each side rather than as a tube lying on the ground.
const OUTER_R: f32 = 15.3;
const OUTER_MINOR: f32 = 2.5;
const OUTER_SQUASH: f32 = 0.55;
/// Inner rampart, the taller one the palisade stands on: as broad as the
/// room between the hall and the ditch allows.
const INNER_R: f32 = 9.8;
const INNER_MINOR: f32 = 2.8;
const INNER_SQUASH: f32 = 0.55;

/// Width of the cut through the inner bank, where the gatehouse towers take
/// its ends, and through the outer one, whose rounded terminals narrow it
/// back to a passage at the crest.
const GATE_GAP: f32 = 4.4;
const OUTER_GAP: f32 = 9.0;
const PASSAGE_W: f32 = 3.0;

/// Palisade: radius of its line, thickness, and the band it occupies. A
/// tube's bore is at most 0.95 of its radius, so the wall is a log's width
/// thick at least.
const PALISADE_R: f32 = INNER_R;
const PALISADE_T: f32 = 0.54;
const PALISADE_BASE: f32 = 0.9;
const PALISADE_TOP: f32 = 4.2;

/// The bailey floor's top, just above the footing's 0.15 m reveal.
const FLOOR_TOP: f32 = 0.24;

pub struct HillFort;

impl CatalogueEntry for HillFort {
    fn slug(&self) -> &'static str {
        "medieval_hill_fort"
    }
    fn name(&self) -> &'static str {
        "Hill Fort"
    }
    fn description(&self) -> &'static str {
        "Twin turf ramparts and a log palisade crowning a crest, a timber gatehouse and a thatched hall within."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// High ground is the whole point of an earthwork fort.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Medieval]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::MEDIEVAL_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 48.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// Close-cropped turf - the ramparts and the bailey floor.
fn turf(dry: [f32; 3], moist: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(moist),
        roughness: Fp(0.95),
        uv_scale: tiles_per_metre(tile::GROUND),
        texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
            color_dry: Fp3(dry),
            color_moist: Fp3(moist),
            macro_scale: Fp64(4.0),
            ..Default::default()
        }),
        ..Default::default()
    }
}

/// Bank turf on the slopes: rough and darker, the grass grown long where
/// nobody walks.
fn bank_turf() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        roughness: Fp(1.0),
        texture: SovereignTextureConfig::Ground(SovereignGroundConfig {
            color_dry: Fp3([0.24, 0.31, 0.13]),
            color_moist: Fp3([0.11, 0.18, 0.07]),
            macro_scale: Fp64(2.5),
            ..Default::default()
        }),
        ..turf([0.24, 0.31, 0.13], [0.11, 0.18, 0.07])
    }
}

/// Bank turf along the crest, paler and cropped where the walk runs.
fn crest_turf() -> SovereignMaterialSettings {
    turf([0.38, 0.46, 0.22], [0.22, 0.33, 0.14])
}

/// Bare trodden earth - the ditch floor and the causeway through the gate.
fn earth() -> SovereignMaterialSettings {
    turf([0.42, 0.34, 0.22], [0.26, 0.20, 0.13])
}

/// A ring path-cut to leave `gap` metres clear at radius `r`: the kept
/// fraction of the turn, as [`with_cut`] takes it.
fn ring_cut(gap: f32, r: f32) -> [f32; 2] {
    let g = gap / (TAU * r) * 0.5;
    [g, 1.0 - g]
}

/// One turf rampart: a torus flattened to `squash` of its half-width and
/// cut open for the gate, turned so the gap faces -Z. Its centreline sits at
/// grade, so the lower half is buried and the bank rises `minor * squash`.
fn rampart(major: f32, minor: f32, squash: f32, gap: f32) -> Generator {
    let kind = with_cut(
        GeneratorKind::Torus {
            minor_radius: Fp(minor),
            major_radius: Fp(major),
            minor_resolution: 12,
            major_resolution: 72,
            common: PrimCommon::with_material(bank_turf()),
        },
        ring_cut(gap, major),
        [0.0, 1.0],
        0.0,
    );
    prim_scaled(kind, [0.0, 0.0, 0.0], quat_y(FRAC_PI_2), [1.0, squash, 1.0])
}

/// The paler crest laid along a [`rampart`] of the same `major`, `minor`
/// and `squash`: a flatter, narrower ring whose top stands 3 cm over the
/// bank's, so it covers the upper three quarters of the bank's width and the
/// darker turf shows only on the slopes below. Its cut stands 5 cm back
/// from the bank's at the gate, so the two cut faces never share a plane.
fn rampart_crest(major: f32, minor: f32, squash: f32, gap: f32) -> Generator {
    let (r, s) = (minor * 0.85, squash * 0.75);
    let kind = with_cut(
        GeneratorKind::Torus {
            minor_radius: Fp(r),
            major_radius: Fp(major),
            minor_resolution: 12,
            major_resolution: 72,
            common: PrimCommon::with_material(crest_turf()),
        },
        ring_cut(gap + 0.1, major),
        [0.0, 1.0],
        0.0,
    );
    let centre = minor * squash + 0.03 - r * s;
    prim_scaled(kind, [0.0, centre, 0.0], quat_y(FRAC_PI_2), [1.0, s, 1.0])
}

/// The earthworks as one level piece. Its root is the ditch floor between
/// the banks - bare earth a hand's breadth above the footing, so the stone
/// never shows between the turf - sunk deep enough to count as footing for
/// the ruin pass, so a fought-over fort keeps its banks level. The root is
/// the one ring that is not turned: a turned root would turn the causeway
/// and everything else with it.
fn earthworks() -> Generator {
    let ditch_depth = 1.4;
    let ditch = prim(
        tube(13.6, 11.8, ditch_depth, 72, earth()),
        [0.0, 0.2 - ditch_depth * 0.5, 0.0],
        id_quat(),
    );
    let mut parts = vec![
        rampart(OUTER_R, OUTER_MINOR, OUTER_SQUASH, OUTER_GAP),
        rampart_crest(OUTER_R, OUTER_MINOR, OUTER_SQUASH, OUTER_GAP),
        rampart(INNER_R, INNER_MINOR, INNER_SQUASH, GATE_GAP),
        rampart_crest(INNER_R, INNER_MINOR, INNER_SQUASH, GATE_GAP),
        // Bailey floor inside the inner bank.
        prim(
            solid(cylinder_tapered(
                7.6,
                0.3,
                48,
                0.0,
                turf([0.36, 0.40, 0.20], [0.22, 0.27, 0.13]),
            )),
            [0.0, FLOOR_TOP - 0.15, 0.0],
            id_quat(),
        ),
    ];
    // Causeway: the trodden way in through both gaps, across the ditch.
    let (way_out, way_in) = (
        -(OUTER_R + OUTER_MINOR + 0.3),
        -(INNER_R - INNER_MINOR) + 0.5,
    );
    parts.push(prim(
        solid(cuboid_tapered([3.4, 0.3, way_in - way_out], 0.0, earth())),
        [0.0, 0.12, (way_in + way_out) * 0.5],
        id_quat(),
    ));

    // The palisade along the inner bank's crest, open at the gatehouse.
    let palisade = with_cut(
        tube(
            PALISADE_R + PALISADE_T * 0.5,
            PALISADE_R - PALISADE_T * 0.5,
            PALISADE_TOP - PALISADE_BASE,
            96,
            upright_boards(timber(WOOD_OAK)),
        ),
        ring_cut(PASSAGE_W + 2.2, PALISADE_R),
        [0.0, 1.0],
        0.0,
    );
    parts.push(prim(
        palisade,
        [0.0, (PALISADE_BASE + PALISADE_TOP) * 0.5, 0.0],
        quat_y(FRAC_PI_2),
    ));
    // Sharpened log tips along its top, stopping at the gatehouse towers.
    let half_gap = (PASSAGE_W + 2.2) * 0.5 / PALISADE_R;
    let n = (TAU * PALISADE_R / 1.0).round() as i32;
    for i in 0..n {
        // Measured from -Z, the gate side, round through +X.
        let a = (i as f32 + 0.5) / n as f32 * TAU;
        if a < half_gap + 0.03 || a > TAU - half_gap - 0.03 {
            continue;
        }
        parts.push(prim(
            cone(0.22, 0.8, 5, timber(WOOD_DARK)),
            [
                PALISADE_R * a.sin(),
                PALISADE_TOP + 0.32,
                -PALISADE_R * a.cos(),
            ],
            id_quat(),
        ));
    }

    // Rounded terminals where the outer bank stops at the way in: a dome of
    // the bank's own section over each cut end.
    let end = OUTER_GAP * 0.5 / OUTER_R;
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim_scaled(
            sphere(OUTER_MINOR, 6, bank_turf()),
            [sx * OUTER_R * end.sin(), 0.0, -OUTER_R * end.cos()],
            id_quat(),
            [1.0, OUTER_SQUASH, 1.0],
        ));
    }

    nest(ditch, parts)
}

/// The timber gatehouse set into the inner bank's gap: two plank towers
/// flanking the passage, a walkway deck over the gate with railings to both
/// sides, a shingled roof over all three, and the gate leaves swung in. The
/// -X tower is the sub-root, so a ruin knocks the gatehouse as one piece.
fn gatehouse() -> Generator {
    let z = -INNER_R;
    let tower = 2.3_f32;
    let tx = PASSAGE_W * 0.5 + tower * 0.5;
    let base = FLOOR_TOP - 0.1;
    let eave = 6.4_f32;
    // Towers and posts run up into the roof, so nothing meets its underside
    // in one plane.
    let into_roof = eave + 0.3;
    let deck_top = 4.3_f32;
    let roof_rise = 1.7_f32;
    // Front and back walkway edges, relative to the towers' centreline.
    let (walk_front, walk_back) = (-tower * 0.5 - 0.4, tower * 0.5 - 0.2);

    let tower_at = |sx: f32| {
        prim(
            solid(cuboid_tapered(
                [tower, into_roof - base, tower],
                0.0,
                upright_boards(timber(WOOD_OAK)),
            )),
            [sx * tx, (base + into_roof) * 0.5, z],
            id_quat(),
        )
    };

    let mut parts = vec![tower_at(1.0)];
    for sx in [-1.0_f32, 1.0] {
        // Corner posts proud of each tower's front, the frame the boards
        // hang on.
        for px in [-1.0_f32, 1.0] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.28, into_roof - base, 0.28],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [
                    sx * tx + px * (tower * 0.5 - 0.1),
                    (base + into_roof) * 0.5,
                    z - tower * 0.5 - 0.06,
                ],
                id_quat(),
            ));
        }
        // An arrow loop in each tower's front, under the walkway.
        parts.push(prim(
            cuboid_tapered([0.18, 1.1, 0.1], 0.0, slot_dark()),
            [sx * tx, 2.7, z - tower * 0.5 - 0.02],
            id_quat(),
        ));
        // Posts carrying the roof over the open walkway.
        for dz in [walk_front, walk_back] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.22, into_roof - deck_top, 0.22],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [
                    sx * (PASSAGE_W * 0.5 - 0.14),
                    (deck_top + into_roof) * 0.5,
                    z + dz,
                ],
                id_quat(),
            ));
        }
    }
    // The walkway deck over the gate, bearing into both towers.
    parts.push(prim(
        solid(cuboid_tapered(
            [PASSAGE_W + 0.6, 0.3, tower + 0.44],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, deck_top - 0.15, z - 0.25],
        id_quat(),
    ));
    // Lintel beam across the passage mouth, under the deck.
    parts.push(prim(
        solid(cuboid_tapered(
            [PASSAGE_W + 0.5, 0.4, 0.4],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, deck_top - 0.5, z - tower * 0.5 - 0.3],
        id_quat(),
    ));
    // Railings along the walkway's front and back edges.
    for dz in [walk_front, walk_back] {
        parts.extend(railing(
            [-PASSAGE_W * 0.5 + 0.3, deck_top, z + dz],
            [PASSAGE_W * 0.5 - 0.3, deck_top, z + dz],
            1.05,
            0.42,
            timber(WOOD_OAK),
        ));
    }
    // Shingled gable over towers and walkway, ridge across the gate.
    let roof_l = PASSAGE_W + 2.0 * tower + 1.2;
    parts.push(gable_roof(
        [roof_l, roof_rise, tower + 1.6],
        [0.0, eave + roof_rise * 0.5, z - 0.2],
        shingle(SLATE_GREY),
    ));
    parts.push(prim(
        solid(cuboid_tapered(
            [roof_l + 0.2, 0.22, 0.26],
            0.0,
            timber(WOOD_DARK),
        )),
        [0.0, eave + roof_rise + 0.09, z - 0.2],
        id_quat(),
    ));
    // Gate leaves, swung inward on their hinges at the passage sides.
    let leaf_w = PASSAGE_W * 0.5 - 0.05;
    let leaf_top = deck_top - 0.7;
    let open = 1.25_f32;
    for sx in [-1.0_f32, 1.0] {
        let hinge = [sx * (PASSAGE_W * 0.5 - 0.06), z - tower * 0.5 + 0.35];
        let mut leaf = prim(
            solid(cuboid_tapered(
                [leaf_w, leaf_top - base, 0.14],
                0.0,
                upright_boards(timber(WOOD_OAK)),
            )),
            [
                hinge[0] - sx * leaf_w * 0.5 * open.cos(),
                (base + leaf_top) * 0.5,
                hinge[1] + leaf_w * 0.5 * open.sin(),
            ],
            quat_y(sx * open),
        );
        // Iron strap across each leaf, in the leaf's own frame.
        leaf.children.push(prim(
            cuboid_tapered([leaf_w - 0.1, 0.12, 0.2], 0.0, iron(IRON_DARK)),
            [0.0, 0.6, 0.0],
            id_quat(),
        ));
        parts.push(leaf);
    }
    // A banner on a pole off the ridge: the fort's mark from below the hill.
    let ridge = eave + roof_rise + 0.2;
    let pole_top = ridge + 3.2;
    parts.push(prim(
        solid(cylinder_tapered(
            0.07,
            pole_top - ridge + 0.3,
            8,
            0.2,
            timber(WOOD_DARK),
        )),
        [0.0, (ridge - 0.3 + pole_top) * 0.5, z - 0.2],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([1.5, 0.95, 0.06], 0.0, cloth(HERALD_RED, HERALD_GOLD)),
        [0.79, pole_top - 0.6, z - 0.2],
        id_quat(),
    ));
    nest(tower_at(-1.0), parts)
}

/// The unlit dark of an arrow loop.
fn slot_dark() -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3([0.04, 0.035, 0.03]),
        roughness: Fp(1.0),
        ..Default::default()
    }
}

/// The great hall: daub walls in an oak frame under a steep thatch gable,
/// its door and shuttered windows facing the gate.
fn great_hall() -> Generator {
    let (l, w) = (9.0_f32, 5.2_f32);
    let cz = 1.3_f32;
    let base = FLOOR_TOP - 0.1;
    let wall_top = base + 3.1;
    let rise = 3.5_f32;
    let front = cz - w * 0.5;

    let mut parts = Vec::new();
    // Oak frame: corner and bay posts proud of the front and back walls,
    // run up into the thatch so nothing meets its underside in one plane.
    for (s, face) in [(-1.0_f32, front), (1.0, cz + w * 0.5)] {
        for k in 0..5 {
            // The door takes the front's middle bay; its jambs stand there.
            if s < 0.0 && k == 2 {
                continue;
            }
            let x = -l * 0.5 + 0.1 + k as f32 * (l - 0.2) / 4.0;
            parts.push(prim(
                solid(cuboid_tapered(
                    [0.26, wall_top + 0.1 - base, 0.2],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [x, (base + wall_top + 0.1) * 0.5, face + s * 0.08],
                id_quat(),
            ));
        }
        // Wall plate and sill beam along the face, less proud than the posts.
        for y in [wall_top - 0.14, base + 0.18] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [l + 0.2, 0.24, 0.18],
                    0.0,
                    timber(WOOD_DARK),
                )),
                [0.0, y, face + s * 0.04],
                id_quat(),
            ));
        }
    }
    // Plank door on the front between oak jambs, a lintel over it.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered([0.22, 2.7, 0.2], 0.0, timber(WOOD_DARK))),
            [sx * 0.86, base + 1.35, front - 0.08],
            id_quat(),
        ));
    }
    parts.push(prim(
        solid(cuboid_tapered([2.1, 0.26, 0.22], 0.0, timber(WOOD_DARK))),
        [0.0, base + 2.75, front - 0.09],
        id_quat(),
    ));
    parts.push(prim(
        solid(cuboid_tapered([1.56, 2.6, 0.12], 0.0, timber(WOOD_OAK))),
        [0.0, base + 1.3, front - 0.1],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([1.3, 0.1, 0.06], 0.0, iron(IRON_DARK)),
        [0.0, base + 1.7, front - 0.18],
        id_quat(),
    ));
    // Two small windows, the hall's firelight showing from dusk.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            cuboid_tapered(
                [0.7, 0.6, 0.06],
                0.0,
                lit_from_dusk(glow([0.95, 0.55, 0.22], 1.8)),
            ),
            [sx * 2.75, base + 1.9, front - 0.02],
            id_quat(),
        ));
        // Open shutter beside it.
        parts.push(prim(
            cuboid_tapered([0.36, 0.66, 0.08], 0.0, timber(WOOD_OAK)),
            [sx * 2.75 + sx * 0.58, base + 1.9, front - 0.02],
            id_quat(),
        ));
    }
    // Steep thatch gable, ridge along X, and a ridge pole proud of the apex.
    parts.push(gable_roof(
        [l + 0.9, rise, w + 1.2],
        [0.0, wall_top + rise * 0.5, cz],
        thatch(THATCH_STRAW),
    ));
    parts.push(prim(
        solid(cuboid_tapered([l + 1.1, 0.24, 0.3], 0.0, timber(WOOD_DARK))),
        [0.0, wall_top + rise + 0.11, cz],
        id_quat(),
    ));

    nest(
        prim(
            solid(cuboid_tapered(
                [l, wall_top + 0.1 - base, w],
                0.0,
                daub(DAUB_CREAM),
            )),
            [0.0, (base + wall_top + 0.1) * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    // Round footing under the banks: its rim stays inside the outer bank's
    // turf, so no stone shows.
    let mut root = footing_disc(14.4, CLEARANCE);
    attach(&mut root, earthworks());
    attach(&mut root, gatehouse());
    attach(&mut root, great_hall());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&HillFort.build(""), "medieval_hill_fort");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = HillFort.build("");
        assert_nothing_floats(&g, "medieval_hill_fort");
        assert_no_z_fighting(&g, "medieval_hill_fort");
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&HillFort.build("")).unwrap().len();
        eprintln!("medieval_hill_fort record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
