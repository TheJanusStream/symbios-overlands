//! Owner Figurehead - a family monument for seafaring worlds (#1796):
//! Nordic, Pirate and Coastal-Resort.
//!
//! A ship's figurehead saved from a broken hull and stood up on the quay: a
//! tarred stem post on a block of harbour stone carries the carved, painted
//! bust, and she holds before her a round shield whose gilt-rimmed face is
//! the room owner's portrait. A ship's lantern hangs off a bracket behind
//! her shoulder.
//!
//! See [`civic::monument`](crate::catalogue::items::civic::monument) for the
//! rules this family shares.

use crate::catalogue::items::nordic::{SHIELD_BLUE, STONE_COLD, WOOD_DARK, rough_stone, timber};
use crate::catalogue::items::pirate::{GOLD_LEAF, IRON_BLACK, bronze, iron, lantern};
use crate::catalogue::items::util::{
    aim_y, blob_capsule, blob_ellipsoid, blob_group, cuboid_tapered, cylinder_tapered, footing,
    id_quat, nest, pfp_panel, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Fp3, Generator, SovereignMaterialSettings};
use crate::seeded_defaults::ThemeArchetype;
use std::f32::consts::FRAC_PI_2;

const PANEL: f32 = 1.45;
const PANEL_Y: f32 = 2.85;
const SHIELD_R: f32 = 1.15;
/// The shield's painted face; the panel stands just proud of it.
const SHIELD_FACE: f32 = -0.16;
/// The figure's paint - the cream a figurehead is limed and varnished in.
const PAINT_CREAM: [f32; 3] = [0.84, 0.80, 0.70];
const BODICE_RED: [f32; 3] = [0.56, 0.14, 0.11];
const HAIR_DARK: [f32; 3] = [0.24, 0.15, 0.09];
const CLEARANCE: f32 = 2.7;

pub struct FigureheadMonument;

impl CatalogueEntry for FigureheadMonument {
    fn slug(&self) -> &'static str {
        "figurehead_monument"
    }
    fn name(&self) -> &'static str {
        "Owner Figurehead"
    }
    fn description(&self) -> &'static str {
        "Carved ship's figurehead on a stem post, holding the owner's portrait on her shield."
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
    let block = prim(
        solid(cuboid_tapered(
            [2.6, 0.46, 2.0],
            0.08,
            rough_stone(STONE_COLD),
        )),
        [0.0, 0.23, 0.0],
        id_quat(),
    );
    // The stem post: everything above stands on it.
    let post = prim(
        solid(cuboid_tapered([0.5, 4.6, 0.5], 0.06, timber(WOOD_DARK))),
        [0.0, 2.76, 0.2],
        id_quat(),
    );
    let mut on_post = shield(did);
    on_post.push(figure());
    on_post.push(bracket());
    on_post.push(prim(
        solid(cuboid_tapered([0.62, 0.16, 0.62], 0.3, timber(WOOD_DARK))),
        [0.0, 5.12, 0.2],
        id_quat(),
    ));
    nest(
        block,
        vec![
            nest(post, on_post),
            footing(2.6, 2.0, [0.0, 0.0], CLEARANCE),
        ],
    )
}

/// The round shield, its gilt rim and boss-less face, and the portrait set
/// square on it inside a thin gilt frame.
fn shield(did: &str) -> Vec<Generator> {
    let stand = quat_x(FRAC_PI_2);
    let mut out = vec![
        // The disc stands on its edge facing -Z; its back meets the post.
        prim(
            solid(cylinder_tapered(
                SHIELD_R,
                0.12,
                20,
                0.0,
                timber(SHIELD_BLUE),
            )),
            [0.0, PANEL_Y, SHIELD_FACE + 0.06],
            stand,
        ),
        prim(
            torus(0.07, SHIELD_R, bronze(GOLD_LEAF, 0x61)),
            [0.0, PANEL_Y, SHIELD_FACE],
            stand,
        ),
        pfp_panel(did, PANEL, [0.0, PANEL_Y, SHIELD_FACE - 0.03]),
    ];
    let bar = 0.07;
    for sx in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [bar, PANEL + bar * 2.0, 0.05],
                0.0,
                bronze(GOLD_LEAF, 0x62),
            )),
            [sx * (PANEL + bar) * 0.5, PANEL_Y, SHIELD_FACE - 0.02],
            id_quat(),
        ));
    }
    for sy in [-1.0_f32, 1.0] {
        out.push(prim(
            solid(cuboid_tapered(
                [PANEL, bar, 0.05],
                0.0,
                bronze(GOLD_LEAF, 0x62),
            )),
            [0.0, PANEL_Y + sy * (PANEL + bar) * 0.5, SHIELD_FACE - 0.02],
            id_quat(),
        ));
    }
    out
}

/// The carved bust, painted as figureheads are: a red bodice blended down
/// onto the post, cream skin - neck, head, breast and the two arms reaching
/// down to grip the shield's rim - and dark hair swept back off her brow,
/// with a gilt diadem in it. Three blended masses, one a colour, with the
/// bodice carrying the other two.
fn figure() -> Generator {
    let at = [0.0, 4.35, -0.2];
    let paint = |color: [f32; 3]| SovereignMaterialSettings {
        base_color: Fp3(color),
        roughness: Fp(0.6),
        ..Default::default()
    };
    let bodice = vec![
        blob_ellipsoid([0.0, 0.0, 0.0], [0.46, 0.62, 0.36], 0.12),
        blob_ellipsoid([0.0, -0.1, 0.3], [0.22, 0.5, 0.14], 0.1),
    ];
    // She leans out over the shield, as a figurehead leans off the stem.
    let mut skin = vec![
        blob_ellipsoid([0.0, 0.4, -0.16], [0.32, 0.18, 0.2], 0.08),
        blob_ellipsoid([0.0, 0.6, -0.14], [0.14, 0.2, 0.14], 0.1),
        blob_ellipsoid([0.0, 0.9, -0.24], [0.21, 0.27, 0.24], 0.08),
    ];
    let hair = vec![
        blob_ellipsoid([0.0, 0.92, -0.04], [0.26, 0.31, 0.27], 0.1),
        blob_ellipsoid([0.0, 0.52, 0.18], [0.27, 0.42, 0.18], 0.1),
    ];
    // Each arm from its shoulder to a hand on the rim, 40 degrees off the
    // vertical either side of the shield's crown.
    for sx in [-1.0_f32, 1.0] {
        let shoulder = [sx * 0.4, 0.3, -0.06];
        let a = 40_f32.to_radians();
        let hand = [
            sx * SHIELD_R * a.sin(),
            PANEL_Y + SHIELD_R * a.cos() - at[1],
            SHIELD_FACE - 0.08 - at[2],
        ];
        let d = [
            hand[0] - shoulder[0],
            hand[1] - shoulder[1],
            hand[2] - shoulder[2],
        ];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        let mid = [
            (hand[0] + shoulder[0]) * 0.5,
            (hand[1] + shoulder[1]) * 0.5,
            (hand[2] + shoulder[2]) * 0.5,
        ];
        skin.push(blob_capsule(
            mid,
            0.1,
            len * 0.5,
            aim_y([d[0] / len, d[1] / len, d[2] / len]),
            0.08,
        ));
        skin.push(blob_ellipsoid(hand, [0.12, 0.1, 0.12], 0.06));
    }
    let hair = nest(
        prim(blob_group(hair, 24, paint(HAIR_DARK)), at, id_quat()),
        vec![prim(
            torus(0.035, 0.23, bronze(GOLD_LEAF, 0x66)),
            [at[0], at[1] + 1.05, at[2] - 0.16],
            quat_x(-0.35),
        )],
    );
    nest(
        prim(blob_group(bodice, 24, paint(BODICE_RED)), at, id_quat()),
        vec![
            prim(blob_group(skin, 32, paint(PAINT_CREAM)), at, id_quat()),
            hair,
        ],
    )
}

/// An iron bracket off the post behind her shoulder, with a ship's lantern
/// hung from it - for the stone and the paint, which go flat at dusk; the
/// portrait is unlit and reads on its own.
fn bracket() -> Generator {
    let arm = prim(
        solid(cuboid_tapered(
            [0.95, 0.08, 0.08],
            0.0,
            iron(IRON_BLACK, 0x63),
        )),
        [0.68, 4.7, 0.2],
        id_quat(),
    );
    nest(
        arm,
        vec![
            prim(
                solid(cylinder_tapered(0.02, 0.16, 6, 0.0, iron(IRON_BLACK, 0x64))),
                [1.1, 4.6, 0.2],
                id_quat(),
            ),
            lantern([1.1, 4.22, 0.2], 0.6, 0x65),
        ],
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
            &FigureheadMonument.build("did:plc:test"),
            "figurehead_monument",
        );
    }

    #[test]
    fn carries_exactly_one_square_owner_panel() {
        assert_owner_panel(&FigureheadMonument, "did:plc:test");
    }

    #[test]
    fn nothing_z_fights_or_floats() {
        let built = FigureheadMonument.build("did:plc:test");
        assert_no_z_fighting(&built, "figurehead_monument");
        assert_nothing_floats(&built, "figurehead_monument");
    }
}
