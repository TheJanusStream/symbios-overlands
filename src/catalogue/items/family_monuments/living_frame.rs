//! Owner Vine Cradle - a family monument for worlds that grow their own
//! (#1796): Solarpunk and Alien-Organic.
//!
//! A frame nobody built: two stems rise out of a timber planter on a pale
//! kerb, each twined about by a thinner vine, and arch over to meet above a
//! woven board that cradles the room owner's portrait between them. Leaves
//! open along both, a few buds glow at every hour, and a lit bloom opens at
//! the crown of the arch.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::solarpunk::{
    CONCRETE_PALE, DOME_GLOW, LEAF_GREEN, MOSS_GREEN, TIMBER_WARM, concrete, foliage, timber,
};
use crate::catalogue::items::util::{
    cuboid_tapered, footing, glow, id_quat, nest, pfp_panel, prim, quat_z, soil, solid, sphere,
    spine, superellipsoid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

const PANEL: f32 = 1.5;
const PANEL_Y: f32 = 2.75;
/// The woven board's front; the portrait stands just proud of it.
const BOARD_FRONT: f32 = 0.01;
/// Where the stems leave the soil.
const SOIL_Y: f32 = 1.1;
/// The stems' line, through the board's middle.
const STEM_Z: f32 = 0.05;
const STEM: [f32; 3] = [0.30, 0.34, 0.16];
const CLEARANCE: f32 = 2.6;

pub struct LivingFrameMonument;

impl CatalogueEntry for LivingFrameMonument {
    fn slug(&self) -> &'static str {
        "living_frame_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Vine Cradle"
    }
    fn description(&self) -> &'static str {
        "Twined living stems arching over a planter, cradling the owner's portrait."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Monument
    }
    // themes() stays empty: a family monument joins its themes' pools
    // through the kindred tables, never as a theme's own.
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[]
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 8.0,
        }
    }
    fn build(&self, local_did: &str) -> Generator {
        build_tree(local_did)
    }
}

fn build_tree(did: &str) -> Generator {
    let kerb = prim(
        solid(cuboid_tapered(
            [3.0, 0.3, 1.3],
            0.02,
            concrete(CONCRETE_PALE),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );
    let trough = prim(
        solid(cuboid_tapered([2.6, 0.78, 0.9], 0.0, timber(TIMBER_WARM))),
        [0.0, 0.68, 0.0],
        id_quat(),
    );
    let mut in_trough = vec![
        prim(
            cuboid_tapered([2.5, 0.06, 0.8], 0.0, soil()),
            [0.0, SOIL_Y - 0.02, 0.0],
            id_quat(),
        ),
        board(did),
    ];
    for sx in [-1.0_f32, 1.0] {
        in_trough.push(stem(sx));
    }
    in_trough.push(bloom());
    // Low greens along the planter's front.
    for k in 0..5 {
        let x = -0.96 + k as f32 * 0.48;
        in_trough.push(prim(
            superellipsoid(
                [0.24, 0.16 + 0.03 * (k % 2) as f32, 0.18],
                1.0,
                1.0,
                foliage(MOSS_GREEN),
            ),
            [x, SOIL_Y + 0.05, -0.2],
            id_quat(),
        ));
    }
    nest(
        kerb,
        vec![
            nest(trough, in_trough),
            footing(3.0, 1.3, [0.0, 0.0], CLEARANCE),
        ],
    )
}

/// The woven board the portrait is set in, held between the stems.
fn board(did: &str) -> Generator {
    let back = prim(
        solid(cuboid_tapered(
            [PANEL + 0.36, PANEL + 0.3, 0.08],
            0.0,
            timber([0.46, 0.36, 0.22]),
        )),
        [0.0, PANEL_Y, BOARD_FRONT + 0.04],
        id_quat(),
    );
    nest(
        back,
        vec![pfp_panel(did, PANEL, [0.0, PANEL_Y, BOARD_FRONT - 0.03])],
    )
}

/// The centre line of the stem on side `sx`: up out of the soil beside the
/// board, then over to the crown of the arch, each station with its radius.
fn stem_line(sx: f32) -> [([f32; 3], f32); 7] {
    [
        ([sx * 0.98, SOIL_Y - 0.2, STEM_Z], 0.1),
        ([sx * 1.02, 1.8, STEM_Z], 0.09),
        ([sx * 1.0, 2.7, STEM_Z], 0.085),
        ([sx * 0.98, 3.55, STEM_Z], 0.08),
        ([sx * 0.78, 4.1, STEM_Z], 0.07),
        ([sx * 0.4, 4.38, STEM_Z], 0.06),
        ([sx * 0.02, 4.46, STEM_Z], 0.05),
    ]
}

/// One stem with its twining vine, its leaves and its glowing buds.
fn stem(sx: f32) -> Generator {
    let line = stem_line(sx);
    let body = prim(spine(&line, 10, 6, foliage(STEM)), [0.0; 3], id_quat());
    // The vine winds round the stem: stations swung to alternate sides of
    // it, close enough that it touches the stem all the way up.
    let vine: Vec<([f32; 3], f32)> = line
        .iter()
        .enumerate()
        .map(|(i, &(p, r))| {
            let side = if i % 2 == 0 { 1.0 } else { -1.0 };
            (
                [p[0] + sx * 0.03 * side, p[1], p[2] + side * (r + 0.01)],
                0.035,
            )
        })
        .collect();
    let mut parts = vec![prim(
        spine(&vine, 6, 6, foliage(LEAF_GREEN)),
        [0.0; 3],
        id_quat(),
    )];
    // Leaves off the stem's outer side and its face, every station but the
    // root, each tilted a little differently.
    for (i, &(p, r)) in line.iter().enumerate().skip(1) {
        let out = if i % 4 == 1 { -0.4 } else { 1.0 };
        parts.push(prim(
            superellipsoid([0.2, 0.035, 0.12], 1.0, 1.0, foliage(LEAF_GREEN)),
            [p[0] + sx * out * (r + 0.12), p[1], p[2] - 0.1],
            quat_z(sx * (0.3 + 0.1 * i as f32)),
        ));
        if i % 2 == 1 {
            parts.push(prim(
                sphere(0.07, 3, glow(DOME_GLOW, 3.0)),
                [p[0] + sx * 0.02, p[1] + 0.05, p[2] - r - 0.02],
                id_quat(),
            ));
        }
    }
    nest(body, parts)
}

/// The bloom at the crown of the arch: a lit heart and five petals round it,
/// turned to face the approach.
fn bloom() -> Generator {
    let at = [0.0, 4.5, STEM_Z - 0.06];
    let heart = prim(sphere(0.12, 4, glow(DOME_GLOW, 3.0)), at, id_quat());
    let petals = (0..5)
        .map(|k| {
            let a = k as f32 * 72.0_f32.to_radians();
            prim(
                superellipsoid([0.09, 0.2, 0.03], 1.0, 1.0, foliage(PETAL)),
                [at[0] - a.sin() * 0.2, at[1] + a.cos() * 0.2, at[2] + 0.02],
                quat_z(a),
            )
        })
        .collect();
    nest(heart, petals)
}

/// Pale petal cream, so the bloom reads against the leaves.
const PETAL: [f32; 3] = [0.86, 0.78, 0.56];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &LivingFrameMonument.build("did:plc:test"),
            "living_frame_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&LivingFrameMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = LivingFrameMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "living_frame_monument");
        assert_nothing_floats(&built, "living_frame_monument");
    }
}
