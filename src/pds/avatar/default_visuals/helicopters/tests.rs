//! The helicopter family's guards (#1614): the boats' and the skiffs' set
//! for a rotorcraft - one machine at every blueprint extreme, the sanitiser
//! round trip, a third of the record budget, determinism, the gate - and
//! the ones a rotorcraft adds: the box holds her fuselage and rests on her
//! skids, her rotors turn at the agreed rates, and her aura leaves her stack.

use super::super::common::{first_difference, touch};
use super::utility;
use super::*;
use crate::catalogue::items::measure::tree_bounds;
use crate::pds::generator::GeneratorKind;
use crate::seeded_defaults::{ChassisFamily, OrnatenessTier, VehicleBlueprint, WearTier};

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

/// One utility helicopter a guard sweeps: the built tree, its plan, its
/// ornateness and a label for the failure.
struct Drawn {
    built: Generator,
    plan: PodPlan,
    ornateness: OrnatenessTier,
    what: String,
}

/// Every utility helicopter the family can draw at those blueprints, on
/// every ornateness-by-wear pair - which is all of them, the axes being
/// independent draws.
fn every_utility() -> Vec<Drawn> {
    let mut ctx = PartCtx::for_seed(a_helicopter_seed());
    let mut out = Vec::new();
    for bp in blueprints() {
        let plan = utility::Utility.plan(&bp);
        for o in OrnatenessTier::ALL {
            for w in WearTier::ALL {
                (ctx.ornateness, ctx.wear) = (o, w);
                out.push(Drawn {
                    built: utility::build_dressed(&ctx, &plan),
                    plan,
                    ornateness: o,
                    what: format!(
                        "a {} m utility helicopter, girth {}, boom {}, {} / {}",
                        bp.length,
                        bp.girth,
                        bp.boom,
                        o.label(),
                        w.label()
                    ),
                });
            }
        }
    }
    out
}

/// `root` with every BLADE pruned - each cuboid a spinning hub carries - so
/// what is left is what her box answers for: the fuselage, the mast, the
/// rotor head and the tail rotor's gearbox cap. The blades are outside it
/// by owner decision 3 (#1611).
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
/// It is what found the prototype's two gaps (#1613): the mast stopped 18 mm
/// under the rotor head and the tail rotor's cap hung 11 mm off the fin.
#[test]
fn a_utility_helicopter_is_one_machine_at_every_blueprint_extreme() {
    for Drawn { built, what, .. } in every_utility() {
        let json = serde_json::to_string(&built).expect("a helicopter serializes");
        let saved: Generator = serde_json::from_str(&json).expect("and reads back");
        touch::assert_one_machine(&saved, &what);
    }
}

/// The sanitiser rewrites nothing she is drawn with, at any blueprint
/// extreme or tier (#1359 rule 8): a peer draws exactly what was built.
#[test]
fn a_utility_helicopter_survives_sanitize_unchanged_at_her_blueprint_extremes() {
    use crate::pds::sanitize_avatar_visuals;
    let mut n = 0;
    for Drawn { built, what, .. } in every_utility() {
        let mut sanitized = built.clone();
        sanitize_avatar_visuals(&mut sanitized);
        if let Some(where_) = first_difference(&built, &sanitized, "0") {
            panic!("{what} was rewritten by the sanitiser at {where_}");
        }
        n += 1;
    }
    assert_eq!(n, 9 * 3 * 3, "the sweep lost a combination");
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
/// 5): with the blades pruned, what she draws spans the box nose to fin and
/// side to side, its top is the top of her rotor head, and her skid bottoms
/// are its floor - what a landed body rests on (#1612).
///
/// Measured on the drawn tree - the mesher's own vertices, posed as the
/// game poses her - rather than on the arithmetic that placed the parts, so
/// a fin raked further or a skid toe turned lower is caught here. The fin's
/// `tail_reach` in particular is a number the PROFILE states and this reads
/// back.
///
/// Two ends are held loosely rather than exactly, both by what a sweep
/// does at its ends. The floor is held to 3 mm below: the skid tube is a
/// Catmull-Rom through its toe's up-turn, which dips a hair under its own
/// control points before it rises. And the nose to 1 cm ahead: the pod's
/// end cap is a disc square to the centreline's last segment, which drops
/// toward the nose, so the disc's lower edge leans out past the nose
/// station - 7.5 mm on the fullest, shortest machine.
#[test]
fn the_box_holds_the_drawn_fuselage() {
    for Drawn {
        mut built,
        plan,
        ornateness,
        what,
    } in every_utility()
    {
        let craft = &utility::Utility;
        apply_travel_pose(&mut built, travel_drop(craft, &plan));
        let [hx, hy, hz] = chassis_half_extents(craft, &plan);
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
        // The pitot is the one part an adorned machine pokes out of her
        // nose: a 2 cm rod, which no box need hold.
        let nose_out = if ornateness == OrnatenessTier::Plain {
            1e-2
        } else {
            plan.len(0.10)
        };
        assert!(
            drawn.min.z >= -hz - nose_out && drawn.min.z <= -hz + 1e-3,
            "{what}: her nose reaches {:.4}, her box {:.4}",
            drawn.min.z,
            -hz
        );
        assert!(
            close(drawn.max.z, hz, 1e-3),
            "{what}: her fin reaches {:.4}, her box {hz:.4}",
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
/// ways so it stays a decision rather than a drift.
#[test]
fn a_helicopter_stands_under_the_air_draft_and_inside_the_mouth() {
    for Drawn {
        built, plan, what, ..
    } in every_utility()
    {
        let craft = &utility::Utility;
        let ground = plan.datum_height();
        let top = tree_bounds(&built).expect("she draws").max.y + ground;
        assert!(
            top <= AIR_DRAFT_CAP,
            "{what}: she stands {top} m over the ground, past {AIR_DRAFT_CAP}"
        );
        let wide = craft.overall_width(&plan);
        assert!(
            wide < GATEWAY_MOUTH,
            "{what}: her fuselage is {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
        );
        let disc = tree_bounds(&built).expect("she draws");
        let span = disc.max.x - disc.min.x;
        assert!(
            (GATEWAY_MOUTH..4.0).contains(&span),
            "{what}: her rotor spans {span} m - decision 3 accepts a disc past \
             the mouth, and no more than a light helicopter's"
        );
    }
}

/// Her rotors turn, always, at the agreed rates (#1613 recommendation 3):
/// exactly two spinning nodes, the main rotor about her up axis at three
/// turns a second carrying her two blades, the tail rotor about her lateral
/// axis at four - both Constant, both inside the sanitiser's ceiling.
#[test]
fn her_rotors_turn_at_the_agreed_rates() {
    use crate::pds::SpinTerm;
    use crate::pds::sanitize::limits::MAX_SPIN_RATE_DEG;
    for Drawn { built, what, .. } in every_utility() {
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
        // The owner's numbers (#1613), as literals: three turns a second and
        // four.
        assert_eq!(
            seen,
            [([0.0, 1.0, 0.0], 1080.0, 3), ([1.0, 0.0, 0.0], 1440.0, 2)],
            "{what}: the main rotor carries its yoke and two blades, the tail \
             rotor two blades"
        );
    }
    assert_eq!(
        (utility::MAIN_ROTOR_DEG_S, utility::TAIL_ROTOR_DEG_S),
        (1080.0, 1440.0)
    );
    // Two blades at three turns a second pass six times a second: the chop
    // her voice is gated at (#1613 recommendation 9).
    assert_eq!(BLADE_PASS_HZ, 6.0);
}

/// A drawn rotor's disc, read off its hub and its blades: the hub's centre
/// in the root's frame, the reach of its blade tips from the hub, and the
/// half-thickness its blades sweep - all from the cuboid corners as drawn,
/// turned by their own rotations.
struct Disc {
    centre: bevy::math::Vec3,
    reach: f32,
    half_thickness: f32,
}

/// The disc the rotor on `hub` sweeps about `axis` (its hub's spin axis).
fn disc(hub: &Generator) -> Disc {
    use bevy::math::{Quat, Vec3};
    let spin = hub.spin.as_ref().expect("a rotor hub spins");
    let axis = Vec3::from(spin.axis.0).normalize();
    let (mut reach, mut half_thickness) = (0.0f32, 0.0f32);
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
            half_thickness = half_thickness.max(along.abs());
            reach = reach.max((p - axis * along).length());
        }
    }
    Disc {
        centre: Vec3::from(hub.transform.translation.0),
        reach,
        half_thickness,
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

/// Her tail rotor sweeps clear of her boom, and of the identity stripes on
/// it, at every blueprint extreme: no vertex of the boom or a stripe, as
/// the mesher draws them, lies inside the slab the tail rotor's blades
/// sweep. The prototype's rotor cut a centimetre into the boom's end on
/// every half turn; `touch` cannot see it, because two parts that overlap
/// also touch.
#[test]
fn her_tail_rotor_sweeps_clear_of_her_boom() {
    use crate::world_builder::primitive_points;
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let colours = helicopter_colours(&ctx);
    let mut closest = f32::MAX;
    for bp in blueprints() {
        let plan = utility::Utility.plan(&bp);
        let built = utility::build_dressed(&ctx, &plan);
        let (_, tail) = rotor_hubs(&built);
        let d = disc(tail);
        // The boom is the body-coloured sweep with no node scale (the pod
        // carries its section scale); the stripes wear the accent.
        let boom: Vec<&Generator> = built
            .children
            .iter()
            .filter(|c| matches!(c.kind, GeneratorKind::Spine { .. }))
            .filter(|c| {
                let m = c.kind.material();
                (m == Some(&colours.body) && c.transform.scale.0 == [1.0; 3])
                    || m == Some(&colours.stripe)
            })
            .collect();
        assert_eq!(boom.len(), 4, "the boom, the fairing and two stripes");
        for part in boom {
            for p in primitive_points(&part.kind) {
                let off = p - d.centre;
                if off.x.abs() <= d.half_thickness {
                    let radial = (off.y * off.y + off.z * off.z).sqrt();
                    assert!(
                        radial > d.reach,
                        "a {} m machine (girth {}, boom {}): her tail rotor sweeps \
                         through her boom at {p}",
                        bp.length,
                        bp.girth,
                        bp.boom
                    );
                } else {
                    closest = closest.min(off.x.abs() - d.half_thickness);
                }
            }
        }
    }
    assert!(
        closest > 0.0 && closest < 1.0,
        "nothing came near: {closest}"
    );
}

/// Her two rotors never cross, at any blueprint extreme: the main rotor's
/// disc and the tail rotor's, each swept as its blades are drawn, keep
/// apart. A tail rotor whose disc's top stands over the main rotor's plane
/// is clear only while it is aft of the main rotor's tips; at the shortest
/// boom and the slimmest cabin the prototype's crossed them by 4 cm, which
/// is why her tail rotor stands lower on her fin.
#[test]
fn her_two_rotors_never_cross() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let mut tightest = f32::MAX;
    for bp in blueprints() {
        let plan = utility::Utility.plan(&bp);
        let built = utility::build_dressed(&ctx, &plan);
        let (main, tail) = rotor_hubs(&built);
        let (m, t) = (disc(main), disc(tail));
        // The height in the main rotor's slab nearest the tail rotor's
        // centre: if even that is beyond the tail disc's reach, the tail
        // disc lies wholly under (or over) the main rotor.
        let y = t
            .centre
            .y
            .clamp(m.centre.y - m.half_thickness, m.centre.y + m.half_thickness);
        let up = (y - t.centre.y).abs();
        let clearance = if up >= t.reach {
            up - t.reach
        } else {
            // At that height the tail disc spans its centre's z plus or
            // minus `w`; the main disc, at the tail disc's inboard face,
            // reaches aft to its centre's z less `l`.
            let w = (t.reach * t.reach - up * up).sqrt();
            let x = (t.centre.x.abs() - t.half_thickness).max(0.0);
            let l = (m.reach * m.reach - x * x).sqrt();
            (m.centre.z - l) - (t.centre.z + w)
        };
        assert!(
            clearance > plan.len(0.01),
            "a {} m machine (girth {}, boom {}): her rotors clear by only {clearance} m",
            bp.length,
            bp.girth,
            bp.boom
        );
        tightest = tightest.min(clearance);
    }
    assert!(tightest < 0.2, "no corner came near: {tightest}");
}

/// The boom carries the identity stripe on BOTH flanks: one sweep each over
/// the same stations, the second drawn tail to nose, which mirrors the
/// sweep's frame and so its band. Read off the mesher's own vertices,
/// because the claim is about which way a transported frame turns, and
/// only the mesher knows.
#[test]
fn the_boom_carries_a_stripe_on_each_flank() {
    use crate::world_builder::primitive_points;
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

/// Every seed drawn as a helicopter is driven by her turbine, and her aura
/// LEAVES HER STACK when it is one that burns: the emitter in her saved
/// record stands at the mouth of the stack drawn on her roof, read off the
/// stack's own node - not off the function that placed the emitter, which
/// would compare one call with itself (#1382 gap 2's lesson).
#[test]
fn a_helicopters_aura_leaves_her_exhaust_stack() {
    use crate::seeded_defaults::AvatarFx;
    use bevy::math::{Quat, Vec3};
    let mut burning = 0;
    for s in helicopter_seeds(600) {
        assert_eq!(propulsion(s), Propulsion::Turbine, "seed {s}");
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
        // The stack: the one cylinder on the root raked about X.
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
            "seed {s}: her {aura:?} issues at {at}, her stack's mouth is at {mouth}"
        );
        burning += 1;
    }
    assert!(burning > 10, "only {burning} burning auras sampled");
}

/// A seeded helicopter's saved record stays well under the soft budget, at
/// a THIRD of it (#1359 rule 9) - the live seeds as saved, FX and voice
/// included, and the heaviest she can draw: every blueprint extreme on her
/// fullest ladder, Ornate and Battered, carrying the heaviest FX overhead a
/// live seed carries. The prototype put her at about 16 KB (#1613).
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
    for bp in blueprints() {
        let plan = utility::Utility.plan(&bp);
        let mut built = utility::build_dressed(&ctx, &plan);
        apply_travel_pose(&mut built, travel_drop(&utility::Utility, &plan));
        worst_corner = worst_corner.max(bytes(&built) + overhead);
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
/// none (#1613 recommendation 7 - wear is the kit's grime).
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    let ctx = PartCtx::for_seed(a_helicopter_seed());
    let plan = utility::Utility.plan(&HelicopterBlueprint::NOMINAL);
    let count = |o: OrnatenessTier, w: WearTier| {
        let mut ctx = ctx;
        (ctx.ornateness, ctx.wear) = (o, w);
        utility::build_dressed(&ctx, &plan).children.len()
    };
    let plain = count(OrnatenessTier::Plain, WearTier::Pristine);
    let adorned = count(OrnatenessTier::Adorned, WearTier::Pristine);
    let ornate = count(OrnatenessTier::Ornate, WearTier::Pristine);
    assert_eq!((adorned - plain, ornate - adorned), (3, 4));
    for w in WearTier::ALL {
        assert_eq!(count(OrnatenessTier::Plain, w), plain, "{w:?}");
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
