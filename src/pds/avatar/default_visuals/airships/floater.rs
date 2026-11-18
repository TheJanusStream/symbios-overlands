//! The organic floater: a living gasbag of the alien worlds - a jellyfish
//! bell wider than it is tall, glowing radial canals and spots on it, a
//! scalloped margin, and long tentacles that ripple round the ring and trail
//! when she sets off; four oral arms from the adorned tier, a crown of
//! glowing nodules and beaded tentacle tips on the ornate (#1663, epic
//! #1656).
//!
//! Prototyped beside the Zeppelin (`target/dump/airships-1656/floater.py`)
//! and ported number for number, the fan-out's six in one pass. Her hull plan
//! stands on end, as the balloon's does: share `x` runs from her crown down
//! to her margin. Her glow - canals, spots, arms and nodules - is the seeded
//! accent's own light, where the prototype gave each sketch a fixed one: a
//! living thing wears its owner's colour as its light. Her tentacles' tips
//! are beads the colour of her margin, where the prototype lit them: a lamp
//! at a swinging tip would light the ground in a sweep its bead hardly shows.
//!
//! # How she moves
//!
//! She swims as a jellyfish does (#1944, epic #1940): every [`PULSE_S`] her
//! bell draws in round its rim and she heaves up, then it opens out again
//! as she sinks back. Her visuals root is the gait's to move, so the pulse
//! is carried by two hidden nodes under it ([`pulse_body`], [`pulse_bell`]):
//!
//! * a **body** at her origin, which slides her whole up and down
//!   ([`HEAVE`] of her length's scale either way);
//! * under it, a **bell** node on her axis that swells across it alone -
//!   weights `[1, 0, 1]`, no kept volume - by [`CONTRACT`] either way, in
//!   phase with the heave and against it, so she is narrowest at the top of
//!   her rise. The bell, its canals and spots, her margin and everything that
//!   hangs from her subumbrella (her oral arms) are its children, and take
//!   its scale: they stay on its surface.
//! * Her tentacles' hubs hang from the body, not the bell, where a swelling
//!   parent would thin them as it narrows. Each hub slides along its own
//!   bearing out from her axis by the bell's amount times its radius, on the
//!   same clock, so it rides the rim exactly: across the bell's axis a point
//!   at radius `r` moves `r` times the amount, and so does the hub.
//!
//! Each tentacle still swings a few degrees about +X from its hub, an eighth
//! of a period behind its neighbour at the pulse's own period, so one ripple
//! runs round the ring; and leans against her acceleration, so the tentacles
//! trail when she sets off and swing forward when she stops - and, since a
//! lean reads a part's travel as drawn, lean a degree or so with her pulse.
//! From the adorned tier a light under her bell, on an oral arm, breathes at
//! the pulse's period; a plain floater draws nothing glowing under her bell
//! for a light to stand on, and carries none. Her box holds everything her
//! clock terms draw at every moment of the pulse, the tentacles' swing
//! included ([`super::swept_bounds`]); a lean follows her travel and is left
//! out of it, so a tentacle's tip may stand a few centimetres past the box
//! while she pulses or sets off - a drawn overhang, nothing that collides.

use crate::pds::avatar::default_visuals::lights;
use crate::pds::avatar::parts::PartCtx;
use crate::pds::generator::Generator;
use crate::pds::types::Fp3;
use crate::pds::{Fp, Slide, SlideTerm, Spin, SpinTerm, Swell, SwellTerm};
use crate::seeded_defaults::{AirshipBlueprint, OrnatenessTier, ParticleAura};

use super::super::Propulsion;
use super::super::common::{
    cuboid, id_quat, prim, quat_xyzw, quat_y, sphere, superellipsoid, sweep, with_cut,
};
use super::{
    AirshipCraft, AirshipFeel, FloaterColours, HullPlan, HullProfile, MIN_DIM, dim, floater_colours,
};

/// Her bell, crown down: a rounded dome to her widest at the margin - a
/// quarter of the blueprint's length tall, at an eighth of its fineness.
pub(super) const PROFILE: HullProfile = HullProfile {
    nose: 0.86,
    parallel: 0.14,
    nose_power: 1.7,
    tail_power: 2.0,
    stations: &[
        0.0, 0.02, 0.06, 0.12, 0.20, 0.30, 0.42, 0.55, 0.68, 0.80, 0.90, 0.98,
    ],
    len_scale: 0.26,
    fine_scale: 0.125,
};

/// Her margin, as a share of her height from the crown.
pub(super) const MARGIN_X: f32 = 0.98;
const BELL_RES: u32 = 32;
const BELL_SAMPLES: u32 = 6;
/// Her radial canals: how many, and each one's width (turns).
const CANALS: usize = 8;
const CANAL_W: f32 = 0.018;
/// Her tentacles: how many, how long, and their swing and lean (degrees;
/// seconds; degrees for each m/s^2).
const TENTACLES: usize = 8;
const TENT_LEN: f32 = 1.15;
const SWING_DEG: f32 = 7.0;
const SWING_S: f32 = 2.6;
const LEAN_GAIN: f32 = 2.0;
const LEAN_S: f32 = 2.4;
/// Her pulse (s): one contraction and one relaxing of her bell, one heave up
/// and back - the tentacles' own ripple, so the whole animal beats as one.
const PULSE_S: f32 = SWING_S;
/// How far her bell draws in across its axis at the top of her rise, and
/// opens out at the bottom of her sink, as a share of its drawn width.
const CONTRACT: f32 = 0.11;
/// How far she heaves up and sinks back either side of where she is drawn,
/// authored in the prototype's metres ([`HullPlan::len`]) so every size of
/// floater pulses in proportion.
const HEAVE: f32 = 0.06;

pub(super) struct Floater;

impl AirshipCraft for Floater {
    fn plan(&self, bp: &AirshipBlueprint) -> HullPlan {
        HullPlan::new(bp, PROFILE)
    }

    fn build(&self, ctx: &PartCtx, plan: &HullPlan) -> Generator {
        let c = floater_colours(ctx);
        let mut root = hidden(plan, &c);
        let mut bell_node = pulse_bell(plan, &c);
        bell(&mut bell_node.children, plan, &c);
        dress(&mut bell_node.children, plan, &c, ctx.ornateness);
        let mut body = pulse_body(plan, &c);
        body.children.push(bell_node);
        tentacles(&mut body.children, plan, &c, ctx.ornateness);
        root.children.push(body);
        root
    }

    fn feel(&self) -> AirshipFeel {
        // She pulses along: 5 of drive over 0.7 tops out at 7.1 m/s, light,
        // and lazy in a turn.
        AirshipFeel {
            mass_factor: 0.7,
            drive_accel: 5.0,
            yaw_accel: 4.0,
            linear_damping: 0.7,
            angular_damping: 3.5,
        }
    }

    fn fx_mount(&self, _aura: ParticleAura, plan: &HullPlan) -> [f32; 3] {
        // Everything drifts from under her bell, among her tentacles.
        [0.0, plan.z(MARGIN_X) - plan.len(0.10), 0.0]
    }

    fn propulsion(&self) -> Propulsion {
        Propulsion::Sail
    }
}

/// A hidden node at her origin: a dark cube a fiftieth of her length
/// across, inside her bell - her root, and the two that carry her pulse.
fn hidden(plan: &HullPlan, c: &FloaterColours) -> Generator {
    let side = dim(plan.len(0.02));
    prim(cuboid([side; 3], c.dark.clone()), [0.0; 3], id_quat())
}

/// One swing of her pulse, `amount` either side: the heave's metres, the
/// bell's share or a hub's metres - all on one clock, with one phase.
fn beat(amount: f32) -> (Fp, Fp, Fp) {
    (Fp(amount), Fp(PULSE_S), Fp::ZERO)
}

/// Her body: a hidden node that heaves her up and down, [`HEAVE`] either
/// way, at the top of the swing as her bell is at its narrowest.
fn pulse_body(plan: &HullPlan, c: &FloaterColours) -> Generator {
    let (amplitude, period, phase) = beat(plan.len(HEAVE));
    let mut body = hidden(plan, c);
    body.slide = Some(Slide::along(
        [0.0, 1.0, 0.0],
        SlideTerm::Swing {
            amplitude,
            period,
            phase,
        },
    ));
    body
}

/// Her bell's node: hidden on her axis, it swells across it alone and
/// against her heave - [`CONTRACT`] narrower as she rises, as much wider as
/// she sinks - taking everything on and under the bell with it.
fn pulse_bell(plan: &HullPlan, c: &FloaterColours) -> Generator {
    let (amount, period, phase) = beat(-CONTRACT);
    let mut node = hidden(plan, c);
    node.swell = Some(Swell {
        weights: Fp3([1.0, 0.0, 1.0]),
        keep_volume: false,
        terms: vec![SwellTerm::Swing {
            amount,
            period,
            phase,
        }],
    });
    node
}

fn bell(kids: &mut Vec<Generator>, plan: &HullPlan, c: &FloaterColours) {
    kids.push(prim(
        sweep(&plan.upright(0.0), BELL_RES, BELL_SAMPLES, c.bell.clone()),
        [0.0; 3],
        id_quat(),
    ));
    // Her radial canals, glowing faintly down the bell.
    for k in 0..CANALS {
        let a = k as f32 / CANALS as f32;
        kids.push(prim(
            with_cut(
                sweep(
                    &plan.upright(plan.len(0.003)),
                    BELL_RES,
                    BELL_SAMPLES,
                    c.canal.clone(),
                ),
                [a, a + CANAL_W],
                [0.08, 1.0],
                0.0,
            ),
            [0.0; 3],
            id_quat(),
        ));
    }
    let ym = plan.z(MARGIN_X);
    let rm = plan.radius(MARGIN_X);
    // The subumbrella: a darker disc just inside the margin.
    kids.push(prim(
        superellipsoid(
            [rm * 0.94, plan.len(0.03), rm * 0.94],
            1.0,
            1.0,
            8,
            32,
            c.under.clone(),
        ),
        [0.0, ym - plan.len(0.012), 0.0],
        id_quat(),
    ));
    // The scalloped margin: a lobe between each pair of canals.
    for k in 0..CANALS * 2 {
        let a = (k as f32 + 0.5) / (CANALS * 2) as f32 * std::f32::consts::TAU;
        kids.push(prim(
            superellipsoid(
                [plan.len(0.10), plan.len(0.035), plan.len(0.05)],
                0.9,
                0.9,
                8,
                12,
                c.margin.clone(),
            ),
            [
                a.cos() * rm * 0.97,
                ym - plan.len(0.02),
                a.sin() * rm * 0.97,
            ],
            quat_xyzw(quat_y(-a)),
        ));
    }
    // Glowing spots on the bell, one between each pair of canals.
    let (ys, rs) = (plan.z(0.45), plan.radius(0.45) - plan.len(0.012));
    for k in 0..CANALS {
        let a = (k as f32 + 0.5) / CANALS as f32 * std::f32::consts::TAU;
        kids.push(prim(
            sphere(plan.len(0.03), 3, c.glow.clone()),
            [a.cos() * rs, ys, a.sin() * rs],
            id_quat(),
        ));
    }
}

fn tentacles(kids: &mut Vec<Generator>, plan: &HullPlan, c: &FloaterColours, tier: OrnatenessTier) {
    let ym = plan.z(MARGIN_X);
    let rm = plan.radius(MARGIN_X);
    let l = plan.len(TENT_LEN);
    for k in 0..TENTACLES {
        let a = (k as f32 + 0.25) / TENTACLES as f32 * std::f32::consts::TAU;
        let at = [
            a.cos() * rm * 0.80,
            ym - plan.len(0.01),
            a.sin() * rm * 0.80,
        ];
        let mut parts = vec![prim(
            sweep(
                &[
                    ([0.0, plan.len(0.03), 0.0], plan.len(0.030)),
                    ([0.0, -l * 0.25, -plan.len(0.05)], plan.len(0.026)),
                    ([0.0, -l * 0.55, plan.len(0.02)], plan.len(0.020)),
                    ([0.0, -l * 0.80, -plan.len(0.06)], plan.len(0.015)),
                    ([0.0, -l, -plan.len(0.10)], MIN_DIM),
                ],
                10,
                4,
                c.tentacles.clone(),
            ),
            [0.0; 3],
            id_quat(),
        )];
        if tier == OrnatenessTier::Ornate {
            // A bead at the tip, the margin's colour rather than a glow (see
            // the module docs).
            parts.push(prim(
                sphere(plan.len(0.018), 3, c.margin.clone()),
                [0.0, -l, -plan.len(0.10)],
                id_quat(),
            ));
        }
        let mut hub = hidden(plan, c);
        hub.transform.translation = Fp3(at);
        hub.children = parts;
        // Out along her bearing as her bell opens, in as it draws in: the
        // bell's own amount times the hub's radius, so it rides the rim.
        let (amplitude, period, phase) = beat(-CONTRACT * rm * 0.80);
        hub.slide = Some(Slide::along(
            [a.cos(), 0.0, a.sin()],
            SlideTerm::Swing {
                amplitude,
                period,
                phase,
            },
        ));
        hub.spin = Some(Spin {
            axis: Fp3([1.0, 0.0, 0.0]),
            terms: vec![
                SpinTerm::Swing {
                    amplitude: Fp(SWING_DEG),
                    period: Fp(SWING_S),
                    phase: Fp(45.0 * k as f32),
                },
                SpinTerm::Lean {
                    gain: Fp(LEAN_GAIN),
                    period: Fp(LEAN_S),
                },
            ],
        });
        kids.push(hub);
    }
}

fn dress(kids: &mut Vec<Generator>, plan: &HullPlan, c: &FloaterColours, tier: OrnatenessTier) {
    if tier == OrnatenessTier::Plain {
        return;
    }
    let ym = plan.z(MARGIN_X);
    // Four oral arms from the bell's middle, frilled and glowing.
    let l = plan.len(0.70);
    for k in 0..4 {
        let a = (k as f32 + 0.5) / 4.0 * std::f32::consts::TAU;
        let (ox, oz) = (a.cos() * plan.len(0.10), a.sin() * plan.len(0.10));
        let elbow = [ox * 1.3, ym - l * 0.35, oz * 1.3];
        let arm = prim(
            sweep(
                &[
                    ([ox, ym + plan.len(0.02), oz], plan.len(0.055)),
                    (elbow, plan.len(0.045)),
                    ([ox * 0.9, ym - l * 0.70, oz * 0.9], plan.len(0.032)),
                    ([ox * 1.2, ym - l, oz * 1.2], plan.len(0.016)),
                ],
                12,
                4,
                c.canal.clone(),
            ),
            [0.0; 3],
            id_quat(),
        );
        // One light under her bell, on the first arm where its sweep passes
        // through its second station: it breathes at her pulse's period.
        kids.push(if k == 0 {
            lights::breath(arm, elbow, PULSE_S)
        } else {
            arm
        });
    }
    if tier == OrnatenessTier::Ornate {
        // A crown of glowing nodules round the bell's top.
        let (yc, rc) = (plan.z(0.14), plan.radius(0.14) - plan.len(0.01));
        for k in 0..5 {
            let a = k as f32 / 5.0 * std::f32::consts::TAU;
            kids.push(prim(
                sphere(plan.len(0.035), 3, c.glow.clone()),
                [a.cos() * rc, yc + plan.len(0.01), a.sin() * rc],
                id_quat(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    //! The floater's pulse (#1944): her hubs ride her rim, she rises as her
    //! bell draws in, her box holds her through it, the sanitiser and her
    //! byte guard leave it be, and her one light glows where it is seen.

    use bevy::math::{Affine3A, Vec2, Vec3};

    use super::super::swept_bounds;
    use super::*;
    use crate::catalogue::items::measure::{transform_of, tree_bounds};
    use crate::pds::avatar::AvatarRecord;
    use crate::pds::{LightShape, LightSource};
    use crate::seeded_defaults::{AirshipType, ChassisFamily};
    use crate::world_builder::spin::Spinner;

    /// The copy offset every spinner here reads (s): any one, shared, as one
    /// copy of a body shares it.
    const EPOCH: f64 = 1234.5;

    /// A UTC second count, where an f32 clock would have lost the pulse.
    const LATE: f64 = 1.8e9;

    /// The floater seeds under `n`.
    fn floater_seeds(n: u64) -> impl Iterator<Item = u64> {
        (0..n).filter(|&s| {
            ChassisFamily::for_seed(s) == ChassisFamily::Airship
                && AirshipType::for_seed(s) == AirshipType::Floater
        })
    }

    /// Her blueprint corners and the nominal, as the family's guards sweep.
    fn blueprints() -> Vec<AirshipBlueprint> {
        let mut out = AirshipBlueprint::corners();
        out.push(AirshipBlueprint::NOMINAL);
        out
    }

    /// Her plan and tree at `bp` on `tier`, her own frame.
    fn built(bp: &AirshipBlueprint, tier: OrnatenessTier) -> (HullPlan, Generator) {
        let seed = floater_seeds(2_000).next().expect("some seed is a floater");
        let mut ctx = PartCtx::for_seed(seed);
        ctx.ornateness = tier;
        let plan = Floater.plan(bp);
        let root = Floater.build(&ctx, &plan);
        (plan, root)
    }

    /// Her body, her bell's node and her tentacles' hubs, under `root`.
    fn pulse_parts(root: &Generator) -> (&Generator, &Generator, Vec<&Generator>) {
        let body = root
            .children
            .iter()
            .find(|g| g.slide.is_some())
            .expect("her body heaves");
        let bell = body
            .children
            .iter()
            .find(|g| g.swell.is_some())
            .expect("her bell swells");
        let hubs = body.children.iter().filter(|g| g.spin.is_some()).collect();
        (body, bell, hubs)
    }

    /// The runtime's spinner for `node`, at its authored pose.
    fn spinner(node: &Generator) -> Spinner {
        Spinner::of_node(node, transform_of(&node.transform), EPOCH).expect("it moves")
    }

    /// Every tree the rim guard reads: her built tree at every blueprint,
    /// and the Catalogue's floater as a peer reads her record.
    fn trees() -> Vec<(String, Generator)> {
        let mut out: Vec<(String, Generator)> = blueprints()
            .iter()
            .map(|bp| {
                let what = format!("a {} m floater, fineness {}", bp.length, bp.fineness);
                (what, built(bp, OrnatenessTier::Plain).1)
            })
            .collect();
        let entry =
            crate::catalogue::avatars::avatar_by_slug("avatar_airship_floater").expect("shipped");
        let json = serde_json::to_string(entry.record().expect("builds")).expect("writes");
        let record: AvatarRecord = serde_json::from_str(&json).expect("reads");
        let root = record.body.visuals().expect("she draws a tree").clone();
        out.push(("the Catalogue's floater, read back".into(), root));
        out
    }

    /// Each of her tentacles' hubs stays on her bell's rim at its own
    /// bearing through her whole pulse, within a millimetre: where the
    /// bell's swell carries the point the hub hangs at, the hub's own slide
    /// carries the hub - as authored and at Effects Reduced, at the clock's
    /// start and at a UTC second count, at every blueprint and in the
    /// Catalogue's record as a peer reads it. And the rim does move: each
    /// hub travels the contraction times its radius.
    #[test]
    fn every_hub_rides_her_rim_through_her_pulse() {
        let mut hubs_read = 0;
        for (what, root) in trees() {
            let (_, bell, hubs) = pulse_parts(&root);
            assert_eq!(hubs.len(), TENTACLES, "{what}");
            assert_eq!(
                bell.transform,
                crate::pds::TransformData::default(),
                "{what}: her bell's node stands on her axis"
            );
            let bell = spinner(bell);
            for hub in hubs {
                let rest = Vec3::from(hub.transform.translation.0);
                let radius = Vec2::new(rest.x, rest.z).length();
                let hub = spinner(hub);
                let mut travel = 0.0_f32;
                for start in [0.0, LATE] {
                    for calm in [1.0, 0.5] {
                        for k in 0..64 {
                            let now = start + f64::from(PULSE_S) * f64::from(k) / 64.0;
                            let rim = bell.clock_pose(now, calm).transform_point(rest);
                            let at = hub.clock_pose(now, calm).translation;
                            assert!(
                                rim.distance(at) < 1e-3,
                                "{what}: at {now} s (calm {calm}) the hub at {at} is {:.5} m \
                                 off its rim point {rim}",
                                rim.distance(at)
                            );
                            travel = travel.max(at.distance(rest));
                        }
                    }
                }
                assert!(
                    travel > 0.99 * CONTRACT * radius,
                    "{what}: a hub at {radius:.3} m travels only {travel:.4} m"
                );
                hubs_read += 1;
            }
        }
        assert_eq!(hubs_read, TENTACLES * 6, "every hub of every tree");
    }

    /// She heaves up as her bell draws in and sinks as it opens, in step:
    /// her bell's width across its axis is `1 - CONTRACT` times her heave's
    /// share of its reach at every moment, never along it, and she reaches
    /// her whole heave and her whole contraction a quarter of a pulse in.
    #[test]
    fn she_rises_as_her_bell_draws_in() {
        let (plan, root) = built(&AirshipBlueprint::NOMINAL, OrnatenessTier::Plain);
        let (body, bell, _) = pulse_parts(&root);
        let (body, bell) = (spinner(body), spinner(bell));
        let heave = plan.len(HEAVE);
        let (mut highest, mut narrowest) = (f32::MIN, f32::MAX);
        for k in 0..96 {
            let now = f64::from(PULSE_S) * f64::from(k) / 96.0 - EPOCH;
            let y = body.clock_pose(now, 1.0).translation.y;
            let s = bell.clock_pose(now, 1.0).scale;
            assert!(
                (s.y - 1.0).abs() < 1e-6 && (s.x - s.z).abs() < 1e-6,
                "her bell swells across its axis alone: {s}"
            );
            let drawn_in = s.x - 1.0;
            assert!(
                (drawn_in + CONTRACT * y / heave).abs() < 1e-4,
                "at {k}/96 she is {y:.4} m up with her bell at {}",
                s.x
            );
            highest = highest.max(y);
            narrowest = narrowest.min(s.x);
        }
        // A quarter of a pulse in, from the clock's start: the top of both.
        let top = f64::from(PULSE_S) / 4.0 - EPOCH;
        assert!((body.clock_pose(top, 1.0).translation.y - heave).abs() < 1e-5);
        assert!((bell.clock_pose(top, 1.0).scale.x - (1.0 - CONTRACT)).abs() < 1e-5);
        assert!(highest > 0.999 * heave && narrowest < 1.0 - 0.999 * CONTRACT);
    }

    /// Every vertex of every part she draws, posed as the clock poses her at
    /// `now` (as authored), in her own frame.
    fn posed_points(root: &Generator, now: f64) -> Vec<Vec3> {
        fn walk(g: &Generator, parent: Affine3A, now: f64, out: &mut Vec<Vec3>) {
            let base = transform_of(&g.transform);
            let pose = Spinner::of_node(g, base, EPOCH).map_or(base, |s| s.clock_pose(now, 1.0));
            let frame = parent * pose.compute_affine();
            out.extend(
                crate::world_builder::primitive_outline(&g.kind)
                    .into_iter()
                    .map(|p| frame.transform_point3(p)),
            );
            for child in &g.children {
                walk(child, frame, now, out);
            }
        }
        let mut out = Vec::new();
        walk(root, Affine3A::IDENTITY, now, &mut out);
        out
    }

    /// Her box holds every vertex she draws at every moment of her pulse -
    /// heaved up and drawn in, sunk and opened out, her tentacles swung
    /// either way - at every blueprint and on every tier.
    #[test]
    fn her_box_holds_her_at_every_moment_of_her_pulse() {
        for bp in blueprints() {
            for tier in OrnatenessTier::ALL {
                let (_, root) = built(&bp, tier);
                let b = swept_bounds(&root);
                let slack = Vec3::splat(1e-4);
                for k in 0..48 {
                    let now = f64::from(PULSE_S) * f64::from(k) / 48.0;
                    for p in posed_points(&root, now) {
                        assert!(
                            p.cmpge(b.min - slack).all() && p.cmple(b.max + slack).all(),
                            "a {} m floater, fineness {}, {}: at {now:.3} s she draws {p} \
                             outside her box {}..{}",
                            bp.length,
                            bp.fineness,
                            tier.label(),
                            b.min,
                            b.max
                        );
                    }
                }
            }
        }
    }

    /// Up to three seeded floaters of each tier, and the Catalogue's: what
    /// each is, its seed and its record.
    fn records() -> Vec<(String, u64, AvatarRecord)> {
        let mut out = Vec::new();
        for tier in OrnatenessTier::ALL {
            out.extend(
                floater_seeds(20_000)
                    .filter(|&s| PartCtx::for_seed(s).ornateness == tier)
                    .take(3)
                    .map(|s| {
                        let what = format!("seed {s} ({})", tier.label());
                        (what, s, AvatarRecord::default_for_seed(s))
                    }),
            );
        }
        let entry =
            crate::catalogue::avatars::avatar_by_slug("avatar_airship_floater").expect("shipped");
        out.push((
            "the Catalogue's floater".into(),
            entry.seed().expect("found"),
            entry.record().expect("builds").clone(),
        ));
        out
    }

    /// The avatar sanitiser leaves her record byte for byte as she is
    /// built - her body's heave, her bell's swell, every hub's slide and her
    /// light all kept, the swell's peak well inside the avatar scale cap -
    /// on every tier and in the Catalogue.
    #[test]
    fn the_sanitiser_leaves_her_pulse_as_built() {
        let records = records();
        assert_eq!(records.len(), 10, "three seeds of every tier");
        for (what, _, record) in records {
            let before = serde_json::to_string(&record).expect("writes");
            let mut sanitized = record.clone();
            sanitized.sanitize();
            assert_eq!(
                serde_json::to_string(&sanitized).expect("writes"),
                before,
                "{what}: the sanitiser rewrote her"
            );
            let root = sanitized.body.visuals().expect("she draws a tree");
            let (body, bell, hubs) = pulse_parts(root);
            assert!(body.moving_slide().is_some() && bell.moving_swell().is_some());
            assert!(hubs.iter().all(|h| h.moving_slide().is_some()), "{what}");
            assert!(
                crate::pds::sanitize::accumulated_scale(root) <= 1.0 + CONTRACT + 1e-6,
                "{what}"
            );
        }
    }

    /// Her record stays inside the airships' guard - a third of the soft
    /// budget (`a_seeded_airships_record_stays_well_inside_the_budget`) - on
    /// every tier: the heaviest seeded floater under 20,000 of each, and her
    /// heaviest drawn corner of each.
    #[test]
    fn her_record_stays_inside_the_airships_third_of_the_budget() {
        use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
        for tier in OrnatenessTier::ALL {
            let seeded = floater_seeds(20_000)
                .filter(|&s| PartCtx::for_seed(s).ornateness == tier)
                .map(|s| {
                    let bytes = serialized_record_bytes(&AvatarRecord::default_for_seed(s))
                        .expect("a seeded record serializes");
                    (bytes, s)
                })
                .max()
                .expect("a floater of every tier");
            let drawn = blueprints()
                .iter()
                .map(|bp| serialized_record_bytes(&built(bp, tier).1).expect("serializes"))
                .max()
                .expect("a corner");
            println!(
                "{}: heaviest seeded record {} bytes (seed {}), heaviest drawn tree {drawn}",
                tier.label(),
                seeded.0,
                seeded.1
            );
            for bytes in [seeded.0, drawn] {
                assert!(
                    bytes * 3 < SOFT_RECORD_BUDGET_BYTES,
                    "{}: {bytes} bytes, past a third of the {SOFT_RECORD_BUDGET_BYTES}-byte \
                     soft budget",
                    tier.label()
                );
            }
        }
    }

    /// Every light in `root`'s tree, the node it hangs on and that node's
    /// parent's frame, in her own frame.
    fn lights_of(root: &Generator) -> Vec<(&Generator, &LightSource, Affine3A)> {
        fn walk<'a>(
            g: &'a Generator,
            parent: Affine3A,
            out: &mut Vec<(&'a Generator, &'a LightSource, Affine3A)>,
        ) {
            if let Some(light) = &g.light {
                out.push((g, light, parent));
            }
            let frame = parent * transform_of(&g.transform).compute_affine();
            for child in &g.children {
                walk(child, frame, out);
            }
        }
        let mut out = Vec::new();
        walk(root, Affine3A::IDENTITY, &mut out);
        out
    }

    /// From the adorned tier she carries one light, her body's only one: a
    /// point light standing under her bell on a part that glows in her
    /// accent - the light's own colour - at a lamp's strength on every face,
    /// seen glowing from the rings of eyes the catalogue's lamps are read
    /// from (#1927), breathing from dusk to dawn once a pulse. A plain
    /// floater draws nothing glowing under her bell and carries none. Read
    /// at every blueprint, and on seeded records of every tier.
    #[test]
    fn her_one_light_breathes_under_her_bell_on_a_part_that_glows_where_it_is_seen() {
        use crate::catalogue::items::lamp_sight::{MIN_GLOW, glow_faults};
        for bp in blueprints() {
            for tier in OrnatenessTier::ALL {
                let (plan, root) = built(&bp, tier);
                let what = format!(
                    "a {} m floater, fineness {}, {}",
                    bp.length,
                    bp.fineness,
                    tier.label()
                );
                let lit = lights_of(&root);
                if tier == OrnatenessTier::Plain {
                    assert!(lit.is_empty(), "{what}: a plain floater carries a light");
                    continue;
                }
                assert_eq!(lit.len(), 1, "{what}: {} lights", lit.len());
                let (lamp, light, parent) = lit[0];
                let glow = lamp
                    .kind
                    .material()
                    .filter(|m| m.emission_strength.0 >= MIN_GLOW)
                    .unwrap_or_else(|| panic!("{what}: her light's part does not glow"));
                assert_eq!(light.color, glow.emission_color.on_wire(), "{what}");
                assert_eq!(light.shape, LightShape::Point, "{what}");
                assert!(light.dusk_to_dawn, "{what}: she glows from dusk to dawn");
                let Some(crate::pds::LightFlicker::Pulse { rate, .. }) = light.flicker else {
                    panic!("{what}: her light does not breathe");
                };
                assert!(
                    (1.0 / rate.0 - PULSE_S).abs() < 1e-3,
                    "{what}: she breathes every {} s",
                    1.0 / rate.0
                );
                let at = (parent * transform_of(&lamp.transform).compute_affine())
                    .transform_point3(Vec3::from(light.offset.0));
                let rm = plan.radius(MARGIN_X);
                assert!(
                    at.y < plan.z(MARGIN_X) && Vec2::new(at.x, at.z).length() < rm,
                    "{what}: her light stands at {at}, not under her bell"
                );
                let ground = tree_bounds(&root).expect("she draws").min.y;
                let faults = glow_faults(&root, ground, MIN_GLOW);
                assert!(faults.is_empty(), "{what}: {faults:?}");
            }
        }
        for (what, seed, record) in records() {
            let root = record.body.visuals().expect("she draws a tree");
            let adorned = PartCtx::for_seed(seed).ornateness != OrnatenessTier::Plain;
            assert_eq!(lights_of(root).len(), usize::from(adorned), "{what}");
        }
    }

    /// Her tentacles' leans read her own pulse - a lean reads a part's
    /// travel as drawn, and each hub slides in and out along its bearing -
    /// so they lean a little with every beat, never far: the most any hub
    /// leans, run on the game's own spin systems at 64 frames a second
    /// once the springs have settled, at her largest blueprint, is between a
    /// fifth of a degree and three degrees.
    #[test]
    fn her_pulse_leans_her_tentacles_a_little() {
        use bevy::prelude::*;
        let bp = AirshipBlueprint {
            length: 3.8,
            fineness: AirshipBlueprint::corners()
                .iter()
                .map(|b| b.fineness)
                .fold(f32::MAX, f32::min),
            ..AirshipBlueprint::NOMINAL
        };
        let (_, root) = built(&bp, OrnatenessTier::Plain);
        let (body, _, hubs) = pulse_parts(&root);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, TransformPlugin));
        app.insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(
            std::time::Duration::from_secs_f64(1.0 / 64.0),
        ));
        crate::world_builder::spin::register(
            &mut app,
            crate::world_builder::spin::SpinClock::Scene,
        );
        let origin = app.world_mut().spawn(Transform::IDENTITY).id();
        let base = |g: &Generator| transform_of(&g.transform);
        let body_at = app
            .world_mut()
            .spawn((
                Spinner::of_node(body, base(body), 0.0).expect("heaves"),
                base(body),
                InheritedVisibility::VISIBLE,
                ChildOf(origin),
            ))
            .id();
        let hubs: Vec<(Entity, Spinner)> = hubs
            .iter()
            .map(|hub| {
                let spinner = Spinner::of_node(hub, base(hub), 0.0).expect("swings");
                let at = app
                    .world_mut()
                    .spawn((
                        spinner.clone(),
                        base(hub),
                        InheritedVisibility::VISIBLE,
                        ChildOf(body_at),
                    ))
                    .id();
                (at, spinner)
            })
            .collect();
        let mut most = 0.0_f32;
        for frame in 0..64 * 30 {
            app.update();
            if frame < 64 * 15 {
                continue;
            }
            let now = app.world().resource::<Time>().elapsed_secs_f64();
            for (at, spinner) in &hubs {
                let about_x = |q: Quat| 2.0 * q.x.atan2(q.w);
                let drawn = about_x(app.world().get::<Transform>(*at).expect("posed").rotation);
                let clock = about_x(spinner.clock_pose(now, 1.0).rotation);
                most = most.max((drawn - clock).abs().to_degrees());
            }
        }
        println!("her tentacles lean at most {most:.2} degrees with her pulse");
        assert!(
            (0.2..3.0).contains(&most),
            "her tentacles lean {most:.2} degrees with her pulse"
        );
    }
}
