//! Owner Toro - a family monument for Feudal-Japan worlds (#1796).
//!
//! A tall stone lantern of the kasuga kind - hexagonal foot, lotus collar,
//! round pole, platform, the fire box lit at every hour between its six
//! stone posts, a hipped hat and a jewel finial - standing on a dressed slab
//! beside a small roofed board on two dark posts, which hangs the room
//! owner's portrait in a lacquered frame under a fold of paper shide.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::feudal_japan::{
    LACQUER_RED, LANTERN_GLOW, PAPER_CREAM, STONE_GREY, TILE_SLATE, TIMBER_DARK, lacquer, paper,
    roof_tile, rough_stone, stone, timber,
};
use crate::catalogue::items::util::{
    cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel,
    prim, solid, sphere,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

const PANEL: f32 = 1.5;
const PANEL_Y: f32 = 2.35;
/// The board's centre line, left of the slab's middle; the lantern stands
/// to its right.
const BOARD_X: f32 = -0.75;
const LANTERN_X: f32 = 1.35;
/// Paler weathered granite for the lantern, so it reads against the board.
const GRANITE: [f32; 3] = [0.62, 0.61, 0.57];
const CLEARANCE: f32 = 2.7;

pub struct StoneLanternMonument;

impl CatalogueEntry for StoneLanternMonument {
    fn slug(&self) -> &'static str {
        "stone_lantern_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Toro"
    }
    fn description(&self) -> &'static str {
        "Tall stone lantern beside a small roofed board hanging the owner's portrait."
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
    let slab = prim(
        solid(cuboid_tapered([4.0, 0.3, 1.8], 0.03, stone(STONE_GREY))),
        [0.0, 0.15, 0.0],
        id_quat(),
    );
    nest(
        slab,
        vec![toro(), board(did), footing(4.0, 1.8, [0.0, 0.0], CLEARANCE)],
    )
}

/// The lantern, foot to finial, each course standing on the one below.
fn toro() -> Generator {
    let x = LANTERN_X;
    let course = |r: f32, h: f32, res: u32, taper: f32, y: f32| {
        prim(
            solid(cylinder_tapered(r, h, res, taper, stone(GRANITE))),
            [x, y, 0.0],
            id_quat(),
        )
    };
    let foot = course(0.48, 0.36, 6, 0.1, 0.48);
    let lotus = course(0.36, 0.2, 12, -0.3, 0.75);
    let pole = course(0.16, 1.82, 12, 0.08, 1.75);
    let platform = course(0.46, 0.24, 6, -0.25, 2.77);
    let hat = course(0.68, 0.38, 6, 0.78, 3.63);
    let jewel_seat = course(0.13, 0.13, 12, 0.0, 3.87);
    let mut fire_box = vec![
        // The fire seen through the box: deep amber at low strength, so it
        // reads as a lit lantern rather than a white blank.
        prim(
            cylinder_tapered(0.25, 0.6, 6, 0.0, glow(LANTERN_GLOW, 1.8)),
            [x, 3.17, 0.0],
            id_quat(),
        ),
        nest(
            hat,
            vec![nest(
                jewel_seat,
                vec![nest(
                    prim(
                        solid(sphere(0.17, 6, stone(GRANITE))),
                        [x, 4.06, 0.0],
                        id_quat(),
                    ),
                    vec![prim(
                        solid(cuboid_tapered([0.12, 0.22, 0.12], 0.95, stone(GRANITE))),
                        [x, 4.28, 0.0],
                        id_quat(),
                    )],
                )],
            )],
        ),
    ];
    // Six stone posts round the fire box, carrying the hat.
    for k in 0..6 {
        let a = (k as f32 * 60.0 + 30.0).to_radians();
        fire_box.push(prim(
            solid(cuboid_tapered([0.09, 0.6, 0.09], 0.0, stone(GRANITE))),
            [x + 0.3 * a.cos(), 3.17, 0.3 * a.sin()],
            id_quat(),
        ));
    }
    nest(
        foot,
        vec![nest(
            lotus,
            vec![nest(pole, vec![nest(platform, fire_box)])],
        )],
    )
}

/// The roofed board: a stone kerb, two dark posts, a head beam, the tiled
/// roof over it, and the lacquer-framed portrait hung between the posts.
fn board(did: &str) -> Generator {
    let x = BOARD_X;
    let kerb = prim(
        solid(cuboid_tapered(
            [2.3, 0.16, 0.44],
            0.0,
            rough_stone(STONE_GREY),
        )),
        [x, 0.38, 0.0],
        id_quat(),
    );
    let beam = prim(
        solid(cuboid_tapered([2.4, 0.2, 0.24], 0.0, timber(TIMBER_DARK))),
        [x, 3.3, 0.0],
        id_quat(),
    );
    let backing = prim(
        solid(cuboid_tapered(
            [PANEL + 0.3, PANEL + 0.3, 0.08],
            0.0,
            timber([0.52, 0.42, 0.30]),
        )),
        [x, PANEL_Y, 0.02],
        id_quat(),
    );
    let mut face = vec![pfp_panel(did, PANEL, [x, PANEL_Y, -0.04])];
    for sy in [-1.0_f32, 1.0] {
        face.push(prim(
            solid(cuboid_tapered(
                [PANEL + 0.34, 0.12, 0.12],
                0.0,
                lacquer(LACQUER_RED),
            )),
            [x, PANEL_Y + sy * (PANEL * 0.5 + 0.1), -0.03],
            id_quat(),
        ));
    }
    let mut parts = vec![
        nest(
            beam,
            vec![
                // Kirizuma roof: a ridge along X, so a gable faces each side.
                prim(
                    solid(cuboid_tapered_xz(
                        [2.7, 0.62, 1.2],
                        [0.06, 0.94],
                        roof_tile(TILE_SLATE),
                    )),
                    [x, 3.71, 0.0],
                    id_quat(),
                ),
                // Paper shide hanging from the beam.
                prim(
                    solid(cuboid_tapered([1.9, 0.22, 0.04], 0.0, paper(PAPER_CREAM))),
                    [x, 3.11, -0.14],
                    id_quat(),
                ),
            ],
        ),
        nest(backing, face),
    ];
    for sx in [-1.0_f32, 1.0] {
        parts.push(prim(
            solid(cylinder_tapered(0.1, 2.96, 12, 0.05, timber(TIMBER_DARK))),
            [x + sx * 1.0, 1.94, 0.0],
            id_quat(),
        ));
    }
    nest(kerb, parts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_owner_panel, assert_sanitize_stable};

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(
            &StoneLanternMonument.build("did:plc:test"),
            "stone_lantern_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&StoneLanternMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = StoneLanternMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "stone_lantern_monument");
        assert_nothing_floats(&built, "stone_lantern_monument");
    }
}
