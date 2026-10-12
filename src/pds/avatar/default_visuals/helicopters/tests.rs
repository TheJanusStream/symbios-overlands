//! The helicopter family's guards (#1614): the boats' and the skiffs' set
//! for a rotorcraft - one machine at every blueprint extreme, the sanitiser
//! round trip, a third of the record budget, determinism, the gate - and
//! the ones a rotorcraft adds: the box holds her fuselage and rests on her
//! skids, her rotors turn at the agreed rates and sweep clear of her and of
//! each other, and her aura leaves her stack. Every one sweeps EVERY type
//! (the trainer's since #1621), each held to its own numbers; and the
//! trainer has four of her own - her truss's struts drawn straight, the
//! root's hub hidden in her upper longeron, her stabiliser bar clear of her
//! bubble, and her identity on her door hoop.

use bevy::math::{Quat, Vec3};

use super::super::common::{first_difference, touch};
use super::*;
use super::{trainer, utility};
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

/// `root` with every BLADE pruned - each cuboid a spinning hub carries - so
/// what is left is what her box answers for: the fuselage, the mast, the
/// rotor head and the tail rotor's gearbox. The blades are outside it by
/// owner decision 3 (#1611).
fn without_blades(root: &Generator) -> Generator {
    let mut out = root.clone();
    if out.spin.is_some() {
        out.children
            .retain(|c| !matches!(c.kind, GeneratorKind::Cuboid { .. }));
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
/// in her box, which is as wide as her ornateness draws her.
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
/// 6), rotors and all: the gate's lintel is over her rotor head and her tail
/// rotor's tip, and her fuselage passes the narrowest mouth. Her rotor's
/// DISC does not, and owner decision 3 of #1611 says so - pinned here both
/// ways so it stays a decision rather than a drift. A light helicopter's
/// disc is a little longer than her fuselage, each type's ceiling its own:
/// the utility's 3.82 m at most, the trainer's 4.27 - at the longest
/// blueprint on the shortest boom, whose scale draws everything else larger
/// to keep her length.
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
        let disc = tree_bounds(&built).expect("she draws");
        let span = disc.max.x - disc.min.x;
        let ceiling = match t {
            HelicopterType::Utility => 4.0,
            HelicopterType::Trainer => 4.4,
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
/// voice is gated at (Hz).
fn agreed_rates(t: HelicopterType) -> (f32, f32, f32) {
    match t {
        // #1613: three turns a second and four, and a 6 Hz chop.
        HelicopterType::Utility => (1080.0, 1440.0, 6.0),
        // #1621 Q-G: two and a half and four, and a 5 Hz slap.
        HelicopterType::Trainer => (900.0, 1440.0, 5.0),
    }
}

/// Her rotors turn, always, at the agreed rates (#1613 recommendation 3,
/// #1621 Q-G): exactly two spinning nodes, the main rotor about her up axis
/// carrying its yoke and two blades, the tail rotor about her lateral axis
/// carrying two - both Constant, both inside the sanitiser's ceiling.
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
        let (main, tail, _) = agreed_rates(t);
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
        (
            utility::MAIN_ROTOR_DEG_S,
            utility::TAIL_ROTOR_DEG_S,
            UTILITY_BLADE_PASS_HZ
        ),
        agreed_rates(HelicopterType::Utility)
    );
    assert_eq!(
        (
            trainer::MAIN_ROTOR_DEG_S,
            trainer::TAIL_ROTOR_DEG_S,
            TRAINER_BLADE_PASS_HZ
        ),
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

/// The main rotor's hub and the tail rotor's, off the root.
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
    for t in HelicopterType::ALL {
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
    for t in HelicopterType::ALL {
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
/// by her turbine, the trainer by her piston engine - and her aura LEAVES
/// HER STACK when it is one that burns: the emitter in her saved record
/// stands at the mouth of the stack she draws, read off the stack's own
/// node - not off the function that placed the emitter, which would compare
/// one call with itself (#1382 gap 2's lesson).
#[test]
fn a_helicopters_aura_leaves_her_exhaust_stack() {
    use crate::seeded_defaults::AvatarFx;
    let mut burning = [0usize; 2];
    for s in helicopter_seeds(900) {
        let t = HelicopterType::for_seed(s);
        let drive = match t {
            HelicopterType::Utility => Propulsion::Turbine,
            HelicopterType::Trainer => Propulsion::PistonRotor,
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
        // The stack: the one cylinder on the root raked backward about X.
        let stack = root
            .children
            .iter()
            .find(|c| {
                matches!(c.kind, GeneratorKind::Cylinder { .. })
                    && c.transform.rotation.0[0].abs() > 0.1
                    && c.transform.rotation.0[0] < 0.0
            })
            .expect("she draws an exhaust stack");
        let GeneratorKind::Cylinder { height, .. } = &stack.kind else {
            unreachable!()
        };
        let mouth = Vec3::from(stack.transform.translation.0)
            + Quat::from_array(stack.transform.rotation.0) * Vec3::new(0.0, height.0 * 0.5, 0.0);
        let at = Vec3::from(emitter.transform.translation.0);
        assert!(
            at.distance(mouth) < 2e-3,
            "seed {s} ({t:?}): her {aura:?} issues at {at}, her stack's mouth is at {mouth}"
        );
        burning[usize::from(t == HelicopterType::Trainer)] += 1;
    }
    assert!(
        burning[0] > 10 && burning[1] > 3,
        "only {burning:?} burning auras sampled (utility, trainer)"
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
/// none (#1613 recommendation 7, #1621 Q-F - wear is the kit's grime). The
/// utility's adorned step adds a pitot and two steps, her ornate one a
/// searchlight's housing and lens and a hook's block and pin; the trainer's
/// adorned step adds a searchlight's housing and lens and two steps, her
/// ornate one two litter pods on two struts each.
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    for (t, steps) in [
        (HelicopterType::Utility, (3, 4)),
        (HelicopterType::Trainer, (4, 6)),
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
