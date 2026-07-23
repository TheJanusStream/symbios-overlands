//! Community center — the Suburban landmark. A long single-storey civic hall
//! with a brick base and rendered walls under a low shingle roof, fronted by
//! a white-columned portico and a lit sign, with a flag pole and foundation
//! shrubs on the lawn. Birdsong drifts over it and a sprinkler mists the
//! grass. The modest civic heart of the neighbourhood.

use std::f32::consts::FRAC_PI_2;

use crate::catalogue::items::civic_campus::column;
use crate::catalogue::items::roadside::{SIGN_AMBER, sign_board};
use crate::catalogue::items::solarpunk::{crop_tufts, foliage};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, glow, id_quat, plane, prim, quat_x, solid, sphere,
    window_card,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::{
    BRICK_TAN, GLASS_TINT, HEDGE_GREEN, RENDER_WHITE, ROOF_GREY, SIDING_BLUE, SIGN_GLOW,
    WOOD_WHITE, brick, enamel, fx, render, shingle, wood,
};

/// Warm hall light — the ceiling glow that reads through the cut window panes
/// as an occupied civic room behind the glass.
const CIVIC_WARM: [f32; 3] = [1.0, 0.90, 0.66];

pub struct CommunityCenter;

impl CatalogueEntry for CommunityCenter {
    fn slug(&self) -> &'static str {
        "community_center"
    }
    fn name(&self) -> &'static str {
        "Community Center"
    }
    fn description(&self) -> &'static str {
        "Low civic hall with a columned portico, lit sign, flag pole, and lawn."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Landmark
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 12.0,
            min_spawn_dist: 45.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let l = 18.0_f32;
    let w = 12.0_f32;
    let base_h = 0.5;
    let brick_h = 1.4;
    let wall_h = 3.6;
    let wall_top = base_h + brick_h + wall_h;
    // Hero face on the -Z front (the render's lead tile): the portico, window
    // band, and lit sign read straight on instead of hiding round the back.
    let front = -w * 0.5;

    let brick_top = base_h + brick_h;
    let wall_cy = brick_top + wall_h * 0.5;
    // The building is a hollow shell: solid rear and side walls, a flat lit
    // ceiling, and a punched front screen of piers/sills/header. Behind the
    // front windows is the whole depth of the hall — floor, downlights, and a
    // dais with a glowing civic emblem at the far wall — so the cut panes look
    // *into* a room instead of onto a wall a metre back (#943).
    let face_z = front + 0.2; // the front-wall (street) plane
    let back_z = w * 0.5 - 0.4; // interior face of the rear wall
    let wall_len = l - 0.4;
    let side_x = wall_len * 0.5 - 0.2; // centreline of the side walls

    let mut prims = vec![
        // Concrete footing — the root.
        prim(
            solid(cuboid_tapered(
                [l + 1.0, base_h, w + 1.0],
                0.0,
                render([0.6, 0.6, 0.6]),
            )),
            [0.0, base_h * 0.5, 0.0],
            id_quat(),
        ),
        // Brick base course (the hall floor sits on its top).
        prim(
            solid(cuboid_tapered([l, brick_h, w], 0.0, brick(BRICK_TAN))),
            [0.0, base_h + brick_h * 0.5, 0.0],
            id_quat(),
        ),
        // Rear wall.
        prim(
            solid(cuboid_tapered(
                [wall_len, wall_h, 0.4],
                0.0,
                render(RENDER_WHITE),
            )),
            [0.0, wall_cy, back_z],
            id_quat(),
        ),
        // Flat ceiling closing the hall under the roof.
        prim(
            solid(cuboid_tapered(
                [wall_len, 0.25, w - 0.4],
                0.0,
                render(RENDER_WHITE),
            )),
            [0.0, wall_top - 0.15, 0.0],
            id_quat(),
        ),
        // Warm hall floor, lit, receding to the far wall.
        prim(
            solid(cuboid_tapered(
                [wall_len - 0.4, 0.15, w - 0.8],
                0.0,
                render([0.68, 0.63, 0.55]),
            )),
            [0.0, brick_top + 0.08, 0.1],
            id_quat(),
        ),
    ];
    // Side walls.
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.4, wall_h, w - 0.4],
                0.0,
                render(RENDER_WHITE),
            )),
            [sx * side_x, wall_cy, 0.1],
            id_quat(),
        ));
    }

    // Interior focal point: a low dais and a glowing civic emblem on the rear
    // wall, so the hall reads with depth through the windows.
    prims.push(prim(
        solid(cuboid_tapered(
            [8.0, 0.4, 1.8],
            0.0,
            wood([0.46, 0.31, 0.18]),
        )),
        [0.0, brick_top + 0.2, back_z - 1.1],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([3.2, 2.0, 0.1], 0.0, glow([0.36, 0.55, 0.9], 2.0)),
        [0.0, brick_top + 2.4, back_z - 0.1],
        id_quat(),
    ));
    // Ceiling downlights spanning the hall.
    for &z in &[-2.5_f32, 0.5, 3.5] {
        prims.push(prim(
            cuboid_tapered([wall_len - 2.5, 0.12, 0.22], 0.0, glow(CIVIC_WARM, 2.2)),
            [0.0, wall_top - 0.4, z],
            id_quat(),
        ));
    }

    // --- Punched front wall: four windows flanking a central entrance bay.

    let win_x = [-6.6_f32, -3.3, 3.3, 6.6];
    let win_half = 0.8;
    let sill_y = brick_top + 0.4;
    let head_y = brick_top + wall_h - 0.5; // top of the window / door openings
    let win_cy = (sill_y + head_y) * 0.5;
    let win_h = head_y - sill_y;
    let door_half = 1.2;

    // Header band spanning the whole face, above every opening.
    prims.push(prim(
        solid(cuboid_tapered(
            [wall_len, brick_top + wall_h - head_y, 0.35],
            0.0,
            render(RENDER_WHITE),
        )),
        [0.0, (head_y + brick_top + wall_h) * 0.5, face_z],
        id_quat(),
    ));
    // Piers between the openings (and at the two ends). They rise only to the
    // opening head so the full-width header stacks on top without a coplanar
    // overlap. `edges` alternates wall-end / opening boundaries; its even
    // chunks are the piers.
    let mut edges = vec![-wall_len * 0.5];
    for (i, &x) in win_x.iter().enumerate() {
        edges.push(x - win_half);
        edges.push(x + win_half);
        if i == 1 {
            edges.push(-door_half);
            edges.push(door_half);
        }
    }
    edges.push(wall_len * 0.5);
    let pier_h = head_y - brick_top;
    let pier_cy = (brick_top + head_y) * 0.5;
    for pair in edges.chunks(2) {
        let [a, b] = [pair[0], pair[1]];
        let pw = b - a;
        if pw > 0.01 {
            prims.push(prim(
                solid(cuboid_tapered(
                    [pw, pier_h, 0.35],
                    0.0,
                    render(RENDER_WHITE),
                )),
                [(a + b) * 0.5, pier_cy, face_z],
                id_quat(),
            ));
        }
    }
    // Sills under the four windows (the entrance bay runs to the floor).
    for &x in &win_x {
        prims.push(prim(
            solid(cuboid_tapered(
                [win_half * 2.0, sill_y - brick_top, 0.35],
                0.0,
                render(RENDER_WHITE),
            )),
            [x, (brick_top + sill_y) * 0.5, face_z],
            id_quat(),
        ));
    }
    // Window glazing — clear panes on planes, cut open over the lit hall.
    for &x in &win_x {
        prims.push(prim(
            plane(
                [win_half * 2.0, win_h],
                window_card(GLASS_TINT, 2, 3, 0.3, 0.04),
            ),
            [x, win_cy, face_z - 0.02],
            quat_x(-FRAC_PI_2),
        ));
    }

    // --- A proper entrance: an approach stair up to a recessed doorway with
    //     double doors and a glazed transom, set back in the central bay.

    // Entrance apron extending the plinth forward to carry the stair and the
    // portico columns.
    prims.push(prim(
        solid(cuboid_tapered(
            [5.6, base_h, 3.6],
            0.0,
            render([0.6, 0.6, 0.6]),
        )),
        [0.0, base_h * 0.5, front - 1.6],
        id_quat(),
    ));
    // Three approach steps rising from the apron to the hall floor.
    let steps = 3;
    let rise = (brick_top - base_h) / steps as f32;
    let depth = 0.5;
    for i in 0..steps {
        prims.push(prim(
            solid(cuboid_tapered(
                [4.6 - i as f32 * 0.3, rise, depth],
                0.0,
                render([0.72, 0.70, 0.66]),
            )),
            [
                0.0,
                base_h + rise * (i as f32 + 0.5),
                face_z - 0.3 - (steps - 1 - i) as f32 * (depth - 0.08),
            ],
            id_quat(),
        ));
    }
    // Reveal walls lining the recessed doorway.
    let door_z = face_z + 1.2; // doors set back into the bay
    let door_cy = brick_top + (head_y - brick_top) * 0.5;
    let door_h = head_y - brick_top;
    for sx in [-1.0_f32, 1.0] {
        prims.push(prim(
            solid(cuboid_tapered(
                [0.15, door_h, 1.2],
                0.0,
                render(RENDER_WHITE),
            )),
            [sx * (door_half - 0.02), door_cy, face_z + 0.6],
            id_quat(),
        ));
    }
    // Double glazed doors, recessed, with a slim mullion between the leaves.
    let leaf_h = door_h - 0.6;
    prims.push(prim(
        plane(
            [door_half * 2.0 - 0.2, leaf_h],
            window_card([0.28, 0.34, 0.40], 2, 3, 0.34, 0.06),
        ),
        [0.0, brick_top + leaf_h * 0.5, door_z],
        quat_x(-FRAC_PI_2),
    ));
    prims.push(prim(
        solid(cuboid_tapered([0.1, leaf_h, 0.12], 0.0, wood(WOOD_WHITE))),
        [0.0, brick_top + leaf_h * 0.5, door_z - 0.1],
        id_quat(),
    ));
    // Glazed transom over the doors.
    prims.push(prim(
        plane(
            [door_half * 2.0 - 0.2, 0.5],
            window_card(GLASS_TINT, 3, 1, 0.3, 0.05),
        ),
        [0.0, brick_top + leaf_h + 0.3, door_z],
        quat_x(-FRAC_PI_2),
    ));

    // Low shingle hip roof.
    prims.push(prim(
        solid(cuboid_tapered(
            [l + 2.0, 2.4, w + 2.0],
            0.45,
            shingle(ROOF_GREY),
        )),
        [0.0, wall_top + 1.2, 0.0],
        id_quat(),
    ));

    // Entrance portico: four classical columns and an entablature beam.
    for x in [-4.0_f32, -1.4, 1.4, 4.0] {
        prims.extend(column(
            x,
            front - 2.2,
            base_h,
            wall_h + brick_h,
            0.3,
            wood(WOOD_WHITE),
        ));
    }
    prims.push(prim(
        solid(cuboid_tapered([9.4, 0.6, 3.0], 0.0, wood(WOOD_WHITE))),
        [0.0, base_h + wall_h + brick_h + 0.3, front - 2.0],
        id_quat(),
    ));
    // Lit sign over the entrance — segmented so it reads lit, not washed.
    prims.extend(sign_board(
        [0.0, base_h + brick_h + 2.6, front - 0.12],
        [6.0, 0.9],
        (4, 1),
        SIGN_AMBER,
        2.4,
        -1.0,
    ));

    // Flag pole with a finial and a small flag.
    let pole_x = -l * 0.5 - 1.5;
    prims.push(prim(
        solid(cylinder_tapered(0.1, 8.0, 8, 0.1, enamel([0.8, 0.8, 0.82]))),
        [pole_x, 4.0, front],
        id_quat(),
    ));
    prims.push(prim(
        solid(sphere(0.18, 3, glow(SIGN_GLOW, 2.0))),
        [pole_x, 8.1, front],
        id_quat(),
    ));
    prims.push(prim(
        cuboid_tapered([0.05, 0.8, 1.3], 0.0, enamel(SIDING_BLUE)),
        [pole_x, 7.2, front - 0.7],
        id_quat(),
    ));

    // Clipped foundation shrubs along the front lawn — leafy clumps, not slabs.
    prims.extend(crop_tufts(
        [-1.0, base_h, front - 0.9],
        [l * 0.7, 1.2],
        6,
        1,
        1.0,
        foliage(HEDGE_GREEN),
    ));

    let mut root = assemble(prims);
    // Signature life: birdsong over the lawn and a sprinkler misting it.
    root.audio = fx::birdsong();
    root.children
        .push(fx::sprinkler_mist([6.0, 0.4, front - 5.0], 0x5B19_DA11));
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&CommunityCenter.build(""), "community_center");
    }

    #[test]
    fn has_lit_sign() {
        assert!(crate::catalogue::items::util::has_emissive(
            &CommunityCenter.build("")
        ));
    }

    /// #943: the built generator survives a serde round-trip unchanged. The
    /// glazing cards set `glass_opacity` to the mirror default (0.30); if
    /// that value is off the fixed-point grid it fails the record's own
    /// equality check after a round-trip (the `window_card` fix snaps it on).
    #[test]
    fn build_round_trips_through_serde() {
        let g = CommunityCenter.build("");
        let back: Generator = serde_json::from_str(&serde_json::to_string(&g).unwrap()).unwrap();
        assert!(
            !crate::state::records_differ(&g, &back),
            "community_center must survive a serde round-trip"
        );
    }
}
