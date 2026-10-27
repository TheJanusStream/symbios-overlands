//! Hilltop observatory - a Civic-Campus secondary that leads a university
//! town from its hill. A stone drum carries a verdigris copper dome with
//! its shutter slit open to the sky on -Z, the telescope inside aimed out
//! of it; a stone lab wing with tall lit windows runs east, and its flat
//! roof is a railed terrace with a small refractor dome of its own.
//!
//! Primitive-built in one flat ground-relative frame. The great dome is two
//! quarter domes stood a slit's width apart, so the slit runs a constant
//! width over the crown; the shutter, slid back, is a torus arc lying in
//! the slit over the rear half.

use std::f32::consts::{FRAC_PI_2, PI};

use crate::catalogue::items::util::{
    attach, cuboid_tapered, cylinder_tapered, footing, glow, id_quat, nest, prim, prim_scaled,
    quat_x, quat_y, quat_z, railing, solid, sphere, torus, with_cut,
};
use crate::catalogue::{CatalogueEntry, Footprint, SitePreference, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    COPPER_VERDIGRIS, LAMP_WARM, MARBLE_WHITE, STEEL_GREY, STONE_PALE, WINDOW_WARM, copper, marble,
    steel, stone,
};

/// The telescope house: its drum's centre, radius and height.
const DRUM_X: f32 = -4.0;
const DRUM_Z: f32 = 1.0;
const DRUM_R: f32 = 4.5;
const DRUM_H: f32 = 6.5;
/// The great dome's radius and its slit's width.
const DOME_R: f32 = 4.6;
const SLIT: f32 = 1.4;
/// How far each quarter dome is pressed across the slit, so the two stand
/// within the drum's round.
const DOME_PRESS: f32 = 0.85;
/// The lab wing: its west and east ends, front and back, and wall top.
const WING_W: f32 = 0.0;
const WING_E: f32 = 9.0;
const WING_FRONT: f32 = -2.5;
const WING_BACK: f32 = 4.5;
const WING_H: f32 = 5.8;
/// Top of the cornice the terrace is laid on.
const TERRACE_Y: f32 = WING_H + 0.05;
/// Spacing circle the settlement keeps round the observatory.
const CLEARANCE: f32 = 11.0;

/// The dark of the dome's inside, seen through the slit.
const DOME_DARK: [f32; 3] = [0.06, 0.06, 0.07];
/// The telescope's enamel.
const SCOPE_WHITE: [f32; 3] = [0.86, 0.86, 0.84];
/// The gilt finial over the dome.
const GILT: [f32; 3] = [0.78, 0.62, 0.26];
/// The drum's oak door.
const OAK: [f32; 3] = [0.34, 0.22, 0.12];

pub struct HilltopObservatory;

impl CatalogueEntry for HilltopObservatory {
    fn slug(&self) -> &'static str {
        "hilltop_observatory"
    }
    fn name(&self) -> &'static str {
        "Hilltop Observatory"
    }
    fn description(&self) -> &'static str {
        "University observatory: a copper dome with its slit open, a stone lab wing, a terrace dome."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// High ground, clear of the town's lights.
    fn site(&self) -> SitePreference {
        SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CivicCampus]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::CAMPUS_BAND
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

/// Plain matt paint, untextured.
fn paint(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        metallic: Fp(0.1),
        ..Default::default()
    }
}

/// A room's light behind a window, lit from dusk to dawn.
fn pane() -> SovereignMaterialSettings {
    crate::catalogue::items::street::lit_from_dusk(glow(WINDOW_WARM, 1.4))
}

/// The telescope house: the drum on its plinth course, its door and slit
/// windows, the cornice, the split dome with its slid-back shutter, and the
/// telescope on its pier inside.
fn telescope_house() -> Generator {
    let (cx, cz) = (DRUM_X, DRUM_Z);
    let top = DRUM_H;
    let front = cz - DRUM_R;
    let mut parts = vec![
        // Plinth course, a step out from the drum.
        prim(
            solid(cylinder_tapered(
                DRUM_R + 0.2,
                0.6,
                40,
                0.0,
                stone(STONE_PALE),
            )),
            [cx, 0.3, cz],
            id_quat(),
        ),
        // Cornice ring under the dome.
        prim(
            solid(torus(0.26, DRUM_R + 0.04, marble(MARBLE_WHITE))),
            [cx, top - 0.1, cz],
            id_quat(),
        ),
        // Door surround and the oak door, on the -Z face.
        prim(
            solid(cuboid_tapered([1.9, 3.0, 0.3], 0.0, marble(MARBLE_WHITE))),
            [cx, 2.0, front],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([1.3, 2.4, 0.3], 0.0, paint(OAK))),
            [cx, 1.8, front - 0.08],
            id_quat(),
        ),
        // Entrance landing.
        prim(
            solid(cuboid_tapered([3.0, 0.3, 1.6], 0.0, stone(STONE_PALE))),
            [cx, 0.15, front - 0.6],
            id_quat(),
        ),
    ];
    // Slit windows round the drum, each turned to face out.
    for dx in [-2.6_f32, 2.6] {
        let dz = -(DRUM_R * DRUM_R - dx * dx).sqrt();
        let turn = quat_y(-(dx / DRUM_R).asin());
        parts.push(prim(
            cuboid_tapered([0.6, 1.6, 0.3], 0.0, pane()),
            [cx + dx, 3.4, cz + dz],
            turn,
        ));
    }
    // The dome: two quarter domes a slit apart, each the half of a hollow
    // hemisphere toward -X (longitudes a quarter to three quarters of a
    // turn), the east one turned a half turn to face it, both pressed in X
    // so the pair keeps to the drum.
    let dome_y = top - 0.05;
    let shell = || {
        solid(with_cut(
            sphere(DOME_R, 6, copper(COPPER_VERDIGRIS)),
            [0.25, 0.75],
            [0.5, 1.0],
            0.94,
        ))
    };
    let press = [DOME_PRESS, 1.0, 1.0];
    parts.push(prim_scaled(
        shell(),
        [cx - SLIT * 0.5, dome_y, cz],
        id_quat(),
        press,
    ));
    parts.push(prim_scaled(
        shell(),
        [cx + SLIT * 0.5, dome_y, cz],
        quat_y(PI),
        press,
    ));
    // A dark floor disc inside, so the slit shows the dark of the house.
    parts.push(prim(
        cylinder_tapered(DRUM_R - 0.4, 0.05, 32, 0.0, paint(DOME_DARK)),
        [cx, top + 0.02, cz],
        id_quat(),
    ));
    // The shutter slid back over the rear half of the slit: a quarter of a
    // torus lying in the slit's plane, from the crown down to the back.
    parts.push(prim(
        solid(with_cut(
            torus(SLIT * 0.5 + 0.1, DOME_R + 0.02, copper(COPPER_VERDIGRIS)),
            [0.0, 0.25],
            [0.0, 1.0],
            0.0,
        )),
        [cx, dome_y, cz],
        quat_z(FRAC_PI_2),
    ));
    // Gilt finial on the shutter's crown.
    parts.push(prim(
        sphere(0.3, 3, steel(GILT)),
        [cx, dome_y + DOME_R + SLIT * 0.5 + 0.15, cz + 0.2],
        id_quat(),
    ));
    // The telescope: a pier, and the tube on it aimed out of the slit.
    parts.push(nest(
        prim(
            solid(cylinder_tapered(0.35, 1.7, 12, 0.1, paint(STEEL_GREY))),
            [cx, top + 0.85, cz],
            id_quat(),
        ),
        vec![prim(
            cylinder_tapered(0.42, 4.0, 16, 0.0, paint(SCOPE_WHITE)),
            [cx, top + 2.1, cz - 0.6],
            quat_x(-0.75),
        )],
    ));
    nest(
        prim(
            solid(cylinder_tapered(DRUM_R, top, 40, 0.0, stone(STONE_PALE))),
            [cx, top * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// A tall lab window centred on `at` in a wall facing -Z, or +X when
/// `facing_x`: the lit pane in a marble surround, a sill under it and a
/// lintel over it.
fn tall_window(at: [f32; 3], facing_x: bool) -> Vec<Generator> {
    let [x, y, z] = at;
    let (w, h) = (1.2_f32, 3.2_f32);
    // Sizes along the wall, up it and out of it, turned to the wall.
    let size = |along: f32, up: f32, out: f32| {
        if facing_x {
            [out, up, along]
        } else {
            [along, up, out]
        }
    };
    let off = |out: f32| {
        if facing_x {
            [x + out, 0.0, z]
        } else {
            [x, 0.0, z - out]
        }
    };
    let at_out = |out: f32, dy: f32| {
        let [px, _, pz] = off(out);
        [px, y + dy, pz]
    };
    vec![
        prim(
            solid(cuboid_tapered(
                size(w + 0.3, h + 0.3, 0.06),
                0.0,
                marble(MARBLE_WHITE),
            )),
            at_out(0.01, 0.0),
            id_quat(),
        ),
        prim(
            cuboid_tapered(size(w, h, 0.08), 0.0, pane()),
            at_out(0.04, 0.0),
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                size(w + 0.4, 0.14, 0.26),
                0.0,
                marble(MARBLE_WHITE),
            )),
            at_out(0.1, -h * 0.5 - 0.22),
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered(
                size(w + 0.5, 0.32, 0.16),
                0.0,
                marble(MARBLE_WHITE),
            )),
            at_out(0.07, h * 0.5 + 0.3),
            id_quat(),
        ),
    ]
}

/// The lab wing: its tall windows, its cornice, and the terrace on its roof
/// with a railing round it and the small refractor dome.
fn lab_wing() -> Generator {
    let cx = (WING_W + WING_E) * 0.5;
    let cz = (WING_FRONT + WING_BACK) * 0.5;
    let w = WING_E - WING_W;
    let d = WING_BACK - WING_FRONT;
    let mut parts = vec![
        // Cornice, proud of the walls all round; its top is the terrace.
        prim(
            solid(cuboid_tapered(
                [w + 0.3, 0.3, d + 0.3],
                0.0,
                marble(MARBLE_WHITE),
            )),
            [cx, WING_H - 0.1, cz],
            id_quat(),
        ),
        // Plinth course.
        prim(
            solid(cuboid_tapered(
                [w + 0.2, 0.7, d + 0.2],
                0.0,
                stone([0.56, 0.54, 0.50]),
            )),
            [cx, 0.35, cz],
            id_quat(),
        ),
    ];
    for x in [1.7_f32, 3.7, 5.7, 7.7] {
        parts.extend(tall_window([x, 3.0, WING_FRONT], false));
    }
    for z in [-0.2_f32, 2.4] {
        parts.extend(tall_window([WING_E, 3.0, z], true));
    }
    // The terrace railing, front, east end and back, clear of the drum.
    // The east run stops a post short of each corner, so no two end posts
    // stand in one place.
    let (x0, x1) = (WING_W + 0.15, WING_E - 0.05);
    let (z0, z1) = (WING_FRONT + 0.05, WING_BACK - 0.05);
    let rail = || steel(STEEL_GREY);
    parts.extend(railing(
        [x0, TERRACE_Y, z0],
        [x1, TERRACE_Y, z0],
        1.0,
        0.42,
        rail(),
    ));
    parts.extend(railing(
        [x1, TERRACE_Y, z0 + 0.12],
        [x1, TERRACE_Y, z1 - 0.12],
        1.0,
        0.42,
        rail(),
    ));
    parts.extend(railing(
        [x1, TERRACE_Y, z1],
        [x0, TERRACE_Y, z1],
        1.0,
        0.42,
        rail(),
    ));
    // The refractor dome: a short stone drum, a copper dome with a narrow
    // slit turned to -Z, and a finial.
    let (rx, rz) = (6.2_f32, 1.4_f32);
    let dome = with_cut(
        sphere(1.55, 6, copper(COPPER_VERDIGRIS)),
        [0.05, 0.95],
        [0.5, 1.0],
        0.9,
    );
    parts.push(nest(
        prim(
            solid(cylinder_tapered(1.5, 1.4, 28, 0.0, stone(STONE_PALE))),
            [rx, TERRACE_Y + 0.68, rz],
            id_quat(),
        ),
        vec![
            prim(solid(dome), [rx, TERRACE_Y + 1.36, rz], quat_y(FRAC_PI_2)),
            prim(
                cylinder_tapered(1.4, 0.05, 24, 0.0, paint(DOME_DARK)),
                [rx, TERRACE_Y + 1.4, rz],
                id_quat(),
            ),
            prim(
                sphere(0.16, 3, steel(GILT)),
                [rx, TERRACE_Y + 2.95, rz],
                id_quat(),
            ),
        ],
    ));
    nest(
        prim(
            solid(cuboid_tapered([w, WING_H, d], 0.0, stone(STONE_PALE))),
            [cx, WING_H * 0.5, cz],
            id_quat(),
        ),
        parts,
    )
}

/// A campus lamp by the door, lit at every hour as a lamp is.
fn lamp() -> Generator {
    let (x, z) = (DRUM_X + 2.4, DRUM_Z - DRUM_R + 0.1);
    nest(
        prim(
            solid(cylinder_tapered(
                0.09,
                3.4,
                10,
                0.2,
                steel([0.16, 0.17, 0.18]),
            )),
            [x, 1.7, z],
            id_quat(),
        ),
        vec![prim(
            sphere(0.24, 3, glow(LAMP_WARM, 3.0)),
            [x, 3.55, z],
            id_quat(),
        )],
    )
}

fn build_tree() -> Generator {
    let x0 = DRUM_X - DRUM_R - 0.2;
    let (z0, z1) = (DRUM_Z - DRUM_R - 0.2, DRUM_Z + DRUM_R + 0.2);
    let x1 = WING_E + 0.3;
    let mut root = footing(
        x1 - x0,
        z1 - z0,
        [(x0 + x1) * 0.5, (z0 + z1) * 0.5],
        CLEARANCE,
    );
    for part in [telescope_house(), lab_wing(), lamp()] {
        attach(&mut root, part);
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
        assert_sanitize_stable(&HilltopObservatory.build(""), "hilltop_observatory");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let built = HilltopObservatory.build("");
        assert_no_z_fighting(&built, "hilltop_observatory");
        assert_nothing_floats(&built, "hilltop_observatory");
    }

    /// A landmark's record stays well inside the 100 KiB budget.
    #[test]
    fn stays_well_inside_the_record_budget() {
        let bytes = serde_json::to_vec(&HilltopObservatory.build(""))
            .expect("a built entry serialises")
            .len();
        assert!(
            bytes < 60_000,
            "hilltop_observatory serialises to {bytes} B"
        );
    }
}
