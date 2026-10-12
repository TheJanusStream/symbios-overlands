//! The airplane family's guards (#1617): the helicopter's set for a
//! fixed-wing - one machine at every blueprint extreme, the sanitiser round
//! trip, a third of the record budget, determinism, the gate - and the ones
//! an airplane adds: her box holds her fuselage, her drawn wheels stand
//! where her record's raycast wheels do, she sits tail-down with the box's
//! tail corner clear of the ground - drawn, and set down on the game's own
//! physics with every drawn tyre on it - her airscrew and wheels turn as
//! agreed, her aura leaves her stack, and no sweep of hers loops past its
//! stations. Every one sweeps EVERY type (the biplane's since #1623, the
//! twin's since #1625), each held to its own numbers; and the biplane has
//! her own - her struts and wires end inside her wings and her cabane's feet
//! in her fuselage, her airscrew turns clear of her cowl ring, her record
//! carries the handling the owner agreed, and she takes off and tops out as
//! it says she will - and so has the twin: her airscrews turn clear of her
//! fuselage and of the ground, her record carries her handling, and she
//! takes off and tops out as it says.

use bevy::math::{Quat, Vec3};

use super::super::common::{first_difference, touch};
use super::*;
use super::{biplane, high_wing, twin};
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

/// The seeds of type `t` under `n`.
fn seeds_of(t: AirplaneType, n: u64) -> impl Iterator<Item = u64> {
    airplane_seeds(n).filter(move |&s| AirplaneType::for_seed(s) == t)
}

/// The attitude each type stands at on her three wheels (degrees): the
/// high-wing's ten (#1616 recommendation 3), the biplane's twelve and the
/// twin's eleven.
fn ground_deg(t: AirplaneType) -> f32 {
    match t {
        AirplaneType::HighWing => 10.0,
        AirplaneType::Biplane => 12.0,
        AirplaneType::Twin => 11.0,
    }
}

/// Each type's fuselage profile.
fn profile(t: AirplaneType) -> FuseProfile {
    match t {
        AirplaneType::HighWing => high_wing::PROFILE,
        AirplaneType::Biplane => biplane::PROFILE,
        AirplaneType::Twin => twin::PROFILE,
    }
}

/// The radii each type's tyres roll on, the mains' and the tail wheel's
/// (prototype frame).
fn tyres(t: AirplaneType) -> [f32; 2] {
    match t {
        AirplaneType::HighWing => [0.155, 0.055],
        AirplaneType::Biplane => [0.165, 0.05],
        AirplaneType::Twin => [0.15, 0.05],
    }
}

/// How each type is driven: the high-wing's flat four (#1617), the
/// biplane's radial (#1623 Q-H) and the twin's two radials (#1625 Q-H).
fn drive(t: AirplaneType) -> Propulsion {
    match t {
        AirplaneType::HighWing => Propulsion::Propeller,
        AirplaneType::Biplane => Propulsion::Radial,
        AirplaneType::Twin => Propulsion::TwinRadial,
    }
}

/// Each type's airscrews, as agreed: how many, their idle and their gain a
/// metre a second - the high-wing's one at 540 and 55 (#1616), the
/// biplane's the same (#1623), the twin's two at 540 and 41 (#1625 Q-H).
fn airscrews(t: AirplaneType) -> (usize, f32, f32) {
    match t {
        AirplaneType::HighWing | AirplaneType::Biplane => (1, 540.0, 55.0),
        AirplaneType::Twin => (2, 540.0, 41.0),
    }
}

/// The blueprints a guard sweeps: every clamp corner, and the nominal -
/// proportion C, the machine the owner saw.
fn blueprints() -> Vec<AirplaneBlueprint> {
    let mut out = AirplaneBlueprint::corners();
    out.push(AirplaneBlueprint::NOMINAL);
    out
}

/// One airplane a guard sweeps: her type, the built tree (drawn frame, no
/// travel pose), its plan and a label for the failure.
struct Drawn {
    craft: AirplaneType,
    built: Generator,
    plan: FusePlan,
    what: String,
}

/// Every airplane the family can draw: every type, at those blueprints, on
/// every ornateness-by-wear pair.
fn every_drawn() -> Vec<Drawn> {
    let mut ctx = PartCtx::for_seed(an_airplane_seed());
    let mut out = Vec::new();
    for t in AirplaneType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            for o in OrnatenessTier::ALL {
                for w in WearTier::ALL {
                    (ctx.ornateness, ctx.wear) = (o, w);
                    out.push(Drawn {
                        craft: t,
                        built: craft(t).build(&ctx, &plan),
                        plan,
                        what: format!(
                            "a {} m {}, girth {}, cabin {}, {} / {}",
                            bp.length,
                            t.label(),
                            bp.girth,
                            bp.cabin,
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

/// Every biplane the family can draw - [`every_drawn`]'s biplanes.
fn every_biplane() -> Vec<Drawn> {
    every_drawn()
        .into_iter()
        .filter(|d| d.craft == AirplaneType::Biplane)
        .collect()
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
/// trip, as a peer reads her. It is what holds the high-wing's spinner into
/// her cowl's face, where her prototype hung it 5 mm off, and the biplane's
/// airscrew on its shaft into her crankcase.
#[test]
fn an_airplane_is_one_machine_at_every_blueprint_extreme() {
    for Drawn { built, what, .. } in every_drawn() {
        let json = serde_json::to_string(&built).expect("an airplane serializes");
        let saved: Generator = serde_json::from_str(&json).expect("and reads back");
        touch::assert_one_machine(&saved, &what);
    }
}

/// The sanitiser rewrites nothing she is drawn with, at any blueprint
/// extreme or tier (#1359 rule 8): a peer draws exactly what was built.
#[test]
fn an_airplane_survives_sanitize_unchanged_at_her_blueprint_extremes() {
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
        AirplaneType::ALL.len() * 9 * 3 * 3,
        "the sweep lost a combination"
    );
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
/// spans the box side to side - or a radial's cowl ring does, where it
/// stands proud of the fuselage (#1623) - and to its roof, its belly hangs
/// the floor's rise under the floor, and her nose - the airscrew's tip, or
/// the twin's own nose, her airscrews being on her wings (#1625) - and the
/// rudder's trailing edge are the box's two ends. Measured on the drawn
/// tree, the mesher's own points - the rudder's edge in particular is a
/// number each PROFILE states and this reads back.
#[test]
fn the_box_holds_the_drawn_airframe() {
    for Drawn {
        craft: t,
        built,
        plan,
        what,
    } in every_drawn()
    {
        let [hx, hy, hz] = plan.half_extents();
        // The fuselage: the root's first child, the one sweep.
        let mut fuselage = built.clone();
        fuselage.children.truncate(1);
        let drawn = tree_bounds(&fuselage).expect("she draws a fuselage");
        let close = |got: f32, want: f32, tol: f32| (got - want).abs() <= tol;
        // Her widest: the cowl ring - the one bored cylinder on her axis -
        // where she draws one, and her fuselage where she does not. The
        // twin's cowls are bored too, out on her wings, and her box does not
        // hold them (#1625).
        let ring = built.children.iter().find(|c| {
            c.transform.translation.0[0].abs() < 1e-3
                && matches!(&c.kind, GeneratorKind::Cylinder { common, .. } if common.torture.hollow.0 > 0.0)
        });
        assert_eq!(
            ring.is_some(),
            profile(t).cowl_ring > 0.0,
            "{what}: a cowl ring is drawn exactly when her profile names one"
        );
        let widest = match ring {
            Some(ring) => {
                let mut alone = built.clone();
                alone.children = vec![ring.clone()];
                tree_bounds(&alone).expect("she draws a cowl ring")
            }
            None => drawn,
        };
        assert!(
            close(widest.max.x, hx, 2e-3) && close(widest.min.x, -hx, 2e-3),
            "{what}: she is {:.4}..{:.4} wide, her box +-{hx:.4}",
            widest.min.x,
            widest.max.x
        );
        assert!(
            drawn.max.x <= hx + 2e-3 && drawn.min.x >= -hx - 2e-3,
            "{what}: her fuselage is {:.4}..{:.4} wide, past her box's +-{hx:.4}",
            drawn.min.x,
            drawn.max.x
        );
        assert!(
            close(drawn.max.y, hy, 1e-2),
            "{what}: her cabin roof is {:.4}, her box's {hy:.4}",
            drawn.max.y
        );
        let belly = -hy - plan.len(profile(t).box_floor_rise);
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
        // Her nose: the airscrew's tip - the hub's lathe, a spinner or a
        // hubcap, its height ahead - or, the twin's airscrews being on her
        // wings, her fuselage's own nose: its tip station. The sweep's
        // section there is a disc square to its spine, which drops toward
        // the tip - the more, the fatter she is, her girth raising each
        // station by its own half-height - so the disc's top leans ahead of
        // its station by its radius times the sine of that drop, read in
        // the sweep's own frame, which her section's scale stretches only
        // upward. Nothing else of her may stand past it.
        let (tip, lean) = match t {
            AirplaneType::HighWing | AirplaneType::Biplane => {
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
                    panic!("{what}: the airscrew's first part is not its spinner");
                };
                let tip = at.z + points.last().expect("a spinner has stations").height.0;
                (tip, 0.0)
            }
            AirplaneType::Twin => {
                let GeneratorKind::Spine { points, .. } = &built.children[0].kind else {
                    panic!("{what}: her first part is not her fuselage's sweep");
                };
                let [p0, p1] = [0, 1].map(|i| Vec3::from(points[i].position.0));
                let (at, radius) = stations_of(&built.children[0])[0];
                (at.z, radius * (p1 - p0).normalize().y.abs())
            }
        };
        assert!(
            close(tip, hz, 1e-3),
            "{what}: her nose is {tip:.4}, her box's {hz:.4}"
        );
        assert!(
            whole.max.z <= hz + lean + 1e-3,
            "{what}: she draws {:.4} ahead, past her box's nose {hz:.4}",
            whole.max.z
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
/// at her drawn angle - the high-wing's ten degrees, the biplane's twelve,
/// the twin's eleven - with every tyre she draws ON the ground - its lowest
/// point within a millimetre of it - and her box's tail corner clear of it
/// (the #1617 review); and knocked in roll there, she comes to rest again
/// (#1625). The guards round this one read her in her own frame, where her
/// legs and her rays agree; standing, her rays fall plumb while her legs
/// lean with her, and her mains stood a centimetre off the ground with every
/// one of them green. Her record as her seed publishes it, for the first
/// seed of each type and those of it under 400 at each extreme of her
/// blueprint.
#[test]
fn set_down_her_drawn_tyres_stand_on_the_ground() {
    use crate::player::sim::FlightBench;
    let mut picked = Vec::new();
    for t in AirplaneType::ALL {
        let seeds: Vec<(u64, AirplaneBlueprint)> = seeds_of(t, 400)
            .map(|s| {
                let bp = VehicleBlueprint::from_seed(s)
                    .and_then(|b| b.airplane().copied())
                    .expect("an airplane seed carries an airplane blueprint");
                (s, bp)
            })
            .collect();
        picked.push(seeds[0].0);
        let keys: [fn(&AirplaneBlueprint) -> f32; 3] = [|b| b.length, |b| b.girth, |b| b.cabin];
        for key in keys {
            let by = |a: &&(u64, AirplaneBlueprint), b: &&(u64, AirplaneBlueprint)| {
                key(&a.1).total_cmp(&key(&b.1))
            };
            picked.extend(seeds.iter().min_by(by).map(|s| s.0));
            picked.extend(seeds.iter().max_by(by).map(|s| s.0));
        }
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
        let want = ground_deg(AirplaneType::for_seed(s));
        assert!(
            (pitch - want).abs() < 0.1,
            "seed {s}: she stands {pitch:.3} degrees tail-down, {want} drawn"
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
        // Knocked in roll, she comes to rest: the twin's mains, far out under
        // her nacelles, damped her roll past what a step can follow, and she
        // rocked on them without end (#1625).
        bench.set_angular_velocity(rotation * Vec3::NEG_Z * 0.5);
        for _ in 0..2 * 64 {
            bench.step();
        }
        let still = bench.position();
        let mut fastest = 0.0f32;
        for _ in 0..64 {
            bench.step();
            fastest = fastest.max(bench.angular_velocity().length());
        }
        let moved = bench.position().distance(still);
        assert!(
            fastest < 0.01 && moved < 1e-3,
            "seed {s}: knocked in roll, she still turns at {fastest:.3} rad/s, \
             {moved:.4} m moved"
        );
    }
}

/// On her back she is rolled off it (#1240 f162): the uprighting assist's
/// roll, her mass times its acceleration, beats what her roof holds her
/// there by - her weight times her box's half-width - at every blueprint
/// extreme of every type. A twin's box widened to her nacelles stayed on
/// her back (#1625).
#[test]
fn on_her_back_she_is_rolled_harder_than_her_roof_holds_her() {
    use crate::config::rover::AIRPLANE_UPRIGHT_ASSIST_ACCEL;
    for t in AirplaneType::ALL {
        for bp in blueprints() {
            let hx = craft(t).plan(&bp).half_extents()[0];
            let held = 9.81 * hx;
            assert!(
                AIRPLANE_UPRIGHT_ASSIST_ACCEL > held,
                "{t:?} {bp:?}: her roof holds her by {held:.2} m/s^2 of roll, \
                 the assist gives {AIRPLANE_UPRIGHT_ASSIST_ACCEL}"
            );
        }
    }
}

/// Standing on her three wheels she is tail-down at her agreed angle - the
/// high-wing's ten degrees (#1616 recommendation 3), the biplane's twelve,
/// the twin's eleven - and her box's tail corner clears the ground by a
/// centimetre at least, at every blueprint extreme. Floored at her belly,
/// the high-wing's stood 3 cm into the ground ahead of her tail wheel.
#[test]
fn she_stands_tail_down_and_her_box_clears_the_ground() {
    for Drawn {
        craft: t,
        built,
        plan,
        what,
    } in every_drawn()
    {
        let [_, hy, hz] = plan.half_extents();
        let wheels = rolling_wheels(&built);
        let main = Vec3::new(0.0, wheels[0].0.y - wheels[0].1, wheels[0].0.z);
        let tail = Vec3::new(0.0, wheels[2].0.y - wheels[2].1, wheels[2].0.z);
        let ground = (tail - main).normalize();
        let angle = (ground.y / -ground.z).atan().to_degrees();
        assert!(
            (angle - ground_deg(t)).abs() < 0.05,
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
        craft: t,
        built,
        plan,
        what,
    } in every_drawn()
    {
        for pitch in [0.0f32, -ground_deg(t)] {
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
            "{what}: her box is {wide} m wide, past the {GATEWAY_MOUTH} m mouth"
        );
        let b = tree_bounds(&built).expect("she draws");
        let span = b.max.x - b.min.x;
        assert!(
            (GATEWAY_MOUTH..6.0).contains(&span),
            "{what}: her wings span {span} m - decision 3 accepts a span past the \
             mouth, and no more than a light airplane's"
        );
    }
}

/// Her airscrews, her wheels and her tail wheel turn as agreed (#1616
/// recommendation 5, #1623 Q-H for the biplane's, the high-wing's own, and
/// #1625 Q-H for the twin's two): her airscrews about her fore-and-aft axis
/// on a speed term ([`airscrews`], each carrying two blades), the three
/// tyres rolling on their own radii, and the tail wheel steering with her
/// turning - and nothing else.
#[test]
fn her_airscrew_and_wheels_turn_as_agreed() {
    for Drawn {
        craft: t,
        built,
        plan,
        what,
    } in every_drawn()
    {
        let (screws, idle_at, gain_at) = airscrews(t);
        let parts = spinning(&built);
        assert_eq!(
            parts.len(),
            screws + 4,
            "{what}: {} spinning nodes",
            parts.len()
        );
        let (mut speed, mut rolls, mut steers) = (0, 0, 0);
        for (node, _) in parts {
            let spin = node.spin.as_ref().expect("a spinning node");
            assert_eq!(spin.terms.len(), 1, "{what}: one term a node");
            match &spin.terms[0] {
                SpinTerm::Speed { idle, gain } => {
                    assert_eq!(spin.axis.0, [0.0, 0.0, 1.0], "{what}");
                    assert_eq!((idle.0, gain.0), (idle_at, gain_at), "{what}");
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
                        tyres(t)
                            .iter()
                            .any(|&r| (plan.len(r) - radius.0).abs() < 1e-5),
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
        assert_eq!((speed, rolls, steers), (screws, 3, 1), "{what}");
    }
}

/// Every seed drawn as an airplane is driven by her own type's engine - the
/// high-wing's flat four, the biplane's radial, the twin's two radials - and
/// her aura LEAVES HER STACK when it is one that burns: the emitter in her
/// saved record stands at the mouth of the stack she draws - under her nose,
/// or on the twin's port nacelle - read off the stack's own node (#1382 gap
/// 2's lesson), for each type.
#[test]
fn an_airplanes_aura_leaves_her_exhaust_stack() {
    use crate::seeded_defaults::AvatarFx;
    let mut burning = vec![0usize; AirplaneType::ALL.len()];
    for s in airplane_seeds(1200) {
        let t = AirplaneType::for_seed(s);
        assert_eq!(propulsion(s), drive(t), "seed {s}");
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
        // The stack. The high-wing's and the biplane's: the one cylinder on
        // the root raked forward about X by less than a quarter turn - the
        // high-wing's pitot lies along her at a quarter, and so does the
        // biplane's cowl ring; her cylinders turn about Z. The twin's: her
        // port engine's, turned 120 degrees about X off its nacelle's
        // flank - her cowls and engines' faces lie along her at a quarter.
        let cylinder = |c: &&Generator| matches!(c.kind, GeneratorKind::Cylinder { .. });
        let stack = match t {
            AirplaneType::HighWing | AirplaneType::Biplane => root
                .children
                .iter()
                .filter(cylinder)
                .find(|c| (0.1..0.5).contains(&c.transform.rotation.0[0])),
            AirplaneType::Twin => root.children.iter().filter(cylinder).find(|c| {
                Quat::from_array(c.transform.rotation.0)
                    .angle_between(Quat::from_rotation_x(120f32.to_radians()))
                    < 1e-3
                    && c.transform.translation.0[0] > 0.0
            }),
        }
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
        burning[AirplaneType::ALL
            .iter()
            .position(|&x| x == t)
            .expect("her type is a type")] += 1;
    }
    assert!(
        burning.iter().all(|&n| n > 5),
        "too few burning auras sampled: {burning:?}"
    );
}

/// A seeded airplane's saved record stays well under the soft budget, at a
/// THIRD of it (#1359 rule 9) - the live seeds as saved, FX and voice
/// included, and the heaviest she can draw: every type at every blueprint
/// extreme on her fullest ladder, Ornate and Battered, carrying the heaviest
/// FX overhead a live seed carries. The prototypes put the high-wing at
/// about 21 KB (#1616), the biplane at 26.5 to 28.4 (#1623) and the twin at
/// 25 to 27.5 (#1625).
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
    for t in AirplaneType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            let mut built = craft(t).build(&ctx, &plan);
            apply_travel_pose(&mut built, 0.0);
            worst_corner = worst_corner.max(bytes(&built) + overhead);
        }
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
/// none (#1616 recommendation 8 - wear is the kit's grime). On the high-wing
/// and the biplane Adorned adds three - the high-wing a pitot and two steps,
/// the biplane a pitot and two wing walks - and Ornate three more, two spats
/// and a beacon; on the twin (#1625 Q-F) Adorned adds four - a pitot, an
/// aerial mast and a wire from it to each fin - and Ornate three, a beacon
/// and two tip tanks.
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    let ctx = PartCtx::for_seed(an_airplane_seed());
    for t in AirplaneType::ALL {
        let plan = craft(t).plan(&AirplaneBlueprint::NOMINAL);
        let count = |o: OrnatenessTier, w: WearTier| {
            let mut ctx = ctx;
            (ctx.ornateness, ctx.wear) = (o, w);
            craft(t).build(&ctx, &plan).children.len()
        };
        let plain = count(OrnatenessTier::Plain, WearTier::Pristine);
        let adorned = count(OrnatenessTier::Adorned, WearTier::Pristine);
        let ornate = count(OrnatenessTier::Ornate, WearTier::Pristine);
        let steps = match t {
            AirplaneType::HighWing | AirplaneType::Biplane => (3, 3),
            AirplaneType::Twin => (4, 3),
        };
        assert_eq!((adorned - plain, ornate - adorned), steps, "{t:?}");
        for w in WearTier::ALL {
            assert_eq!(count(OrnatenessTier::Plain, w), plain, "{t:?} {w:?}");
        }
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
    for Drawn { built, what, .. } in every_drawn() {
        walk(&built, &what);
    }
}

/// A sweep's stations in the drawn frame, its node's place and scale
/// applied - each `(position, radius)`.
fn stations_of(g: &Generator) -> Vec<(Vec3, f32)> {
    let GeneratorKind::Spine { points, .. } = &g.kind else {
        panic!("not a sweep: {:?}", g.kind);
    };
    let (at, scale) = (
        Vec3::from(g.transform.translation.0),
        Vec3::from(g.transform.scale.0),
    );
    points
        .iter()
        .map(|p| (at + Vec3::from(p.position.0) * scale, p.radius.0))
        .collect()
}

/// How far into a flattened wing `p` stands: `(dz / r)^2 + (dy / (r t))^2`
/// over the wing's drawn section at `p`'s span station - its centre and
/// radius read off its own stations, between which an evenly spaced sweep
/// runs straight, and `t` its node's flattening. Under 1 is inside.
fn into_wing(wing: &Generator, p: Vec3) -> f32 {
    let flat = wing.transform.scale.0[1];
    let st = stations_of(wing);
    let w = st
        .windows(2)
        .find(|w| w[0].0.x <= p.x && p.x <= w[1].0.x)
        .expect("a strut's end stands inside the wing's span");
    let t = (p.x - w[0].0.x) / (w[1].0.x - w[0].0.x);
    let (c, r) = (w[0].0.lerp(w[1].0, t), w[0].1 + (w[1].1 - w[0].1) * t);
    ((p.z - c.z) / r).powi(2) + ((p.y - c.y) / (r * flat)).powi(2)
}

/// The same for the fuselage, whose stations run nose to tail: how far into
/// its drawn section at `p`'s station `p` stands, under 1 inside.
fn into_fuselage(fuselage: &Generator, p: Vec3) -> f32 {
    let section = fuselage.transform.scale.0[1];
    let st = stations_of(fuselage);
    let w = st
        .windows(2)
        .find(|w| w[0].0.z >= p.z && p.z >= w[1].0.z)
        .expect("a cabane's foot stands inside the fuselage's length");
    let t = (w[0].0.z - p.z) / (w[0].0.z - w[1].0.z);
    let (c, r) = (w[0].0.lerp(w[1].0, t), w[0].1 + (w[1].1 - w[0].1) * t);
    (p.x / r).powi(2) + ((p.y - c.y) / (r * section)).powi(2)
}

/// The biplane's struts and wires end inside her wings (#1623): each end of
/// every interplane post, every diagonal and every wire well inside the
/// wing it stands in - read off the wings as drawn - and each cabane rod's
/// head in the upper wing and its foot in the fuselage. One machine at every
/// extreme says that each touches SOMETHING; this says that each touches
/// what holds it up.
#[test]
fn a_biplanes_struts_and_wires_end_inside_her_wings() {
    for Drawn { built, what, .. } in every_biplane() {
        let kids = &built.children;
        let wings: Vec<usize> = (0..kids.len())
            .filter(|&i| {
                matches!(&kids[i].kind, GeneratorKind::Spine { points, .. } if points.len() == 15)
            })
            .collect();
        assert_eq!(wings.len(), 2, "{what}: she draws two wings");
        let (lower, upper) = (&kids[wings[0]], &kids[wings[1]]);
        assert!(
            stations_of(upper)[7].0.y > stations_of(lower)[7].0.y,
            "{what}: the first wing is the lower"
        );
        // The struts and the wires follow the wings, in the prototype's
        // order: an interplane N a side, a cabane N a side, an X a side.
        for (i, rod) in kids[wings[1] + 1..wings[1] + 17].iter().enumerate() {
            let st = stations_of(rod);
            assert_eq!(st.len(), 2, "{what}: rod {i} is a straight rod");
            let (low, high) = if st[0].0.y < st[1].0.y {
                (st[0].0, st[1].0)
            } else {
                (st[1].0, st[0].0)
            };
            let head = into_wing(upper, high);
            assert!(
                head < 0.8,
                "{what}: rod {i}'s head stands {head:.3} into the upper wing"
            );
            let cabane = (6..12).contains(&i);
            let (foot, holds, limit) = if cabane {
                (into_fuselage(&kids[0], low), "fuselage", 0.97)
            } else {
                (into_wing(lower, low), "lower wing", 0.8)
            };
            assert!(
                foot < limit,
                "{what}: rod {i}'s foot stands {foot:.3} into the {holds}"
            );
        }
    }
}

/// The biplane's airscrew turns clear of her cowl ring (#1623, the port's
/// second change): every point of both blades stands ahead of the ring's
/// lip, at every blueprint extreme - and a blade turning about her
/// fore-and-aft axis moves none of them fore or aft. The prototype hung the
/// hub inside the ring's mouth, where her blades swept through its wall.
#[test]
fn a_biplanes_airscrew_turns_clear_of_her_cowl_ring() {
    use crate::world_builder::primitive_points;
    for Drawn {
        built, plan, what, ..
    } in every_biplane()
    {
        let ring = built
            .children
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
        let (screw, at) = spinning(&built)
            .into_iter()
            .find(|(g, _)| {
                matches!(
                    g.spin.as_ref().map(|s| &s.terms[0]),
                    Some(SpinTerm::Speed { .. })
                )
            })
            .expect("she has an airscrew");
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
        assert_eq!(blades, 2, "{what}: a {blades}-blade airscrew");
        assert!(
            aft - lip > plan.len(0.005),
            "{what}: her blades reach {:.4} m ahead of the cowl ring's lip",
            aft - lip
        );
    }
}

/// The biplane's record carries the handling the owner agreed (#1623 Q-G),
/// and the high-wing's the preset's: the biplane's stall, level-flight
/// speed and rates as written, her thrust per kilo the high-wing's and her
/// drag per kilo 1.4 times it - the same engine on a draggier airframe.
#[test]
fn a_biplanes_record_carries_her_handling() {
    use crate::pds::{AirplaneParams, LocomotionConfig};
    let params = |t: AirplaneType| {
        let s = seeds_of(t, 600).next().expect("some seed is of every type");
        match super::super::build_for_seed(s).1 {
            LocomotionConfig::Airplane(p) => *p,
            _ => panic!("seed {s}: an airplane flies the Airplane preset"),
        }
    };
    let (biplane, high_wing) = (
        params(AirplaneType::Biplane),
        params(AirplaneType::HighWing),
    );
    let handling = |p: &AirplaneParams| {
        (
            p.min_airspeed.0,
            p.cruise_speed.0,
            p.roll_rate.0,
            p.pitch_rate.0,
            p.yaw_rate.0,
        )
    };
    assert_eq!(handling(&biplane), (9.0, 17.0, 120.0, 50.0, 25.0));
    assert_eq!(handling(&high_wing), handling(&AirplaneParams::default()));
    let per_kilo = |p: &AirplaneParams| (p.thrust.0 / p.mass.0, p.drag_coefficient.0 / p.mass.0);
    let ((bt, bd), (ht, hd)) = (per_kilo(&biplane), per_kilo(&high_wing));
    assert!(
        (bt / ht - 1.0).abs() < 1e-4,
        "thrust per kilo {bt} against {ht}"
    );
    assert!(
        (bd / hd - 1.4).abs() < 1e-4,
        "drag per kilo {bd} against {hd}"
    );
}

/// On the game's own physics the biplane flies as her handling says (#1623
/// Q-G): set down on her three wheels, full power alone lifts her within
/// the run the agent's pilot sweeps clear (40 m) - her stance is her wings'
/// angle, as the high-wing's is - and held at full power hands off, she
/// settles near 21 m/s, well under the high-wing on the same engine.
#[test]
fn a_biplane_takes_off_on_power_alone_and_tops_out_under_the_high_wing() {
    use crate::pds::AvatarRecord;
    use crate::player::sim::FlightBench;
    use bevy::prelude::KeyCode;
    let record = |t: AirplaneType| {
        AvatarRecord::default_for_seed(seeds_of(t, 600).next().expect("a seed of the type"))
    };
    let biplane = record(AirplaneType::Biplane);
    let mut bench = FlightBench::set_down(&biplane, 0.0, 100.0);
    bench.hold(&[KeyCode::Space]);
    let mut lifted = None;
    for _ in 0..8 * 64 {
        bench.step();
        if lifted.is_none() && bench.underside() > 1.0 {
            lifted = Some(100.0 - bench.position().z);
        }
    }
    let lifted = lifted.expect("she never left the ground");
    assert!(lifted < 38.0, "a {lifted:.1} m take-off run");
    let top = |record: &AvatarRecord| {
        let mut bench = FlightBench::new(record, Vec3::new(0.0, 120.0, 400.0));
        bench.set_velocity(Vec3::new(0.0, 0.0, -18.0));
        bench.set_throttle(1.0);
        bench.hold(&[]);
        for _ in 0..30 * 64 {
            bench.step();
        }
        bench.velocity().length()
    };
    // Measured at the port: 21.6 against the high-wing's 25.1, and a 19 m
    // run.
    let (b, h) = (top(&biplane), top(&record(AirplaneType::HighWing)));
    assert!((20.0..23.0).contains(&b), "she tops out at {b:.2} m/s");
    assert!(
        h - b > 2.5,
        "she tops out at {b:.2} m/s, the high-wing at {h:.2}"
    );
}

/// Every twin the family can draw - [`every_drawn`]'s twins.
fn every_twin() -> Vec<Drawn> {
    every_drawn()
        .into_iter()
        .filter(|d| d.craft == AirplaneType::Twin)
        .collect()
}

/// An airscrew's disc, read off its hub and its blades as drawn: the hub's
/// place in the drawn frame and the furthest any blade's corner reaches
/// from the shaft's axis.
fn airscrew_disc(hub: &Generator, at: Vec3) -> (Vec3, f32) {
    let mut reach = 0.0f32;
    for blade in hub
        .children
        .iter()
        .filter(|c| matches!(c.kind, GeneratorKind::Cuboid { .. }))
    {
        let GeneratorKind::Cuboid { size, .. } = &blade.kind else {
            unreachable!()
        };
        let half = Vec3::from(size.0) * 0.5;
        let place = Vec3::from(blade.transform.translation.0);
        let turn = Quat::from_array(blade.transform.rotation.0).normalize();
        for x in [-1.0f32, 1.0] {
            for y in [-1.0f32, 1.0] {
                for z in [-1.0f32, 1.0] {
                    let p = place + turn * (Vec3::new(x, y, z) * half);
                    reach = reach.max(p.truncate().length());
                }
            }
        }
    }
    (at, reach)
}

/// The twin's airscrews turn clear of her fuselage and of the ground (#1625
/// Q-C), at every blueprint extreme and on every tier: each disc's inboard
/// edge stands outboard of the fuselage's flank wherever the fuselage is
/// drawn within 3 cm of the disc's plane, and flying level - her tail up on
/// her take-off run - each disc's lowest point stands over the line her
/// mains' tyres meet the ground on. Standing tail-down, her discs only rise.
#[test]
fn a_twins_airscrews_turn_clear_of_her_fuselage_and_the_ground() {
    let mut tightest = (f32::MAX, f32::MAX);
    for Drawn {
        built, plan, what, ..
    } in every_twin()
    {
        // The fuselage as drawn: its sweep's own vertices through its node.
        let fuselage = &built.children[0];
        let (place, scale) = (
            Vec3::from(fuselage.transform.translation.0),
            Vec3::from(fuselage.transform.scale.0),
        );
        let flank: Vec<Vec3> = crate::world_builder::primitive_outline(&fuselage.kind)
            .into_iter()
            .map(|p| place + p * scale)
            .collect();
        let wheels = rolling_wheels(&built);
        let ground = wheels[0].0.y - wheels[0].1;
        let screws: Vec<(Vec3, f32)> = spinning(&built)
            .into_iter()
            .filter(|(g, _)| {
                matches!(
                    g.spin.as_ref().map(|s| &s.terms[0]),
                    Some(SpinTerm::Speed { .. })
                )
            })
            .map(|(g, at)| airscrew_disc(g, at))
            .collect();
        assert_eq!(screws.len(), 2, "{what}: {} airscrews", screws.len());
        for (at, reach) in screws {
            let near = flank
                .iter()
                .filter(|p| (p.z - at.z).abs() < plan.len(0.03))
                .map(|p| p.x.abs())
                .fold(0.0f32, f32::max);
            let side = at.x.abs() - reach - near;
            let under = at.y - reach - ground;
            assert!(
                side > plan.len(0.05),
                "{what}: an airscrew turns {side:.4} m off her fuselage"
            );
            assert!(
                under > plan.len(0.05),
                "{what}: an airscrew turns {under:.4} m over the ground flying level"
            );
            tightest = (
                tightest.0.min(side / plan.len(1.0)),
                tightest.1.min(under / plan.len(1.0)),
            );
        }
    }
    assert!(
        tightest.0 < 0.2 && tightest.1 < 0.2,
        "nothing came near her airscrews: {tightest:?}"
    );
}

/// What her voice thrums at is what her airscrews do flying level (#1625
/// Q-H): two blades each at the runtime's ceiling pass at her twin-radial
/// voice's gate, and her airscrews reach that ceiling at or under the speed
/// her own handling flies level at.
#[test]
fn a_twins_airscrews_reach_her_voice_by_her_level_flight() {
    assert_eq!(TWIN_BLADE_PASS_HZ, 2.0 * MAX_SPIN_RATE_DEG / 360.0);
    let level = twin::Twin
        .feel()
        .handling
        .expect("she flies her own handling")
        .cruise;
    let reached = (MAX_SPIN_RATE_DEG - twin::PROP_IDLE_DEG_S) / twin::PROP_GAIN_DEG_PER_M;
    assert!(
        reached <= level,
        "her airscrews reach the ceiling at {reached} m/s, past her level-flight speed of {level}"
    );
}

/// The twin's record carries the handling the owner agreed (#1625 Q-G):
/// her stall, level-flight speed and rates as written, her thrust per kilo
/// the high-wing's and her drag per kilo 0.85 times it - a cleaner airframe
/// on the same power for her weight.
#[test]
fn a_twins_record_carries_her_handling() {
    use crate::pds::{AirplaneParams, LocomotionConfig};
    let params = |t: AirplaneType| {
        let s = seeds_of(t, 3000)
            .next()
            .expect("some seed is of every type");
        match super::super::build_for_seed(s).1 {
            LocomotionConfig::Airplane(p) => *p,
            _ => panic!("seed {s}: an airplane flies the Airplane preset"),
        }
    };
    let (twin, high_wing) = (params(AirplaneType::Twin), params(AirplaneType::HighWing));
    let handling = |p: &AirplaneParams| {
        (
            p.min_airspeed.0,
            p.cruise_speed.0,
            p.roll_rate.0,
            p.pitch_rate.0,
            p.yaw_rate.0,
        )
    };
    assert_eq!(handling(&twin), (13.0, 24.0, 70.0, 40.0, 22.0));
    let per_kilo = |p: &AirplaneParams| (p.thrust.0 / p.mass.0, p.drag_coefficient.0 / p.mass.0);
    let ((tt, td), (ht, hd)) = (per_kilo(&twin), per_kilo(&high_wing));
    assert!(
        (tt / ht - 1.0).abs() < 1e-4,
        "thrust per kilo {tt} against {ht}"
    );
    assert!(
        (td / hd - 0.85).abs() < 1e-4,
        "drag per kilo {td} against {hd}"
    );
}

/// On the game's own physics the twin flies as her handling says (#1625
/// Q-G): set down on her three wheels, full power alone lifts her within
/// 38 m, inside the 40 m run the agent's pilot sweeps clear, and held at
/// full power hands off she tops out over a metre a second faster than the
/// high-wing on the same power for her weight.
#[test]
fn a_twin_takes_off_on_power_alone_and_tops_out_over_the_high_wing() {
    use crate::pds::AvatarRecord;
    use crate::player::sim::FlightBench;
    use bevy::prelude::KeyCode;
    let record = |t: AirplaneType| {
        AvatarRecord::default_for_seed(seeds_of(t, 3000).next().expect("a seed of the type"))
    };
    let twin = record(AirplaneType::Twin);
    let mut bench = FlightBench::set_down(&twin, 0.0, 100.0);
    bench.hold(&[KeyCode::Space]);
    let mut lifted = None;
    for _ in 0..8 * 64 {
        bench.step();
        if lifted.is_none() && bench.underside() > 1.0 {
            lifted = Some(100.0 - bench.position().z);
        }
    }
    let lifted = lifted.expect("she never left the ground");
    assert!(lifted < 38.0, "a {lifted:.1} m take-off run");
    let top = |record: &AvatarRecord| {
        let mut bench = FlightBench::new(record, Vec3::new(0.0, 120.0, 400.0));
        bench.set_velocity(Vec3::new(0.0, 0.0, -20.0));
        bench.set_throttle(1.0);
        bench.hold(&[]);
        for _ in 0..30 * 64 {
            bench.step();
        }
        bench.velocity().length()
    };
    let (w, h) = (top(&twin), top(&record(AirplaneType::HighWing)));
    assert!(
        w - h > 1.0,
        "she tops out at {w:.2} m/s, the high-wing at {h:.2}"
    );
}
