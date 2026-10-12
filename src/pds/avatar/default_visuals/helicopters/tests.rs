//! The helicopter family's guards (#1614): the boats' and the skiffs' set
//! for a rotorcraft - one machine at every blueprint extreme, the sanitiser
//! round trip, a third of the record budget, determinism, the gate - and
//! the ones a rotorcraft adds: the box holds her fuselage and rests on her
//! skids, her rotors turn at the agreed rates and sweep clear of her and of
//! each other, and her aura leaves her stack. Every one sweeps EVERY type
//! (the trainer's since #1621, the autogyro's since #1624), each held to
//! its own numbers - the two with a tail rotor, its own; and the trainer
//! has four of her own - her truss's struts drawn straight, the root's hub
//! hidden in her upper longeron, her stabiliser bar clear of her bubble,
//! and her identity on her door hoop. The autogyro has seven: her rotor,
//! her airscrew and her wheels turn as agreed, her airscrew's thrum is her
//! voice's at her top speed, she stands tail-down on three wheels, her
//! rotor clears her airframe, her airscrew clears her cowl ring, the root's
//! hub hides in her fuselage, and her identity is on her cheat line.

use bevy::math::{Affine3A, Quat, Vec3};

use super::super::common::{first_difference, touch};
use super::*;
use super::{autogyro, trainer, utility};
use crate::catalogue::items::measure::tree_bounds;
use crate::pds::generator::GeneratorKind;
use crate::seeded_defaults::{ChassisFamily, OrnatenessTier, VehicleBlueprint, WearTier};
use crate::world_builder::primitive_points;

fn a_helicopter_seed() -> u64 {
    (0u64..600)
        .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Helicopter)
        .expect("some seed is a helicopter")
}

/// The helicopter seeds under `n`.
fn helicopter_seeds(n: u64) -> impl Iterator<Item = u64> {
    (0..n).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Helicopter)
}

/// The blueprints a guard sweeps: every clamp corner, and the nominal -
/// proportion B, the machine the owner saw.
fn blueprints() -> Vec<HelicopterBlueprint> {
    let mut out = HelicopterBlueprint::corners();
    out.push(HelicopterBlueprint::NOMINAL);
    out
}

/// One helicopter a guard sweeps: her type, the built tree, its plan, its
/// ornateness and a label for the failure.
struct Drawn {
    craft: HelicopterType,
    built: Generator,
    plan: PodPlan,
    ornateness: OrnatenessTier,
    what: String,
}

/// Every helicopter the family can draw: every type, at those blueprints,
/// on every ornateness-by-wear pair - which is all of them, the axes being
/// independent draws.
fn every_drawn() -> Vec<Drawn> {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    let mut out = Vec::new();
    for t in HelicopterType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            for o in OrnatenessTier::ALL {
                for w in WearTier::ALL {
                    (ctx.ornateness, ctx.wear) = (o, w);
                    out.push(Drawn {
                        craft: t,
                        built: craft(t).build(&ctx, &plan),
                        plan,
                        ornateness: o,
                        what: format!(
                            "a {} m {}, girth {}, boom {}, {} / {}",
                            bp.length,
                            t.label(),
                            bp.girth,
                            bp.boom,
                            o.label(),
                            w.label()
                        ),
                    });
                }
            }
        }
    }
    out
}

/// `root` with every BLADE pruned - each cuboid a spinning hub carries that
/// does not spin of itself - so what is left is what her box answers for:
/// the fuselage, the mast, the rotor head and the tail rotor's gearbox. The
/// blades are outside it by owner decision 3 (#1611). A hub a hub carries
/// is no blade: the autogyro's tail wheel rolls on one inside the one it
/// steers on.
fn without_blades(root: &Generator) -> Generator {
    let mut out = root.clone();
    if out.spin.is_some() {
        out.children
            .retain(|c| !(matches!(c.kind, GeneratorKind::Cuboid { .. }) && c.spin.is_none()));
    }
    out.children = out.children.iter().map(without_blades).collect();
    out
}

/// Every node in `root` that turns, with its path.
fn spinning(root: &Generator) -> Vec<(&Generator, String)> {
    fn walk<'a>(g: &'a Generator, path: String, out: &mut Vec<(&'a Generator, String)>) {
        if g.spin.is_some() {
            out.push((g, path.clone()));
        }
        for (i, c) in g.children.iter().enumerate() {
            walk(c, format!("{path}/{i}"), out);
        }
    }
    let mut out = Vec::new();
    walk(root, "0".into(), &mut out);
    out
}

/// A root child's mesh vertices in the drawn frame: its own transform -
/// scale, then turn, then place - applied to the mesher's local points.
fn drawn_points(part: &Generator) -> Vec<Vec3> {
    let t = &part.transform;
    let (at, turn, scale) = (
        Vec3::from(t.translation.0),
        Quat::from_array(t.rotation.0).normalize(),
        Vec3::from(t.scale.0),
    );
    primitive_points(&part.kind)
        .into_iter()
        .map(|p| at + turn * (p * scale))
        .collect()
}

/// Every helicopter seed draws a helicopter, and draws ITS OWN type - and
/// every type is picked by some seed under 600, so the fleet the owner
/// meets holds all of them.
#[test]
fn every_helicopter_seed_resolves_to_a_built_craft() {
    let mut total = 0;
    let mut picked: Vec<HelicopterType> = Vec::new();
    for s in helicopter_seeds(600) {
        let t = HelicopterType::for_seed(s);
        if !picked.contains(&t) {
            picked.push(t);
        }
        let (_, plan) = body_for(s).expect("a helicopter seed has a body");
        let (nose, tail) = plan.nose_and_tail();
        assert!(nose - tail > 2.5, "seed {s}: degenerate fuselage");
        total += 1;
    }
    assert!(total > 50, "too few helicopters sampled: {total}");
    for t in HelicopterType::ALL {
        assert!(picked.contains(&t), "no helicopter seed under 600 is {t:?}");
    }
}

/// She is one machine at every blueprint extreme and on every tier: every
/// part meets another, so nothing floats - the owner's complaint about the
/// roadster prototype, asked of the geometry by arithmetic (#1364). Read
/// after a JSON round trip, as a peer reads her.
///
/// It is what found the utility prototype's two gaps (#1613): the mast
/// stopped 18 mm under the rotor head and the tail rotor's cap hung 11 mm
/// off the fin.
#[test]
fn a_helicopter_is_one_machine_at_every_blueprint_extreme() {
    for Drawn { built, what, .. } in every_drawn() {
        let json = serde_json::to_string(&built).expect("a helicopter serializes");
        let saved: Generator = serde_json::from_str(&json).expect("and reads back");
        touch::assert_one_machine(&saved, &what);
    }
}

/// The sanitiser rewrites nothing she is drawn with, at any blueprint
/// extreme or tier (#1359 rule 8): a peer draws exactly what was built.
#[test]
fn a_helicopter_survives_sanitize_unchanged_at_her_blueprint_extremes() {
    use crate::pds::sanitize_avatar_visuals;
    let mut n = 0;
    for Drawn { built, what, .. } in every_drawn() {
        let mut sanitized = built.clone();
        sanitize_avatar_visuals(&mut sanitized);
        if let Some(where_) = first_difference(&built, &sanitized, "0") {
            panic!("{what} was rewritten by the sanitiser at {where_}");
        }
        n += 1;
    }
    assert_eq!(
        n,
        HelicopterType::ALL.len() * 9 * 3 * 3,
        "the sweep lost a combination"
    );
}

/// Building the same seed twice gives the same tree, bit for bit.
#[test]
fn a_seeded_helicopter_builds_deterministically() {
    for s in helicopter_seeds(300) {
        assert_eq!(
            build(s, None),
            build(s, None),
            "seed {s} is not deterministic"
        );
    }
}

/// Her box holds her fuselage and rests on her skids (#1613 recommendation
/// 5): with the blades pruned, what she draws spans the box nose to tail and
/// side to side, its top is the top of her rotor head, and her skid bottoms
/// are its floor - what a landed body rests on (#1612).
///
/// Measured on the drawn tree - the mesher's own vertices, posed as the
/// game poses her - rather than on the arithmetic that placed the parts, so
/// a fin raked further or a skid toe turned lower is caught here. The
/// profile's `tail_reach` in particular is a number it states and this reads
/// back: the utility's fin, and the trainer's guard stirrup.
///
/// Two ends are held loosely rather than exactly, both by what a sweep
/// does at its ends. The floor is held to 3 mm below: a skid tube is a
/// Catmull-Rom through its toe's up-turn, which dips a hair under its own
/// control points before it rises. And the nose to 1 cm ahead: a nose's end
/// cap is a disc square to the centreline's last segment, which leans out
/// past the nose station. (The trainer's blunt nose leans out far further,
/// 3.7 to 7.5 cm, and her box's nose is MEASURED off her drawn bubble -
/// `PodPlan::with_nose_reach` - so she is held to the same centimetre.)
/// Past that, the utility pokes ONE part out of it on purpose: her pitot,
/// from her nose when she is adorned. The trainer's ornate litter pods are
/// in her box, which is as wide as her ornateness draws her. And the
/// autogyro's box is read off her as she stands, tail-down (#1624): her
/// nose and tail off her plain machine, her width off her tier's - so here
/// it is held across every tier, and her floor is her mains' tyres.
#[test]
fn the_box_holds_the_drawn_fuselage() {
    for Drawn {
        craft: t,
        mut built,
        plan,
        ornateness,
        what,
    } in every_drawn()
    {
        let c = craft(t);
        apply_travel_pose(&mut built, travel_drop(c, &plan));
        let [hx, hy, hz] = chassis_half_extents(c, &plan, ornateness);
        let drawn = tree_bounds(&without_blades(&built)).expect("she draws something");
        // The travel pose yaws her 180 degrees: her nose is at -Z, her port
        // side at -X. The box is symmetric, so the extents read alike.
        let close = |got: f32, want: f32, tol: f32| (got - want).abs() <= tol;
        assert!(
            close(drawn.max.y, hy, 1e-3),
            "{what}: her rotor head tops out at {:.4}, her box at {hy:.4}",
            drawn.max.y
        );
        assert!(
            drawn.min.y <= -hy + 1e-3 && drawn.min.y >= -hy - 3e-3,
            "{what}: her skids reach {:.4}, her box's floor is {:.4}",
            drawn.min.y,
            -hy
        );
        let pitot = t == HelicopterType::Utility && ornateness != OrnatenessTier::Plain;
        let nose_out = if pitot { plan.len(0.10) } else { 1e-2 };
        assert!(
            drawn.min.z >= -hz - nose_out && drawn.min.z <= -hz + 1e-3,
            "{what}: her nose reaches {:.4}, her box {:.4}",
            drawn.min.z,
            -hz
        );
        assert!(
            close(drawn.max.z, hz, 1e-3),
            "{what}: her tail reaches {:.4}, her box {hz:.4}",
            drawn.max.z
        );
        assert!(
            close(drawn.max.x, hx, 1e-3) && close(drawn.min.x, -hx, 1e-3),
            "{what}: she is drawn {:.4}..{:.4} wide, her box +-{hx:.4}",
            drawn.min.x,
            drawn.max.x
        );
    }
}

/// She stands under the air draft and inside the gateway mouth (#1359 rule
/// 6), rotors and all: the gate's lintel is over her rotor head, her tail
/// rotor's tip and her autogyro's tilted disc, and her fuselage passes the
/// narrowest mouth. Her rotor's DISC does not, and owner decision 3 of
/// #1611 says so - pinned here both ways so it stays a decision rather than
/// a drift. The disc is read off its blades as drawn ([`disc`]), whatever
/// way they stand round it: the autogyro's three span less than it across
/// her. A light helicopter's disc is a little longer than her fuselage,
/// each type's ceiling its own: the utility's 3.82 m at most, the
/// trainer's 4.27 and the autogyro's 4.43 - at the longest blueprint on the
/// shortest boom, whose scale draws everything else larger to keep her
/// length.
#[test]
fn a_helicopter_stands_under_the_air_draft_and_inside_the_mouth() {
    for Drawn {
        craft: t,
        built,
        plan,
        ornateness,
        what,
    } in every_drawn()
    {
        let ground = plan.datum_height();
        let top = tree_bounds(&built).expect("she draws").max.y + ground;
        assert!(
            top <= AIR_DRAFT_CAP,
            "{what}: she stands {top} m over the ground, past {AIR_DRAFT_CAP}"
        );
        let wide = craft(t).overall_width(&plan, ornateness);
        assert!(
            wide < GATEWAY_MOUTH,
            "{what}: her fuselage is {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
        );
        let span = 2.0 * disc(main_rotor(&built)).reach;
        let ceiling = match t {
            HelicopterType::Utility => 4.0,
            HelicopterType::Trainer => 4.4,
            HelicopterType::Autogyro => 4.6,
        };
        assert!(
            (GATEWAY_MOUTH..ceiling).contains(&span),
            "{what}: her rotor spans {span} m - decision 3 accepts a disc past \
             the mouth, and no more than a light helicopter's"
        );
    }
}

/// The rates each type's rotors were agreed at, as literals: the main
/// rotor's and the tail rotor's (degrees a second), and the blade pass her
/// voice is gated at (Hz) - for a type driven round her rotor's torque by a
/// tail rotor. The autogyro has none: her free rotor turns with her speed,
/// and her own guard holds it
/// (`the_autogyros_rotor_airscrew_and_wheels_turn_as_agreed`).
fn agreed_rates(t: HelicopterType) -> Option<(f32, f32, f32)> {
    match t {
        // #1613: three turns a second and four, and a 6 Hz chop.
        HelicopterType::Utility => Some((1080.0, 1440.0, 6.0)),
        // #1621 Q-G: two and a half and four, and a 5 Hz slap.
        HelicopterType::Trainer => Some((900.0, 1440.0, 5.0)),
        HelicopterType::Autogyro => None,
    }
}

/// Every type with a tail rotor.
fn tail_rotored() -> impl Iterator<Item = HelicopterType> {
    HelicopterType::ALL
        .into_iter()
        .filter(|&t| agreed_rates(t).is_some())
}

/// Her rotors turn, always, at the agreed rates (#1613 recommendation 3,
/// #1621 Q-G): exactly two spinning nodes, the main rotor about her up axis
/// carrying its yoke and two blades, the tail rotor about her lateral axis
/// carrying two - both Constant, both inside the sanitiser's ceiling. On
/// every type with a tail rotor.
#[test]
fn her_rotors_turn_at_the_agreed_rates() {
    use crate::pds::SpinTerm;
    use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
    for Drawn {
        craft: t,
        built,
        what,
        ..
    } in every_drawn()
    {
        let Some((main, tail, _)) = agreed_rates(t) else {
            continue;
        };
        let rotors = spinning(&built);
        assert_eq!(rotors.len(), 2, "{what}: {} spinning nodes", rotors.len());
        let mut seen = Vec::new();
        for (node, path) in rotors {
            let spin = node.spin.as_ref().expect("a spinning node");
            let [SpinTerm::Constant { rate }] = spin.terms.as_slice() else {
                panic!("{what}: rotor {path} does not turn at a steady rate");
            };
            assert!(rate.0 <= MAX_SPIN_RATE_DEG, "{what}: {path} at {}", rate.0);
            let blades = node
                .children
                .iter()
                .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
                .count();
            seen.push((spin.axis.0, rate.0, blades));
        }
        seen.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(
            seen,
            [([0.0, 1.0, 0.0], main, 3), ([1.0, 0.0, 0.0], tail, 2)],
            "{what}: the main rotor carries its yoke and two blades, the tail \
             rotor two blades"
        );
    }
    // Each type's constants are its agreed numbers, and two blades at the
    // main rotor's rate pass at the rate her voice is gated at.
    assert_eq!(
        Some((
            utility::MAIN_ROTOR_DEG_S,
            utility::TAIL_ROTOR_DEG_S,
            UTILITY_BLADE_PASS_HZ
        )),
        agreed_rates(HelicopterType::Utility)
    );
    assert_eq!(
        Some((
            trainer::MAIN_ROTOR_DEG_S,
            trainer::TAIL_ROTOR_DEG_S,
            TRAINER_BLADE_PASS_HZ
        )),
        agreed_rates(HelicopterType::Trainer)
    );
}

/// A drawn rotor's disc, read off its hub and its blades: the hub's centre
/// in the root's frame, the reach of its blade tips from the hub, and the
/// slab its blades sweep along the shaft, from the hub's centre - all from
/// the cuboid corners as drawn, turned by their own rotations. The slab is
/// one-sided because a blade need not turn on its hub's plane: the
/// trainer's tail rotor turns on the outer end of its shaft.
struct Disc {
    centre: Vec3,
    reach: f32,
    slab: (f32, f32),
}

/// The disc the rotor on `hub` sweeps about `axis` (its hub's spin axis).
fn disc(hub: &Generator) -> Disc {
    let spin = hub.spin.as_ref().expect("a rotor hub spins");
    let axis = Vec3::from(spin.axis.0).normalize();
    let (mut reach, mut slab) = (0.0f32, (f32::MAX, f32::MIN));
    for blade in &hub.children {
        let GeneratorKind::Cuboid { size, .. } = &blade.kind else {
            continue;
        };
        let half = Vec3::from(size.0) * 0.5;
        let at = Vec3::from(blade.transform.translation.0);
        let turn = Quat::from_array(blade.transform.rotation.0).normalize();
        for corner in [-1.0f32, 1.0].iter().flat_map(|&x| {
            [-1.0f32, 1.0]
                .iter()
                .flat_map(move |&y| [-1.0f32, 1.0].map(|z| Vec3::new(x, y, z)))
        }) {
            let p = at + turn * (corner * half);
            let along = p.dot(axis);
            slab = (slab.0.min(along), slab.1.max(along));
            reach = reach.max((p - axis * along).length());
        }
    }
    Disc {
        centre: Vec3::from(hub.transform.translation.0),
        reach,
        slab,
    }
}

/// The main rotor's hub, off the root: the hub turning about its own up
/// axis that carries blades - the autogyro's tail wheel steers about its
/// up axis too, and carries a wheel.
fn main_rotor(root: &Generator) -> &Generator {
    root.children
        .iter()
        .find(|c| {
            c.spin.as_ref().is_some_and(|s| s.axis.0 == [0.0, 1.0, 0.0])
                && c.children
                    .iter()
                    .any(|b| matches!(b.kind, GeneratorKind::Cuboid { .. }))
        })
        .expect("a main rotor's hub on the root")
}

/// The main rotor's hub and the tail rotor's, off the root of a type with
/// a tail rotor.
fn rotor_hubs(root: &Generator) -> (&Generator, &Generator) {
    let about = |axis: [f32; 3]| {
        root.children
            .iter()
            .find(|c| c.spin.as_ref().is_some_and(|s| s.axis.0 == axis))
            .expect("a rotor hub on the root")
    };
    (about([0.0, 1.0, 0.0]), about([1.0, 0.0, 0.0]))
}

/// Her tail rotor sweeps clear of everything else she draws, at every
/// blueprint extreme: no vertex of any other part, as the mesher draws it,
/// lies inside the slab the tail rotor's blades sweep within their reach.
/// The utility prototype's rotor cut a centimetre into her boom's end on
/// every half turn; the trainer's turns beside the gearbox, the longerons'
/// ends and the guard stirrup. `touch` cannot see it, because two parts
/// that overlap also touch.
#[test]
fn her_tail_rotor_sweeps_clear_of_the_rest_of_her() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for t in tail_rotored() {
        let mut closest = f32::MAX;
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            let built = craft(t).build(&ctx, &plan);
            let (_, tail) = rotor_hubs(&built);
            let d = disc(tail);
            for part in built.children.iter().filter(|c| c.spin.is_none()) {
                for p in drawn_points(part) {
                    let off = p - d.centre;
                    if (d.slab.0..=d.slab.1).contains(&off.x) {
                        let radial = (off.y * off.y + off.z * off.z).sqrt();
                        assert!(
                            radial > d.reach,
                            "a {} m {} (girth {}, boom {}): her tail rotor sweeps \
                             through her at {p}",
                            bp.length,
                            t.label(),
                            bp.girth,
                            bp.boom
                        );
                    } else if (off.y * off.y + off.z * off.z).sqrt() <= d.reach {
                        let out = (d.slab.0 - off.x).max(off.x - d.slab.1);
                        closest = closest.min(out);
                    }
                }
            }
        }
        assert!(
            closest > 0.0 && closest < 0.2,
            "{t:?}: nothing came near her tail rotor: {closest}"
        );
    }
}

/// Her two rotors never cross, at any blueprint extreme: the main rotor's
/// disc and the tail rotor's, each swept as its blades are drawn, keep
/// apart. A tail rotor whose disc's top stands over the main rotor's plane
/// is clear only while it is aft of the main rotor's tips; at the shortest
/// boom and the slimmest cabin the utility prototype's crossed them by 4 cm,
/// which is why her tail rotor stands lower on her fin.
#[test]
fn her_two_rotors_never_cross() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for t in tail_rotored() {
        let mut tightest = f32::MAX;
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            let built = craft(t).build(&ctx, &plan);
            let (main, tail) = rotor_hubs(&built);
            let (m, d) = (disc(main), disc(tail));
            // The height in the main rotor's slab nearest the tail rotor's
            // centre: if even that is beyond the tail disc's reach, the tail
            // disc lies wholly under (or over) the main rotor.
            let y = d
                .centre
                .y
                .clamp(m.centre.y + m.slab.0, m.centre.y + m.slab.1);
            let up = (y - d.centre.y).abs();
            let clearance = if up >= d.reach {
                up - d.reach
            } else {
                // At that height the tail disc spans its centre's z plus or
                // minus `w`; the main disc, at the tail disc's inboard face,
                // reaches aft to its centre's z less `l`.
                let w = (d.reach * d.reach - up * up).sqrt();
                let (lo, hi) = (d.centre.x + d.slab.0, d.centre.x + d.slab.1);
                let x = if lo <= 0.0 && hi >= 0.0 {
                    0.0
                } else {
                    lo.abs().min(hi.abs())
                };
                let l = (m.reach * m.reach - x * x).sqrt();
                (m.centre.z - l) - (d.centre.z + w)
            };
            assert!(
                clearance > plan.len(0.01),
                "a {} m {} (girth {}, boom {}): her rotors clear by only {clearance} m",
                bp.length,
                t.label(),
                bp.girth,
                bp.boom
            );
            tightest = tightest.min(clearance);
        }
        // The utility's tail rotor stands up by her main rotor's tips; the
        // trainer's turns 31 cm under hers.
        let near = match t {
            HelicopterType::Utility => 0.2,
            HelicopterType::Trainer => 0.4,
            HelicopterType::Autogyro => unreachable!("she has no tail rotor"),
        };
        assert!(tightest < near, "{t:?}: no corner came near: {tightest}");
    }
}

/// The utility's boom carries the identity stripe on BOTH flanks: one sweep
/// each over the same stations, the second drawn tail to nose, which mirrors
/// the sweep's frame and so its band. Read off the mesher's own vertices,
/// because the claim is about which way a transported frame turns, and
/// only the mesher knows.
#[test]
fn the_boom_carries_a_stripe_on_each_flank() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let plan = utility::Utility.plan(&HelicopterBlueprint::NOMINAL);
    let built = utility::build_dressed(&ctx, &plan);
    let colours = helicopter_colours(&ctx);
    let stripes: Vec<&Generator> = built
        .children
        .iter()
        .filter(|c| c.kind.material() == Some(&colours.stripe))
        .collect();
    assert_eq!(stripes.len(), 2, "{} stripe sweeps", stripes.len());
    let mut sides: Vec<f32> = stripes
        .iter()
        .map(|g| {
            let pts = primitive_points(&g.kind);
            pts.iter().map(|p| p.x).sum::<f32>() / pts.len() as f32
        })
        .collect();
    sides.sort_by(f32::total_cmp);
    let boom_r = plan.len(0.04);
    assert!(
        sides[0] < -boom_r * 0.5 && sides[1] > boom_r * 0.5,
        "the two stripes sit at mean x {sides:?} - not one on each flank"
    );
}

/// The trainer carries her identity on her door hoop (#1621): one band of
/// the seeded accent all the way round her bubble where the doors hang -
/// and nowhere else - held clear of the dark glass it lies on.
#[test]
fn the_trainers_door_hoop_carries_her_identity() {
    for s in
        helicopter_seeds(600).filter(|&s| HelicopterType::for_seed(s) == HelicopterType::Trainer)
    {
        let ctx = PartCtx::for_seed(s);
        let colours: TrainerColours = trainer_colours(&ctx);
        let (_, plan) = body_for(s).expect("a trainer seed has a body");
        let built = trainer::build_dressed(&ctx, &plan);
        let hoops: Vec<&Generator> = built
            .children
            .iter()
            .filter(|c| c.kind.material() == Some(&colours.stripe))
            .collect();
        assert_eq!(hoops.len(), 1, "seed {s}: {} identity parts", hoops.len());
        let torture = hoops[0].kind.torture().expect("a sweep");
        assert_eq!(torture.path_cut.0, [0.0, 1.0], "seed {s}: the hoop is cut");
    }
}

/// Every seed drawn as a helicopter is driven as her type is - the utility
/// by her turbine, the trainer by her piston engine, the autogyro by her
/// radial - and her aura LEAVES HER STACK when it is one that burns: the
/// emitter in her saved record stands at the mouth of the stack she draws,
/// read off the stack's own node - not off the function that placed the
/// emitter, which would compare one call with itself (#1382 gap 2's
/// lesson). The utility's stack stands on her roof and the trainer's under
/// her engine, each raked aft, its mouth its upper end; the autogyro's
/// hangs under her cowl raked 30 degrees forward - 18 as she stands
/// tail-down - its mouth its lower end.
#[test]
fn a_helicopters_aura_leaves_her_exhaust_stack() {
    use crate::seeded_defaults::AvatarFx;
    let mut burning = vec![0usize; HelicopterType::ALL.len()];
    for s in helicopter_seeds(900) {
        let t = HelicopterType::for_seed(s);
        let drive = match t {
            HelicopterType::Utility => Propulsion::Turbine,
            HelicopterType::Trainer => Propulsion::PistonRotor,
            HelicopterType::Autogyro => Propulsion::Radial,
        };
        assert_eq!(propulsion(s), drive, "seed {s}");
        let aura = AvatarFx::for_seed(s).aura;
        if !matches!(
            aura,
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers
        ) {
            continue;
        }
        let (body, _) = super::super::build_for_seed(s);
        let root = body.visuals().expect("a helicopter draws a tree");
        let emitter = root
            .children
            .iter()
            .find(|c| matches!(c.kind, GeneratorKind::ParticleSystem(_)))
            .expect("a burning aura hangs an emitter");
        // The stack, and which end of it is its mouth.
        let cylinder = |c: &&Generator| matches!(c.kind, GeneratorKind::Cylinder { .. });
        let (stack, end) = match t {
            // The one cylinder on the root raked backward about X.
            HelicopterType::Utility | HelicopterType::Trainer => (
                root.children.iter().filter(cylinder).find(|c| {
                    c.transform.rotation.0[0].abs() > 0.1 && c.transform.rotation.0[0] < 0.0
                }),
                0.5,
            ),
            // The one cylinder raked forward as she stands.
            HelicopterType::Autogyro => (
                root.children.iter().filter(cylinder).find(|c| {
                    let rake = (30.0 - autogyro::GROUND_DEG).to_radians();
                    Quat::from_array(c.transform.rotation.0)
                        .angle_between(Quat::from_rotation_x(rake))
                        < 1e-3
                }),
                -0.5,
            ),
        };
        let stack = stack.expect("she draws an exhaust stack");
        let GeneratorKind::Cylinder { height, .. } = &stack.kind else {
            unreachable!()
        };
        let mouth = Vec3::from(stack.transform.translation.0)
            + Quat::from_array(stack.transform.rotation.0) * Vec3::new(0.0, height.0 * end, 0.0);
        let at = Vec3::from(emitter.transform.translation.0);
        assert!(
            at.distance(mouth) < 2e-3,
            "seed {s} ({t:?}): her {aura:?} issues at {at}, her stack's mouth is at {mouth}"
        );
        burning[HelicopterType::ALL
            .iter()
            .position(|&x| x == t)
            .expect("her type is a type")] += 1;
    }
    assert!(
        burning[0] > 10 && burning[1] > 3 && burning[2] > 3,
        "only {burning:?} burning auras sampled (utility, trainer, autogyro)"
    );
}

/// A seeded helicopter's saved record stays well under the soft budget, at
/// a THIRD of it (#1359 rule 9) - the live seeds as saved, FX and voice
/// included, and the heaviest she can draw: every type at every blueprint
/// extreme on her fullest ladder, Ornate and Battered, carrying the heaviest
/// FX overhead a live seed carries. The prototypes put the utility at about
/// 16 KB (#1613) and the trainer at 21 to 27 (#1621).
#[test]
fn a_seeded_helicopters_record_stays_well_inside_the_budget() {
    use crate::pds::avatar::AvatarRecord;
    use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
    let bytes = |t: &Generator| serialized_record_bytes(t).expect("a helicopter serializes");
    let (mut worst_seed, mut overhead) = (0usize, 0usize);
    for s in helicopter_seeds(400) {
        let saved = serialized_record_bytes(&AvatarRecord::default_for_seed(s))
            .expect("a record serializes");
        worst_seed = worst_seed.max(saved);
        overhead = overhead.max(saved.saturating_sub(bytes(&build(s, None))));
    }
    assert!(worst_seed > 0 && overhead > 0, "nothing was measured");
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    ctx.ornateness = OrnatenessTier::Ornate;
    ctx.wear = WearTier::Battered;
    let mut worst_corner = 0usize;
    for t in HelicopterType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            let mut built = craft(t).build(&ctx, &plan);
            apply_travel_pose(&mut built, travel_drop(craft(t), &plan));
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
    }
    for (what, worst) in [
        ("seeded helicopter", worst_seed),
        ("fully dressed corner", worst_corner),
    ] {
        assert!(
            worst * 3 < SOFT_RECORD_BUDGET_BYTES,
            "the heaviest {what} is {worst} bytes, past a third of the \
             {SOFT_RECORD_BUDGET_BYTES}-byte soft budget"
        );
    }
}

/// The tier ladder is real: each step of ornateness adds parts, wear adds
/// none (#1613 recommendation 7, #1621 Q-F, #1624 Q-G - wear is the kit's
/// grime). The utility's adorned step adds a pitot and two steps, her
/// ornate one a searchlight's housing and lens and a hook's block and pin;
/// the trainer's adorned step adds a searchlight's housing and lens and two
/// steps, her ornate one two litter pods on two struts each; the
/// autogyro's adorned step adds a pitot and two steps, her ornate one two
/// spats and a beacon.
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (t, steps) in [
        (HelicopterType::Utility, (3, 4)),
        (HelicopterType::Trainer, (4, 6)),
        (HelicopterType::Autogyro, (3, 3)),
    ] {
        let plan = craft(t).plan(&HelicopterBlueprint::NOMINAL);
        let count = |o: OrnatenessTier, w: WearTier| {
            let mut ctx = ctx;
            (ctx.ornateness, ctx.wear) = (o, w);
            craft(t).build(&ctx, &plan).children.len()
        };
        let plain = count(OrnatenessTier::Plain, WearTier::Pristine);
        let adorned = count(OrnatenessTier::Adorned, WearTier::Pristine);
        let ornate = count(OrnatenessTier::Ornate, WearTier::Pristine);
        assert_eq!((adorned - plain, ornate - adorned), steps, "{t:?}");
        for w in WearTier::ALL {
            assert_eq!(count(OrnatenessTier::Plain, w), plain, "{t:?} {w:?}");
        }
    }
}

/// The blueprint the guards sweep is the blueprint the seeds reach: every
/// live seed's three numbers sit inside the corners' box.
#[test]
fn the_corners_bound_every_seeded_helicopter() {
    let corners = HelicopterBlueprint::corners();
    let span = |f: &dyn Fn(&HelicopterBlueprint) -> f32| {
        let lo = corners.iter().map(f).fold(f32::MAX, f32::min);
        let hi = corners.iter().map(f).fold(f32::MIN, f32::max);
        lo..=hi
    };
    let (length, girth, boom) = (span(&|b| b.length), span(&|b| b.girth), span(&|b| b.boom));
    for s in helicopter_seeds(600) {
        let bp = VehicleBlueprint::from_seed(s)
            .and_then(|b| b.helicopter().copied())
            .expect("a helicopter seed has a blueprint");
        assert!(
            length.contains(&bp.length),
            "seed {s}: length {}",
            bp.length
        );
        assert!(girth.contains(&bp.girth), "seed {s}: girth {}", bp.girth);
        assert!(boom.contains(&bp.boom), "seed {s}: boom {}", bp.boom);
    }
}

/// The trainer's truss's sweeps, off a built tree: the three zig-zags, each
/// a whole-round body-coloured sweep trimmed along its path
/// (`boom::zigzag`).
fn zigzags<'a>(built: &'a Generator, colours: &TrainerColours) -> Vec<&'a Generator> {
    built
        .children
        .iter()
        .filter(|c| c.kind.material() == Some(&colours.body))
        .filter(|c| {
            // Whole round, trimmed along: the chin is a cut band of the
            // same colour, and no zig-zag.
            matches!(c.kind, GeneratorKind::Spine { .. })
                && c.kind
                    .torture()
                    .is_some_and(|t| t.path_cut.0 == [0.0, 1.0] && t.profile_cut.0 != [0.0, 1.0])
        })
        .collect()
}

/// Her truss's struts are drawn straight (#1621): every vertex of each
/// face's zig-zag, as the mesher draws it, lies within the strut's own
/// radius and the taper's 6.5 mm of a strut between two of its REAL
/// corners - none of the trimmed leads, and none bowed. Drawn without the
/// leads, the first strut of each face bowed 4 cm off its chord.
#[test]
fn her_truss_struts_are_drawn_straight() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = trainer_colours(&ctx);
    for bp in blueprints() {
        let plan = trainer::Trainer.plan(&bp);
        let built = trainer::build_dressed(&ctx, &plan);
        let faces = zigzags(&built, &colours);
        assert_eq!(faces.len(), 3, "three faces, three zig-zags");
        for face in faces {
            let GeneratorKind::Spine { points, .. } = &face.kind else {
                unreachable!()
            };
            // The real corners: every stored point but the two leads.
            let corners: Vec<Vec3> = points[1..points.len() - 1]
                .iter()
                .map(|p| Vec3::from(p.position.0))
                .collect();
            let r = points[1].radius.0;
            let slack = r + plan.len(0.0065);
            for v in primitive_points(&face.kind) {
                let off = corners
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (w[0], w[1]);
                        let t = ((v - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                        v.distance(a + (b - a) * t)
                    })
                    .fold(f32::MAX, f32::min);
                assert!(
                    off <= slack,
                    "a {} m trainer (girth {}, boom {}): a strut is drawn {:.4} off its \
                     chord at {v}, past {slack:.4}",
                    bp.length,
                    bp.girth,
                    bp.boom,
                    off
                );
            }
        }
    }
}

/// The root's hidden hub hides inside her upper longeron at every blueprint
/// (#1621): PodPlan hangs the drawn frame's origin at the craft's middle,
/// which on the trainer falls in the OPEN truss - drawn as the utility's
/// 2 cm cube on her datum, it floated there as a dark speck. Every corner of
/// the cube stands inside the longeron AS DRAWN: inside the flats of its
/// eight-sided tube, along the centreline its sweep was given.
#[test]
fn her_root_hub_hides_in_the_upper_longeron() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = trainer_colours(&ctx);
    for bp in blueprints() {
        let plan = trainer::Trainer.plan(&bp);
        let built = trainer::build_dressed(&ctx, &plan);
        let GeneratorKind::Cuboid { size, .. } = &built.kind else {
            panic!("the root is a hub cube");
        };
        let half = Vec3::from(size.0) * 0.5;
        // The upper longeron: the body-coloured tube down her centreline,
        // one station a frame, its radius the same at every one and uncut -
        // the painted chin is a cut band over the bubble's own stations.
        let longeron = built
            .children
            .iter()
            .find_map(|c| match &c.kind {
                GeneratorKind::Spine {
                    points, resolution, ..
                } if c.kind.material() == Some(&colours.body)
                    && points.len() > 2
                    && points.iter().all(|p| p.position.0[0] == 0.0)
                    && points.iter().all(|p| p.radius == points[0].radius)
                    && c.kind.torture().is_none_or(|t| {
                        t.path_cut.0 == [0.0, 1.0] && t.profile_cut.0 == [0.0, 1.0]
                    }) =>
                {
                    Some((points, *resolution))
                }
                _ => None,
            })
            .expect("she draws an upper longeron");
        let (points, res) = longeron;
        let radius = points[0].radius.0 * (std::f32::consts::PI / res as f32).cos();
        let line: Vec<Vec3> = points.iter().map(|p| Vec3::from(p.position.0)).collect();
        for corner in [-1.0f32, 1.0].iter().flat_map(|&x| {
            [-1.0f32, 1.0]
                .iter()
                .flat_map(move |&y| [-1.0f32, 1.0].map(|z| Vec3::new(x, y, z)))
        }) {
            let p = corner * half;
            let off = line
                .windows(2)
                .map(|w| {
                    let (a, b) = (w[0], w[1]);
                    let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                    p.distance(a + (b - a) * t)
                })
                .fold(f32::MAX, f32::min);
            assert!(
                off < radius,
                "a {} m trainer (girth {}, boom {}): the root hub's corner {p} stands \
                 {off:.4} off the longeron's line, outside its {radius:.4} tube",
                bp.length,
                bp.girth,
                bp.boom
            );
        }
    }
}

/// Her stabiliser bar turns clear over her bubble at every blueprint
/// (#1621): no vertex of the bubble within the bar's reach of the mast
/// stands as high as the bar's weights turn - the weights read off the
/// main rotor's hub as drawn. The fullest bubble rises within a few
/// centimetres of them.
#[test]
fn her_stabiliser_bar_clears_her_bubble() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = trainer_colours(&ctx);
    let mut tightest = f32::MAX;
    for bp in blueprints() {
        let plan = trainer::Trainer.plan(&bp);
        let built = trainer::build_dressed(&ctx, &plan);
        let (main, _) = rotor_hubs(&built);
        // The weights: the cylinders the hub carries off its axis.
        let (mut reach, mut under) = (0.0f32, f32::MAX);
        for part in &main.children {
            let GeneratorKind::Cylinder { radius, height, .. } = &part.kind else {
                continue;
            };
            let at = Vec3::from(part.transform.translation.0);
            if at.z.abs() > 1e-4 {
                reach = reach.max(at.z.abs() + height.0 * 0.5);
                under = under.min(at.y - radius.0);
            }
        }
        assert!(reach > 0.0, "her bar carries no weights");
        let hub = Vec3::from(main.transform.translation.0);
        let mast = [hub.x, hub.y + under, hub.z];
        let bubble = built
            .children
            .iter()
            .find(|c| c.kind.material() == Some(&colours.glass))
            .expect("she draws a bubble");
        for p in drawn_points(bubble) {
            let from_mast = ((p.x - mast[0]).powi(2) + (p.z - mast[2]).powi(2)).sqrt();
            if from_mast <= reach {
                tightest = tightest.min(mast[1] - p.y);
            }
        }
        assert!(
            tightest > 0.0,
            "a {} m trainer (girth {}, boom {}): her bubble rises {:.4} into her \
             stabiliser bar",
            bp.length,
            bp.girth,
            bp.boom,
            -tightest
        );
    }
    assert!(
        tightest < 0.1,
        "the bar never came near her bubble: {tightest}"
    );
}

/// Every point a node draws under `parent`, and every node under it: the
/// mesher's own vertices through each node's transform.
fn points_under(node: &Generator, parent: Affine3A, out: &mut Vec<Vec3>) {
    let t = &node.transform;
    let world = parent
        * Affine3A::from_scale_rotation_translation(
            Vec3::from(t.scale.0),
            Quat::from_array(t.rotation.0).normalize(),
            Vec3::from(t.translation.0),
        );
    out.extend(
        primitive_points(&node.kind)
            .into_iter()
            .map(|p| world.transform_point3(p)),
    );
    for c in &node.children {
        points_under(c, world, out);
    }
}

/// The autogyro at every blueprint the guards sweep, on `ctx`'s tiers: her
/// blueprint, her plan and her tree as built.
fn autogyros(ctx: &PartCtx) -> Vec<(HelicopterBlueprint, PodPlan, Generator)> {
    blueprints()
        .into_iter()
        .map(|bp| {
            let plan = autogyro::Autogyro.plan(&bp);
            (bp, plan, autogyro::build_dressed(ctx, &plan))
        })
        .collect()
}

/// Her rotor, her airscrew and her wheels turn as agreed (#1624 Q-C,
/// Q-I), at every blueprint extreme and on every tier: her rotor free on a
/// Speed spin about its own leaning axis - 120 degrees a second at rest and
/// 90 more a metre a second - carrying its head and three blades; her
/// airscrew on a Speed spin about her nose - 540 and 76 - carrying its
/// hubcap, two blades and its shaft; her two mains rolling on their tyres'
/// radius; and her tail wheel steering, the airplanes' way, round a wheel
/// that rolls on its own. Six turning nodes, none of them Constant: nothing
/// on her turns but with her.
#[test]
fn the_autogyros_rotor_airscrew_and_wheels_turn_as_agreed() {
    use crate::pds::SpinTerm;
    for Drawn {
        craft: t,
        built,
        plan,
        what,
        ..
    } in every_drawn()
    {
        if t != HelicopterType::Autogyro {
            continue;
        }
        let turning = spinning(&built);
        assert_eq!(turning.len(), 6, "{what}: {} turning nodes", turning.len());
        let (mut speeds, mut rolls, mut steers) = (Vec::new(), Vec::new(), 0);
        for (node, path) in &turning {
            let spin = node.spin.as_ref().expect("a turning node");
            let blades = node
                .children
                .iter()
                .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
                .count();
            match spin.terms.as_slice() {
                [SpinTerm::Speed { idle, gain }] => {
                    speeds.push((spin.axis.0, idle.0, gain.0, blades, node.children.len()));
                }
                [SpinTerm::Roll { radius }] => rolls.push(radius.0),
                [SpinTerm::Steer { gain, limit }] => {
                    assert_eq!((gain.0, limit.0), (0.4, 25.0), "{what}: {path}");
                    steers += 1;
                }
                terms => panic!("{what}: {path} turns by {terms:?}"),
            }
        }
        speeds.sort_by(|a, b| a.1.total_cmp(&b.1));
        assert_eq!(
            speeds,
            [
                ([0.0, 1.0, 0.0], 120.0, 90.0, 3, 4),
                ([0.0, 0.0, 1.0], 540.0, 76.0, 2, 4)
            ],
            "{what}: her rotor carries its head and three blades, her airscrew its \
             hubcap, two blades and its shaft"
        );
        rolls.sort_by(f32::total_cmp);
        let want = [plan.len(0.05), plan.len(0.15), plan.len(0.15)];
        assert!(
            rolls.len() == 3 && rolls.iter().zip(want).all(|(a, b)| (a - b).abs() < 1e-6),
            "{what}: her wheels roll on {rolls:?}, her tyres are {want:?}"
        );
        assert_eq!(steers, 1, "{what}: one tail wheel steers");
    }
    // Each constant is its agreed number.
    assert_eq!(
        (
            autogyro::ROTOR_IDLE_DEG_S,
            autogyro::ROTOR_GAIN_DEG_PER_M,
            autogyro::BLADES,
            autogyro::PROP_IDLE_DEG_S,
            autogyro::PROP_GAIN_DEG_PER_M,
            autogyro::TAIL_STEER_GAIN,
            autogyro::TAIL_STEER_LIMIT
        ),
        (120.0, 90.0, 3, 540.0, 76.0, 0.4, 25.0)
    );
}

/// What her voice thrums at is what her airscrew does at her top speed
/// (#1624 Q-I): her radial voice is the biplane's, gated at two blades
/// passing at the runtime's ceiling, and her airscrew reaches that ceiling
/// within half a metre a second under the top speed her cyclic and her
/// damping give her - where the biplane's reaches it under her level-flight
/// speed. Her free rotor never reaches the ceiling at all: about 3.3 turns
/// a second at her top speed.
#[test]
fn the_autogyros_airscrew_reaches_her_voice_at_her_top_speed() {
    use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
    let feel = craft(HelicopterType::Autogyro).feel();
    let top = feel.cyclic_accel / feel.linear_damping;
    let reached = (MAX_SPIN_RATE_DEG - autogyro::PROP_IDLE_DEG_S) / autogyro::PROP_GAIN_DEG_PER_M;
    assert!(
        reached <= top && top - reached < 0.5,
        "her airscrew reaches its ceiling at {reached} m/s, her top speed is {top}"
    );
    assert_eq!(
        2.0 * MAX_SPIN_RATE_DEG / 360.0,
        super::super::airplanes::BIPLANE_BLADE_PASS_HZ,
        "two blades at the ceiling pass at her voice's gate"
    );
    let rotor = autogyro::ROTOR_IDLE_DEG_S + autogyro::ROTOR_GAIN_DEG_PER_M * top;
    assert!(
        rotor < MAX_SPIN_RATE_DEG,
        "her free rotor turns at {rotor} degrees a second at her top speed"
    );
}

/// She stands tail-down on her three wheels (#1624 Q-B), at every
/// blueprint extreme: drawn pitched twelve degrees nose-up - her fuselage's
/// line, nose station to tail post, turned that far from the level
/// profile's - her mains' axles a tyre's radius over the ground under her
/// box and each tyre's lowest vertex on it, her tail wheel's axle its own
/// tyre's radius over it, and that tyre, turned with her, its facets within
/// two millimetres of it. The ground is her box's floor
/// (`the_box_holds_the_drawn_fuselage`); this is what stands on it.
#[test]
fn the_autogyro_stands_tail_down_on_her_three_wheels() {
    use crate::pds::SpinTerm;
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (bp, plan, built) in autogyros(&ctx) {
        let what = format!(
            "a {} m autogyro, girth {}, boom {}",
            bp.length, bp.girth, bp.boom
        );
        let ground = -plan.datum_height();
        let fuselage = &built.children[0];
        let GeneratorKind::Spine { points, .. } = &fuselage.kind else {
            panic!("{what}: her first part is her fuselage");
        };
        let scale = Vec3::from(fuselage.transform.scale.0);
        let drawn = |i: usize| Vec3::from(points[i].position.0) * scale;
        let level: Vec<Vec3> = plan
            .stations()
            .map(|(z, y, _)| Vec3::from(plan.at(0.0, y, z)))
            .collect();
        let pitch = |nose: Vec3, tail: Vec3| (nose.y - tail.y).atan2(nose.z - tail.z);
        let turned =
            pitch(drawn(0), drawn(points.len() - 1)) - pitch(level[0], level[level.len() - 1]);
        assert!(
            (turned.to_degrees() - autogyro::GROUND_DEG).abs() < 0.01,
            "{what}: she stands {} degrees nose-up",
            turned.to_degrees()
        );
        let (mut mains, mut tails) = (0, 0);
        for hub in built.children.iter().filter(|c| c.spin.is_some()) {
            let terms = &hub.spin.as_ref().expect("a turning hub").terms;
            let mut tyre = Vec::new();
            points_under(hub, Affine3A::IDENTITY, &mut tyre);
            let low = tyre.iter().map(|p| p.y).fold(f32::MAX, f32::min) - ground;
            let axle = hub.transform.translation.0[1] - ground;
            match terms.as_slice() {
                [SpinTerm::Roll { .. }] => {
                    assert!(
                        (axle - plan.len(0.15)).abs() < 1e-5 && low.abs() < 1e-5,
                        "{what}: a main's axle stands {axle} over the ground, its tyre {low}"
                    );
                    mains += 1;
                }
                [SpinTerm::Steer { .. }] => {
                    assert!(
                        (axle - plan.len(0.05)).abs() < 1e-4 && (0.0..0.002).contains(&low),
                        "{what}: the tail wheel's axle stands {axle} over the ground, its \
                         tyre {low}"
                    );
                    tails += 1;
                }
                _ => {}
            }
        }
        assert_eq!((mains, tails), (2, 1), "{what}: three wheels");
    }
}

/// Her rotor turns clear of her airframe at every blueprint extreme (#1624
/// Q-C): leaning aft eighteen degrees as she stands, its disc holds nothing
/// of her beyond its own head - no vertex of any part that does not turn,
/// as the mesher draws it, lies within the slab its blades sweep inside
/// their reach and outside twice the head's radius, where the housing and
/// the pylon's top stand - her fuselage stands at least half a metre under
/// it, along its axis, wherever it is under it, and her fin is out of its
/// reach altogether.
#[test]
fn the_autogyros_rotor_clears_her_airframe() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = autogyro_colours(&ctx);
    let mut tightest = f32::MAX;
    for (bp, plan, built) in autogyros(&ctx) {
        let what = format!(
            "a {} m autogyro, girth {}, boom {}",
            bp.length, bp.girth, bp.boom
        );
        let rotor = main_rotor(&built);
        let d = disc(rotor);
        let turn = Quat::from_array(rotor.transform.rotation.0).normalize();
        let head = plan.len(0.06) * 2.0;
        let mut under = f32::MAX;
        for (i, part) in built.children.iter().enumerate() {
            if part.spin.is_some() {
                continue;
            }
            for p in drawn_points(part) {
                let q = turn.inverse() * (p - d.centre);
                let radial = (q.x * q.x + q.z * q.z).sqrt();
                if radial < head || radial > d.reach {
                    continue;
                }
                assert!(
                    !(d.slab.0..=d.slab.1).contains(&q.y),
                    "{what}: her rotor sweeps through her at {p}"
                );
                if i == 0 {
                    under = under.min(d.slab.0 - q.y);
                }
            }
        }
        assert!(
            under > plan.len(0.5) && under < f32::MAX,
            "{what}: her fuselage stands {under} m under her rotor"
        );
        tightest = tightest.min(under / plan.len(1.0));
        let fin = built
            .children
            .iter()
            .find(|c| {
                c.kind.material() == Some(&colours.body)
                    && c.transform.scale.0[0] < 0.5
                    && c.transform.scale.0[1] == 1.0
            })
            .expect("she draws a fin");
        for p in drawn_points(fin) {
            let q = turn.inverse() * (p - d.centre);
            let radial = (q.x * q.x + q.z * q.z).sqrt();
            assert!(
                radial > d.reach,
                "{what}: her fin stands in her rotor's reach"
            );
        }
    }
    assert!(
        tightest < 0.8,
        "her rotor never came near her fuselage: {tightest}"
    );
}

/// Her airscrew turns clear of her cowl ring (#1624 Q-D): every point of
/// both blades stands ahead of the ring's lip at every blueprint extreme -
/// the biplane's guard on her nose, which is the biplane's. Read off her
/// LEVEL tree, where the ring's axis is her fore-and-aft one; her stance
/// turns both the same way.
#[test]
fn the_autogyros_airscrew_turns_clear_of_her_cowl_ring() {
    use crate::pds::SpinTerm;
    for bp in blueprints() {
        let plan = autogyro::Autogyro.plan(&bp);
        let level = autogyro::draw_level(&plan, OrnatenessTier::Plain);
        let ring = level
            .iter()
            .find(|c| {
                matches!(&c.kind, GeneratorKind::Cylinder { common, .. } if common.torture.hollow.0 > 0.0)
            })
            .expect("she draws a cowl ring");
        let GeneratorKind::Cylinder { height, .. } = &ring.kind else {
            unreachable!()
        };
        // Laid along her by a quarter turn about +X: its height is fore and
        // aft.
        let lip = ring.transform.translation.0[2] + height.0 * 0.5;
        let screw = level
            .iter()
            .find(|c| {
                matches!(
                    c.spin.as_ref().map(|s| &s.terms[0]),
                    Some(SpinTerm::Speed { .. })
                ) && c.spin.as_ref().is_some_and(|s| s.axis.0 == [0.0, 0.0, 1.0])
            })
            .expect("she has an airscrew");
        let at = Vec3::from(screw.transform.translation.0);
        let mut aft = f32::MAX;
        let mut blades = 0;
        for blade in screw
            .children
            .iter()
            .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
        {
            let turn = Quat::from_array(blade.transform.rotation.0);
            let place = Vec3::from(blade.transform.translation.0);
            for p in primitive_points(&blade.kind) {
                aft = aft.min((at + place + turn * p).z);
            }
            blades += 1;
        }
        assert_eq!(blades, 2, "a {blades}-blade airscrew");
        assert!(
            aft - lip > plan.len(0.005),
            "a {} m autogyro (girth {}, boom {}): her blades reach {:.4} m ahead of \
             the cowl ring's lip",
            bp.length,
            bp.girth,
            bp.boom,
            aft - lip
        );
    }
}

/// The root's hidden hub hides inside her fuselage at every blueprint
/// (#1624): PodPlan hangs the drawn frame's origin at her box's middle,
/// which standing tail-down is about 1.2 m behind her mains, where her
/// fuselage has dropped with the turn - drawn on the LEVEL centreline's
/// height, the prototype's, it stood 2 cm under her crown. Every corner of
/// the cube stands well inside the fuselage AS DRAWN: inside the flats of
/// its section, round in the sweep's own frame, along the centreline its
/// stations were turned to. And the hub is ON that centreline, within the
/// 2.2 cm her profile's datum claims, where it crosses her box's middle.
#[test]
fn her_root_hub_hides_in_her_fuselage() {
    use std::f32::consts::PI;
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (bp, _, built) in autogyros(&ctx) {
        let GeneratorKind::Cuboid { size, .. } = &built.kind else {
            panic!("the root is a hub cube");
        };
        let half = Vec3::from(size.0) * 0.5;
        let fuselage = &built.children[0];
        let GeneratorKind::Spine {
            points, resolution, ..
        } = &fuselage.kind
        else {
            panic!("her first part is her fuselage");
        };
        let scale = Vec3::from(fuselage.transform.scale.0);
        let flats = (PI / *resolution as f32).cos();
        let line: Vec<(Vec3, f32)> = points
            .iter()
            .map(|p| (Vec3::from(p.position.0), p.radius.0))
            .collect();
        // The standing centreline's height where it crosses her box's
        // middle (drawn frame): the hub is at the origin.
        let centre = line
            .windows(2)
            .map(|w| (w[0].0 * scale, w[1].0 * scale))
            .find(|(a, b)| (a.z >= 0.0) != (b.z >= 0.0))
            .map(|(a, b)| a.y + (b.y - a.y) * a.z / (a.z - b.z))
            .expect("her fuselage spans her box's middle");
        assert!(
            centre.abs() < 0.022,
            "a {} m autogyro (girth {}, boom {}): her standing centreline crosses her \
             box's middle {centre:.4} m from the root's hub",
            bp.length,
            bp.girth,
            bp.boom
        );
        for corner in [-1.0f32, 1.0].iter().flat_map(|&x| {
            [-1.0f32, 1.0]
                .iter()
                .flat_map(move |&y| [-1.0f32, 1.0].map(|z| Vec3::new(x, y, z)))
        }) {
            // In the sweep's own frame its sections are round.
            let p = corner * half / scale;
            let (off, radius) = line
                .windows(2)
                .map(|w| {
                    let ((a, ra), (b, rb)) = (w[0], w[1]);
                    let t = ((p - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                    (p.distance(a + (b - a) * t), ra + (rb - ra) * t)
                })
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .expect("a fuselage has segments");
            assert!(
                off < radius * flats * 0.5,
                "a {} m autogyro (girth {}, boom {}): the root hub's corner {} stands \
                 {off:.4} off her fuselage's line, half her {:.4} section is {:.4}",
                bp.length,
                bp.girth,
                bp.boom,
                corner * half,
                radius * flats,
                radius * flats * 0.5
            );
        }
    }
}

/// The autogyro carries her identity on her cheat line (#1624, the port's
/// first change): one band of the seeded accent down EACH flank, and
/// nowhere else.
#[test]
fn the_autogyros_cheat_line_carries_her_identity() {
    let mut n = 0;
    for s in
        helicopter_seeds(900).filter(|&s| HelicopterType::for_seed(s) == HelicopterType::Autogyro)
    {
        let ctx = PartCtx::for_seed(s);
        let colours = autogyro_colours(&ctx);
        let (_, plan) = body_for(s).expect("an autogyro seed has a body");
        let built = autogyro::build_dressed(&ctx, &plan);
        let mut sides: Vec<f32> = built
            .children
            .iter()
            .filter(|c| c.kind.material() == Some(&colours.stripe))
            .map(|g| {
                let pts = primitive_points(&g.kind);
                pts.iter().map(|p| p.x).sum::<f32>() / pts.len() as f32
            })
            .collect();
        sides.sort_by(f32::total_cmp);
        assert!(
            sides.len() == 2 && sides[0] < -0.05 && sides[1] > 0.05,
            "seed {s}: her accent is drawn at mean x {sides:?}"
        );
        n += 1;
    }
    assert!(n > 5, "only {n} autogyro seeds sampled");
}
