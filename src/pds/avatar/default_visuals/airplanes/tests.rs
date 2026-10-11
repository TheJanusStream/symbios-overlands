//! The airplane family's guards (#1617): the helicopter's set for a
//! fixed-wing - one machine at every blueprint extreme, the sanitiser round
//! trip, a third of the record budget, determinism, the gate - and the ones
//! an airplane adds: her box holds her fuselage, her drawn wheels stand
//! where her record's raycast wheels do, she sits tail-down with the box's
//! tail corner clear of the ground - drawn, and set down on the game's own
//! physics with every drawn tyre on it - her airscrew and wheels turn as
//! agreed, her aura leaves her stack, and no sweep of hers loops past its
//! stations.

use bevy::math::{Quat, Vec3};

use super::super::common::{first_difference, touch};
use super::high_wing;
use super::*;
use crate::catalogue::items::measure::tree_bounds;
use crate::pds::generator::GeneratorKind;
use crate::pds::{SpinTerm, sanitize::limits::MAX_SPIN_RATE_DEG};
use crate::seeded_defaults::{ChassisFamily, OrnatenessTier, VehicleBlueprint, WearTier};

fn an_airplane_seed() -> u64 {
    (0u64..600)
        .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airplane)
        .expect("some seed is an airplane")
}

/// The airplane seeds under `n`.
fn airplane_seeds(n: u64) -> impl Iterator<Item = u64> {
    (0..n).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airplane)
}

/// The blueprints a guard sweeps: every clamp corner, and the nominal -
/// proportion C, the machine the owner saw.
fn blueprints() -> Vec<AirplaneBlueprint> {
    let mut out = AirplaneBlueprint::corners();
    out.push(AirplaneBlueprint::NOMINAL);
    out
}

/// One high-wing monoplane a guard sweeps: the built tree (drawn frame, no
/// travel pose), its plan and a label for the failure.
struct Drawn {
    built: Generator,
    plan: FusePlan,
    what: String,
}

/// Every high-wing monoplane the family can draw at those blueprints, on
/// every ornateness-by-wear pair.
fn every_high_wing() -> Vec<Drawn> {
    let mut ctx = PartCtx::for_seed(an_airplane_seed());
    let mut out = Vec::new();
    for bp in blueprints() {
        let plan = high_wing::HighWing.plan(&bp);
        for o in OrnatenessTier::ALL {
            for w in WearTier::ALL {
                (ctx.ornateness, ctx.wear) = (o, w);
                out.push(Drawn {
                    built: high_wing::build_dressed(&ctx, &plan),
                    plan,
                    what: format!(
                        "a {} m high-wing monoplane, girth {}, cabin {}, {} / {}",
                        bp.length,
                        bp.girth,
                        bp.cabin,
                        o.label(),
                        w.label()
                    ),
                });
            }
        }
    }
    out
}

/// Every node in `root` that turns, with its translation summed down from
/// the root - where it stands in the drawn frame.
fn spinning(root: &Generator) -> Vec<(&Generator, Vec3)> {
    fn walk<'a>(g: &'a Generator, at: Vec3, out: &mut Vec<(&'a Generator, Vec3)>) {
        let here = at + Vec3::from(g.transform.translation.0);
        if g.spin.is_some() {
            out.push((g, here));
        }
        for c in &g.children {
            walk(c, here, out);
        }
    }
    let mut out = Vec::new();
    for c in &root.children {
        walk(c, Vec3::ZERO, &mut out);
    }
    out
}

/// Her rolling wheels in the drawn frame - each hub, and its tyre's radius -
/// the mains first.
fn rolling_wheels(built: &Generator) -> Vec<(Vec3, f32)> {
    let mut wheels: Vec<(Vec3, f32)> = spinning(built)
        .into_iter()
        .filter_map(
            |(g, at)| match g.spin.as_ref().map(|s| s.terms[0].clone()) {
                Some(SpinTerm::Roll { radius }) => Some((at, radius.0)),
                _ => None,
            },
        )
        .collect();
    wheels.sort_by(|a, b| b.0.z.total_cmp(&a.0.z));
    wheels
}

/// Every airplane seed draws an airplane, and draws ITS OWN type - and every
/// type is picked by some seed under 600.
#[test]
fn every_airplane_seed_resolves_to_a_built_craft() {
    let mut total = 0;
    let mut picked: Vec<AirplaneType> = Vec::new();
    for s in airplane_seeds(600) {
        let t = AirplaneType::for_seed(s);
        if !picked.contains(&t) {
            picked.push(t);
        }
        let (_, plan) = body_for(s).expect("an airplane seed has a body");
        assert!(plan.airframe_len() > 2.5, "seed {s}: degenerate airframe");
        total += 1;
    }
    assert!(total > 50, "too few airplanes sampled: {total}");
    for t in AirplaneType::ALL {
        assert!(picked.contains(&t), "no airplane seed under 600 is {t:?}");
    }
}

/// She is one machine at every blueprint extreme and on every tier: every
/// part meets another, so nothing floats (#1364). Read after a JSON round
/// trip, as a peer reads her. It is what holds the spinner into the cowl's
/// face, where the prototype hung it 5 mm off.
#[test]
fn a_high_wing_monoplane_is_one_machine_at_every_blueprint_extreme() {
    for Drawn { built, what, .. } in every_high_wing() {
        let json = serde_json::to_string(&built).expect("an airplane serializes");
        let saved: Generator = serde_json::from_str(&json).expect("and reads back");
        touch::assert_one_machine(&saved, &what);
    }
}

/// The sanitiser rewrites nothing she is drawn with, at any blueprint
/// extreme or tier (#1359 rule 8): a peer draws exactly what was built.
#[test]
fn a_high_wing_monoplane_survives_sanitize_unchanged_at_her_blueprint_extremes() {
    use crate::pds::sanitize_avatar_visuals;
    let mut n = 0;
    for Drawn { built, what, .. } in every_high_wing() {
        let mut sanitized = built.clone();
        sanitize_avatar_visuals(&mut sanitized);
        if let Some(where_) = first_difference(&built, &sanitized, "0") {
            panic!("{what} was rewritten by the sanitiser at {where_}");
        }
        n += 1;
    }
    assert_eq!(n, 9 * 3 * 3, "the sweep lost a combination");
}

/// Building the same seed twice gives the same tree, bit for bit - and so
/// does her locomotion.
#[test]
fn a_seeded_airplane_builds_deterministically() {
    for s in airplane_seeds(300) {
        assert_eq!(
            build(s, None),
            build(s, None),
            "seed {s} is not deterministic"
        );
        assert_eq!(feel_and_layout(s), feel_and_layout(s), "seed {s}");
    }
}

/// Her box holds her airframe (#1616 recommendation 4): the fuselage's sweep
/// spans the box side to side and to its roof, its belly hangs the floor's
/// rise under the floor, and the spinner's tip and the rudder's trailing
/// edge are the box's two ends. Measured on the drawn tree, the mesher's own
/// points - the rudder's edge in particular is a number the PROFILE states
/// and this reads back.
#[test]
fn the_box_holds_the_drawn_airframe() {
    for Drawn {
        built, plan, what, ..
    } in every_high_wing()
    {
        let [hx, hy, hz] = plan.half_extents();
        // The fuselage: the root's first child, the one sweep.
        let mut fuselage = built.clone();
        fuselage.children.truncate(1);
        let drawn = tree_bounds(&fuselage).expect("she draws a fuselage");
        let close = |got: f32, want: f32, tol: f32| (got - want).abs() <= tol;
        assert!(
            close(drawn.max.x, hx, 2e-3) && close(drawn.min.x, -hx, 2e-3),
            "{what}: her fuselage is {:.4}..{:.4} wide, her box +-{hx:.4}",
            drawn.min.x,
            drawn.max.x
        );
        assert!(
            close(drawn.max.y, hy, 1e-2),
            "{what}: her cabin roof is {:.4}, her box's {hy:.4}",
            drawn.max.y
        );
        let belly = -hy - plan.len(high_wing::PROFILE.box_floor_rise);
        assert!(
            close(drawn.min.y, belly, 1e-2),
            "{what}: her belly is {:.4}, {belly:.4} under her box's floor",
            drawn.min.y
        );
        assert!(
            drawn.min.z >= -hz,
            "{what}: her tail post reaches {:.4}, past her box's {:.4}",
            drawn.min.z,
            -hz
        );
        let whole = tree_bounds(&built).expect("she draws");
        assert!(
            close(whole.min.z, -hz, 5e-3),
            "{what}: her rudder reaches {:.4}, her box {:.4}",
            whole.min.z,
            -hz
        );
        // The spinner's tip: the airscrew hub's lathe, its height ahead.
        let (screw, at) = spinning(&built)
            .into_iter()
            .find(|(g, _)| {
                matches!(
                    g.spin.as_ref().map(|s| &s.terms[0]),
                    Some(SpinTerm::Speed { .. })
                )
            })
            .expect("she has an airscrew");
        let GeneratorKind::Lathe { points, .. } = &screw.children[0].kind else {
            panic!("{what}: the airscrew's first part is not the spinner");
        };
        let tip = at.z + points.last().expect("a spinner has stations").height.0;
        assert!(
            close(tip, hz, 1e-3),
            "{what}: her spinner's tip is {tip:.4}, her box's nose {hz:.4}"
        );
    }
}

/// Her drawn wheels stand where her record's raycast wheels do (#1617): the
/// mains `gear_track` of the half-width out and `gear_ahead` of the
/// half-length ahead, and the tail wheel `tail_wheel_aft` behind - read off
/// the rolling hubs she draws and the record her seed publishes, so a wheel
/// moved in one is caught in the other. How far they reach is the bench's
/// to say (`set_down_her_drawn_tyres_stand_on_the_ground`): her rays fall
/// plumb, so a reach is right only standing.
#[test]
fn her_drawn_wheels_stand_where_her_record_stands_them() {
    use crate::pds::LocomotionConfig;
    let mut seen = 0;
    for s in airplane_seeds(300) {
        let (_, loco) = super::super::build_for_seed(s);
        let LocomotionConfig::Airplane(p) = loco else {
            panic!("seed {s}: an airplane flies the Airplane preset");
        };
        let (craft, plan) = body_for(s).expect("an airplane seed has a body");
        let wheels = rolling_wheels(&craft.build(&PartCtx::for_seed(s), &plan));
        let [hx, _, hz] = p.chassis_half_extents.0;
        assert_eq!(wheels.len(), 3, "seed {s}: {} rolling wheels", wheels.len());
        for (i, (at, _)) in wheels.iter().enumerate() {
            let (want_x, want_z) = if i < 2 {
                (p.gear_track.0 * hx, p.gear_ahead.0 * hz)
            } else {
                (0.0, -p.tail_wheel_aft.0 * hz)
            };
            assert!(
                (at.x.abs() - want_x).abs() < 1e-3 && (at.z - want_z).abs() < 1e-3,
                "seed {s}: wheel {i} drawn at ({:.4}, {:.4}), her record has ({want_x:.4}, \
                 {want_z:.4})",
                at.x.abs(),
                at.z
            );
        }
        seen += 1;
    }
    assert!(seen > 20, "only {seen} airplanes sampled");
}

/// Set down on the game's own physics, a seeded airplane settles tail-down
/// at her drawn ten degrees with every tyre she draws ON the ground - its
/// lowest point within a millimetre of it - and her box's tail corner clear
/// of it (the #1617 review). The guards round this one read her in her own
/// frame, where her legs and her rays agree; standing, her rays fall plumb
/// while her legs lean with her, and her mains stood a centimetre off the
/// ground with every one of them green. Her record as her seed publishes
/// it, for the first airplane seed and those under 400 at each extreme of
/// her blueprint.
#[test]
fn set_down_her_drawn_tyres_stand_on_the_ground() {
    use crate::player::sim::FlightBench;
    let seeds: Vec<(u64, AirplaneBlueprint)> = airplane_seeds(400)
        .map(|s| {
            let bp = VehicleBlueprint::from_seed(s)
                .and_then(|b| b.airplane().copied())
                .expect("an airplane seed carries an airplane blueprint");
            (s, bp)
        })
        .collect();
    let mut picked = vec![seeds[0].0];
    let keys: [fn(&AirplaneBlueprint) -> f32; 3] = [|b| b.length, |b| b.girth, |b| b.cabin];
    for key in keys {
        let by = |a: &&(u64, AirplaneBlueprint), b: &&(u64, AirplaneBlueprint)| {
            key(&a.1).total_cmp(&key(&b.1))
        };
        picked.extend(seeds.iter().min_by(by).map(|s| s.0));
        picked.extend(seeds.iter().max_by(by).map(|s| s.0));
    }
    picked.sort_unstable();
    picked.dedup();
    // The travel pose turns her round: her drawn nose is +Z, her body's -Z.
    let turned = Quat::from_rotation_y(std::f32::consts::PI);
    for s in picked {
        let record = crate::pds::AvatarRecord::default_for_seed(s);
        let mut bench = FlightBench::set_down(&record, 0.0, 100.0);
        for _ in 0..4 * 64 {
            bench.step();
        }
        let (at, rotation) = (bench.position(), bench.rotation());
        let pitch = (rotation * Vec3::NEG_Z).y.asin().to_degrees();
        assert!(
            (pitch - 10.0).abs() < 0.1,
            "seed {s}: she stands {pitch:.3} degrees tail-down"
        );
        let (craft, plan) = body_for(s).expect("an airplane seed has a body");
        let wheels = rolling_wheels(&craft.build(&PartCtx::for_seed(s), &plan));
        for (i, (hub, radius)) in wheels.into_iter().enumerate() {
            let lowest = (at + rotation * (turned * hub)).y - radius;
            assert!(
                lowest.abs() < 1e-3,
                "seed {s}: wheel {i}'s tyre bottoms {:.2} mm {} the ground",
                lowest.abs() * 1e3,
                if lowest > 0.0 { "over" } else { "into" }
            );
        }
        let clear = bench.underside();
        assert!(
            clear > 0.01,
            "seed {s}: her box's tail corner stands {clear:.4} m over the ground"
        );
    }
}

/// Standing on her three wheels she is tail-down at the agreed ten degrees
/// (#1616 recommendation 3), and her box's tail corner clears the ground by
/// a centimetre at least - at every blueprint extreme. Floored at her belly,
/// it stood 3 cm into the ground ahead of her tail wheel.
#[test]
fn she_stands_tail_down_and_her_box_clears_the_ground() {
    for Drawn {
        built, plan, what, ..
    } in every_high_wing()
    {
        let [_, hy, hz] = plan.half_extents();
        let wheels = rolling_wheels(&built);
        let main = Vec3::new(0.0, wheels[0].0.y - wheels[0].1, wheels[0].0.z);
        let tail = Vec3::new(0.0, wheels[2].0.y - wheels[2].1, wheels[2].0.z);
        let ground = (tail - main).normalize();
        let angle = (ground.y / -ground.z).atan().to_degrees();
        assert!(
            (angle - 10.0).abs() < 0.05,
            "{what}: she stands {angle:.3} degrees tail-down"
        );
        // The box's tail corner over the line through both contacts.
        let corner = Vec3::new(0.0, -hy, -hz) - main;
        let up = Vec3::new(0.0, -ground.z, ground.y);
        let clear = corner.dot(up);
        assert!(
            clear > 0.01,
            "{what}: the box's tail corner clears by {clear:.4}"
        );
    }
}

/// She stands under the air draft (#1359 rule 6) in both her poses - level
/// in the air and tail-down on the ground, airscrew blade up - and her
/// fuselage passes the narrowest mouth. Her wing does not, and owner
/// decision 3 of #1611 says so - pinned here both ways so it stays a
/// decision rather than a drift.
#[test]
fn an_airplane_stands_under_the_air_draft_and_inside_the_mouth() {
    use super::super::common::{quat_x, quat_xyzw};
    for Drawn {
        built, plan, what, ..
    } in every_high_wing()
    {
        for pitch in [0.0f32, -10.0] {
            let mut posed = built.clone();
            posed.transform.rotation = quat_xyzw(quat_x(pitch.to_radians()));
            let b = tree_bounds(&posed).expect("she draws");
            let tall = b.max.y - b.min.y;
            assert!(
                tall <= AIR_DRAFT_CAP,
                "{what}: she stands {tall} m tall pitched {pitch}, past {AIR_DRAFT_CAP}"
            );
        }
        let wide = plan.half_extents()[0] * 2.0;
        assert!(
            wide < GATEWAY_MOUTH,
            "{what}: her fuselage is {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
        );
        let b = tree_bounds(&built).expect("she draws");
        let span = b.max.x - b.min.x;
        assert!(
            (GATEWAY_MOUTH..6.0).contains(&span),
            "{what}: her wing spans {span} m - decision 3 accepts a span past the \
             mouth, and no more than a light airplane's"
        );
    }
}

/// Her airscrew, her wheels and her tail wheel turn as agreed (#1616
/// recommendation 5): exactly five spinning nodes - the airscrew about her
/// fore-and-aft axis on a speed term (540 idling and 55 a metre a second,
/// carrying two blades), the three tyres rolling on their own radii, and
/// the tail wheel steering with her turning.
#[test]
fn her_airscrew_and_wheels_turn_as_agreed() {
    for Drawn {
        built, plan, what, ..
    } in every_high_wing()
    {
        let parts = spinning(&built);
        assert_eq!(parts.len(), 5, "{what}: {} spinning nodes", parts.len());
        let (mut speed, mut rolls, mut steers) = (0, 0, 0);
        for (node, _) in parts {
            let spin = node.spin.as_ref().expect("a spinning node");
            assert_eq!(spin.terms.len(), 1, "{what}: one term a node");
            match &spin.terms[0] {
                SpinTerm::Speed { idle, gain } => {
                    assert_eq!(spin.axis.0, [0.0, 0.0, 1.0], "{what}");
                    assert_eq!((idle.0, gain.0), (540.0, 55.0), "{what}");
                    assert!(idle.0 <= MAX_SPIN_RATE_DEG);
                    let blades = node
                        .children
                        .iter()
                        .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
                        .count();
                    assert_eq!(blades, 2, "{what}: a {blades}-blade airscrew");
                    speed += 1;
                }
                SpinTerm::Roll { radius } => {
                    assert_eq!(spin.axis.0, [1.0, 0.0, 0.0], "{what}");
                    assert!(
                        [plan.len(0.155), plan.len(0.055)]
                            .iter()
                            .any(|r| (r - radius.0).abs() < 1e-5),
                        "{what}: a wheel rolls on {}",
                        radius.0
                    );
                    rolls += 1;
                }
                SpinTerm::Steer { gain, limit } => {
                    assert_eq!(spin.axis.0, [0.0, 1.0, 0.0], "{what}");
                    assert_eq!((gain.0, limit.0), (0.4, 25.0), "{what}");
                    steers += 1;
                }
                other => panic!("{what}: a part turns by {other:?}"),
            }
        }
        assert_eq!((speed, rolls, steers), (1, 3, 1), "{what}");
    }
}

/// Every seed drawn as an airplane is driven by her propeller, and her aura
/// LEAVES HER STACK when it is one that burns: the emitter in her saved
/// record stands at the mouth of the stack drawn under her cowl, read off
/// the stack's own node (#1382 gap 2's lesson).
#[test]
fn an_airplanes_aura_leaves_her_exhaust_stack() {
    use crate::seeded_defaults::AvatarFx;
    let mut burning = 0;
    for s in airplane_seeds(600) {
        assert_eq!(propulsion(s), Propulsion::Propeller, "seed {s}");
        let aura = AvatarFx::for_seed(s).aura;
        if !matches!(
            aura,
            ParticleAura::Exhaust | ParticleAura::Steam | ParticleAura::Embers
        ) {
            continue;
        }
        let (body, _) = super::super::build_for_seed(s);
        let root = body.visuals().expect("an airplane draws a tree");
        let emitter = root
            .children
            .iter()
            .find(|c| matches!(c.kind, GeneratorKind::ParticleSystem(_)))
            .expect("a burning aura hangs an emitter");
        // The stack: the one cylinder on the root raked forward about X by
        // less than a quarter turn - the pitot lies along her at a quarter.
        let stack = root
            .children
            .iter()
            .find(|c| {
                matches!(c.kind, GeneratorKind::Cylinder { .. })
                    && (0.1..0.5).contains(&c.transform.rotation.0[0])
            })
            .expect("she draws an exhaust stack");
        let GeneratorKind::Cylinder { height, .. } = &stack.kind else {
            unreachable!()
        };
        let mouth = Vec3::from(stack.transform.translation.0)
            + Quat::from_array(stack.transform.rotation.0) * Vec3::new(0.0, -height.0 * 0.5, 0.0);
        let at = Vec3::from(emitter.transform.translation.0);
        assert!(
            at.distance(mouth) < 2e-3,
            "seed {s}: her {aura:?} issues at {at}, her stack's mouth is at {mouth}"
        );
        burning += 1;
    }
    assert!(burning > 10, "only {burning} burning auras sampled");
}

/// A seeded airplane's saved record stays well under the soft budget, at a
/// THIRD of it (#1359 rule 9) - the live seeds as saved, FX and voice
/// included, and the heaviest she can draw: every blueprint extreme on her
/// fullest ladder, Ornate and Battered, carrying the heaviest FX overhead a
/// live seed carries. The prototype put her at about 21 KB (#1616).
#[test]
fn a_seeded_airplanes_record_stays_well_inside_the_budget() {
    use crate::pds::avatar::AvatarRecord;
    use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
    let bytes = |t: &Generator| serialized_record_bytes(t).expect("an airplane serializes");
    let (mut worst_seed, mut overhead) = (0usize, 0usize);
    for s in airplane_seeds(400) {
        let saved = serialized_record_bytes(&AvatarRecord::default_for_seed(s))
            .expect("a record serializes");
        worst_seed = worst_seed.max(saved);
        overhead = overhead.max(saved.saturating_sub(bytes(&build(s, None))));
    }
    assert!(worst_seed > 0 && overhead > 0, "nothing was measured");
    let mut ctx = PartCtx::for_seed(an_airplane_seed());
    ctx.ornateness = OrnatenessTier::Ornate;
    ctx.wear = WearTier::Battered;
    let mut worst_corner = 0usize;
    for bp in blueprints() {
        let plan = high_wing::HighWing.plan(&bp);
        let mut built = high_wing::build_dressed(&ctx, &plan);
        apply_travel_pose(&mut built, 0.0);
        worst_corner = worst_corner.max(bytes(&built) + overhead);
    }
    for (what, worst) in [
        ("seeded airplane", worst_seed),
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
/// none (#1616 recommendation 8 - wear is the kit's grime). Adorned adds a
/// pitot and two steps; ornate two spats and a beacon on top.
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    let ctx = PartCtx::for_seed(an_airplane_seed());
    let plan = high_wing::HighWing.plan(&AirplaneBlueprint::NOMINAL);
    let count = |o: OrnatenessTier, w: WearTier| {
        let mut ctx = ctx;
        (ctx.ornateness, ctx.wear) = (o, w);
        high_wing::build_dressed(&ctx, &plan).children.len()
    };
    let plain = count(OrnatenessTier::Plain, WearTier::Pristine);
    let adorned = count(OrnatenessTier::Adorned, WearTier::Pristine);
    let ornate = count(OrnatenessTier::Ornate, WearTier::Pristine);
    assert_eq!((adorned - plain, ornate - adorned), (3, 3));
    for w in WearTier::ALL {
        assert_eq!(count(OrnatenessTier::Plain, w), plain, "{w:?}");
    }
}

/// The blueprint the guards sweep is the blueprint the seeds reach: every
/// live seed's three numbers sit inside the corners' box.
#[test]
fn the_corners_bound_every_seeded_airplane() {
    let corners = AirplaneBlueprint::corners();
    let span = |f: &dyn Fn(&AirplaneBlueprint) -> f32| {
        let lo = corners.iter().map(f).fold(f32::MAX, f32::min);
        let hi = corners.iter().map(f).fold(f32::MIN, f32::max);
        lo..=hi
    };
    let (length, girth, cabin) = (span(&|b| b.length), span(&|b| b.girth), span(&|b| b.cabin));
    for s in airplane_seeds(600) {
        let bp = VehicleBlueprint::from_seed(s)
            .and_then(|b| b.airplane().copied())
            .expect("an airplane seed has a blueprint");
        assert!(
            length.contains(&bp.length),
            "seed {s}: length {}",
            bp.length
        );
        assert!(girth.contains(&bp.girth), "seed {s}: girth {}", bp.girth);
        assert!(cabin.contains(&bp.cabin), "seed {s}: cabin {}", bp.cabin);
    }
}

/// No sweep she draws loops past its own stations (#1616's trap): a sweep
/// is a Catmull-Rom parameterised by station, not by distance, and the
/// prototype's first wing - stations bunched at the tips beside a 4 m gap -
/// looped 8 cm past each tip and back. Every sweep is sampled as the mesher
/// samples it, its end stations reflected, and held within a centimetre of
/// the box its stations span, centre and radius both - in TRUE metres, its
/// node's scale applied. A centimetre is an eighth of that loop, and room
/// for the half-centimetre a spline bulges where a chord starts to close
/// toward a tip; measured in stored units, a flattened wing's heights read
/// seven times taller than they are.
#[test]
fn no_sweep_of_hers_loops_past_its_stations() {
    fn catmull(p: [[f32; 4]; 4], t: f32) -> [f32; 4] {
        std::array::from_fn(|i| {
            let [a, b, c, d] = [p[0][i], p[1][i], p[2][i], p[3][i]];
            0.5 * (2.0 * b
                + (c - a) * t
                + (2.0 * a - 5.0 * b + 4.0 * c - d) * t * t
                + (3.0 * b - a - 3.0 * c + d) * t * t * t)
        })
    }
    fn walk(g: &Generator, what: &str) {
        if let GeneratorKind::Spine { points, .. } = &g.kind {
            let [sx, sy, sz] = g.transform.scale.0;
            let pts: Vec<[f32; 4]> = points
                .iter()
                .map(|p| {
                    let [x, y, z] = p.position.0;
                    [x * sx, y * sy, z * sz, p.radius.0]
                })
                .collect();
            let n = pts.len();
            if n >= 3 {
                let reflect = |a: [f32; 4], b: [f32; 4]| std::array::from_fn(|i| 2.0 * a[i] - b[i]);
                let mut ext = vec![reflect(pts[0], pts[1])];
                ext.extend(&pts);
                ext.push(reflect(pts[n - 1], pts[n - 2]));
                let lo: [f32; 4] =
                    std::array::from_fn(|i| pts.iter().map(|p| p[i]).fold(f32::MAX, f32::min));
                let hi: [f32; 4] =
                    std::array::from_fn(|i| pts.iter().map(|p| p[i]).fold(f32::MIN, f32::max));
                for w in ext.windows(4) {
                    for k in 0..=20 {
                        let q = catmull([w[0], w[1], w[2], w[3]], k as f32 / 20.0);
                        for i in 0..4 {
                            let past = (lo[i] - q[i]).max(q[i] - hi[i]);
                            assert!(
                                past < 1e-2,
                                "{what}: a sweep of {n} stations loops {past:.4} past them"
                            );
                        }
                    }
                }
            }
        }
        for c in &g.children {
            walk(c, what);
        }
    }
    for Drawn { built, what, .. } in every_high_wing() {
        walk(&built, &what);
    }
}
