//! The helicopter family's guards (#1614): the boats' and the skiffs' set
//! for a rotorcraft - one machine at every blueprint extreme, the sanitiser
//! round trip, a third of the record budget, determinism, the gate - and the
//! ones a rotorcraft adds: the box holds her fuselage and rests on her
//! skids, her rotors turn at the agreed rates and sweep clear of her and of
//! each other, her aura leaves her stack, and every lit part she draws shows
//! (#1619). Every one sweeps EVERY type (the trainer's since #1621, the
//! autogyro's since #1624), each held to its own numbers - the two with a
//! tail rotor, its own; and the trainer has four of her own - her truss's
//! struts drawn straight, the root's hub hidden in her upper longeron, her
//! stabiliser bar clear of her bubble, and her identity on her door hoop.
//! The autogyro has seven: her rotor, her airscrew and her wheels turn as
//! agreed, her airscrew's thrum is her voice's at her top speed, she stands
//! tail-down on three wheels, her rotor clears her airframe, her airscrew
//! clears her cowl ring, the root's hub hides in her fuselage, and her
//! identity is on her cheat line - and since #1619 an eighth, her box is set
//! by the parts that should set it, as the coaxial's and the tandem's are.
//! The coaxial has eight (#1628): her rotors and her pusher turn as agreed,
//! her pusher reaches the spin ceiling by her top speed, her two rotors
//! clear each other and her airframe, her pusher clears its shroud and its
//! stators hold it, her exhaust aura issues from her port exhaust, her
//! identity is on her cheat line, her beacon stands proud of her fairing,
//! and her box is set by the parts that should set it; and the autogyro's
//! guard that her root's hub hides in her fuselage holds hers too, and the
//! utility's (#1619). The tandem has eight (#1630): her rotors turn as
//! agreed and as her voice says, her two rotors clear each other and her
//! airframe, her four wheels stand on the ground, her root's hub hides in
//! her cabin, her exhaust aura issues from her port engine, her identity is
//! on her cheat line, her beacon stands proud of her aft pylon, and her box
//! is set by the parts that should set it.

use bevy::math::{Affine3A, Quat, Vec3};

use super::super::common::{first_difference, sight, touch};
use super::*;
use super::{autogyro, coaxial, tandem, trainer, utility};
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
/// Past that, two types poke ONE part out of it on purpose: the utility her
/// pitot, from her nose when she is adorned, and the tandem her refuelling
/// probe, 35 cm past hers when she is ornate (#1630). The trainer's ornate
/// litter pods are in her box, which is as wide as her ornateness draws her.
/// The autogyro's box is read off her as she stands, tail-down (#1624): her
/// nose and tail off her plain machine, her width off her tier's - so here
/// it is held across every tier, and her floor is her mains' tyres. And an
/// adorned tandem's hoist stands out over her starboard door alone, so her
/// box, which is symmetric, reaches her on that side only.
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
        let probe = t == HelicopterType::Tandem && ornateness == OrnatenessTier::Ornate;
        let nose_out = if pitot {
            plan.len(0.10)
        } else if probe {
            plan.len(0.36)
        } else {
            1e-2
        };
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
        // Yawed, her starboard side - where a tandem's hoist stands - is at
        // +X.
        let hoisted = t == HelicopterType::Tandem && ornateness != OrnatenessTier::Plain;
        let port_side = if hoisted {
            drawn.min.x >= -hx - 1e-3
        } else {
            close(drawn.min.x, -hx, 1e-3)
        };
        assert!(
            close(drawn.max.x, hx, 1e-3) && port_side,
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
/// length. The coaxial's disc is shorter than her fuselage unless her boom
/// is short, and 3.56 m at most. The tandem's two discs are each shorter
/// still (#1630): 3.37 m at most, and on her smallest machine, 2.49 m, they
/// pass inside the mouth - which decision 3 never forbade - so hers is the
/// one floor under it.
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
        let (floor, ceiling) = match t {
            HelicopterType::Utility => (GATEWAY_MOUTH, 4.0),
            HelicopterType::Trainer => (GATEWAY_MOUTH, 4.4),
            HelicopterType::Autogyro => (GATEWAY_MOUTH, 4.6),
            HelicopterType::Coaxial => (GATEWAY_MOUTH, 3.6),
            HelicopterType::Tandem => (2.45, 3.4),
        };
        assert!(
            (floor..ceiling).contains(&span),
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
        HelicopterType::Autogyro | HelicopterType::Coaxial | HelicopterType::Tandem => None,
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
            HelicopterType::Autogyro | HelicopterType::Coaxial | HelicopterType::Tandem => {
                unreachable!("she has no tail rotor")
            }
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
/// radial, the coaxial and the tandem by their turbines under two rotors -
/// and her aura LEAVES HER STACK when it is one that burns: the
/// emitter in her saved record stands at the mouth of the stack she draws,
/// read off the stack's own node - not off the function that placed the
/// emitter, which would compare one call with itself (#1382 gap 2's
/// lesson). The utility's stack stands on her roof and the trainer's under
/// her engine, each raked aft, its mouth its upper end; the autogyro's
/// hangs under her cowl raked 30 degrees forward - 18 as she stands
/// tail-down - its mouth its lower end; and the tandem's is her port engine
/// pod, its mouth its tail (#1630).
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
            HelicopterType::Coaxial | HelicopterType::Tandem => Propulsion::CoaxialTurbine,
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
            // Her port engine pod, the trim-coloured sweep to port: its last
            // station is its mouth.
            HelicopterType::Tandem => {
                let trim = tandem_colours(&PartCtx::for_seed(s)).trim;
                let pod = root
                    .children
                    .iter()
                    .find(|c| {
                        c.kind.material() == Some(&trim)
                            && matches!(&c.kind, GeneratorKind::Spine { points, .. }
                                if points[0].position.0[0] > 0.0)
                    })
                    .expect("she draws a port engine pod");
                let GeneratorKind::Spine { points, .. } = &pod.kind else {
                    unreachable!()
                };
                let mouth = Vec3::from(points[points.len() - 1].position.0);
                let at = Vec3::from(emitter.transform.translation.0);
                assert!(
                    at.distance(mouth) < 2e-3,
                    "seed {s} ({t:?}): her {aura:?} issues at {at}, her engine's tail is at {mouth}"
                );
                burning[4] += 1;
                continue;
            }
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
            // Her themes pick no aura that burns - see the count below.
            HelicopterType::Coaxial => unreachable!("seed {s}: a coaxial's {aura:?}"),
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
    // The coaxial's five themes pick none that burns - neon haze, arcane
    // motes and a thruster's wash - so her exhaust's mouth is her own
    // guard's (`the_coaxials_exhaust_issues_from_her_port_exhaust`).
    assert!(
        burning[0] > 10 && burning[1] > 3 && burning[2] > 3 && burning[3] == 0 && burning[4] > 0,
        "{burning:?} burning auras sampled (utility, trainer, autogyro, coaxial, tandem)"
    );
}

/// A seeded helicopter's saved record stays well under the soft budget, at
/// a THIRD of it (#1359 rule 9) - the live seeds as saved, FX and voice
/// included, and the heaviest she can draw: every type at every blueprint
/// extreme on her fullest ladder, Ornate and Battered, carrying the heaviest
/// FX overhead a live seed carries. The prototypes put the utility at about
/// 16 KB (#1613), the trainer at 21 to 27 (#1621), the coaxial at 22 to 25
/// (#1628) and the tandem at 18 to 21 (#1630).
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
/// spats and a beacon; the coaxial's adorned step adds a sensor ball, its
/// lens and two steps, her ornate one her stub wings, two lamp pods and
/// their lamps, and a beacon; the tandem's adorned step adds a step and a
/// hoist's boom, winch, cable and hook, her ornate one a refuelling probe
/// and a beacon.
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (t, steps) in [
        (HelicopterType::Utility, (3, 4)),
        (HelicopterType::Trainer, (4, 6)),
        (HelicopterType::Autogyro, (3, 3)),
        (HelicopterType::Coaxial, (4, 6)),
        (HelicopterType::Tandem, (5, 2)),
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

/// The root's hidden hub hides inside her fuselage at every blueprint, on
/// every type whose first part is her fuselage's sweep: PodPlan hangs the
/// drawn frame's origin at her box's middle. The autogyro's (#1624) is
/// about 1.2 m behind her mains standing tail-down, where her fuselage has
/// dropped with the turn - drawn on the LEVEL centreline's height, the
/// prototype's, it stood 2 cm under her crown; the coaxial's (#1628) falls
/// in her tail cone, a third of a metre aft of her cabin's rear; and the
/// utility's (#1613, guarded since #1619) in her pod's tail. Every corner of
/// the cube stands well inside the fuselage AS DRAWN: inside the flats of
/// its section, round in the sweep's own frame, along the centreline its
/// stations were turned to. And the hub is near that centreline, within the
/// reach each profile's datum claims, where it crosses her box's middle:
/// 2.2 cm for the autogyro, 2 for the coaxial (1.5 at worst, a corner's
/// girth and boom moving her centreline off the nominal machine's), and
/// 9.5 for the utility, whose datum is her CABIN's centreline height - her
/// pod's centreline has climbed 4.2 to 8.9 cm over it by her box's middle.
/// The trainer's hub is her upper longeron's guard's, and the tandem's her
/// cabin's.
#[test]
fn her_root_hub_hides_in_her_fuselage() {
    use std::f32::consts::PI;
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let measured = [
        (HelicopterType::Utility, 0.095),
        (HelicopterType::Autogyro, 0.022),
        (HelicopterType::Coaxial, 0.02),
    ];
    for (t, near, bp, built) in measured.into_iter().flat_map(|(t, near)| {
        blueprints().into_iter().map(move |bp| {
            let plan = craft(t).plan(&bp);
            (t, near, bp, craft(t).build(&ctx, &plan))
        })
    }) {
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
            centre.abs() < near,
            "a {} m {} (girth {}, boom {}): her centreline crosses her box's middle \
             {centre:.4} m from the root's hub",
            bp.length,
            t.label(),
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
                "a {} m {} (girth {}, boom {}): the root hub's corner {} stands \
                 {off:.4} off her fuselage's line, half her {:.4} section is {:.4}",
                bp.length,
                t.label(),
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

/// The coaxial at every blueprint the guards sweep, on `ctx`'s tiers: her
/// blueprint, her plan and her tree as built.
fn coaxials(ctx: &PartCtx) -> Vec<(HelicopterBlueprint, PodPlan, Generator)> {
    blueprints()
        .into_iter()
        .map(|bp| {
            let plan = coaxial::Coaxial.plan(&bp);
            (bp, plan, coaxial::build_dressed(ctx, &plan))
        })
        .collect()
}

/// A coaxial's label for a failure.
fn coaxial_what(bp: &HelicopterBlueprint) -> String {
    format!(
        "a {} m coaxial, girth {}, boom {}",
        bp.length, bp.girth, bp.boom
    )
}

/// Her rotors and her pusher turn as agreed (#1628 Q-C, Q-G), at every
/// blueprint extreme and on every tier: two rotors about her mast at the
/// same rate the opposite ways - three turns a second, the utility's - each
/// carrying its head and three blades, the upper over the lower and
/// carrying her cap as well; and her pusher on a Speed spin about her tail's
/// axis, idling at 540 and gaining 63 a metre a second, carrying its spinner
/// and five blades. Three turning nodes: nothing else on her turns.
#[test]
fn the_coaxials_rotors_and_pusher_turn_as_agreed() {
    use crate::pds::SpinTerm;
    for Drawn {
        craft: t,
        built,
        what,
        ..
    } in every_drawn()
    {
        if t != HelicopterType::Coaxial {
            continue;
        }
        let turning = spinning(&built);
        assert_eq!(turning.len(), 3, "{what}: {} turning nodes", turning.len());
        let (mut rotors, mut pushers) = (Vec::new(), Vec::new());
        for (node, path) in &turning {
            let spin = node.spin.as_ref().expect("a turning node");
            let blades = node
                .children
                .iter()
                .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
                .count();
            match spin.terms.as_slice() {
                [SpinTerm::Constant { rate }] => rotors.push((
                    node.transform.translation.0[1],
                    (spin.axis.0, rate.0, blades, node.children.len()),
                )),
                [SpinTerm::Speed { idle, gain }] => {
                    pushers.push((spin.axis.0, idle.0, gain.0, blades, node.children.len()));
                }
                terms => panic!("{what}: {path} turns by {terms:?}"),
            }
        }
        rotors.sort_by(|a, b| a.0.total_cmp(&b.0));
        let rotors: Vec<_> = rotors.into_iter().map(|(_, r)| r).collect();
        assert_eq!(
            rotors,
            [
                ([0.0, 1.0, 0.0], 1080.0, 3, 4),
                ([0.0, 1.0, 0.0], -1080.0, 3, 5)
            ],
            "{what}: her lower rotor carries its head and three blades, her upper \
             turns the other way and carries her cap too"
        );
        assert_eq!(
            pushers,
            [([0.0, 0.0, 1.0], 540.0, 63.0, 5, 6)],
            "{what}: her pusher carries its spinner and five blades"
        );
    }
    // Each constant is its agreed number, and three blades at her rotors'
    // rate pass at the rate her voice gallops at.
    assert_eq!(
        (
            coaxial::ROTOR_DEG_S,
            coaxial::BLADES,
            coaxial::PROP_IDLE_DEG_S,
            coaxial::PROP_GAIN_DEG_PER_M
        ),
        (1080.0, 3, 540.0, 63.0)
    );
    assert_eq!(COAXIAL_BLADE_PASS_HZ, 9.0);
}

/// What her pusher does at her top speed (#1628 Q-G): it reaches the
/// runtime's spin ceiling, four turns a second, within half a metre a second
/// under the top speed her cyclic and her damping give her - so it turns
/// fastest as she flies fastest, where the prototype's 60 left it short of
/// the ceiling. Her rotors turn at the utility's three turns a second, under
/// it.
#[test]
fn the_coaxials_pusher_spins_up_to_the_ceiling_by_her_top_speed() {
    use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
    let feel = craft(HelicopterType::Coaxial).feel();
    let top = feel.cyclic_accel / feel.linear_damping;
    let reached = (MAX_SPIN_RATE_DEG - coaxial::PROP_IDLE_DEG_S) / coaxial::PROP_GAIN_DEG_PER_M;
    assert!(
        reached <= top && top - reached < 0.5,
        "her pusher reaches its ceiling at {reached} m/s, her top speed is {top}"
    );
    const { assert!(coaxial::ROTOR_DEG_S < MAX_SPIN_RATE_DEG) };
}

/// Her two rotors turn clear of each other and of her airframe at every
/// blueprint extreme (#1628 Q-C): the slab the lower rotor's blades sweep
/// stands wholly under the upper's, by more than 15 cm; and no vertex of
/// any part that does not turn, as the mesher draws it, lies in the ring
/// either rotor's blades sweep - inside its slab, between its blades' roots
/// and their tips. Her mast stands inside the roots, her fairing under the
/// lower rotor - the nearest thing to either - and her tail beyond their
/// reach.
#[test]
fn the_coaxials_rotors_clear_each_other_and_her_airframe() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let mut nearest = f32::MAX;
    for (bp, plan, built) in coaxials(&ctx) {
        let what = coaxial_what(&bp);
        let mut hubs: Vec<&Generator> = built
            .children
            .iter()
            .filter(|c| c.spin.as_ref().is_some_and(|s| s.axis.0 == [0.0, 1.0, 0.0]))
            .collect();
        hubs.sort_by(|a, b| a.transform.translation.0[1].total_cmp(&b.transform.translation.0[1]));
        let [lower, upper] = hubs.as_slice() else {
            panic!("{what}: {} rotors", hubs.len());
        };
        let (lo, up) = (disc(lower), disc(upper));
        let gap = (up.centre.y + up.slab.0) - (lo.centre.y + lo.slab.1);
        assert!(
            gap > plan.len(0.15),
            "{what}: her rotors' blades pass {gap} m apart"
        );
        for (rotor, d) in [(lower, &lo), (upper, &up)] {
            // How near her mast a blade comes: each blade's inner end, its
            // middle drawn back half its length along its own span.
            let root = rotor
                .children
                .iter()
                .filter_map(|b| {
                    let GeneratorKind::Cuboid { size, .. } = &b.kind else {
                        return None;
                    };
                    let along = Quat::from_array(b.transform.rotation.0).normalize()
                        * Vec3::X
                        * (size.0[0] * 0.5);
                    let end = Vec3::from(b.transform.translation.0) - along;
                    Some((end.x * end.x + end.z * end.z).sqrt())
                })
                .fold(f32::MAX, f32::min);
            for part in built.children.iter().filter(|c| c.spin.is_none()) {
                for p in drawn_points(part) {
                    let q = p - d.centre;
                    let radial = (q.x * q.x + q.z * q.z).sqrt();
                    if radial < root || radial > d.reach {
                        continue;
                    }
                    assert!(
                        !(d.slab.0..=d.slab.1).contains(&q.y),
                        "{what}: a rotor sweeps through her at {p}"
                    );
                    let off = (d.slab.0 - q.y).max(q.y - d.slab.1);
                    nearest = nearest.min(off / plan.len(1.0));
                }
            }
        }
    }
    assert!(nearest < 0.1, "nothing came near her rotors: {nearest}");
}

/// Her pusher turns clear of its shroud and its stators at every blueprint
/// extreme (#1628 Q-B): its blades' tips stand inside the shroud's bore by
/// more than a centimetre, the slab they sweep inside the shroud's length,
/// and the stators ahead of that slab by more than a centimetre. And the
/// stators hold the shroud: each one's axis stands inside its length by more
/// than 4 mm, so its outer end is held in the shroud's wall at every boom -
/// on a stretched station of their own, the longest boom drew them onto its
/// front lip, half of each rod ahead of it. (A third of each rod's width
/// stands ahead of the lip at every boom: the agreed prototype's.)
#[test]
fn the_coaxials_pusher_turns_clear_of_her_shroud() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = coaxial_colours(&ctx);
    for (bp, plan, built) in coaxials(&ctx) {
        let what = coaxial_what(&bp);
        let pusher = built
            .children
            .iter()
            .find(|c| c.spin.as_ref().is_some_and(|s| s.axis.0 == [0.0, 0.0, 1.0]))
            .expect("she has a pusher");
        let d = disc(pusher);
        let shroud = built
            .children
            .iter()
            .find(|c| {
                matches!(&c.kind, GeneratorKind::Cylinder { common, .. } if common.torture.hollow.0 > 0.0)
            })
            .expect("she draws a shroud");
        let GeneratorKind::Cylinder {
            radius,
            height,
            common,
            ..
        } = &shroud.kind
        else {
            unreachable!()
        };
        let bore = radius.0 * common.torture.hollow.0;
        assert!(
            bore - d.reach > plan.len(0.01),
            "{what}: her pusher's tips stand {} m inside her shroud's bore",
            bore - d.reach
        );
        // Laid along her by a quarter turn about +X: its height is fore and
        // aft.
        let middle = shroud.transform.translation.0[2];
        let (aft, fore) = (middle - height.0 * 0.5, middle + height.0 * 0.5);
        let sweep = (d.centre.z + d.slab.0, d.centre.z + d.slab.1);
        assert!(
            sweep.0 > aft && sweep.1 < fore,
            "{what}: her pusher sweeps {sweep:?}, her shroud runs {aft}..{fore}"
        );
        // The stators: the straight rods in her body's colour.
        let stators: Vec<&Generator> = built
            .children
            .iter()
            .filter(|c| {
                c.kind.material() == Some(&colours.body)
                    && matches!(&c.kind, GeneratorKind::Spine { points, .. } if points.len() == 2)
            })
            .collect();
        assert_eq!(stators.len(), 3, "{what}: {} stators", stators.len());
        for stator in &stators {
            let GeneratorKind::Spine { points, .. } = &stator.kind else {
                unreachable!()
            };
            let axis = (points[0].position.0[2] + points[1].position.0[2]) * 0.5;
            assert!(
                axis > aft && fore - axis > plan.len(0.004),
                "{what}: a stator's axis stands at {axis}, her shroud runs {aft}..{fore}"
            );
        }
        let behind = stators
            .iter()
            .flat_map(|s| drawn_points(s))
            .map(|p| p.z)
            .fold(f32::MAX, f32::min);
        assert!(
            behind - sweep.1 > plan.len(0.01),
            "{what}: her stators stand {} m ahead of her pusher",
            behind - sweep.1
        );
    }
}

/// Her exhaust aura issues from her port exhaust's mouth (#1628): what she
/// trails an exhaust, steam or embers from is the upper end of the exhaust
/// she draws on her fairing's port side - read off the drawn node, at every
/// blueprint extreme, as no live seed of hers burns one. Her themes pick
/// neon haze, arcane motes and a thruster's wash, and those issue on her
/// centreline under her keel, between her skids.
#[test]
fn the_coaxials_exhaust_issues_from_her_port_exhaust() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = coaxial_colours(&ctx);
    for (bp, plan, built) in coaxials(&ctx) {
        let what = coaxial_what(&bp);
        let exhausts: Vec<&Generator> = built
            .children
            .iter()
            .filter(|c| {
                matches!(c.kind, GeneratorKind::Cylinder { .. })
                    && c.kind.material() == Some(&colours.dark)
            })
            .collect();
        assert_eq!(exhausts.len(), 2, "{what}: {} exhausts", exhausts.len());
        let port = exhausts
            .iter()
            .find(|c| c.transform.translation.0[0] > 0.0)
            .expect("an exhaust to port");
        let GeneratorKind::Cylinder { height, .. } = &port.kind else {
            unreachable!()
        };
        let mouth = Vec3::from(port.transform.translation.0)
            + Quat::from_array(port.transform.rotation.0) * Vec3::new(0.0, height.0 * 0.5, 0.0);
        let at = Vec3::from(coaxial::Coaxial.fx_mount(ParticleAura::Exhaust, &plan));
        assert!(
            at.distance(mouth) < 1e-4,
            "{what}: her exhaust issues at {at}, her port exhaust's mouth is at {mouth}"
        );
        let keel = drawn_points(&built.children[0]);
        for aura in [
            ParticleAura::Thruster,
            ParticleAura::NeonHaze,
            ParticleAura::ArcaneMotes,
        ] {
            let at = Vec3::from(coaxial::Coaxial.fx_mount(aura, &plan));
            let under = keel
                .iter()
                .filter(|p| (p.z - at.z).abs() < plan.len(0.05))
                .map(|p| p.y)
                .fold(f32::MAX, f32::min);
            assert!(
                at.x == 0.0 && at.y < under,
                "{what}: her {aura:?} issues at {at}, her keel there is at {under}"
            );
        }
    }
}

/// The coaxial carries her identity on her cheat line (#1628, the port's
/// first change): one band of the seeded accent down EACH flank, nose to
/// tail, and nowhere else.
#[test]
fn the_coaxials_cheat_line_carries_her_identity() {
    let mut n = 0;
    for s in
        helicopter_seeds(900).filter(|&s| HelicopterType::for_seed(s) == HelicopterType::Coaxial)
    {
        let ctx = PartCtx::for_seed(s);
        let colours = coaxial_colours(&ctx);
        let (_, plan) = body_for(s).expect("a coaxial seed has a body");
        let built = coaxial::build_dressed(&ctx, &plan);
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
    assert!(n > 5, "only {n} coaxial seeds sampled");
}

/// Her beacon stands on her fairing's top, more than half of it showing
/// (#1628): stood on her fuselage's crown, as it was first drawn - in the
/// prototype too - it was buried in the fairing, 2.2 to 4.2 cm under its
/// skin. Read as the root's hub is: in the fairing's own frame, where its
/// sections are round, the beacon's middle stands farther from the
/// fairing's line than the fairing's radius there, by more than a quarter
/// of its own radius, at every blueprint extreme. (Not off the fairing's
/// vertices round its station: the fairing falls away aft there, so its
/// rings tilt and the next one forward stands its top higher, beside the
/// beacon.)
#[test]
fn the_coaxials_beacon_stands_proud_of_her_fairing() {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    ctx.ornateness = OrnatenessTier::Ornate;
    let colours = coaxial_colours(&ctx);
    for (bp, _, built) in coaxials(&ctx) {
        let what = coaxial_what(&bp);
        let beacon = built
            .children
            .iter()
            .find(|c| c.kind.material() == Some(&colours.beacon))
            .expect("an ornate coaxial carries a beacon");
        let GeneratorKind::Sphere { radius, .. } = &beacon.kind else {
            panic!("{what}: her beacon is not a sphere");
        };
        // The fairing: the sweep in her body's colour drawn squashed - her
        // fuselage is drawn taller, her stub wings far flatter.
        let fairing = built
            .children
            .iter()
            .find(|c| {
                matches!(c.kind, GeneratorKind::Spine { .. })
                    && c.kind.material() == Some(&colours.body)
                    && (0.5..1.0).contains(&c.transform.scale.0[1])
            })
            .expect("she draws a fairing");
        let GeneratorKind::Spine { points, .. } = &fairing.kind else {
            unreachable!()
        };
        // In the sweep's own frame its sections are round.
        let at = Vec3::from(beacon.transform.translation.0) / Vec3::from(fairing.transform.scale.0);
        let (off, round) = points
            .windows(2)
            .map(|w| {
                let (a, ra) = (Vec3::from(w[0].position.0), w[0].radius.0);
                let (b, rb) = (Vec3::from(w[1].position.0), w[1].radius.0);
                let t = ((at - a).dot(b - a) / (b - a).length_squared()).clamp(0.0, 1.0);
                (at.distance(a + (b - a) * t), ra + (rb - ra) * t)
            })
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .expect("a fairing has segments");
        assert!(
            off - round > radius.0 * 0.25,
            "{what}: her beacon's middle stands {:.4} outside her fairing, in its own \
             frame; its radius is {}",
            off - round,
            radius.0
        );
    }
}

/// Her box is set by the parts that should set it (#1628): the box guard
/// reads her drawn tree back against a box MEASURED off the same tree, so
/// on its own it cannot see a part grow past her ends or her sides. Here
/// each is named, at every blueprint extreme and on every tier. Her nose is
/// her fuselage's tip, under a centimetre ahead of its first station (its
/// end cap's lean); her tail is her shroud's aft lip, 5 cm aft of her boom's
/// end. Her sides are her nav lights' outer faces, or her skids' on a cabin
/// fuller than girth 1.27; an adorned machine's steps come within half a
/// millimetre of both there and are counted too, but never stand wider. An
/// ornate machine's sides are her stub wings' lamp pods.
#[test]
fn the_coaxials_box_is_set_by_her_nose_her_shroud_and_her_widest_parts() {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    for ornateness in OrnatenessTier::ALL {
        ctx.ornateness = ornateness;
        let colours = coaxial_colours(&ctx);
        for (bp, plan, built) in coaxials(&ctx) {
            let what = format!("{}, {}", coaxial_what(&bp), ornateness.label());
            let (nose, tail) = plan.nose_and_tail();
            let tip = drawn_points(&built.children[0])
                .into_iter()
                .map(|p| p.z)
                .fold(f32::MIN, f32::max);
            let first = plan.at(0.0, 0.0, plan.nose())[2];
            assert!(
                (nose - tip).abs() < 1e-4 && tip > first && tip - first < plan.len(0.01),
                "{what}: her box's nose is at {nose}, her fuselage's tip at {tip}, its \
                 first station at {first}"
            );
            let shroud = built
                .children
                .iter()
                .find(|c| {
                    matches!(&c.kind, GeneratorKind::Cylinder { common, .. } if common.torture.hollow.0 > 0.0)
                })
                .expect("she draws a shroud");
            let lip = drawn_points(shroud)
                .into_iter()
                .map(|p| p.z)
                .fold(f32::MAX, f32::min);
            let reach = (plan.at(0.0, 0.0, coaxial::PROFILE.boom_end)[2] - tail) / plan.len(1.0);
            assert!(
                (tail - lip).abs() < 1e-4 && (reach - 0.05).abs() < 0.003,
                "{what}: her box's tail is at {tail}, {reach} m aft of her boom's end; \
                 her shroud's aft lip at {lip}"
            );
            // The furthest the parts `pick` keeps stand off her centreline,
            // as drawn.
            let outer = |pick: &dyn Fn(&Generator) -> bool| {
                built
                    .children
                    .iter()
                    .filter(|c| pick(c))
                    .flat_map(drawn_points)
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max)
            };
            let nav = outer(&|c| {
                c.kind.material() == Some(&colours.port)
                    || c.kind.material() == Some(&colours.starboard)
            });
            // The skids: the two metal sweeps each drawn at one x.
            let skids = outer(&|c| {
                c.kind.material() == Some(&colours.metal)
                    && matches!(&c.kind, GeneratorKind::Spine { points, .. }
                        if points.iter().all(|p| (p.position.0[0] - points[0].position.0[0]).abs() < 1e-6))
            });
            let steps = outer(&|c| {
                c.kind.material() == Some(&colours.metal)
                    && matches!(c.kind, GeneratorKind::Cuboid { .. })
            });
            let pods = outer(&|c| {
                c.kind.material() == Some(&colours.trim)
                    && matches!(c.kind, GeneratorKind::Spine { .. })
            });
            let named = match ornateness {
                OrnatenessTier::Plain => nav.max(skids),
                OrnatenessTier::Adorned => nav.max(skids).max(steps),
                OrnatenessTier::Ornate => pods,
            };
            let half = coaxial::Coaxial.overall_width(&plan, ornateness) * 0.5;
            assert!(
                named > 0.0 && (half - named).abs() < 1e-3,
                "{what}: her box is +-{half} wide; her nav lights reach {nav}, her skids \
                 {skids}, her steps {steps}, her lamp pods {pods}"
            );
        }
    }
}

/// The tandem at every blueprint the guards sweep, on `ctx`'s tiers: her
/// blueprint, her plan and her tree as built.
fn tandems(ctx: &PartCtx) -> Vec<(HelicopterBlueprint, PodPlan, Generator)> {
    blueprints()
        .into_iter()
        .map(|bp| {
            let plan = tandem::Tandem.plan(&bp);
            (bp, plan, tandem::build_dressed(ctx, &plan))
        })
        .collect()
}

/// A tandem's label for a failure.
fn tandem_what(bp: &HelicopterBlueprint) -> String {
    format!(
        "a {} m tandem, girth {}, boom {}",
        bp.length, bp.girth, bp.boom
    )
}

/// A tandem's two rotors' hubs, the front one first: the root's children
/// turning about her up axis.
fn tandem_rotors(built: &Generator) -> [&Generator; 2] {
    let mut hubs: Vec<&Generator> = built
        .children
        .iter()
        .filter(|c| c.spin.as_ref().is_some_and(|s| s.axis.0 == [0.0, 1.0, 0.0]))
        .collect();
    hubs.sort_by(|a, b| b.transform.translation.0[2].total_cmp(&a.transform.translation.0[2]));
    let [front, rear] = hubs.as_slice() else {
        panic!("{} rotors", hubs.len());
    };
    [front, rear]
}

/// Where `p` (the parent's frame) stands against the superellipsoid `node`
/// draws: its implicit function, read in its own frame - under one inside
/// its skin, one on it, over one outside. Barr's, as the mesher lays it:
/// `Y` is its north-south axis, `X` and `Z` its section.
fn superellipsoid_level(node: &Generator, p: Vec3) -> f32 {
    let GeneratorKind::Superellipsoid {
        half_extents,
        exponent_ns,
        exponent_ew,
        ..
    } = &node.kind
    else {
        panic!("not a superellipsoid: {:?}", node.kind);
    };
    let t = &node.transform;
    let local = Quat::from_array(t.rotation.0).normalize().inverse()
        * (p - Vec3::from(t.translation.0))
        / Vec3::from(t.scale.0);
    let q = local / Vec3::from(half_extents.0);
    let (ns, ew) = (exponent_ns.0, exponent_ew.0);
    (q.x.abs().powf(2.0 / ew) + q.z.abs().powf(2.0 / ew)).powf(ew / ns) + q.y.abs().powf(2.0 / ns)
}

/// Her rotors turn as agreed (#1630 Q-B, Q-C, Q-H), at every blueprint
/// extreme and on every tier: two rotors about her up axis at the same rate
/// the opposite ways - three turns a second, the coaxial's - each carrying
/// its head and three blades, the rear one higher and aft of the front and
/// carrying her cap as well. Two turning nodes: nothing else on her turns.
/// And her blades pass as often as the coaxial's, whose double chop is her
/// voice: what is heard is what is seen.
#[test]
fn the_tandems_rotors_turn_as_agreed() {
    use crate::pds::SpinTerm;
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    for ornateness in OrnatenessTier::ALL {
        ctx.ornateness = ornateness;
        for (bp, _, built) in tandems(&ctx) {
            let what = format!("{}, {}", tandem_what(&bp), ornateness.label());
            let turning = spinning(&built);
            assert_eq!(turning.len(), 2, "{what}: {} turning nodes", turning.len());
            let read = |hub: &Generator| {
                let spin = hub.spin.as_ref().expect("a rotor turns");
                let [SpinTerm::Constant { rate }] = spin.terms.as_slice() else {
                    panic!("{what}: a rotor turns by {:?}", spin.terms);
                };
                let blades = hub
                    .children
                    .iter()
                    .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
                    .count();
                (rate.0, blades, hub.children.len())
            };
            let [front, rear] = tandem_rotors(&built);
            assert_eq!(
                [read(front), read(rear)],
                [(1080.0, 3, 4), (-1080.0, 3, 5)],
                "{what}: her front rotor carries its head and three blades, her rear \
                 turns the other way and carries her cap too"
            );
            let (f, r) = (front.transform.translation.0, rear.transform.translation.0);
            assert!(
                r[1] > f[1] && r[2] < f[2],
                "{what}: her rear rotor stands at {r:?}, her front at {f:?}"
            );
        }
    }
    assert_eq!((tandem::ROTOR_DEG_S, tandem::BLADES), (1080.0, 3));
    assert_eq!(tandem::BLADE_PASS_HZ, COAXIAL_BLADE_PASS_HZ);
    assert_eq!(
        craft(HelicopterType::Tandem).propulsion(),
        Propulsion::CoaxialTurbine
    );
}

/// Her two rotors turn clear of each other and of her airframe at every
/// blueprint extreme and on every tier (#1630 Q-C): their discs overlap over
/// her cabin - the Chinook's silhouette - and the slab the front rotor's
/// blades sweep stands wholly under the rear's, by more than 18 cm; and no
/// vertex of any part that does not turn, as the mesher draws it, lies in
/// the ring either rotor's blades sweep - inside its slab, between its
/// blades' roots and their tips. Her masts stand inside the roots; the
/// nearest thing to either is her aft pylon's top, 3 cm under the rear
/// rotor's blades.
#[test]
fn the_tandems_rotors_clear_each_other_and_her_airframe() {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    let mut nearest = f32::MAX;
    for ornateness in OrnatenessTier::ALL {
        ctx.ornateness = ornateness;
        for (bp, plan, built) in tandems(&ctx) {
            let what = format!("{}, {}", tandem_what(&bp), ornateness.label());
            let [front, rear] = tandem_rotors(&built);
            let (fd, rd) = (disc(front), disc(rear));
            let apart = fd.centre.z - rd.centre.z;
            assert!(
                apart < fd.reach + rd.reach,
                "{what}: her discs stand {apart} m apart and do not overlap"
            );
            let gap = (rd.centre.y + rd.slab.0) - (fd.centre.y + fd.slab.1);
            assert!(
                gap > plan.len(0.18),
                "{what}: her rotors' blades pass {gap} m apart"
            );
            for (rotor, d) in [(front, &fd), (rear, &rd)] {
                // How near its mast a blade comes: each blade's inner end,
                // its middle drawn back half its length along its own span.
                let root = rotor
                    .children
                    .iter()
                    .filter_map(|b| {
                        let GeneratorKind::Cuboid { size, .. } = &b.kind else {
                            return None;
                        };
                        let along = Quat::from_array(b.transform.rotation.0).normalize()
                            * Vec3::X
                            * (size.0[0] * 0.5);
                        let end = Vec3::from(b.transform.translation.0) - along;
                        Some((end.x * end.x + end.z * end.z).sqrt())
                    })
                    .fold(f32::MAX, f32::min);
                for part in built.children.iter().filter(|c| c.spin.is_none()) {
                    for p in drawn_points(part) {
                        let q = p - d.centre;
                        let radial = (q.x * q.x + q.z * q.z).sqrt();
                        if radial < root || radial > d.reach {
                            continue;
                        }
                        assert!(
                            !(d.slab.0..=d.slab.1).contains(&q.y),
                            "{what}: a rotor sweeps through her at {p}"
                        );
                        let off = (d.slab.0 - q.y).max(q.y - d.slab.1);
                        nearest = nearest.min(off / plan.len(1.0));
                    }
                }
            }
        }
    }
    assert!(nearest < 0.05, "nothing came near her rotors: {nearest}");
}

/// She stands on her four wheels (#1630 Q-A), at every blueprint extreme:
/// each tyre's lowest vertex, as the mesher draws it, is on the ground -
/// half a millimetre either way - so she stands level on all four; and the
/// lowest thing else on her, her wheels' hubs, stands more than 5 cm over
/// it.
#[test]
fn the_tandem_stands_on_her_four_wheels() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (bp, plan, built) in tandems(&ctx) {
        let what = tandem_what(&bp);
        let ground = -plan.datum_height();
        let lowest = |g: &Generator| {
            drawn_points(g)
                .into_iter()
                .map(|p| p.y)
                .fold(f32::MAX, f32::min)
        };
        let tyre = |c: &&Generator| matches!(c.kind, GeneratorKind::Torus { .. });
        let tyres: Vec<f32> = built.children.iter().filter(tyre).map(lowest).collect();
        assert_eq!(tyres.len(), 4, "{what}: {} tyres", tyres.len());
        for low in &tyres {
            assert!(
                (low - ground).abs() < 5e-4,
                "{what}: a tyre's lowest point stands {} m over the ground",
                low - ground
            );
        }
        let rest = built
            .children
            .iter()
            .filter(|c| !tyre(c))
            .map(lowest)
            .fold(f32::MAX, f32::min);
        assert!(
            rest - ground > plan.len(0.05),
            "{what}: something else of her stands {} m over the ground",
            rest - ground
        );
    }
}

/// The root's hidden hub hides deep in her cabin at every blueprint
/// (#1630): PodPlan hangs the drawn frame's origin at her box's middle, a
/// little ahead of her cabin's, on her datum, which her girth moves about a
/// centimetre and a half off the cabin's centreline. Every corner of the
/// cube stands well inside the cabin's own skin - its superellipsoid's
/// implicit function under a tenth there, where the skin is one.
#[test]
fn the_tandems_root_hub_hides_in_her_cabin() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (bp, _, built) in tandems(&ctx) {
        let what = tandem_what(&bp);
        let GeneratorKind::Cuboid { size, .. } = &built.kind else {
            panic!("the root is a hub cube");
        };
        let half = Vec3::from(size.0) * 0.5;
        let cabin = &built.children[0];
        let worst = [-1.0f32, 1.0]
            .iter()
            .flat_map(|&x| {
                [-1.0f32, 1.0]
                    .iter()
                    .flat_map(move |&y| [-1.0f32, 1.0].map(|z| Vec3::new(x, y, z)))
            })
            .map(|corner| superellipsoid_level(cabin, corner * half))
            .fold(0.0f32, f32::max);
        assert!(
            worst < 0.1,
            "{what}: the root hub's corners stand at {worst} of her cabin's skin"
        );
    }
}

/// Her exhaust aura issues from her port engine pod's tail (#1630): what she
/// trails an exhaust, steam or embers from is the last station of the
/// trim-coloured pod on her aft pylon's port flank, read off the drawn node
/// at every blueprint extreme. Every other aura issues on her centreline
/// under her belly, between her sponsons.
#[test]
fn the_tandems_exhaust_issues_from_her_port_engine() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = tandem_colours(&ctx);
    for (bp, plan, built) in tandems(&ctx) {
        let what = tandem_what(&bp);
        let pods: Vec<&[crate::pds::generator::SpinePoint]> = built
            .children
            .iter()
            .filter(|c| c.kind.material() == Some(&colours.trim))
            .filter_map(|c| match &c.kind {
                GeneratorKind::Spine { points, .. } => Some(points.as_slice()),
                _ => None,
            })
            .collect();
        assert_eq!(pods.len(), 2, "{what}: {} engine pods", pods.len());
        let port = pods
            .iter()
            .find(|p| p[0].position.0[0] > 0.0)
            .expect("an engine to port");
        let mouth = Vec3::from(port[port.len() - 1].position.0);
        for aura in [
            ParticleAura::Exhaust,
            ParticleAura::Steam,
            ParticleAura::Embers,
        ] {
            let at = Vec3::from(tandem::Tandem.fx_mount(aura, &plan));
            assert!(
                at.distance(mouth) < 1e-5,
                "{what}: her {aura:?} issues at {at}, her engine's tail is at {mouth}"
            );
        }
        let belly = drawn_points(&built.children[0])
            .into_iter()
            .map(|p| p.y)
            .fold(f32::MAX, f32::min);
        for aura in [
            ParticleAura::Thruster,
            ParticleAura::NeonHaze,
            ParticleAura::ArcaneMotes,
            ParticleAura::Wake,
            ParticleAura::Vent,
        ] {
            let at = Vec3::from(tandem::Tandem.fx_mount(aura, &plan));
            assert!(
                at.x == 0.0 && at.y < belly,
                "{what}: her {aura:?} issues at {at}, her belly is at {belly}"
            );
        }
    }
}

/// The tandem carries her identity on her cheat line (#1630, the port's
/// first change): one line of the seeded accent down EACH flank, and
/// nowhere else.
#[test]
fn the_tandems_cheat_line_carries_her_identity() {
    let mut n = 0;
    for s in
        helicopter_seeds(3000).filter(|&s| HelicopterType::for_seed(s) == HelicopterType::Tandem)
    {
        let ctx = PartCtx::for_seed(s);
        let colours = tandem_colours(&ctx);
        let (_, plan) = body_for(s).expect("a tandem seed has a body");
        let built = tandem::build_dressed(&ctx, &plan);
        let mut sides: Vec<f32> = built
            .children
            .iter()
            .filter(|c| c.kind.material() == Some(&colours.stripe))
            .map(|g| {
                let pts = drawn_points(g);
                pts.iter().map(|p| p.x).sum::<f32>() / pts.len() as f32
            })
            .collect();
        sides.sort_by(f32::total_cmp);
        assert!(
            sides.len() == 2 && sides[0] < -0.1 && sides[1] > 0.1,
            "seed {s}: her accent is drawn at mean x {sides:?}"
        );
        n += 1;
    }
    assert!(n > 5, "only {n} tandem seeds sampled");
}

/// Her beacon stands on her aft pylon's back, more than half of it showing
/// (#1630): drawn where the prototype first had it, 0.55 of the pylon's
/// half-length aft of its middle, it stood 10 cm inside the pylon, and none
/// of it showed on the agreed sheet. Read in the pylon's own frame, the
/// beacon's middle stands outside the pylon's skin, and still would a
/// quarter of its radius further in - at every blueprint extreme. And its
/// back stands ahead of her tail light's, so it never sets her box's tail.
#[test]
fn the_tandems_beacon_stands_proud_of_her_pylon() {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    ctx.ornateness = OrnatenessTier::Ornate;
    let colours = tandem_colours(&ctx);
    for (bp, _, built) in tandems(&ctx) {
        let what = tandem_what(&bp);
        let find = |m: &crate::pds::texture::SovereignMaterialSettings| {
            built
                .children
                .iter()
                .find(|c| c.kind.material() == Some(m))
                .expect("she draws it")
        };
        let beacon = find(&colours.beacon);
        let GeneratorKind::Sphere { radius, .. } = &beacon.kind else {
            panic!("{what}: her beacon is not a sphere");
        };
        // The aft pylon: the tallest of the upright rounded boxes on her
        // centreline - the hump over her cockpit is the other.
        let pylon = built
            .children
            .iter()
            .filter(|c| {
                matches!(c.kind, GeneratorKind::Superellipsoid { .. })
                    && c.transform.rotation.0 == [0.0, 0.0, 0.0, 1.0]
                    && c.transform.translation.0[0] == 0.0
            })
            .max_by(|a, b| {
                let tall = |g: &Generator| match &g.kind {
                    GeneratorKind::Superellipsoid { half_extents, .. } => half_extents.0[1],
                    _ => 0.0,
                };
                tall(a).total_cmp(&tall(b))
            })
            .expect("she draws an aft pylon");
        let at = Vec3::from(beacon.transform.translation.0);
        let level = superellipsoid_level(pylon, at + Vec3::Z * (radius.0 * 0.25));
        assert!(
            level > 1.0,
            "{what}: a quarter of its radius further in, her beacon's middle stands at \
             {level} of her pylon's skin"
        );
        let back = |g: &Generator| {
            drawn_points(g)
                .into_iter()
                .map(|p| p.z)
                .fold(f32::MAX, f32::min)
        };
        let light = find(&colours.white);
        assert!(
            back(beacon) > back(light),
            "{what}: her beacon's back at {} stands aft of her tail light's at {}",
            back(beacon),
            back(light)
        );
    }
}

/// Her box is set by the parts that should set it (#1630): the box guard
/// reads her drawn tree back against a box MEASURED off the same tree, so
/// on its own it cannot see a part grow past her ends or her sides. Here
/// each is named, at every blueprint extreme and on every tier. Her nose is
/// her nose's tip, under 2 cm ahead of its first station: its end cap's
/// lean, 0.5 cm on her slimmest cabin and 1.7 on her fullest, whose girth
/// lifts the station behind the tip nearly five times as far as the tip.
/// Her tail is her tail light's back, a centimetre aft of her aft pylon and
/// 5 to 13 cm short of the station her length runs to. Her sides are her
/// sponsons' outer faces, and an adorned machine's her hoist's winch, on
/// her starboard side alone. An ornate machine's refuelling probe reaches
/// 35 cm past her nose's first station, outside her box on purpose, and
/// nothing else of her does.
#[test]
fn the_tandems_box_is_set_by_her_nose_her_tail_light_and_her_widest_parts() {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    for ornateness in OrnatenessTier::ALL {
        ctx.ornateness = ornateness;
        let colours = tandem_colours(&ctx);
        for (bp, plan, built) in tandems(&ctx) {
            let what = format!("{}, {}", tandem_what(&bp), ornateness.label());
            let (nose, tail) = plan.nose_and_tail();
            let fore = |g: &Generator| {
                drawn_points(g)
                    .into_iter()
                    .map(|p| p.z)
                    .fold(f32::MIN, f32::max)
            };
            let tip = fore(&built.children[1]);
            let first = plan.at(0.0, 0.0, plan.nose())[2];
            assert!(
                (nose - tip).abs() < 1e-4 && tip >= first && tip - first < plan.len(0.02),
                "{what}: her box's nose is at {nose}, her nose's tip at {tip}, its \
                 first station at {first}"
            );
            let light = built
                .children
                .iter()
                .find(|c| c.kind.material() == Some(&colours.white))
                .expect("she draws a tail light");
            let back = drawn_points(light)
                .into_iter()
                .map(|p| p.z)
                .fold(f32::MAX, f32::min);
            let reach = (plan.at(0.0, 0.0, tandem::PROFILE.boom_end)[2] - tail) / plan.len(1.0);
            assert!(
                (tail - back).abs() < 1e-4 && (-0.14..-0.04).contains(&reach),
                "{what}: her box's tail is at {tail}, {reach} m aft of her boom's end; \
                 her tail light's back at {back}"
            );
            // The one part past her nose: an ornate machine's probe.
            let past: Vec<f32> = built
                .children
                .iter()
                .filter(|c| c.spin.is_none())
                .map(fore)
                .filter(|&z| z > nose + 1e-4)
                .collect();
            if ornateness == OrnatenessTier::Ornate {
                let out = past.first().map(|z| (z - first) / plan.len(1.0));
                assert!(
                    past.len() == 1 && out.is_some_and(|o| (o - 0.35).abs() < 0.01),
                    "{what}: {} parts stand past her nose, by {out:?} m",
                    past.len()
                );
            } else {
                assert!(past.is_empty(), "{what}: {past:?} stand past her nose");
            }
            // The furthest the parts `pick` keeps stand off her centreline,
            // as drawn.
            let outer = |pick: &dyn Fn(&Generator) -> bool| {
                built
                    .children
                    .iter()
                    .filter(|c| pick(c))
                    .flat_map(drawn_points)
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max)
            };
            // The sponsons: the rounded boxes in her body's colour off her
            // centreline.
            let sponsons = outer(&|c| {
                c.kind.material() == Some(&colours.body)
                    && matches!(c.kind, GeneratorKind::Superellipsoid { .. })
                    && c.transform.translation.0[0] != 0.0
            });
            // The winch: the one dark block on her that does not turn.
            let winch = |c: &Generator| {
                c.spin.is_none()
                    && c.kind.material() == Some(&colours.dark)
                    && matches!(c.kind, GeneratorKind::Cuboid { .. })
            };
            let named = match ornateness {
                OrnatenessTier::Plain => sponsons,
                OrnatenessTier::Adorned | OrnatenessTier::Ornate => {
                    let starboard = built
                        .children
                        .iter()
                        .filter(|c| winch(c))
                        .all(|c| c.transform.translation.0[0] < 0.0);
                    assert!(starboard, "{what}: her hoist stands to port");
                    outer(&winch)
                }
            };
            let half = tandem::Tandem.overall_width(&plan, ornateness) * 0.5;
            assert!(
                named > 0.0 && (half - named).abs() < 1e-3,
                "{what}: her box is +-{half} wide; her sponsons reach {sponsons}, \
                 her winch {}",
                outer(&winch)
            );
        }
    }
}

/// Every lit part she draws shows past the rest of her (#1619), on every
/// type at every blueprint extreme, on her fullest ladder: at least one
/// vertex in thirty of each sees out ([`sight::lit_shares`]). A lamp drawn
/// inside its host shows none - the coaxial's beacon in her fairing as her
/// prototype first stood it, and the tandem's in her aft pylon. The least
/// that show are the tandem's nav lights, a 2 mm cap at her sponsons'
/// noses: one vertex in sixteen at her fullest girth. Read on a kit that
/// lights nothing but her lamps.
#[test]
fn every_lit_part_she_draws_shows() {
    let seed = helicopter_seeds(600)
        .find(|&s| !PartCtx::for_seed(s).materials.emissive_accents())
        .expect("some helicopter seed wears an unlit kit");
    let mut ctx = PartCtx::for_seed(seed);
    ctx.ornateness = OrnatenessTier::Ornate;
    for t in HelicopterType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            let shares = sight::lit_shares(&craft(t).build(&ctx, &plan));
            let what = format!(
                "a {} m {}, girth {}, boom {}",
                bp.length,
                t.label(),
                bp.girth,
                bp.boom
            );
            assert!(shares.len() >= 3, "{what}: {} lit parts", shares.len());
            for (i, share) in shares {
                assert!(
                    share > 1.0 / 30.0,
                    "{what}: her lit part {i} shows {:.1} % of its vertices",
                    share * 100.0
                );
            }
        }
    }
}

/// The autogyro's box is set by the parts that should set it (#1619, the
/// coaxial's guard for the type whose box was first measured, #1624): the
/// box guard reads her standing tree back against a box MEASURED off the
/// same tree, so on its own it cannot see a part grow past her ends or her
/// sides. Here each is named, at every blueprint extreme and on every
/// tier. Her nose is her hubcap's tip, raised with her nose as she stands;
/// her tail is her fuselage's tail post or, on her shorter booms, her fin's
/// trailing edge, whichever her stance carries further aft; her sides are
/// her tailplane's nav lights, outboard of her tyres - and on an ornate
/// machine her spats, outboard of both.
#[test]
fn the_autogyros_box_is_set_by_her_hubcap_her_tail_and_her_widest_parts() {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    for ornateness in OrnatenessTier::ALL {
        ctx.ornateness = ornateness;
        let colours = autogyro_colours(&ctx);
        for (bp, plan, built) in autogyros(&ctx) {
            let what = format!(
                "a {} m autogyro, girth {}, boom {}, {}",
                bp.length,
                bp.girth,
                bp.boom,
                ornateness.label()
            );
            let (nose, tail) = plan.nose_and_tail();
            // The airscrew: the hub turning about her fore-and-aft axis on
            // her speed. Its hubcap is what is left of it, blades pruned.
            let airscrew = built
                .children
                .iter()
                .find(|c| {
                    c.spin.as_ref().is_some_and(|s| {
                        s.axis.0 == [0.0, 0.0, 1.0]
                            && matches!(s.terms.as_slice(), [crate::pds::SpinTerm::Speed { .. }])
                    })
                })
                .expect("she draws an airscrew");
            let mut cap = Vec::new();
            points_under(&without_blades(airscrew), Affine3A::IDENTITY, &mut cap);
            let tip = cap.iter().map(|p| p.z).fold(f32::MIN, f32::max);
            assert!(
                (nose - tip).abs() < 1e-4,
                "{what}: her box's nose is at {nose}, her hubcap's tip at {tip}"
            );
            // Her tail: the fuselage, and her fin - the body-coloured sweep
            // and its trim cap drawn flat across her.
            let aft = |g: &Generator| {
                drawn_points(g)
                    .into_iter()
                    .map(|p| p.z)
                    .fold(f32::MAX, f32::min)
            };
            let post = aft(&built.children[0]);
            let fin = built
                .children
                .iter()
                .filter(|c| {
                    (c.kind.material() == Some(&colours.body)
                        || c.kind.material() == Some(&colours.trim))
                        && matches!(c.kind, GeneratorKind::Spine { .. })
                        && c.transform.scale.0[0] < 0.5
                })
                .map(aft)
                .fold(f32::MAX, f32::min);
            assert!(
                fin < f32::MAX && (tail - post.min(fin)).abs() < 1e-4,
                "{what}: her box's tail is at {tail}, her tail post's end at {post}, her \
                 fin's trailing edge at {fin}"
            );
            // Her sides.
            let outer = |pick: &dyn Fn(&Generator) -> bool| {
                built
                    .children
                    .iter()
                    .filter(|c| pick(c))
                    .flat_map(drawn_points)
                    .map(|p| p.x.abs())
                    .fold(0.0f32, f32::max)
            };
            let nav = outer(&|c| {
                c.kind.material() == Some(&colours.port)
                    || c.kind.material() == Some(&colours.starboard)
            });
            // The spats: her body's sweeps off her centreline.
            let spats = outer(&|c| {
                c.kind.material() == Some(&colours.body)
                    && matches!(&c.kind, GeneratorKind::Spine { points, .. }
                        if points.iter().all(|p| p.position.0[0].abs() > 0.1))
            });
            let named = match ornateness {
                OrnatenessTier::Ornate => spats,
                OrnatenessTier::Plain | OrnatenessTier::Adorned => nav,
            };
            let half = autogyro::Autogyro.overall_width(&plan, ornateness) * 0.5;
            assert!(
                named > 0.0 && (half - named).abs() < 1e-3,
                "{what}: her box is +-{half} wide; her nav lights reach {nav}, her spats \
                 {spats}"
            );
        }
    }
}
