//! Trailer home — the Suburban *poor* landmark. A single-wide mobile home on
//! cinder-block supports with a shallow metal roof, a window AC unit, and a
//! little entry step. The trailer-lot counterpart to the
//! [`community_center`](super::community_center): same theme, opposite end of
//! the prosperity axis (`Poor`), so a destitute room grows this instead of
//! the civic hall.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, plane, prim, quat_x, solid,
    window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{GLASS_TINT, SIDING_BLUE, TRAILER_WHITE, enamel, render, siding};

/// Warm lamp light inside the trailer — the glow that shows through the cut
/// window panes as a lived-in room rather than a dark box.
const LAMP_WARM: [f32; 3] = [1.0, 0.84, 0.56];

pub struct TrailerHome;

impl CatalogueEntry for TrailerHome {
    fn slug(&self) -> &'static str {
        "trailer_home"
    }
    fn name(&self) -> &'static str {
        "Trailer Home"
    }
    fn description(&self) -> &'static str {
        "Single-wide mobile home on cinder blocks with a metal roof and AC unit."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_POOR
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 9.0,
            min_spawn_dist: 40.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 11.0_f32;
    let d = 4.0_f32;
    let floor_y = 0.7_f32;
    let body_h = 2.6_f32;
    // Hero face (windows, door, AC, awning) on the -Z front.
    let front = -d * 0.5;

    let mut prims = vec![
        // Concrete pad — the root.
        prim(
            solid(cuboid_tapered(
                [l + 0.6, 0.3, d + 0.6],
                0.0,
                render([0.5, 0.5, 0.51]),
            )),
            [0.0, 0.15, 0.0],
            id_quat(),
        ),
    ];

    // Cinder-block supports.
    for sx in [-1.0_f32, -0.33, 0.33, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered(
                    [0.5, floor_y - 0.3, 0.5],
                    0.0,
                    render([0.45, 0.45, 0.46]),
                )),
                [sx * l * 0.45, 0.3 + (floor_y - 0.3) * 0.5, sz * d * 0.35],
                id_quat(),
            ));
        }
    }
    // Vinyl skirting hiding the under-trailer voids; the front run is a hair
    // short (one panel sagging off) — weathered but intact.
    for (sz, frac) in [(-1.0_f32, 0.96_f32), (1.0, 1.0)] {
        prims.push(prim(
            solid(cuboid_tapered(
                [l * frac, floor_y - 0.32, 0.08],
                0.0,
                render([0.56, 0.55, 0.5]),
            )),
            [0.0, 0.3 + (floor_y - 0.32) * 0.5, sz * (d * 0.5 + 0.02)],
            id_quat(),
        ));
    }

    // --- Hollow body: a shell of siding, so the front windows show the lit
    //     room inside rather than a slab of glass on a solid wall (#944).

    let body_top = floor_y + body_h;
    let back_z = d * 0.5 - 0.075; // interior face of the rear wall
    // Rear wall, side walls, and ceiling.
    prims.push(prim(
        solid(cuboid_tapered(
            [l, body_h, 0.15],
            0.0,
            siding(TRAILER_WHITE),
        )),
        [0.0, floor_y + body_h * 0.5, back_z],
        id_quat(),
    ));
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.15, body_h, d],
                0.0,
                siding(TRAILER_WHITE),
            )),
            [sx * (l * 0.5 - 0.075), floor_y + body_h * 0.5, 0.0],
            id_quat(),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([l, 0.15, d], 0.0, siding(TRAILER_WHITE))),
        [0.0, body_top - 0.075, 0.0],
        id_quat(),
    ));
    // Interior: a warm floor, a pale back liner, a couch and a ceiling lamp,
    // so the cut panes look into a small lived-in room with real depth.
    prims.push(prim(
        solid(cuboid_tapered(
            [l - 0.3, 0.1, d - 0.3],
            0.0,
            render([0.66, 0.60, 0.5]),
        )),
        [0.0, floor_y + 0.05, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [l - 0.3, body_h - 0.3, 0.05],
            0.0,
            render([0.82, 0.80, 0.76]),
        )),
        [0.0, floor_y + body_h * 0.5, back_z - 0.05],
        id_quat(),
    ));
    prims.push(prim(
        solid(cuboid_tapered(
            [2.4, 0.6, 0.8],
            0.0,
            enamel([0.42, 0.36, 0.44]),
        )),
        [-1.6, floor_y + 0.35, back_z - 0.55],
        id_quat(),
    ));
    // Ceiling lamp + a warm wash up the back wall, so the room glows through
    // the windows rather than reading as a dark box.
    prims.push(prim(
        cuboid_tapered([l - 3.0, 0.14, 0.6], 0.0, glow(LAMP_WARM, 2.8)),
        [0.0, body_top - 0.25, 0.1],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([l - 1.2, 1.4, 0.05], 0.0, glow([1.0, 0.9, 0.7], 1.2)),
        [0.0, floor_y + 1.4, back_z - 0.12],
        id_quat(),
    ));

    // Accent stripe.
    prims.push(prim(
        cuboid_tapered([l + 0.05, 0.3, d + 0.05], 0.0, enamel(SIDING_BLUE)),
        [0.0, floor_y + body_h * 0.6, 0.0],
        id_quat(),
    ));
    // Shallow metal roof — dulled, weathered but intact.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 0.5, 0.4, d + 0.5],
            0.1,
            enamel([0.64, 0.63, 0.59]),
        )),
        [0.0, floor_y + body_h + 0.2, 0.0],
        id_quat(),
    ));

    // --- Punched front wall (-Z): two sliding windows and an entry door.

    let win = [(-3.5_f32, 0.8_f32), (0.3, 1.0)]; // (centre-x, half-width)
    let door_x = 3.6_f32;
    let door_half = 0.45;
    let sill_y = floor_y + 0.8;
    let head_y = floor_y + 2.05;

    // Header band across the top of every opening.
    prims.push(prim(
        solid(cuboid_tapered(
            [l, body_top - head_y, 0.15],
            0.0,
            siding(TRAILER_WHITE),
        )),
        [0.0, (head_y + body_top) * 0.5, front],
        id_quat(),
    ));
    // Piers between the openings and at the ends (the even chunks of `edges`).
    let mut edges = vec![-l * 0.5];
    for &(x, half) in &win {
        edges.push(x - half);
        edges.push(x + half);
    }
    edges.push(door_x - door_half);
    edges.push(door_x + door_half);
    edges.push(l * 0.5);
    let pier_cy = (floor_y + head_y) * 0.5;
    let pier_h = head_y - floor_y;
    for pair in edges.chunks(2) {
        let [a, b] = [pair[0], pair[1]];
        if b - a > 0.01 {
            prims.push(prim(
                solid(cuboid_tapered(
                    [b - a, pier_h, 0.15],
                    0.0,
                    siding(TRAILER_WHITE),
                )),
                [(a + b) * 0.5, pier_cy, front],
                id_quat(),
            ));
        }
    }
    // Sills under the windows (the door runs to the floor).
    for &(x, half) in &win {
        prims.push(prim(
            solid(cuboid_tapered(
                [half * 2.0, sill_y - floor_y, 0.15],
                0.0,
                siding(TRAILER_WHITE),
            )),
            [x, (floor_y + sill_y) * 0.5, front],
            id_quat(),
        ));
    }
    // Sliding-window glazing — clear panes on planes, cut open over the room.
    for &(x, half) in &win {
        prims.push(prim(
            plane(
                [half * 2.0, head_y - sill_y],
                window_card(GLASS_TINT, 3, 1, 0.3, 0.05),
            ),
            [x, (sill_y + head_y) * 0.5, front - 0.02],
            quat_x(-FRAC_PI_2),
        ));
    }

    // Entry door: a solid leaf filling its opening, with a small frosted
    // vision panel near the top.
    prims.push(prim(
        solid(cuboid_tapered(
            [door_half * 2.0, head_y - floor_y, 0.1],
            0.0,
            enamel([0.7, 0.68, 0.62]),
        )),
        [door_x, (floor_y + head_y) * 0.5, front - 0.02],
        id_quat(),
    ));
    prims.push(prim(
        plane([0.5, 0.55], window_card([0.6, 0.66, 0.7], 1, 1, 0.7, 0.14)),
        [door_x, head_y - 0.45, front - 0.09],
        quat_x(-FRAC_PI_2),
    ));
    // Concrete entry step below the door.
    prims.push(prim(
        solid(cuboid_tapered(
            [1.1, floor_y, 0.7],
            0.0,
            render([0.5, 0.5, 0.5]),
        )),
        [door_x, floor_y * 0.5, front - 0.5],
        id_quat(),
    ));
    // Small flat awning over the door on two diagonal brackets.
    prims.push(prim(
        solid(cuboid_tapered([1.5, 0.08, 0.9], 0.0, enamel(SIDING_BLUE))),
        [door_x, floor_y + 2.05, front - 0.45],
        id_quat(),
    ));
    for bx in [-0.6_f32, 0.6] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.06, 0.06, 0.9],
                0.0,
                enamel([0.55, 0.55, 0.55]),
            )),
            [door_x + bx, floor_y + 1.9, front - 0.45],
            quat_x(0.5),
        ));
    }
    // Window AC unit.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.8, 0.6, 0.5],
            0.0,
            enamel([0.78, 0.78, 0.78]),
        )),
        [-l * 0.3, floor_y + 1.3, front - 0.3],
        id_quat(),
    ));

    // Propane tank on a low stand at the end.
    prims.push(prim(
        solid(cuboid_tapered(
            [0.5, 0.3, 0.6],
            0.0,
            render([0.5, 0.5, 0.5]),
        )),
        [l * 0.5 + 0.7, 0.45, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        solid(cylinder_tapered(
            0.32,
            1.1,
            12,
            0.0,
            enamel([0.86, 0.86, 0.82]),
        )),
        [l * 0.5 + 0.7, 0.85, 0.0],
        quat_x(FRAC_PI_2),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TrailerHome.build(""), "trailer_home");
    }

    /// #943/#944: the glazing cards set `glass_opacity` to the mirror default
    /// (0.30); the `window_card` fix snaps it onto the fixed-point grid so a
    /// room embedding this landmark survives its own serde equality check.
    #[test]
    fn build_round_trips_through_serde() {
        let g = TrailerHome.build("");
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "trailer_home must survive a serde round-trip"
        );
    }
}
