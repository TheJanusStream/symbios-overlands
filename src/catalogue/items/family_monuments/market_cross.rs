//! Owner Market Cross - a family monument for Medieval worlds (#1796).
//!
//! The cross a market town raises where its roads meet: three octagonal
//! steps carry a socket stone and an octagonal shaft, which opens halfway up
//! into a gabled tabernacle whose carved niche holds the room owner's
//! portrait. The shaft runs on through the gable to a stone cross, and an
//! iron lantern hangs off one jamb of the niche.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::medieval::{
    FORGE_ORANGE, IRON_DARK, STONE_GREY, STONE_PALE, iron, stone,
};
use crate::catalogue::items::util::{
    cuboid_tapered, cuboid_tapered_xz, cylinder_tapered, footing, glow, id_quat, nest, pfp_panel,
    prim, solid,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

const PANEL: f32 = 1.5;
const PANEL_Y: f32 = 3.42;
/// The niche's back face; the panel stands just proud of it.
const NICHE_BACK: f32 = -0.15;
/// The sunk back of the niche, darker than the dressed stone round it so the
/// recess reads at a distance.
const STONE_SHADE: [f32; 3] = [0.40, 0.38, 0.36];
const CLEARANCE: f32 = 2.8;

pub struct MarketCrossMonument;

impl CatalogueEntry for MarketCrossMonument {
    fn slug(&self) -> &'static str {
        "market_cross_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Market Cross"
    }
    fn description(&self) -> &'static str {
        "Stone market cross on stepped octagonal plinths, the owner's portrait in its niche."
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
    // Three octagonal steps, each standing on the one below.
    let step = |r: f32, h: f32, y: f32| {
        prim(
            solid(cylinder_tapered(r, h, 8, 0.0, stone(STONE_PALE))),
            [0.0, y, 0.0],
            id_quat(),
        )
    };
    let socket = prim(
        solid(cuboid_tapered([0.92, 0.42, 0.92], 0.12, stone(STONE_GREY))),
        [0.0, 1.11, 0.0],
        id_quat(),
    );
    let shaft = prim(
        solid(cylinder_tapered(0.33, 0.9, 8, 0.1, stone(STONE_GREY))),
        [0.0, 1.73, 0.0],
        id_quat(),
    );
    // A two-stage corbel the tabernacle grows out of, so the niche block does
    // not perch on the shaft like a box on a stick.
    let corbel = prim(
        solid(cuboid_tapered([0.86, 0.14, 0.86], 0.0, stone(STONE_PALE))),
        [0.0, 2.2, 0.0],
        id_quat(),
    );
    let corbel_head = prim(
        solid(cuboid_tapered([1.4, 0.13, 0.7], 0.0, stone(STONE_PALE))),
        [0.0, 2.25, 0.1],
        id_quat(),
    );
    nest(
        step(1.75, 0.32, 0.16),
        vec![
            nest(
                step(1.38, 0.3, 0.47),
                vec![nest(
                    step(1.02, 0.28, 0.76),
                    vec![nest(
                        socket,
                        vec![nest(
                            shaft,
                            vec![nest(corbel, vec![nest(corbel_head, vec![tabernacle(did)])])],
                        )],
                    )],
                )],
            ),
            footing(3.5, 3.5, [0.0, 0.0], CLEARANCE),
        ],
    )
}

/// The niche block the shaft opens into: its sunk back, the portrait, the
/// jambs, sill and lintel round it, and the gable and cross above.
fn tabernacle(did: &str) -> Generator {
    let back = prim(
        solid(cuboid_tapered([2.0, 2.3, 0.5], 0.0, stone(STONE_SHADE))),
        [0.0, 3.45, NICHE_BACK + 0.25],
        id_quat(),
    );
    let mut parts = vec![
        pfp_panel(did, PANEL, [0.0, PANEL_Y, NICHE_BACK - 0.05]),
        // Sill under the niche.
        prim(
            solid(cuboid_tapered([1.62, 0.2, 0.52], 0.0, stone(STONE_PALE))),
            [0.0, PANEL_Y - PANEL * 0.5 - 0.12, NICHE_BACK - 0.06],
            id_quat(),
        ),
        // Lintel over it.
        prim(
            solid(cuboid_tapered([2.04, 0.28, 0.74], 0.0, stone(STONE_PALE))),
            [0.0, 4.45, -0.1],
            id_quat(),
        ),
        nest(cornice(), gable()),
        lantern(),
    ];
    for sx in [-1.0_f32, 1.0] {
        // Jambs either side, standing proud of the back so the portrait sits
        // in a reveal.
        parts.push(prim(
            solid(cuboid_tapered([0.26, 2.26, 0.7], 0.0, stone(STONE_PALE))),
            [sx * 0.9, 3.45, -0.11],
            id_quat(),
        ));
        // A colonette on each jamb's inner arris.
        parts.push(prim(
            solid(cylinder_tapered(0.075, 2.2, 8, 0.0, stone(STONE_GREY))),
            [sx * 0.82, 3.45, -0.47],
            id_quat(),
        ));
    }
    nest(back, parts)
}

/// The moulded cap over the lintel.
fn cornice() -> Generator {
    prim(
        solid(cuboid_tapered([2.3, 0.18, 1.06], 0.0, stone(STONE_GREY))),
        [0.0, 4.69, 0.04],
        id_quat(),
    )
}

/// The gable over the niche - its ridge runs front to back, so the point
/// faces the approach - with pinnacles on its corners and the upper shaft
/// and cross rising out of it.
fn gable() -> Vec<Generator> {
    let roof = prim(
        solid(cuboid_tapered_xz(
            [2.16, 0.78, 0.88],
            [0.96, 0.0],
            stone(STONE_GREY),
        )),
        [0.0, 5.17, 0.04],
        id_quat(),
    );
    let upper = prim(
        solid(cylinder_tapered(0.16, 1.4, 8, 0.15, stone(STONE_PALE))),
        [0.0, 6.0, 0.04],
        id_quat(),
    );
    let mut out = vec![nest(
        roof,
        vec![nest(
            upper,
            vec![
                prim(
                    solid(cuboid_tapered([0.22, 0.96, 0.22], 0.0, stone(STONE_PALE))),
                    [0.0, 7.05, 0.04],
                    id_quat(),
                ),
                prim(
                    solid(cuboid_tapered([0.86, 0.2, 0.18], 0.0, stone(STONE_PALE))),
                    [0.0, 7.2, 0.04],
                    id_quat(),
                ),
            ],
        )],
    )];
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered([0.26, 0.56, 0.26], 0.95, stone(STONE_PALE))),
            [sx * 0.98, 5.06, -0.3],
            id_quat(),
        ));
    }
    out
}

/// An iron lantern on a bracket off the right jamb - for the stone, which
/// goes flat at dusk; the portrait is unlit and reads on its own.
fn lantern() -> Generator {
    let arm = prim(
        solid(cuboid_tapered([0.5, 0.08, 0.08], 0.0, iron(IRON_DARK))),
        [1.25, 3.95, -0.3],
        id_quat(),
    );
    let cap = prim(
        solid(cuboid_tapered([0.3, 0.14, 0.3], 0.6, iron(IRON_DARK))),
        [1.42, 3.85, -0.3],
        id_quat(),
    );
    nest(
        arm,
        vec![nest(
            cap,
            vec![
                prim(
                    cuboid_tapered([0.2, 0.3, 0.2], 0.2, glow(FORGE_ORANGE, 2.2)),
                    [1.42, 3.64, -0.3],
                    id_quat(),
                ),
                prim(
                    solid(cuboid_tapered([0.26, 0.06, 0.26], 0.0, iron(IRON_DARK))),
                    [1.42, 3.47, -0.3],
                    id_quat(),
                ),
            ],
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
        assert_sanitize_stable(
            &MarketCrossMonument.build("did:plc:test"),
            "market_cross_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&MarketCrossMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = MarketCrossMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "market_cross_monument");
        assert_nothing_floats(&built, "market_cross_monument");
    }
}
