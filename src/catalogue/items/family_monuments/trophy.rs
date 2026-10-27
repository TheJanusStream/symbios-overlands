//! Owner Trophy - a family monument for Sports-and-Rec worlds (#1796).
//!
//! The cup the club has won, on show: a dark marble pedestal on a concrete
//! step carries a tall gold cup with a handle either side, and on the
//! pedestal's face hangs a gold medal on a ribbon whose disc is the room
//! owner's portrait. Two uplighters at the foot light the cup at dusk.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::civic::marble;
use crate::catalogue::items::sports_rec::{
    CONCRETE_GREY, FLOOD_LIT, SEAT_BLUE, SEAT_RED, STEEL_GREY, concrete, enamel, steel,
};
use crate::catalogue::items::steampunk::brass;
use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, footing, glow, id_quat, lathe, nest, pfp_panel, prim, quat_x,
    quat_z, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;
use std::f32::consts::FRAC_PI_2;

const PANEL: f32 = 1.36;
/// The medal's centre, and its disc's radius.
const MEDAL_Y: f32 = 1.84;
const MEDAL_R: f32 = 1.05;
/// The pedestal's front face, and the top it carries the cup on.
const PEDESTAL_FRONT: f32 = -0.65;
const PEDESTAL_TOP: f32 = 3.3;
const GOLD: [f32; 3] = [0.86, 0.66, 0.24];
const MARBLE_DARK: [f32; 3] = [0.16, 0.18, 0.24];
const CLEARANCE: f32 = 2.7;

pub struct TrophyMonument;

impl CatalogueEntry for TrophyMonument {
    fn slug(&self) -> &'static str {
        "trophy_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Trophy"
    }
    fn description(&self) -> &'static str {
        "Gold cup on a marble pedestal, the owner's portrait on the medal hung from it."
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
    let step = prim(
        solid(cuboid_tapered(
            [2.9, 0.3, 2.1],
            0.02,
            concrete(CONCRETE_GREY),
        )),
        [0.0, 0.15, 0.0],
        id_quat(),
    );
    let h = PEDESTAL_TOP - 0.29;
    let pedestal = prim(
        solid(cuboid_tapered([2.3, h, 1.3], 0.0, marble(MARBLE_DARK))),
        [0.0, 0.29 + h * 0.5, PEDESTAL_FRONT + 0.65],
        id_quat(),
    );
    let cornice = prim(
        solid(cuboid_tapered([2.5, 0.18, 1.5], 0.08, marble(MARBLE_DARK))),
        [0.0, PEDESTAL_TOP + 0.08, 0.0],
        id_quat(),
    );
    let mut on_pedestal = medal(did);
    on_pedestal.push(nest(cornice, vec![cup(PEDESTAL_TOP + 0.17)]));
    let mut on_step = vec![nest(pedestal, on_pedestal)];
    for sx in [-1.0_f32, 1.0] {
        on_step.push(uplighter(sx * 1.15));
    }
    on_step.push(footing(2.9, 2.1, [0.0, 0.0], CLEARANCE));
    nest(step, on_step)
}

/// The cup standing at `base`: one turned gold body - foot, stem, knop and
/// the flared bowl, closed by a shallow dish at the lip - and a ring handle
/// either side.
fn cup(base: f32) -> Generator {
    let body = prim(
        solid(lathe(
            &[
                (0.56, 0.0),
                (0.5, 0.14),
                (0.16, 0.36),
                (0.14, 0.52),
                (0.24, 0.62),
                (0.13, 0.76),
                (0.3, 0.98),
                (0.58, 1.34),
                (0.7, 1.82),
                (0.76, 1.96),
                (0.0, 1.86),
            ],
            24,
            false,
            brass(GOLD),
        )),
        [0.0, base, 0.0],
        id_quat(),
    );
    let handles = [-1.0_f32, 1.0]
        .map(|sx| {
            prim(
                torus(0.06, 0.34, brass(GOLD)),
                [sx * 0.78, base + 1.4, 0.0],
                quat_x(FRAC_PI_2),
            )
        })
        .to_vec();
    nest(body, handles)
}

/// The medal on the pedestal's face: its gold disc and rim, the portrait,
/// and the ribbon it hangs from, two straps meeting at a gold clasp.
fn medal(did: &str) -> Vec<Generator> {
    let stand = quat_x(FRAC_PI_2);
    let face = PEDESTAL_FRONT - 0.08;
    let mut out = vec![
        prim(
            solid(cylinder_tapered(MEDAL_R, 0.08, 28, 0.0, brass(GOLD))),
            [0.0, MEDAL_Y, PEDESTAL_FRONT - 0.035],
            stand,
        ),
        prim(
            torus(0.05, MEDAL_R - 0.02, brass([0.72, 0.52, 0.18])),
            [0.0, MEDAL_Y, face],
            stand,
        ),
        pfp_panel(did, PANEL, [0.0, MEDAL_Y, face - 0.03]),
        prim(
            solid(cuboid_tapered([0.46, 0.14, 0.07], 0.0, brass(GOLD))),
            [0.0, MEDAL_Y + MEDAL_R + 0.02, PEDESTAL_FRONT - 0.035],
            id_quat(),
        ),
    ];
    // The ribbon's straps, red and blue, splayed up to the cornice; one stands
    // two centimetres prouder so they never cross in one plane.
    for (sx, color, z) in [(-1.0_f32, SEAT_RED, -0.0), (1.0, SEAT_BLUE, -0.02)] {
        out.push(prim(
            solid(cuboid_tapered([0.32, 0.56, 0.03], 0.0, enamel(color))),
            [
                sx * 0.2,
                MEDAL_Y + MEDAL_R + 0.25,
                PEDESTAL_FRONT - 0.01 + z,
            ],
            quat_z(-sx * 0.42),
        ));
    }
    out
}

/// A steel uplighter at the step's front corner, its lens turned to the cup.
fn uplighter(x: f32) -> Generator {
    let housing = prim(
        solid(cuboid_tapered([0.32, 0.2, 0.32], 0.1, steel(STEEL_GREY))),
        [x, 0.39, -0.85],
        id_quat(),
    );
    nest(
        housing,
        vec![prim(
            cuboid_tapered([0.22, 0.03, 0.22], 0.0, glow(FLOOD_LIT, 2.4)),
            [x, 0.49, -0.85],
            id_quat(),
        )],
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&TrophyMonument.build("did:plc:test"), "trophy_monument");
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&TrophyMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = TrophyMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "trophy_monument");
        assert_nothing_floats(&built, "trophy_monument");
    }
}
