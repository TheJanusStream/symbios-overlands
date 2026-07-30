//! Quay Capstan — the machine a harbour warps ships in with.
//!
//! A capstan stepped on a paved pad at the water's edge: barrel, whelps,
//! pawl rim and drumhead, four bars shipped and two sockets empty, with the
//! hawser it is heaving on led away round a bollard and coiled on the stones.
//!
//! # What makes it read as working rather than as furniture
//!
//! Three things, and none of them is the capstan itself — the kit's shared
//! [`capstan`] supplies that. It is the ROPE that says the
//! machine is doing something: a hawser taken to the barrel, turned round a
//! bollard, and running off the pad toward whatever is being warped in. A
//! capstan with no rope on it is a drum on a plinth, which is exactly how a
//! prop of this size fails.
//!
//! The two empty sockets do the other half. Six filled sockets is a
//! ceremonial diagram; four filled and two bare says the bars are stowed
//! where the crew left them, which is what a working quay looks like.

use crate::catalogue::items::util::{
    cuboid_tapered, cylinder_tapered, face_uv_offset, footing, id_quat, nest, prim, solid, sphere,
    strut, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::pds::generator::FaceKey;
use crate::seeded_defaults::{ProsperityBand, ThemeArchetype};

use super::{
    DECK_HOLY, HULL_OAK, IRON_BLACK, PORT_BAND, ROPE_HEMP, STONE_QUAY, board, capstan, cobbles, fx,
    hemp, iron,
};

/// The paved pad — the sub-root every footprint guard measures against
/// (#972 lesson 19).
const PAD: [f32; 3] = [5.4, 0.26, 5.4];
const GROUND: f32 = PAD[1];

/// The stone step the capstan is bedded into, and its top — where the
/// barrel is stepped.
const STEP_R: f32 = 1.0;
const STEP_H: f32 = 0.34;
const DECK: f32 = GROUND + STEP_H;

/// How many bars are shipped, of the six sockets.
const BARS: usize = 4;

/// Where the bollard the hawser turns round stands, and how tall it is.
const BOLLARD: [f32; 3] = [1.85, 0.0, -1.7];
const BOLLARD_H: f32 = 0.72;

/// Hero side — the render tool and the settlement placer both look down `-Z`.
const FRONT: f32 = -1.0;

pub struct QuayCapstan;

impl CatalogueEntry for QuayCapstan {
    fn slug(&self) -> &'static str {
        "quay_capstan"
    }
    fn name(&self) -> &'static str {
        "Quay Capstan"
    }
    fn description(&self) -> &'static str {
        "A capstan on a paved pad with its bars shipped and a hawser led round the bollard."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Pirate]
    }
    fn prosperity_band(&self) -> ProsperityBand {
        PORT_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 3.2,
            min_spawn_dist: 12.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// The hawser: barrel → bollard → off the pad.
///
/// Every leg is a [`strut`] between two points that both exist, so the rope
/// cannot end up pointing near its own bollard rather than at it — the fault
/// that cost this kit three separate fixes before the helper existed (#1028,
/// #1030). The height at each end is the height of the thing it is made
/// fast to, which is what makes the run read as a rope under load rather
/// than as a stick lying about.
fn hawser() -> Vec<Generator> {
    let barrel = [0.42, DECK + 0.5, 0.0];
    let bollard_head = [BOLLARD[0], GROUND + BOLLARD_H - 0.12, BOLLARD[2]];
    // Off the pad on the hero side, dipping toward the water.
    let away = [
        BOLLARD[0] + 0.35,
        GROUND + 0.14,
        FRONT * (PAD[2] * 0.5 + 0.2),
    ];
    vec![
        strut(barrel, bollard_head, 0.055, 6, hemp(ROPE_HEMP)),
        strut(bollard_head, away, 0.055, 6, hemp(ROPE_HEMP)),
    ]
}

fn build_tree() -> Generator {
    let pad_c = [0.0, GROUND * 0.5, 0.0];
    let mut paving = cobbles(STONE_QUAY, 0xB0);
    paving.uv_offset = face_uv_offset(FaceKey::Top, pad_c);

    let mut carried = vec![
        footing(PAD[0], PAD[2], [0.0, 0.0], 3.2),
        // Stone step, bedded into the paving rather than laid on it: equal
        // heights would put its underside and the pad's top on one plane
        // across the whole disc (#1028's coplanar family).
        prim(
            solid(cylinder_tapered(
                STEP_R,
                STEP_H + 0.06,
                16,
                0.08,
                cobbles(STONE_QUAY, 0xB1),
            )),
            [0.0, DECK - (STEP_H + 0.06) * 0.5 + 0.03, 0.0],
            id_quat(),
        ),
        // The machine itself — the kit's shared assembly.
        capstan([0.0, DECK, 0.0], BARS, 0xB2),
        // Bollard the hawser turns round.
        prim(
            solid(cylinder_tapered(
                0.19,
                BOLLARD_H,
                12,
                0.12,
                iron(IRON_BLACK, 0xB3),
            )),
            [BOLLARD[0], GROUND + BOLLARD_H * 0.5, BOLLARD[2]],
            id_quat(),
        ),
        prim(
            solid(cylinder_tapered(0.25, 0.1, 12, 0.3, iron(IRON_BLACK, 0xB4))),
            [BOLLARD[0], GROUND + BOLLARD_H, BOLLARD[2]],
            id_quat(),
        ),
    ];
    carried.extend(hawser());

    // The two bars that are NOT shipped, stowed on the stones where the crew
    // dropped them — the detail that makes four-of-six read as deliberate.
    // Their CENTRES are derived from the pad's edge and the bar's own reach,
    // so a bar lying near the rim keeps both ends on the stones whatever
    // either dimension becomes (#972 lesson 8). Placed by eye at −1.9 the
    // first one hung 30 mm off, which the footprint guard caught.
    const STOW_REACH: f32 = 0.9;
    for (z, a) in [(1.5_f32, 0.4_f32), (1.05, 0.1)] {
        let x = -(PAD[0] * 0.5 - STOW_REACH * a.cos() - 0.2);
        carried.push(strut(
            [
                x - STOW_REACH * a.cos(),
                GROUND + 0.07,
                z - STOW_REACH * a.sin(),
            ],
            [
                x + STOW_REACH * a.cos(),
                GROUND + 0.07,
                z + STOW_REACH * a.sin(),
            ],
            0.065,
            6,
            board(DECK_HOLY),
        ));
    }

    // A coil of the hawser's slack, and a shot of chain, on the stones.
    carried.push(prim(
        torus(0.055, 0.34, hemp(ROPE_HEMP)),
        [-1.5, GROUND + 0.055, -1.5],
        id_quat(),
    ));
    carried.push(prim(
        torus(0.05, 0.27, hemp(ROPE_HEMP)),
        [-1.5, GROUND + 0.15, -1.5],
        id_quat(),
    ));
    for (i, dx) in [0.0_f32, 0.3, 0.6].into_iter().enumerate() {
        carried.push(prim(
            sphere(0.11, 3, iron(IRON_BLACK, 0xB5 + i as u32)),
            [1.5 + dx, GROUND + 0.11, 1.85],
            id_quat(),
        ));
    }
    // A swifter bar leaning against the step, and a bucket of slush.
    carried.push(prim(
        solid(cuboid_tapered([0.5, 0.42, 0.5], 0.22, board(HULL_OAK))),
        [-1.05, GROUND + 0.21, 1.9],
        id_quat(),
    ));

    let mut root = nest(
        prim(solid(cuboid_tapered(PAD, 0.0, paving)), pad_c, id_quat()),
        carried,
    );
    root.audio = fx::rigging_creak();
    root
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure;
    use crate::catalogue::items::util::{
        assert_no_glazing_on_solids, assert_no_tilted_parents, assert_sanitize_stable, rotate_by,
        window_cards,
    };

    fn built() -> Generator {
        QuayCapstan.build("")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "quay_capstan");
    }

    #[test]
    fn no_rotated_node_carries_an_offset_child() {
        assert_no_tilted_parents(&built(), "quay_capstan");
    }

    #[test]
    fn the_capstan_carries_no_glazing() {
        let g = built();
        assert_no_glazing_on_solids(&g, "quay_capstan");
        assert!(window_cards(&g).is_empty(), "a capstan has grown a window");
    }

    /// Four bars shipped, two sockets empty — and the empty pair accounted
    /// for on the stones.
    ///
    /// The count is the read: six filled is a ceremonial diagram, four filled
    /// with two lying where the crew dropped them is a working quay. Bars are
    /// selected by their stock radius, which is what defines them, not by
    /// height or position (#972 lesson 24).
    #[test]
    fn four_bars_are_shipped_and_the_other_two_are_stowed() {
        use crate::pds::GeneratorKind as K;
        fn bars(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - 0.065).abs() < 0.005
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                bars(c, here, out);
            }
        }
        let mut found = Vec::new();
        bars(&built(), [0.0; 3], &mut found);
        assert_eq!(
            found.len(),
            BARS + 2,
            "expected {} shipped bars plus two stowed, found {}",
            BARS,
            found.len()
        );
        let shipped = found.iter().filter(|(a, _)| a[1] > DECK).count();
        assert_eq!(shipped, BARS, "{shipped} bars are in their sockets");
        // Every bar is level — that is what a bar in a socket is, and it is
        // the property the hand-rolled rotation used to get wrong.
        for (a, b) in &found {
            assert!(
                (a[1] - b[1]).abs() < 1e-3,
                "a bar runs from {a:?} to {b:?} — it is not level"
            );
        }
    }

    /// The hawser makes a continuous run from the barrel round the bollard
    /// and off the pad.
    ///
    /// Read from the BUILT struts (#972 lesson 21), because the fault this
    /// guards is a leg that points *near* its bollard rather than at it —
    /// which looks right from three of four angles and is what cost this kit
    /// three separate fixes. Continuity is checked as a join, not as a pair
    /// of positions.
    #[test]
    fn the_hawser_runs_unbroken_from_barrel_to_water() {
        use crate::pds::GeneratorKind as K;
        fn legs(g: &Generator, at: [f32; 3], out: &mut Vec<([f32; 3], [f32; 3])>) {
            let t = g.transform.translation.0;
            let here = [at[0] + t[0], at[1] + t[1], at[2] + t[2]];
            if let K::Cylinder { radius, height, .. } = &g.kind
                && (radius.0 - 0.055).abs() < 0.005
            {
                let tip = rotate_by(g.transform.rotation.0, [0.0, height.0 * 0.5, 0.0]);
                out.push((
                    [here[0] + tip[0], here[1] + tip[1], here[2] + tip[2]],
                    [here[0] - tip[0], here[1] - tip[1], here[2] - tip[2]],
                ));
            }
            for c in &g.children {
                legs(c, here, out);
            }
        }
        let mut found = Vec::new();
        legs(&built(), [0.0; 3], &mut found);
        assert_eq!(found.len(), 2, "the hawser is two legs");
        let dist = |a: [f32; 3], b: [f32; 3]| {
            ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
        };
        // Some end of leg 0 meets some end of leg 1, at the bollard head.
        let (a0, b0) = found[0];
        let (a1, b1) = found[1];
        let join = [(a0, a1), (a0, b1), (b0, a1), (b0, b1)]
            .into_iter()
            .map(|(p, q)| (dist(p, q), p))
            .min_by(|x, y| x.0.partial_cmp(&y.0).expect("finite"))
            .expect("four candidate joins");
        assert!(
            join.0 < 0.08,
            "the hawser's two legs do not meet — nearest ends are {} m apart",
            join.0
        );
        assert!(
            (join.1[0] - BOLLARD[0]).abs() < 0.3 && (join.1[2] - BOLLARD[2]).abs() < 0.3,
            "the legs meet at {:?}, not at the bollard at {BOLLARD:?} — the \\
             rope is not turned round anything",
            join.1
        );
        // And the far end leaves the pad, or the hawser is heaving on itself.
        let far = [a0, b0, a1, b1]
            .into_iter()
            .map(|p| p[2])
            .fold(f32::MAX, f32::min);
        assert!(
            far <= -(PAD[2] * 0.5) + 0.05,
            "the hawser's free end stops at z = {far}, still on the pad"
        );
    }

    /// Everything stands on the pad it is nested under (#972 lessons 8, 19).
    #[test]
    fn every_part_stands_on_the_pad() {
        let g = built();
        let half = [PAD[0] * 0.5, PAD[2] * 0.5];
        let mut checked = 0;
        for p in measure::solids(&g) {
            // The hawser leaves the pad on purpose — that is the point of it.
            if p.kind_tag == "Cylinder" && p.bounds.size().y < 0.2 && p.bounds.min.z < -half[1] {
                continue;
            }
            checked += 1;
            assert!(
                p.bounds.min.x >= -half[0] - 1e-3 && p.bounds.max.x <= half[0] + 1e-3,
                "{} at {:?} overhangs the pad in X",
                p.kind_tag,
                p.bounds.center()
            );
        }
        assert!(checked > 12, "only {checked} parts examined");
    }
}
