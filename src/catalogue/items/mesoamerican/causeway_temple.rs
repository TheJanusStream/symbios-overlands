//! Causeway temple - a Mesoamerican secondary by a lake. A stepped platform
//! of three talud-tablero tiers - each a battered slope under a framed,
//! red-painted panel that oversails it - carries a plastered shrine under a
//! tall thatch roof, a sacred fire burning before its door. A stair climbs
//! the -Z face from a raised stone causeway that runs out toward the water,
//! where two carved stelae stand at its end.
//!
//! Where the [step pyramid](super::step_pyramid) is battered terraces and a
//! roof comb, this is the highland order: the tablero's shadowed panel, a
//! lower and broader platform, and the causeway that ties it to the lake.
//! The settlement turns a shore building's front to the water, so the
//! causeway and the stair are the -Z hero face.

use std::f32::consts::PI;

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, nest,
    prim, quat_y, solid, sphere, wedge,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    FIRE_ORANGE, GOLD_WARM, JADE_GREEN, LIMESTONE_PALE, MESO_BAND, OBSIDIAN_BLACK, STUCCO_CREAM,
    STUCCO_RED, THATCH_STRAW, fx, gold, jade, limestone, obsidian, painted, thatch,
};

/// Spacing circle the settlement keeps round the temple.
const CLEARANCE: f32 = 14.0;

/// The platform: its centre on Z and the half side of its base.
const PLAT_Z: f32 = 3.0;
const PLAT_HALF: f32 = 7.5;

/// The three tiers, bottom up: the talud's base half side, its height and
/// its batter (as `cuboid_tapered`'s taper), then the tablero's height.
const TIERS: [(f32, f32, f32, f32); 3] = [
    (7.5, 1.4, 0.107, 1.4),
    (6.1, 1.2, 0.131, 1.2),
    (4.8, 1.1, 0.146, 1.0),
];
/// Where the bottom talud starts: below grade, so the platform counts as
/// footing for the ruin pass and stands level in a fought-over room.
const BASE_Y: f32 = -0.6;
/// How far a tablero oversails the top of the talud under it, per side.
const OVERSAIL: f32 = 0.15;

/// The causeway: half its width, its far end on Z, and its top.
const WAY_HALF: f32 = 1.9;
const WAY_END: f32 = -16.0;
const WAY_TOP: f32 = 0.9;

/// The stair: half its width, and its foot on Z, on the causeway.
const STAIR_HALF: f32 = 2.1;
const STAIR_FOOT: f32 = -8.5;

pub struct CausewayTemple;

impl CatalogueEntry for CausewayTemple {
    fn slug(&self) -> &'static str {
        "causeway_temple"
    }
    fn name(&self) -> &'static str {
        "Causeway Temple"
    }
    fn description(&self) -> &'static str {
        "Talud-tablero platform and thatched shrine by a lake, reached by a raised causeway between two stelae."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// A lake temple stands where its causeway can run out to the water.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Shore
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Mesoamerican]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        MESO_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The height of the platform's top, the summit the shrine stands on.
fn summit() -> f32 {
    BASE_Y + TIERS.iter().map(|t| t.1 + t.3).sum::<f32>()
}

/// The platform: three talud-tablero tiers, the bottom talud its root; the
/// stair with its balustrades up the -Z face, and the fire before the
/// shrine.
fn platform() -> Generator {
    let mut parts = Vec::new();
    let mut root = None;
    let mut y = BASE_Y;
    for (k, &(half, talud_h, batter, tablero_h)) in TIERS.iter().enumerate() {
        let talud = prim(
            solid(cuboid_tapered(
                [2.0 * half, talud_h, 2.0 * half],
                batter,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, y + talud_h * 0.5, PLAT_Z],
            id_quat(),
        );
        if k == 0 {
            root = Some(talud);
        } else {
            parts.push(talud);
        }
        y += talud_h;
        // The tablero: a red panel, oversailing the talud's top, framed by
        // cream mouldings at its foot and its head that stand proud of it.
        let t = half * (1.0 - batter) + OVERSAIL;
        parts.push(prim(
            solid(cuboid_tapered(
                [2.0 * t, tablero_h - 0.1, 2.0 * t],
                0.0,
                painted(STUCCO_RED),
            )),
            [0.0, y + tablero_h * 0.5, PLAT_Z],
            id_quat(),
        ));
        for (band_y, band_h) in [(y + 0.12, 0.26), (y + tablero_h - 0.13, 0.26)] {
            parts.push(prim(
                solid(cuboid_tapered(
                    [2.0 * t + 0.24, band_h, 2.0 * t + 0.24],
                    0.0,
                    painted(STUCCO_CREAM),
                )),
                [0.0, band_y, PLAT_Z],
                id_quat(),
            ));
        }
        y += tablero_h;
    }
    let top = y;
    let front_top = PLAT_Z - (TIERS[2].0 * (1.0 - TIERS[2].2) + OVERSAIL);

    // The stair: a solid wedge from the causeway to the summit, turned so it
    // climbs toward +Z, its head buried in the top tier, with treads proud
    // of its slope.
    let run = front_top - STAIR_FOOT + 1.2;
    let rise = top + 0.05;
    let foot = STAIR_FOOT - 0.9;
    parts.push(prim(
        solid(wedge([2.0 * STAIR_HALF, rise, run], painted(STUCCO_CREAM))),
        [0.0, rise * 0.5, foot + run * 0.5],
        quat_y(PI),
    ));
    let steps = 11;
    let slope = rise / run;
    for i in 1..steps {
        let z = foot + run * i as f32 / steps as f32;
        let h = (z - foot) * slope;
        if h < WAY_TOP + 0.2 {
            continue; // under the causeway's top
        }
        parts.push(prim(
            solid(cuboid_tapered(
                [2.0 * STAIR_HALF - 0.3, 0.22, 0.42],
                0.0,
                limestone(LIMESTONE_PALE),
            )),
            [0.0, h - 0.04, z - 0.12],
            id_quat(),
        ));
    }
    // The balustrades (alfardas): steeper red wedges standing above the
    // stair's edges.
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(wedge([0.7, rise + 0.5, run - 0.4], painted(STUCCO_RED))),
            [
                sx * (STAIR_HALF + 0.33),
                // Set a hair lower than the stair, so their soles differ.
                (rise + 0.5) * 0.5 - 0.05,
                foot + 0.4 + (run - 0.4) * 0.5,
            ],
            quat_y(PI),
        ));
    }

    // The sacred fire on a stone brazier before the shrine.
    let fire_z = PLAT_Z - 2.6;
    parts.push(prim(
        solid(cylinder_tapered(
            0.6,
            0.8,
            12,
            -0.3,
            limestone(STUCCO_CREAM),
        )),
        [0.0, top + 0.38, fire_z],
        id_quat(),
    ));
    parts.push(prim(
        sphere(0.45, 4, glow(FIRE_ORANGE, 5.0)),
        [0.0, top + 0.85, fire_z],
        id_quat(),
    ));
    parts.push(fx::sacred_flame([0.0, top + 1.0, fire_z], 0xCA05_E7A1));

    nest(root.expect("three tiers"), parts)
}

/// The shrine on the summit: plastered walls under a red frieze, a dark
/// doorway between two red pillars on -Z, and a tall hipped thatch roof.
fn shrine() -> Generator {
    let top = summit();
    let (w, d, cz) = (5.6_f32, 4.6_f32, PLAT_Z + 0.8);
    let wall_top = top + 2.8;
    let front = cz - d * 0.5;
    let rise = 3.8_f32;
    let parts = vec![
        // Frieze band round the wall head.
        prim(
            solid(cuboid_tapered(
                [w + 0.2, 0.6, d + 0.2],
                0.0,
                painted(STUCCO_RED),
            )),
            [0.0, wall_top - 0.3, cz],
            id_quat(),
        ),
        // A gilded disc on the frieze over the door.
        prim(
            solid(cylinder_tapered(0.36, 0.1, 16, 0.0, gold(GOLD_WARM))),
            [0.0, wall_top - 0.3, front - 0.12],
            crate::catalogue::items::util::quat_x(std::f32::consts::FRAC_PI_2),
        ),
        // The doorway's dark, and the pillars either side of it.
        prim(
            cuboid_tapered([1.6, 2.0, 0.08], 0.0, obsidian(OBSIDIAN_BLACK)),
            [0.0, top + 1.0, front - 0.02],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.45, 2.2, 0.4], 0.0, painted(STUCCO_RED))),
            [-1.05, top + 1.1, front - 0.12],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.45, 2.2, 0.4], 0.0, painted(STUCCO_RED))),
            [1.05, top + 1.1, front - 0.12],
            id_quat(),
        ),
        // The tall thatch roof, hipped to a short ridge.
        prim(
            solid(cuboid_tapered_xz(
                [w + 1.2, rise, d + 1.2],
                [0.55, 0.9],
                thatch(THATCH_STRAW),
            )),
            [0.0, wall_top + rise * 0.5, cz],
            id_quat(),
        ),
    ];
    nest(
        prim(
            solid(cuboid_tapered(
                [w, wall_top + 0.1 - (top - 0.05), d],
                0.0,
                painted(STUCCO_CREAM),
            )),
            [0.0, (top - 0.05 + wall_top + 0.1) * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// The raised causeway from the stair's foot out toward the water: a
/// plastered stone road on a buried core, red kerbs along its edges.
fn causeway() -> Generator {
    let near = STAIR_FOOT + 0.3;
    let len = near - WAY_END;
    let cz = (near + WAY_END) * 0.5;
    let bottom = -1.5_f32;
    let mut parts = Vec::new();
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [0.4, 0.35, len - 0.1],
                0.0,
                painted(STUCCO_RED),
            )),
            [sx * (WAY_HALF - 0.18), WAY_TOP + 0.12, cz],
            id_quat(),
        ));
    }
    nest(
        prim(
            solid(cuboid_tapered(
                [2.0 * WAY_HALF, WAY_TOP - bottom, len],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [0.0, (WAY_TOP + bottom) * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// A carved stela on its plinth, its relief on the -Z face: a headdress, a
/// jade mask, and a column of painted glyph blocks.
fn stela(x: f32) -> Generator {
    let z = WAY_END + 1.2;
    let (plinth_top, h) = (0.45_f32, 3.2_f32);
    let face = z - 0.24;
    let parts = vec![
        prim(
            solid(cuboid_tapered(
                [1.15, h, 0.46],
                0.08,
                limestone(LIMESTONE_PALE),
            )),
            [x, plinth_top - 0.05 + h * 0.5, z],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.95, 0.55, 0.12], 0.2, gold(GOLD_WARM))),
            [x, plinth_top + h - 0.55, face],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.5, 0.55, 0.14], 0.1, jade(JADE_GREEN))),
            [x, plinth_top + h - 1.15, face - 0.01],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.62, 0.34, 0.1], 0.0, painted(STUCCO_RED))),
            [x, plinth_top + 1.25, face + 0.01],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.62, 0.34, 0.1], 0.0, painted(STUCCO_RED))),
            [x, plinth_top + 0.75, face + 0.01],
            id_quat(),
        ),
    ];
    nest(
        prim(
            solid(cuboid_tapered(
                [1.6, 0.6, 1.0],
                0.0,
                limestone(STUCCO_CREAM),
            )),
            [x, plinth_top - 0.3, z],
            id_quat(),
        ),
        parts,
    )
}

fn build_tree() -> Generator {
    let mut root = footing(
        2.0 * PLAT_HALF + 0.4,
        2.0 * PLAT_HALF + 0.4,
        [0.0, PLAT_Z],
        CLEARANCE,
    );
    attach(&mut root, platform());
    attach(&mut root, shrine());
    attach(&mut root, causeway());
    for sx in [-1.0_f32, 1.0] {
        attach(&mut root, stela(sx * (WAY_HALF + 1.2)));
    }
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CausewayTemple.build(""), "causeway_temple");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = CausewayTemple.build("");
        assert_nothing_floats(&g, "causeway_temple");
        assert_no_z_fighting(&g, "causeway_temple");
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&CausewayTemple.build("")).unwrap().len();
        eprintln!("causeway_temple record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
