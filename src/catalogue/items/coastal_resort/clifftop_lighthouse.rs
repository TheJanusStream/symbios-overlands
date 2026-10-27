//! Clifftop lighthouse - a Coastal-Resort secondary that leads a crest. The
//! resort's own lighthouse up on the headland: a whitewashed tower ringed in
//! two red bands, a railed gallery on brackets and a glazed lantern room
//! under a red dome, its lamp lit at every hour as a lamp is. At its foot the
//! keeper's cottage with teal shutters and lit windows, and on the -Z front a
//! railed viewing terrace with a brass telescope on its tripod, a bench and a
//! striped parasol, where the visitors come up to look out to sea.
//!
//! Where the classical [lighthouse](crate::catalogue::items::ancient) is a
//! working beacon on a harbour mole, this one is kept for the view.

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use crate::catalogue::items::street::lit_from_dusk;
use crate::catalogue::items::util::{
    BALUSTER_PITCH, attach, cone, cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing,
    glow, id_quat, lathe, lit_interior, nest, plane, prim, quat_mul, quat_x, quat_y, railing,
    solid, sphere, strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_TEAL, AWNING_WHITE, BUOY_RED, DECK_PALE, DECK_WOOD, DRIFT_GREY, GLASS_AQUA,
    PILING_GREY, STUCCO_WHITE, canvas, concrete, enamel, fx, pane_grid, plank, shingle, steel,
    stucco,
};

/// Spacing circle the settlement keeps round the headland.
const CLEARANCE: f32 = 9.0;
/// Top of the whitewashed platform everything stands on.
const PAD: f32 = 0.45;
/// Where the tower stands, the plinth round its foot and its five courses -
/// height and colour - from the ground up, each narrowing by `TAPER`.
const TOWER_AT: [f32; 2] = [2.2, 1.8];
const PLINTH_R: f32 = 2.6;
const PLINTH_H: f32 = 0.6;
const BASE_R: f32 = 2.1;
const TAPER: f32 = 0.07;
const COURSES: [(f32, bool); 5] = [
    (4.0, false),
    (2.6, true),
    (4.4, false),
    (2.6, true),
    (4.0, false),
];
/// Gallery deck radius, lantern-room apothem and glazing height.
const GALLERY_R: f32 = 2.45;
const LANTERN_R: f32 = 1.35;
const GLAZE_H: f32 = 1.7;
/// Lantern-room panes round the ring.
const PANES: usize = 8;

/// The keeper's cottage: centre in plan and size.
const COTTAGE_AT: [f32; 2] = [-4.0, 2.4];
const COTTAGE: [f32; 3] = [5.4, 2.8, 4.0];
/// The viewing terrace's deck: centre in plan, size and its front edge.
const TERRACE_AT: [f32; 2] = [-0.8, -4.6];
const TERRACE: [f32; 3] = [9.0, 0.2, 4.4];

pub struct ClifftopLighthouse;

impl CatalogueEntry for ClifftopLighthouse {
    fn slug(&self) -> &'static str {
        "clifftop_lighthouse"
    }
    fn name(&self) -> &'static str {
        "Clifftop Lighthouse"
    }
    fn description(&self) -> &'static str {
        "Red-banded white lighthouse on a headland, a keeper's cottage and a viewing terrace with a telescope."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    /// Leads a settlement of its theme as its landmark (#1783).
    fn leads(&self) -> bool {
        true
    }
    /// The headland, high over the sea.
    fn site(&self) -> crate::catalogue::SitePreference {
        crate::catalogue::SitePreference::Crest
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
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

/// Plain gloss paint, untextured - the rails, bars and small fittings, so
/// the many small parts stay cheap.
fn paint(color: [f32; 3]) -> SovereignMaterialSettings {
    SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.35),
        metallic: Fp(0.2),
        ..Default::default()
    }
}

/// The turn that stands a flat card up facing out from the tower's axis at
/// `a` round it (0 at +Z).
fn facing_out(a: f32) -> crate::pds::Fp4 {
    quat_mul(quat_y(a + PI), quat_x(-FRAC_PI_2))
}

/// A small window let into the tower's wall at `a` round it and `y` up: a
/// lit room behind a four-pane card, its sill a white ledge.
fn tower_window(a: f32, y: f32, r: f32) -> Vec<Generator> {
    let [x, z] = TOWER_AT;
    let out = |d: f32| [x + d * a.sin(), y, z + d * a.cos()];
    vec![
        prim(
            cuboid_tapered(
                [0.62, 0.95, 0.06],
                0.0,
                lit_from_dusk(lit_interior([0.80, 0.62, 0.38], 0.9)),
            ),
            out(r - 0.01),
            quat_y(a),
        ),
        prim(
            plane([0.66, 1.0], pane_grid(GLASS_AQUA, 0.0, (2, 2))),
            out(r + 0.05),
            facing_out(a),
        ),
        prim(
            cuboid_tapered([0.8, 0.08, 0.2], 0.0, paint(AWNING_WHITE)),
            [x + (r + 0.05) * a.sin(), y - 0.53, z + (r + 0.05) * a.cos()],
            quat_y(a),
        ),
    ]
}

/// The lantern room on the gallery deck: its red parapet, the eight panes
/// and their bars, the lamp on its pedestal, the cornice, the dome, its
/// ventilator ball and the lightning rod.
fn lantern(deck_top: f32) -> Generator {
    let [x, z] = TOWER_AT;
    let parapet_h = 0.75;
    let parapet = prim(
        solid(cylinder_tapered(
            LANTERN_R + 0.12,
            parapet_h,
            PANES as u32 * 2,
            0.0,
            enamel(BUOY_RED),
        )),
        [x, deck_top + parapet_h * 0.5, z],
        id_quat(),
    );
    let sill = deck_top + parapet_h;
    let mut parts = Vec::new();
    let pane_w = 2.0 * LANTERN_R * (PI / PANES as f32).tan();
    let corner_r = LANTERN_R / (PI / PANES as f32).cos();
    for i in 0..PANES {
        let a = (i as f32 + 0.5) * TAU / PANES as f32;
        parts.push(prim(
            plane([pane_w + 0.04, GLAZE_H], pane_grid(GLASS_AQUA, 0.0, (2, 3))),
            [
                x + LANTERN_R * a.sin(),
                sill + GLAZE_H * 0.5,
                z + LANTERN_R * a.cos(),
            ],
            facing_out(a),
        ));
        let c = i as f32 * TAU / PANES as f32;
        parts.push(prim(
            cylinder_tapered(0.05, GLAZE_H + 0.1, 6, 0.0, paint(BUOY_RED)),
            [
                x + corner_r * c.sin(),
                sill + GLAZE_H * 0.5,
                z + corner_r * c.cos(),
            ],
            id_quat(),
        ));
    }
    // The lamp: a pedestal and the lit lens drum ringed by its prisms.
    parts.push(prim(
        cylinder_tapered(0.22, 0.5, 10, 0.0, paint(DRIFT_GREY)),
        [x, sill + 0.25, z],
        id_quat(),
    ));
    let lens_y = sill + 0.95;
    parts.push(prim(
        cylinder_tapered(0.5, 1.0, 16, 0.0, glow([1.0, 0.80, 0.42], 4.2)),
        [x, lens_y, z],
        id_quat(),
    ));
    for dy in [-0.3_f32, 0.0, 0.3] {
        parts.push(prim(
            torus(0.04, 0.52, paint(DRIFT_GREY)),
            [x, lens_y + dy, z],
            id_quat(),
        ));
    }
    // Cornice, dome, ventilator and rod.
    let roof = sill + GLAZE_H;
    parts.push(prim(
        solid(cylinder_tapered(
            corner_r + 0.12,
            0.2,
            PANES as u32 * 2,
            0.0,
            enamel(BUOY_RED),
        )),
        [x, roof + 0.1, z],
        id_quat(),
    ));
    parts.push(prim(
        solid(lathe(
            &[
                (corner_r + 0.02, 0.0),
                (1.35, 0.45),
                (0.95, 0.9),
                (0.45, 1.15),
                (0.0, 1.22),
            ],
            24,
            true,
            enamel(BUOY_RED),
        )),
        [x, roof + 0.18, z],
        id_quat(),
    ));
    let vent_y = roof + 0.18 + 1.22 + 0.16;
    parts.push(prim(
        sphere(0.22, 6, paint(DRIFT_GREY)),
        [x, vent_y, z],
        id_quat(),
    ));
    parts.push(prim(
        cylinder_tapered(0.025, 1.3, 5, 0.3, paint(DRIFT_GREY)),
        [x, vent_y + 0.8, z],
        id_quat(),
    ));
    nest(parapet, parts)
}

/// The tower: its stone plinth, the banded courses, a door on the -Z side
/// and windows climbing it, the bracketed gallery with its rail, and the
/// lantern room on top.
fn tower() -> Generator {
    let [x, z] = TOWER_AT;
    let plinth = prim(
        solid(cylinder_tapered(
            PLINTH_R,
            PLINTH_H + 0.3,
            24,
            0.04,
            concrete(PILING_GREY),
        )),
        [x, PAD + PLINTH_H * 0.5 - 0.15, z],
        id_quat(),
    );
    let mut y = PAD + PLINTH_H;
    let mut r = BASE_R;
    let mut courses = Vec::new();
    for &(h, red) in &COURSES {
        let mat = if red {
            enamel(BUOY_RED)
        } else {
            stucco(STUCCO_WHITE)
        };
        courses.push(prim(
            solid(cylinder_tapered(r, h, 28, TAPER, mat)),
            [x, y + h * 0.5, z],
            id_quat(),
        ));
        y += h;
        r *= 1.0 - TAPER;
    }
    let top = y;
    let top_r = r;
    // The radius of the wall at height `at` above the plinth.
    let wall_r = |at: f32| {
        let mut base = PAD + PLINTH_H;
        let mut r = BASE_R;
        for &(h, _) in &COURSES {
            if at <= base + h {
                return r * (1.0 - TAPER * (at - base) / h);
            }
            base += h;
            r *= 1.0 - TAPER;
        }
        r
    };
    let mut parts = courses;
    // The door, on the -Z side under a little canopy.
    let door_a = PI;
    let door_r = wall_r(PAD + PLINTH_H + 1.1);
    parts.push(prim(
        cuboid_tapered([1.0, 2.1, 0.12], 0.0, plank(AWNING_TEAL)),
        [x, PAD + PLINTH_H + 1.05, z - door_r - 0.02],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([1.5, 0.1, 0.8], 0.0, enamel(BUOY_RED)),
        [x, PAD + PLINTH_H + 2.4, z - door_r - 0.35],
        quat_x(0.15),
    ));
    // Windows climbing the tower, one turned a little further round each.
    for (k, at) in [5.6_f32, 9.6, 13.4, 16.6].into_iter().enumerate() {
        let a = door_a + 0.5 * if k % 2 == 0 { -1.0 } else { 1.0 } * (k as f32 * 0.4 + 0.3);
        parts.extend(tower_window(a, at, wall_r(at)));
    }
    // Gallery: the deck on eight brackets, its rail of two rings on
    // balusters.
    let deck_h = 0.22;
    let deck_top = top + deck_h;
    parts.push(prim(
        solid(cylinder_tapered(
            GALLERY_R,
            deck_h,
            28,
            0.0,
            enamel(DRIFT_GREY),
        )),
        [x, top + deck_h * 0.5, z],
        id_quat(),
    ));
    let mut rail = Vec::new();
    for i in 0..8 {
        let a = i as f32 * TAU / 8.0 + 0.2;
        rail.push(strut(
            [
                x + (top_r - 0.08) * a.sin(),
                top - 0.9,
                z + (top_r - 0.08) * a.cos(),
            ],
            [
                x + (GALLERY_R - 0.25) * a.sin(),
                top + 0.02,
                z + (GALLERY_R - 0.25) * a.cos(),
            ],
            0.06,
            6,
            paint(DRIFT_GREY),
        ));
    }
    let rail_r = GALLERY_R - 0.08;
    for i in 0..18 {
        let a = i as f32 * TAU / 18.0;
        rail.push(prim(
            cylinder_tapered(0.03, 1.0, 5, 0.0, paint(AWNING_WHITE)),
            [x + rail_r * a.sin(), deck_top + 0.5, z + rail_r * a.cos()],
            id_quat(),
        ));
    }
    for dy in [0.45_f32, 0.98] {
        rail.push(prim(
            torus(0.04, rail_r, paint(AWNING_WHITE)),
            [x, deck_top + dy, z],
            id_quat(),
        ));
    }
    parts.extend(rail);
    parts.push(lantern(deck_top));
    nest(plinth, parts)
}

/// The keeper's cottage: whitewashed walls, a shingled gable roof with a
/// chimney, a teal door and two shuttered windows lit from within.
fn cottage() -> Generator {
    let [x, z] = COTTAGE_AT;
    let [w, h, d] = COTTAGE;
    let front = z - d * 0.5;
    let body = prim(
        solid(cuboid_tapered(COTTAGE, 0.0, stucco(STUCCO_WHITE))),
        [x, PAD + h * 0.5, z],
        id_quat(),
    );
    let mut parts = vec![
        prim(
            solid(cuboid_tapered_xz(
                [w + 0.6, 1.7, d + 0.7],
                [0.0, 0.99],
                shingle(DRIFT_GREY),
            )),
            [x, PAD + h + 0.85, z],
            id_quat(),
        ),
        prim(
            solid(cuboid_tapered([0.6, 1.6, 0.6], 0.0, stucco(STUCCO_WHITE))),
            [x - w * 0.3, PAD + h + 1.3, z + 0.7],
            id_quat(),
        ),
        prim(
            cuboid_tapered([1.0, 2.0, 0.1], 0.0, plank(AWNING_TEAL)),
            [x + 0.4, PAD + 1.0, front - 0.05],
            id_quat(),
        ),
    ];
    for wx in [x - 1.6, x + 2.0] {
        let wy = PAD + 1.55;
        parts.push(prim(
            cuboid_tapered(
                [0.9, 1.0, 0.04],
                0.0,
                lit_from_dusk(lit_interior([0.80, 0.62, 0.38], 0.9)),
            ),
            [wx, wy, front - 0.025],
            id_quat(),
        ));
        parts.push(prim(
            plane([0.94, 1.04], pane_grid(GLASS_AQUA, 0.0, (2, 2))),
            [wx, wy, front - 0.07],
            quat_x(-FRAC_PI_2),
        ));
        for side in [-1.0_f32, 1.0] {
            parts.push(prim(
                cuboid_tapered([0.42, 1.1, 0.08], 0.0, plank(AWNING_TEAL)),
                [wx + side * 0.7, wy, front - 0.035],
                id_quat(),
            ));
        }
    }
    nest(body, parts)
}

/// The viewing terrace: its deck, a railing round its three open sides, the
/// brass telescope on its tripod, a bench and a striped parasol.
fn terrace() -> Generator {
    let [x, z] = TERRACE_AT;
    let [w, t, d] = TERRACE;
    let top = PAD + t;
    let deck = prim(
        solid(cuboid_tapered(TERRACE, 0.0, plank(DECK_PALE))),
        [x, PAD + t * 0.5, z],
        id_quat(),
    );
    let (x0, x1) = (x - w * 0.5 + 0.08, x + w * 0.5 - 0.08);
    let (z0, z1) = (z - d * 0.5 + 0.08, z + d * 0.5 - 0.3);
    let white = paint(AWNING_WHITE);
    let mut parts = Vec::new();
    parts.extend(railing(
        [x0, top, z0],
        [x1, top, z0],
        1.0,
        BALUSTER_PITCH,
        white.clone(),
    ));
    parts.extend(railing(
        [x0, top, z0 + 0.11],
        [x0, top, z1],
        1.0,
        BALUSTER_PITCH,
        white.clone(),
    ));
    parts.extend(railing(
        [x1, top, z0 + 0.11],
        [x1, top, z1],
        1.0,
        BALUSTER_PITCH,
        white,
    ));
    // The telescope: a tripod, its head, and the brass tube aimed out to
    // sea a little above the horizon.
    let brass = steel([0.74, 0.56, 0.26]);
    let (tx, tz) = (x - 1.6, z0 + 1.0);
    let head = [tx, top + 1.25, tz];
    for k in 0..3 {
        let a = k as f32 * TAU / 3.0;
        parts.push(strut(
            [tx + 0.45 * a.sin(), top, tz + 0.45 * a.cos()],
            [head[0], head[1] - 0.05, head[2]],
            0.025,
            5,
            paint(DRIFT_GREY),
        ));
    }
    parts.push(prim(
        cuboid_tapered([0.16, 0.16, 0.2], 0.0, paint(DRIFT_GREY)),
        head,
        id_quat(),
    ));
    let up = 0.17_f32;
    let aim = quat_x(-(FRAC_PI_2 - up));
    let tube_len = 1.2;
    let tube_c = [tx, head[1] + 0.14, tz - 0.1];
    parts.push(prim(
        cylinder_tapered(0.08, tube_len, 10, -0.25, brass.clone()),
        tube_c,
        aim,
    ));
    parts.push(prim(
        cylinder_tapered(0.03, 0.2, 6, 0.0, brass),
        [
            tx,
            tube_c[1] - (tube_len * 0.5 + 0.08) * up.sin(),
            tube_c[2] + (tube_len * 0.5 + 0.08) * up.cos(),
        ],
        aim,
    ));
    // A bench along the front rail.
    let (bx, bz) = (x + 2.0, z0 + 0.75);
    parts.push(nest(
        prim(
            solid(cuboid_tapered([1.8, 0.08, 0.45], 0.0, plank(DECK_WOOD))),
            [bx, top + 0.45, bz],
            id_quat(),
        ),
        vec![
            prim(
                cuboid_tapered([0.08, 0.41, 0.4], 0.0, paint(DRIFT_GREY)),
                [bx - 0.75, top + 0.205, bz],
                id_quat(),
            ),
            prim(
                cuboid_tapered([0.08, 0.41, 0.4], 0.0, paint(DRIFT_GREY)),
                [bx + 0.75, top + 0.205, bz],
                id_quat(),
            ),
            prim(
                cuboid_tapered([1.8, 0.4, 0.06], 0.0, plank(DECK_WOOD)),
                [bx, top + 0.68, bz + 0.2],
                quat_x(0.15),
            ),
        ],
    ));
    // The parasol by the bench.
    let (px, pz) = (x + 3.4, z + 0.6);
    parts.push(nest(
        prim(
            cylinder_tapered(0.04, 2.3, 6, 0.0, paint(DRIFT_GREY)),
            [px, top + 1.15, pz],
            id_quat(),
        ),
        vec![prim(
            cone(1.3, 0.55, 16, canvas(AWNING_RED, AWNING_WHITE)),
            [px, top + 2.2, pz],
            id_quat(),
        )],
    ));
    nest(deck, parts)
}

fn build_tree() -> Generator {
    let mut root = footing(12.4, 12.2, [-0.8, -0.4], CLEARANCE);
    root.audio = fx::sea_breeze();
    // The whitewashed platform the headland is levelled to.
    attach(
        &mut root,
        prim(
            solid(cuboid_tapered(
                [12.6, 0.4, 12.4],
                0.0,
                concrete(PILING_GREY),
            )),
            [-0.8, PAD - 0.2, -0.4],
            id_quat(),
        ),
    );
    attach(&mut root, tower());
    attach(&mut root, cottage());
    attach(&mut root, terrace());
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, has_emissive};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&ClifftopLighthouse.build(""), "clifftop_lighthouse");
    }

    #[test]
    fn nothing_floats_or_z_fights() {
        let g = ClifftopLighthouse.build("");
        assert_nothing_floats(&g, "clifftop_lighthouse");
        assert_no_z_fighting(&g, "clifftop_lighthouse");
    }

    /// The lamp is lit at every hour; the windows only from dusk.
    #[test]
    fn keeps_its_lamp() {
        assert!(has_emissive(&ClifftopLighthouse.build("")));
    }

    #[test]
    fn the_record_stays_light() {
        let bytes = serde_json::to_vec(&ClifftopLighthouse.build(""))
            .unwrap()
            .len();
        eprintln!("clifftop_lighthouse record: {bytes} B");
        assert!(bytes < 60_000, "{bytes} B");
    }
}
