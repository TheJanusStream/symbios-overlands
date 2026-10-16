//! The airship family's guards (#1657): the fleet's set for a ship of the
//! air - one machine at every blueprint extreme, the sanitiser round trip, a
//! third of the record budget, determinism, the gate - and the ones a hull
//! of revolution adds: her box holds her airscrews' whole discs and she is
//! centred in it, her airscrews turn opposite ways in pairs and clear her
//! hull, her hull's spline never loops past its stations, and every lit part
//! she draws shows. Every one sweeps EVERY type, each held to its own hull;
//! a type's own guards live beside it (the Zeppelin's in `zeppelin`).

use bevy::math::{Affine3A, Quat, Vec3};

use super::super::common::{first_difference, sight, touch};
use super::*;
use crate::pds::SpinTerm;
use crate::seeded_defaults::{ChassisFamily, OrnatenessTier, VehicleBlueprint, WearTier};

/// The airship seeds under `n`.
fn airship_seeds(n: u64) -> impl Iterator<Item = u64> {
    (0..n).filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airship)
}

fn an_airship_seed() -> u64 {
    airship_seeds(600).next().expect("some seed is an airship")
}

/// The blueprints a guard sweeps: every clamp corner, and the nominal -
/// proportion B, the ship the owner saw.
fn blueprints() -> Vec<AirshipBlueprint> {
    let mut out = AirshipBlueprint::corners();
    out.push(AirshipBlueprint::NOMINAL);
    out
}

/// One airship a guard sweeps: her type, the built tree (her own frame), its
/// plan, its ornateness and a label for the failure.
struct Drawn {
    built: Generator,
    plan: HullPlan,
    ornateness: OrnatenessTier,
    what: String,
}

/// Every airship the family can draw: every type, at those blueprints, on
/// every ornateness-by-wear pair.
fn every_drawn() -> Vec<Drawn> {
    let mut ctx = PartCtx::for_seed(an_airship_seed());
    let mut out = Vec::new();
    for t in AirshipType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            for o in OrnatenessTier::ALL {
                for w in WearTier::ALL {
                    (ctx.ornateness, ctx.wear) = (o, w);
                    out.push(Drawn {
                        built: craft(t).build(&ctx, &plan),
                        plan,
                        ornateness: o,
                        what: format!(
                            "a {} m {}, fineness {}, {} / {}",
                            bp.length,
                            t.label(),
                            bp.fineness,
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

/// Every node in `root` that turns, with its world transform (her own
/// frame).
fn spinning(root: &Generator) -> Vec<(&Generator, Affine3A)> {
    fn walk<'a>(g: &'a Generator, parent: Affine3A, out: &mut Vec<(&'a Generator, Affine3A)>) {
        let t = &g.transform;
        let here = parent
            * Affine3A::from_scale_rotation_translation(
                Vec3::from(t.scale.0),
                Quat::from_array(t.rotation.0).normalize(),
                Vec3::from(t.translation.0),
            );
        if g.spin.is_some() {
            out.push((g, here));
        }
        for c in &g.children {
            walk(c, here, out);
        }
    }
    let mut out = Vec::new();
    walk(root, Affine3A::IDENTITY, &mut out);
    out
}

/// Every airship seed draws an airship, and draws ITS OWN type - and every
/// type is picked by some seed under 600, so the fleet the owner meets holds
/// all of them. The craft axis answers for her too, as it does for every
/// other vehicle family since #1657.
#[test]
fn every_airship_seed_draws_her_own_type() {
    let mut total = 0;
    let mut picked: Vec<AirshipType> = Vec::new();
    for s in airship_seeds(600) {
        let t = AirshipType::for_seed(s);
        if !picked.contains(&t) {
            picked.push(t);
        }
        assert_eq!(
            crate::seeded_defaults::CraftType::for_seed(s),
            Some(crate::seeded_defaults::CraftType::Airship(t)),
            "seed {s}: the craft axis disagrees with the type pick"
        );
        let (_, plan, _) = drawn_for(s, None).expect("an airship seed draws a ship");
        assert!(plan.length() > 3.0, "seed {s}: degenerate hull");
        total += 1;
    }
    assert!(total > 50, "too few airships sampled: {total}");
    for t in AirshipType::ALL {
        assert!(picked.contains(&t), "no airship seed under 600 is {t:?}");
    }
}

/// She is one machine at every blueprint extreme and on every tier: every
/// part meets another, so nothing floats (#1364's rule, asked by
/// arithmetic). Read after a JSON round trip, as a peer reads her.
#[test]
fn an_airship_is_one_machine_at_every_blueprint_extreme() {
    for Drawn { built, what, .. } in every_drawn() {
        let json = serde_json::to_string(&built).expect("an airship serializes");
        let saved: Generator = serde_json::from_str(&json).expect("and reads back");
        touch::assert_one_machine(&saved, &what);
    }
}

/// The sanitiser rewrites nothing she is drawn with, at any blueprint
/// extreme or tier (#1359 rule 8): a peer draws exactly what was built.
#[test]
fn an_airship_survives_sanitize_unchanged_at_her_blueprint_extremes() {
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
        AirshipType::ALL.len() * 5 * 3 * 3,
        "the sweep lost a combination"
    );
}

/// Building the same seed twice gives the same tree, bit for bit.
#[test]
fn a_seeded_airship_builds_deterministically() {
    for s in airship_seeds(300) {
        assert_eq!(
            build(s, None),
            build(s, None),
            "seed {s} is not deterministic"
        );
    }
}

/// Her box holds everything she draws, her airscrews' discs swept whole, and
/// is no bigger; and she is posed centred in it - nose to tail, side to side
/// and keel to crown - so avian, which centres a collider on its body, holds
/// her where she is drawn, and a landed ship rests on her lowest point
/// (#1612). Read through the seeded record, as the game builds it.
#[test]
fn her_box_holds_her_and_she_is_centred_in_it() {
    let mut checked = 0;
    for s in airship_seeds(400) {
        let (body, loco) = super::super::build_for_seed(s);
        let crate::pds::LocomotionConfig::Helicopter(p) = &loco else {
            panic!("airship seed {s} is not on the helicopter preset");
        };
        let half = Vec3::from(p.chassis_half_extents.0);
        let root = body.visuals().expect("an airship draws a tree");
        // Her travel pose's yaw and drop, then her aura's emitter, which is
        // no part of her: what is left is the ship.
        let mut ship = root.clone();
        ship.children
            .retain(|c| !matches!(c.kind, crate::pds::GeneratorKind::ParticleSystem { .. }));
        let b = swept_bounds(&ship);
        for (axis, (lo, hi)) in [(b.min.x, b.max.x), (b.min.y, b.max.y), (b.min.z, b.max.z)]
            .into_iter()
            .enumerate()
        {
            // A ten-thousandth and a hair: the wire's grid, which the box is
            // written on, and the travel yaw's rounding.
            assert!(
                (lo + half[axis]).abs() < 5e-4 && (hi - half[axis]).abs() < 5e-4,
                "airship seed {s}, axis {axis}: draws {lo:.4}..{hi:.4} in a box of +-{:.4}",
                half[axis]
            );
        }
        checked += 1;
    }
    assert!(checked > 50, "only {checked} airship seeds under 400");
}

/// Her airscrews turn opposite ways in pairs - a ship's engines are handed,
/// so she does not yaw on her own torque: each one off her centreline has a
/// twin at its mirror image turning the other way, and one ON it (an ornate
/// Zeppelin's keel engine) has none to answer - and each one's whole disc
/// turns clear of her hull, at every blueprint extreme and on every tier.
#[test]
fn her_airscrews_counter_rotate_and_turn_clear_of_her_hull() {
    for Drawn {
        built, plan, what, ..
    } in every_drawn()
    {
        let mut turns: Vec<(Vec3, f32)> = Vec::new();
        for (hub, world) in spinning(&built) {
            let spin = hub.spin.as_ref().expect("a spinning hub");
            let rate = spin
                .terms
                .iter()
                .map(|t| match t {
                    SpinTerm::Constant { rate } => rate.0,
                    _ => 0.0,
                })
                .sum::<f32>();
            // Her airscrews turn about the travel axis, ahead or astern.
            let axis = world.transform_vector3(Vec3::from(spin.axis.0)).normalize();
            assert!(
                axis.z.abs() > 0.999,
                "{what}: an airscrew turns about {axis:?}, not along her"
            );
            turns.push((world.transform_point3(Vec3::ZERO), rate * axis.z));
            // Its disc: the furthest any blade reaches from the hub, in the
            // hub's plane, against the hull's skin at the hub's station.
            let centre = world.transform_point3(Vec3::ZERO);
            let reach = hub
                .children
                .iter()
                .flat_map(|c| {
                    let t = &c.transform;
                    let local = Affine3A::from_scale_rotation_translation(
                        Vec3::from(t.scale.0),
                        Quat::from_array(t.rotation.0).normalize(),
                        Vec3::from(t.translation.0),
                    );
                    crate::world_builder::primitive_outline(&c.kind)
                        .into_iter()
                        .map(move |p| local.transform_point3(p))
                })
                .map(|p| p.truncate().length())
                .fold(0.0f32, f32::max);
            let skin = plan.radius(plan.x(centre.z));
            let clearance = centre.truncate().length() - reach - skin;
            assert!(
                clearance > 0.01,
                "{what}: an airscrew at {centre:?} turns {clearance:.3} m off her hull"
            );
        }
        assert!(turns.len() >= 4, "{what}: {} airscrews", turns.len());
        for &(at, turn) in &turns {
            if at.x.abs() < 1e-3 {
                continue;
            }
            let mirror = Vec3::new(-at.x, at.y, at.z);
            let twin = turns
                .iter()
                .find(|(other, _)| other.distance(mirror) < 1e-3)
                .unwrap_or_else(|| panic!("{what}: the airscrew at {at:?} has no twin"));
            assert!(
                (twin.1 + turn).abs() < 1e-3 && turn != 0.0,
                "{what}: the airscrews at {at:?} and its mirror turn {turn} and {}",
                twin.1
            );
        }
    }
}

/// Her hull's sweep never loops past its own stations: a uniform
/// Catmull-Rom through stations bunched at one end overshoots and loops back
/// (#1617's square wing tips), so the drawn hull is read back - it stays
/// within her length and never swells a hundredth past her girth. (Between
/// two stations of the same radius the spline bulges a fifth of a per cent
/// toward its neighbours' mean - a loop is tens of per cent.)
#[test]
fn her_hull_never_loops_past_its_stations() {
    for Drawn {
        built, plan, what, ..
    } in every_drawn()
    {
        // The hull is the first part her builder draws.
        let hull = &built.children[0];
        let points = crate::world_builder::primitive_outline(&hull.kind);
        let half = plan.length() * 0.5;
        for p in &points {
            assert!(
                p.z.abs() <= half + 1e-3,
                "{what}: her hull reaches z {:.4} past her {half:.4} m half-length",
                p.z
            );
            assert!(
                p.truncate().length() <= plan.girth() * 1.01,
                "{what}: her hull swells to {:.4} past her girth {:.4}",
                p.truncate().length(),
                plan.girth()
            );
        }
    }
}

/// Every lit part she draws shows past the rest of her (#1619), on every
/// type at every blueprint extreme, on her fullest ladder: at least one
/// vertex in thirty of each sees out ([`sight::lit_shares`]). Read on a kit
/// that lights nothing but her lamps and windows.
#[test]
fn every_lit_part_she_draws_shows() {
    let seed = airship_seeds(600)
        .find(|&s| !PartCtx::for_seed(s).materials.emissive_accents())
        .expect("some airship seed wears an unlit kit");
    let mut ctx = PartCtx::for_seed(seed);
    ctx.ornateness = OrnatenessTier::Ornate;
    for t in AirshipType::ALL {
        for bp in blueprints() {
            let plan = craft(t).plan(&bp);
            let shares = sight::lit_shares(&craft(t).build(&ctx, &plan));
            let what = format!("a {} m {}, fineness {}", bp.length, t.label(), bp.fineness);
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

/// A seeded airship's record stays inside a THIRD of the soft budget - the
/// fleet's rule for a seeded craft (#1359), which leaves an owner two thirds
/// to build on - read off the saved records of real seeds and off the
/// heaviest drawn corner.
#[test]
fn a_seeded_airships_record_stays_well_inside_the_budget() {
    use crate::pds::avatar::AvatarRecord;
    use crate::pds::record_size::{SOFT_RECORD_BUDGET_BYTES, serialized_record_bytes};
    let mut worst = (0, String::new());
    for s in airship_seeds(300) {
        let saved = serialized_record_bytes(&AvatarRecord::default_for_seed(s))
            .expect("a seeded record serializes");
        if saved > worst.0 {
            worst = (saved, format!("seed {s}'s saved record"));
        }
    }
    for Drawn { built, what, .. } in every_drawn() {
        let bytes = serialized_record_bytes(&built).expect("an airship serializes");
        if bytes > worst.0 {
            worst = (bytes, what);
        }
    }
    assert!(
        worst.0 * 3 < SOFT_RECORD_BUDGET_BYTES,
        "the heaviest, {}, is {} bytes, past a third of the {SOFT_RECORD_BUDGET_BYTES}-byte soft \
         budget",
        worst.1,
        worst.0
    );
}

/// Ornateness adds parts and wear does not: the ladder is masses, and wear
/// is the finish kit's grime alone.
#[test]
fn ornateness_adds_parts_and_wear_does_not() {
    fn count(g: &Generator) -> usize {
        1 + g.children.iter().map(count).sum::<usize>()
    }
    let drawn = every_drawn();
    for chunk in drawn.chunks(9) {
        // Ornateness-major, wear-minor: [plain x3, adorned x3, ornate x3].
        let n: Vec<usize> = chunk.iter().map(|d| count(&d.built)).collect();
        for tier in 0..3 {
            let row = &n[tier * 3..tier * 3 + 3];
            assert!(
                row.iter().all(|&k| k == row[0]),
                "{}: wear changed the part count {row:?}",
                chunk[tier * 3].what
            );
        }
        assert!(
            n[0] < n[3] && n[3] < n[6],
            "{}: the ladder does not add parts ({} / {} / {})",
            chunk[0].what,
            n[0],
            n[3],
            n[6]
        );
        assert_eq!(chunk[0].ornateness, OrnatenessTier::Plain);
    }
}

/// She passes the gate landed (#1359 rule 6): her box stands under the air
/// draft and fits the narrowest gateway's mouth, at every blueprint extreme
/// and on every tier.
#[test]
fn she_stands_under_the_air_draft_and_inside_the_mouth() {
    use super::super::{AIR_DRAFT_CAP, GATEWAY_MOUTH};
    for Drawn { built, what, .. } in every_drawn() {
        let b = swept_bounds(&built);
        let (w, h) = (b.max.x - b.min.x, b.max.y - b.min.y);
        assert!(h < AIR_DRAFT_CAP, "{what}: {h:.3} m tall");
        assert!(w < GATEWAY_MOUTH, "{what}: {w:.3} m wide");
    }
}

/// The corners the guards sweep bound every seeded airship: a seed's hull is
/// never longer, shorter, fuller or slimmer than some corner's.
#[test]
fn the_corners_bound_every_seeded_airship() {
    let corners = AirshipBlueprint::corners();
    let lo = |f: fn(&AirshipBlueprint) -> f32| corners.iter().map(f).fold(f32::MAX, f32::min);
    let hi = |f: fn(&AirshipBlueprint) -> f32| corners.iter().map(f).fold(f32::MIN, f32::max);
    for s in airship_seeds(2000) {
        let bp = VehicleBlueprint::from_seed(s)
            .and_then(|b| b.airship().copied())
            .expect("an airship seed has an airship blueprint");
        assert!(
            (lo(|b| b.length)..=hi(|b| b.length)).contains(&bp.length),
            "seed {s}: length {} outside the corners",
            bp.length
        );
        assert!(
            (lo(|b| b.fineness)..=hi(|b| b.fineness)).contains(&bp.fineness),
            "seed {s}: fineness {} outside the corners",
            bp.fineness
        );
    }
}

/// Every type flies as its own feel says, and no two built types carry the
/// same feel tuple (so a new type that copies the hero's is a decision, not
/// an accident).
#[test]
fn every_airship_type_has_a_feel_of_her_own() {
    let feels = every_feel();
    for (i, (an, a)) in feels.iter().enumerate() {
        assert!(
            a.mass_factor > 0.0 && a.drive_accel > 0.0 && a.linear_damping > 0.0,
            "the {an} cannot fly: {a:?}"
        );
        for (bn, b) in feels.iter().skip(i + 1) {
            assert_ne!(a, b, "the {an} and the {bn} carry the same feel tuple");
        }
    }
}
