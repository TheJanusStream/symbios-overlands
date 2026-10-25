//! Jetty - a place for a boat (#1782): a timber deck on paired piles running
//! out from the shore to a wider head where a boat moors, steel bollards at
//! the head's outer corners, a ladder down to the water and a lamp that is
//! lit from dusk to dawn.
//!
//! [`jetty`] draws one of any length: its origin where the deck leaves the
//! land, its deck's top at the origin's height, running out along its front
//! (-Z) and back [`BACK`] metres onto the bank. Its piles reach
//! [`PILE_DEPTH`] under the deck. [`moored`] draws one with boats at its
//! head, afloat at the water its deck stands [`FREEBOARD`] over - as a
//! seeded world places it, at the water's height, the compile walking it
//! to the shore (`world_builder::compile::pad::meet_the_shore`). The
//! catalogue's own is [`ENTRY_LENGTH`] long, with no boat.

use super::{PAINT_WHITE, STEEL, TIMBER, TIMBER_DARK, lamp, paint, steel, timber, timber_plain};
use crate::catalogue::items::util::{cuboid_tapered, cylinder_tapered, id_quat, nest, prim, solid};
use crate::catalogue::{CatalogueEntry, Craft, Footprint, StructureRole};
use crate::pds::Generator;

/// The deck's width (m).
pub const WIDTH: f32 = 2.0;
/// The head's width across the deck and its depth along it (m).
pub const HEAD: [f32; 2] = [4.4, 3.2];
/// How far the deck runs back onto the bank past the origin (m).
pub const BACK: f32 = 2.5;
/// How far under the deck's top its piles reach (m).
pub const PILE_DEPTH: f32 = 10.6;
/// The shortest jetty: its head and a pile pair before it (m).
pub const SHORTEST: f32 = HEAD[1] + 3.0;
/// The catalogue's jetty's length (m).
pub const ENTRY_LENGTH: f32 = 14.0;
/// A seeded world's jetty's length (m, #1782): out to 0.8 m of water or
/// more off 95 % of the shores the compile walks one to, measured over a
/// fleet's full maps. The compile reads its head's depth at it.
pub const SEEDED_LENGTH: f32 = 18.0;
/// How high a jetty's deck stands over the water a world places it at (m).
pub const FREEBOARD: f32 = 0.6;
/// How far either side of a jetty's middle a boat lies at its head (m):
/// the head's half width, a boat's half beam and a gap.
pub const BERTH_OFF: f32 = HEAD[0] * 0.5 + 0.95 + 0.35;

const DECK_T: f32 = 0.1;
/// A stringer's width and depth (m).
const STRINGER: [f32; 2] = [0.14, 0.24];
const PILE_R: f32 = 0.13;
/// How far apart the walkway's pile pairs stand (m), the first this far out.
const PILE_STEP: f32 = 3.0;
const FIRST_PILE: f32 = 1.0;
/// A bollard's radius and height (m).
const BOLLARD: [f32; 2] = [0.14, 0.45];
/// How far the ladder reaches under the deck (m), and its rungs' spacing.
const LADDER_DROP: f32 = 2.0;
const RUNG_STEP: f32 = 0.3;
/// The lamp's post height (m).
const LAMP_POST: f32 = 2.6;

pub struct Jetty;

impl CatalogueEntry for Jetty {
    fn slug(&self) -> &'static str {
        "jetty"
    }
    fn name(&self) -> &'static str {
        "Jetty"
    }
    fn description(&self) -> &'static str {
        "Timber jetty on paired piles, a wider head with bollards, a ladder and a lamp: a \
         place to moor a boat."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Port
    }
    fn serves(&self) -> Option<Craft> {
        Some(Craft::Boat)
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 8.0,
            min_spawn_dist: 20.0,
        }
    }
    fn build(&self, _local_did: &str) -> Generator {
        jetty(ENTRY_LENGTH)
    }
}

/// A jetty `length` metres from where it leaves the land to its head's end
/// (at least [`SHORTEST`]); see the module docs for its frame.
pub fn jetty(length: f32) -> Generator {
    let length = length.max(SHORTEST);
    // The walkway runs from BACK onto the bank to where the head starts.
    let head_start = -(length - HEAD[1]);
    let walk_len = BACK - head_start;
    let walk_z = (BACK + head_start) * 0.5;
    let head_z = -(length - HEAD[1] * 0.5);
    let deck_y = -DECK_T * 0.5;

    let deck = prim(
        solid(cuboid_tapered(
            [WIDTH, DECK_T, walk_len],
            0.0,
            timber(TIMBER),
        )),
        [0.0, deck_y, walk_z],
        id_quat(),
    );
    let mut parts = vec![prim(
        solid(cuboid_tapered(
            [HEAD[0], DECK_T, HEAD[1]],
            0.0,
            timber(TIMBER),
        )),
        [0.0, deck_y, head_z],
        id_quat(),
    )];

    // The stringers under the walkway, and the head's beams across.
    let under = -DECK_T - STRINGER[1] * 0.5;
    for side in [-1.0, 1.0] {
        parts.push(prim(
            solid(cuboid_tapered(
                [STRINGER[0], STRINGER[1], walk_len],
                0.0,
                timber_plain(TIMBER_DARK),
            )),
            [side * (WIDTH * 0.5 - STRINGER[0] * 0.5), under, walk_z],
            id_quat(),
        ));
        parts.push(prim(
            solid(cuboid_tapered(
                [HEAD[0], STRINGER[1], STRINGER[0]],
                0.0,
                timber_plain(TIMBER_DARK),
            )),
            [
                0.0,
                under,
                head_z + side * (HEAD[1] * 0.5 - STRINGER[0] * 0.5),
            ],
            id_quat(),
        ));
    }

    // The piles: in pairs under the walkway, one at each of the head's
    // corners, each reaching PILE_DEPTH under the deck's top.
    let pile = |x: f32, z: f32| {
        prim(
            solid(cylinder_tapered(
                PILE_R,
                PILE_DEPTH - DECK_T,
                10,
                0.0,
                timber_plain(TIMBER_DARK),
            )),
            [x, -DECK_T - (PILE_DEPTH - DECK_T) * 0.5, z],
            id_quat(),
        )
    };
    let pair_x = WIDTH * 0.5 - PILE_R;
    // A pair under the deck's bank end too, which a bank lower than the
    // deck leaves standing on them.
    parts.push(pile(-pair_x, BACK - 0.5));
    parts.push(pile(pair_x, BACK - 0.5));
    let mut z = -FIRST_PILE;
    while z > head_start + PILE_STEP * 0.5 {
        parts.push(pile(-pair_x, z));
        parts.push(pile(pair_x, z));
        z -= PILE_STEP;
    }
    let (corner_x, corner_z) = (HEAD[0] * 0.5 - PILE_R - 0.05, HEAD[1] * 0.5 - PILE_R - 0.05);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        parts.push(pile(sx * corner_x, head_z + sz * corner_z));
    }

    // Bollards at the head's outer corners, white-capped.
    let bollard_z = -length + 0.45;
    for side in [-1.0, 1.0] {
        let x = side * (HEAD[0] * 0.5 - 0.4);
        parts.push(prim(
            solid(cylinder_tapered(
                BOLLARD[0],
                BOLLARD[1],
                10,
                0.15,
                steel(STEEL),
            )),
            [x, BOLLARD[1] * 0.5, bollard_z],
            id_quat(),
        ));
        parts.push(prim(
            cylinder_tapered(BOLLARD[0] * 1.3, 0.06, 10, 0.0, paint(PAINT_WHITE)),
            [x, BOLLARD[1] + 0.03, bollard_z],
            id_quat(),
        ));
    }

    // A ladder down the head's +X side to the water.
    let ladder_x = HEAD[0] * 0.5 + 0.04;
    let ladder_z = head_z;
    for dz in [-0.22, 0.22] {
        parts.push(prim(
            cuboid_tapered([0.05, LADDER_DROP + 0.4, 0.05], 0.0, steel(STEEL)),
            [ladder_x, 0.4 - (LADDER_DROP + 0.4) * 0.5, ladder_z + dz],
            id_quat(),
        ));
    }
    let mut rung = -RUNG_STEP;
    while rung > -LADDER_DROP {
        parts.push(prim(
            cuboid_tapered([0.04, 0.04, 0.44], 0.0, steel(STEEL)),
            [ladder_x, rung, ladder_z],
            id_quat(),
        ));
        rung -= RUNG_STEP;
    }

    // A lamp on a post at the head's landward corner, lit from dusk.
    let lamp_x = -(HEAD[0] * 0.5 - 0.35);
    let lamp_z = head_start - 0.35;
    parts.push(prim(
        solid(cylinder_tapered(0.06, LAMP_POST, 8, 0.1, steel(STEEL))),
        [lamp_x, LAMP_POST * 0.5, lamp_z],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.22, 0.3, 0.22], 0.0, lamp([1.0, 0.78, 0.45], 3.0)),
        [lamp_x, LAMP_POST + 0.15, lamp_z],
        id_quat(),
    ));
    parts.push(prim(
        cuboid_tapered([0.3, 0.05, 0.3], 0.3, steel(STEEL)),
        [lamp_x, LAMP_POST + 0.325, lamp_z],
        id_quat(),
    ));

    nest(deck, parts)
}

/// A jetty `length` long ([`jetty`]) with a boat moored at its head for
/// each of `berths`: the side of the deck she lies on (-1 or +1, across
/// its +X), and whether her stem points out to sea - afloat, her keel
/// `longboat::DRAUGHT` under the water the deck stands [`FREEBOARD`] over,
/// lying along the jetty.
pub fn moored(length: f32, berths: &[(f32, bool)]) -> Generator {
    use crate::catalogue::items::pirate::longboat;
    use crate::catalogue::items::util::{attach, quat_mul, quat_y};
    let length = length.max(SHORTEST);
    let mut jetty = jetty(length);
    for &(side, seaward) in berths {
        let mut boat = longboat::afloat();
        // Her stem (+X) turned out along the jetty's front (-Z), or back
        // along it: a quarter turn about +Y either way.
        let turn = if seaward {
            std::f32::consts::FRAC_PI_2
        } else {
            -std::f32::consts::FRAC_PI_2
        };
        let [x, y, z] = boat.transform.translation.0;
        // libm's (#1132): the boats are in the record every peer derives.
        let (s, c) = (libm::sinf(turn), libm::cosf(turn));
        boat.transform.rotation = quat_mul(quat_y(turn), boat.transform.rotation);
        boat.transform.translation = crate::pds::Fp3([
            side.signum() * BERTH_OFF + x * c + z * s,
            -(FREEBOARD + longboat::DRAUGHT) + y,
            -(length - HEAD[1] * 0.5) - x * s + z * c,
        ]);
        attach(&mut jetty, boat);
    }
    jetty
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;

    /// Its deck stands at its origin's height, its piles reach PILE_DEPTH
    /// under it, and it runs out its length along its front: a seeded
    /// world's shore asks of it just that.
    #[test]
    fn a_jetty_runs_its_length_out_at_its_deck_height() {
        for length in [SHORTEST, 14.0, 26.0] {
            let jetty = jetty(length);
            let top = measure::coarse_top(&jetty).expect("it draws");
            assert!(
                (top - (LAMP_POST + 0.35)).abs() < 0.01,
                "{length}: the lamp's cap tops it at {top}"
            );
            let solids = measure::solids(&jetty);
            let reach = solids
                .iter()
                .map(|piece| piece.bounds.min[2])
                .fold(f32::MAX, f32::min);
            assert!((reach + length).abs() < 0.01, "{length}: out {reach}");
            let deck_top = solids
                .iter()
                .filter(|piece| piece.bounds.max[2] > 0.0)
                .map(|piece| piece.bounds.max[1])
                .fold(f32::MIN, f32::max);
            assert!(deck_top.abs() < 1e-3, "{length}: deck top at {deck_top}");
            let deepest = solids
                .iter()
                .map(|piece| piece.bounds.min[1])
                .fold(f32::MAX, f32::min);
            assert!(
                (deepest + PILE_DEPTH).abs() < 0.01,
                "{length}: piles to {deepest}"
            );
        }
    }

    /// A moored boat lies at the head, either side, along the jetty, her
    /// keel her draught under the water the deck stands its freeboard over.
    #[test]
    fn a_moored_boat_floats_at_the_head_along_the_jetty() {
        use crate::catalogue::items::pirate::longboat;
        let length = 18.0;
        let bare = jetty(length).children.len();
        let jetty = moored(length, &[(1.0, true), (-1.0, false)]);
        assert_eq!(jetty.children.len(), bare + 2);
        let root = jetty.transform.translation.0;
        for (boat, side) in jetty.children[bare..].iter().zip([1.0f32, -1.0]) {
            let alone = crate::pds::Generator {
                transform: crate::pds::TransformData {
                    translation: crate::pds::Fp3([
                        root[0] + boat.transform.translation.0[0],
                        root[1] + boat.transform.translation.0[1],
                        root[2] + boat.transform.translation.0[2],
                    ]),
                    ..boat.transform.clone()
                },
                ..boat.clone()
            };
            let bounds = measure::tree_bounds(&alone).expect("she draws");
            let keel = -(FREEBOARD + longboat::DRAUGHT);
            assert!(
                (bounds.min.y - keel).abs() < 0.05,
                "keel at {}",
                bounds.min.y
            );
            let middle = (bounds.min.x + bounds.max.x) * 0.5;
            assert!(
                (middle - side * BERTH_OFF).abs() < 0.3,
                "beside at {middle}"
            );
            let (along, across) = (bounds.max.z - bounds.min.z, bounds.max.x - bounds.min.x);
            assert!(
                along > across * 2.0,
                "she lies along it: {along} by {across}"
            );
            assert!(bounds.min.z > -length - 3.0 && bounds.max.z < -length + HEAD[1] + 3.0);
        }
    }
}
