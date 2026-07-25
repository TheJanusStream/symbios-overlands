//! Swing set — a Suburban prop. A galvanised A-frame swing set with two
//! chain-hung seats: the centrepiece of a back yard.

use crate::catalogue::items::coastal_resort::{POOL_AQUA, water};
use crate::catalogue::items::util::{
    assemble, cuboid_tapered, cylinder_tapered, id_quat, prim, quat_x, solid, torus,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::Generator;
use crate::seeded_defaults::ThemeArchetype;

use super::enamel;

/// Galvanised steel frame.
const FRAME: [f32; 3] = [0.60, 0.62, 0.64];
/// Dark chain.
const SEAT: [f32; 3] = [0.14, 0.16, 0.18];
/// Bright seat colours — a red bucket seat and a blue plank seat.
const SEAT_RED: [f32; 3] = [0.75, 0.18, 0.16];
const SEAT_BLUE: [f32; 3] = [0.18, 0.34, 0.62];
/// Seat width, across the swing's travel.
const SEAT_W: f32 = 0.5;
/// How far in from the seat's centreline each chain hangs — just inside the
/// ends, which is where a real swing's chains are shackled.
const CHAIN_INSET: f32 = SEAT_W * 0.5 - 0.03;

pub struct SwingSet;

impl CatalogueEntry for SwingSet {
    fn slug(&self) -> &'static str {
        "swing_set"
    }
    fn name(&self) -> &'static str {
        "Swing Set"
    }
    fn description(&self) -> &'static str {
        "Galvanised A-frame swing set with two chain-hung seats."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Prop
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::Suburban]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::SUB_BAND
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: 2.0,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

fn build_tree() -> Generator {
    let bar_y = 2.4_f32;
    let leg_len = 2.6_f32;
    let splay = 0.358_f32;

    // Top bar — the root.
    let mut prims = vec![prim(
        solid(cuboid_tapered([4.0, 0.12, 0.12], 0.0, enamel(FRAME))),
        [0.0, bar_y, 0.0],
        id_quat(),
    )];

    // A-frame legs at each end, splayed fore and aft.
    //
    // The tilt is **negative** in `sz`, and that sign is the whole shape.
    // `quat_x(θ)` carries a prim's local `+Y` toward `+Z`, so a positive tilt
    // on the leg standing at `+z` throws its *top* further out and drags its
    // *foot* toward the centreline — a V, meeting at the ground, with the top
    // bar bridging thin air (#973). Negating it converges the tops on `z = 0`
    // under the bar and splays the feet, which is the A the frame is named
    // for. `splay` was always tuned for this sign: at 0.358 rad a half-leg
    // reaches `1.3 · sin θ = 0.455`, so a top starting at `z = 0.45` lands on
    // the centreline to within 6 mm.
    for sx in [-1.0_f32, 1.0] {
        for sz in [-1.0_f32, 1.0] {
            prims.push(prim(
                solid(cuboid_tapered([0.1, leg_len, 0.1], 0.0, enamel(FRAME))),
                [sx * 2.0, bar_y - leg_len * 0.5 + 0.05, sz * 0.45],
                quat_x(-sz * splay),
            ));
        }
    }

    // Two chain-hung seats — one red, one blue.
    //
    // A seat hangs from its **sides**, so the pair of chains is offset along
    // the seat's width (X) and sits on its depth centreline (Z). Offsetting
    // them in Z instead hung both chains off the seat's front and back edges,
    // on one point of its width — a swing that would corkscrew rather than
    // swing, and visibly wrong from the side (#974). `SEAT_W` is the seat's
    // width and the chain inset is derived from it, so the two cannot drift.
    for (i, sx) in [-0.85_f32, 0.85].into_iter().enumerate() {
        for cx in [-CHAIN_INSET, CHAIN_INSET] {
            prims.push(prim(
                solid(cuboid_tapered([0.03, 1.45, 0.03], 0.0, enamel(SEAT))),
                [sx + cx, bar_y - 0.75, 0.0],
                id_quat(),
            ));
        }
        let seat_col = if i == 0 { SEAT_RED } else { SEAT_BLUE };
        prims.push(prim(
            solid(cuboid_tapered([SEAT_W, 0.08, 0.26], 0.0, enamel(seat_col))),
            [sx, bar_y - 1.5, 0.0],
            id_quat(),
        ));
    }

    // A small kiddie pool beside the frame: an inflatable rim and aqua water.
    let px = 2.9_f32;
    prims.push(prim(
        solid(torus(0.16, 0.85, enamel([0.92, 0.55, 0.2]))),
        [px, 0.16, 0.0],
        id_quat(),
    ));
    prims.push(prim(
        cylinder_tapered(0.8, 0.1, 16, 0.0, water(POOL_AQUA)),
        [px, 0.12, 0.0],
        id_quat(),
    ));

    assemble(prims)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::assert_sanitize_stable;
    use crate::pds::GeneratorKind;

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&SwingSet.build(""), "swing_set");
    }

    /// #974: each seat hangs from its **sides**.
    ///
    /// The chains straddle the seat's width and sit on its depth centreline.
    /// Offset along the depth instead — which is how this was authored — both
    /// chains hang off the seat's front and back edges at one point of its
    /// width, so the seat is slung on an axis it would corkscrew around rather
    /// than swing on.
    #[test]
    fn the_chains_hang_from_the_sides_of_each_seat() {
        let root = SwingSet.build("");
        let mut seats = 0;
        for seat in &root.children {
            let GeneratorKind::Cuboid { size, material, .. } = &seat.kind else {
                continue;
            };
            if size.0[0] != SEAT_W || material.base_color.0 == FRAME {
                continue;
            }
            let at = seat.transform.translation.0;
            // The chains are the slender verticals above this seat.
            let mut chains: Vec<[f32; 3]> = root
                .children
                .iter()
                .filter_map(|c| match &c.kind {
                    GeneratorKind::Cuboid { size, .. } if size.0[0] < 0.05 => {
                        Some(c.transform.translation.0)
                    }
                    _ => None,
                })
                .filter(|t| (t[0] - at[0]).abs() < SEAT_W && t[1] > at[1])
                .collect();
            chains.sort_by(|a, b| a[0].total_cmp(&b[0]));
            assert_eq!(chains.len(), 2, "seat at {at:?} is not hung by two chains");
            for c in &chains {
                assert!(
                    (c[2] - at[2]).abs() < 1e-4,
                    "chain at {c:?} hangs off the seat's front or back edge, not its side"
                );
            }
            let span = chains[1][0] - chains[0][0];
            assert!(
                span > SEAT_W * 0.7 && span < SEAT_W,
                "chains span {span} across a {SEAT_W} m seat — they must straddle \
                 its width without overhanging it"
            );
            seats += 1;
        }
        assert_eq!(seats, 2, "expected two hung seats");
    }

    /// #973: the legs make an **A**, not a V.
    ///
    /// The frame is four tilted legs whose splay comes entirely from the sign
    /// of a rotation, so the failure is a one-character edit that leaves the
    /// code looking right, the sanitiser happy and the prop standing on a
    /// pinched point with the top bar bridging nothing. Each leg's top must
    /// converge on the bar's centreline and its foot must splay away from it.
    #[test]
    fn the_legs_converge_under_the_top_bar() {
        /// Rotate a local point by a unit quaternion `[x, y, z, w]`.
        fn rotate(q: [f32; 4], p: [f32; 3]) -> [f32; 3] {
            let (qx, qy, qz, qw) = (q[0], q[1], q[2], q[3]);
            // t = 2 · (q_vec × p), then p + q_w · t + q_vec × t.
            let t = [
                2.0 * (qy * p[2] - qz * p[1]),
                2.0 * (qz * p[0] - qx * p[2]),
                2.0 * (qx * p[1] - qy * p[0]),
            ];
            [
                p[0] + qw * t[0] + qy * t[2] - qz * t[1],
                p[1] + qw * t[1] + qz * t[0] - qx * t[2],
                p[2] + qw * t[2] + qx * t[1] - qy * t[0],
            ]
        }

        let root = SwingSet.build("");
        let mut legs = 0;
        for leg in &root.children {
            let GeneratorKind::Cuboid { size, .. } = &leg.kind else {
                continue;
            };
            // The legs are the only tilted prims in the tree.
            let q = leg.transform.rotation.0;
            if q[0].abs() < 1e-4 {
                continue;
            }
            let half = size.0[1] * 0.5;
            let t = leg.transform.translation.0;
            let end = |sy: f32| {
                let r = rotate(q, [0.0, sy * half, 0.0]);
                [t[0] + r[0], t[1] + r[1], t[2] + r[2]]
            };
            let (top, foot) = (end(1.0), end(-1.0));
            assert!(
                top[1] > foot[1],
                "leg at {t:?} is upside down: top {top:?}, foot {foot:?}"
            );
            assert!(
                top[2].abs() < 0.05,
                "leg at {t:?} meets the bar at z = {}, not on its centreline — \
                 the legs splay upward and the frame is a V",
                top[2]
            );
            assert!(
                foot[2].abs() > 0.6,
                "leg at {t:?} plants its foot at z = {}, too close to the \
                 centreline to stand the frame up",
                foot[2]
            );
            // And the top reaches the bar it is meant to hold. `assemble`
            // rebases every child against the root, and the root *is* the top
            // bar, so a leg's coordinates are already relative to the bar's
            // centre — the top must land within the bar's own depth of zero.
            assert!(
                top[1].abs() < 0.1,
                "leg at {t:?} stops {} from the bar's centre",
                top[1]
            );
            legs += 1;
        }
        assert_eq!(legs, 4, "expected four tilted A-frame legs");
    }
}
