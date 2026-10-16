//! The Zeppelin's own guards (#1657): her box is set by the parts that
//! should set it, her nav lights are on the aviation sides, and her identity
//! is on her cheat line. The family's sweep (`super::super::tests`) holds her
//! to everything every airship owes.

use bevy::math::Vec3;

use super::*;
use crate::pds::generator::GeneratorKind;
use crate::seeded_defaults::{ChassisFamily, OrnatenessTier};

fn ctx_at(ornateness: OrnatenessTier) -> PartCtx {
    let seed = (0u64..600)
        .find(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Airship)
        .expect("some seed is an airship");
    let mut ctx = PartCtx::for_seed(seed);
    ctx.ornateness = ornateness;
    ctx
}

fn blueprints() -> Vec<AirshipBlueprint> {
    let mut out = AirshipBlueprint::corners();
    out.push(AirshipBlueprint::NOMINAL);
    out
}

/// How far an airscrew's blades reach from its hub's axis (`+Z`, the hub's
/// own frame): the furthest corner of any blade block it carries, read off
/// each block's size and transform alone.
fn airscrew_reach(hub: &Generator) -> f32 {
    hub.children
        .iter()
        .filter_map(|blade| match &blade.kind {
            GeneratorKind::Cuboid { size, .. } => Some((blade, Vec3::from(size.0) * 0.5)),
            _ => None,
        })
        .flat_map(|(blade, half)| {
            let at = Vec3::from(blade.transform.translation.0);
            let turn = bevy::math::Quat::from_array(blade.transform.rotation.0).normalize();
            (0..8).map(move |i| {
                let pick = |bit: u32, v: f32| if i & bit == 0 { -v } else { v };
                at + turn * Vec3::new(pick(1, half.x), pick(2, half.y), pick(4, half.z))
            })
        })
        .map(|corner| corner.truncate().length())
        .fold(0.0, f32::max)
}

/// The swept bounds of one part of her, alone, through its own transform.
fn part_bounds(part: &Generator) -> crate::catalogue::items::measure::Bounds {
    super::super::swept_bounds(part)
}

/// Her box is set by the parts that should set it, at every blueprint
/// extreme and on every tier - because her box is MEASURED off her, the
/// family's box guard reads her tree back against itself and cannot see a
/// part grow past her ends or her sides (#1628's lesson). Her nose is her
/// mooring cone's tip; her tail her tail light; her crown her dorsal fin's
/// tip; her keel her car's landing bumper, or an ornate ship's keel engine's
/// airscrew; her sides the forward engine cars' airscrews.
#[test]
fn her_box_is_set_by_the_parts_that_should_set_it() {
    for ornateness in OrnatenessTier::ALL {
        let ctx = ctx_at(ornateness);
        let c = airship_colours(&ctx);
        for bp in blueprints() {
            let plan = Zeppelin.plan(&bp);
            let built = Zeppelin.build(&ctx, &plan);
            let what = format!(
                "a {} m Zeppelin at fineness {}, {ornateness:?}",
                bp.length, bp.fineness
            );
            let b = super::super::swept_bounds(&built);
            let kids = &built.children;
            let find = |name: &str, pick: &dyn Fn(&Generator) -> bool| -> Vec<&Generator> {
                let found: Vec<&Generator> = kids.iter().filter(|g| pick(g)).collect();
                assert!(!found.is_empty(), "{what}: no {name}");
                found
            };
            let material_is =
                |g: &Generator, m: &crate::pds::texture::SovereignMaterialSettings| {
                    g.kind.common().is_some_and(|k| &k.material == m)
                };
            let close = |got: f32, want: f32, tol: f32, face: &str| {
                assert!(
                    (got - want).abs() <= tol,
                    "{what}: her box's {face} is {got:.4}, not her setter's {want:.4}"
                );
            };
            // Nose: the mooring cone, the metal lathe forward.
            let cone = find("mooring cone", &|g| {
                matches!(g.kind, GeneratorKind::Lathe { .. })
                    && material_is(g, &c.metal)
                    && g.transform.translation.0[2] > 0.0
            });
            close(b.max.z, part_bounds(cone[0]).max.z, 1e-4, "nose");
            close(
                b.max.z,
                hull::mooring_tip(&plan),
                1e-4,
                "nose (as the hull places it)",
            );
            // Tail: the white light.
            let lamp = find("tail light", &|g| material_is(g, &c.white));
            close(b.min.z, part_bounds(lamp[0]).min.z, 1e-4, "tail");
            // Crown: the dorsal fin, the fin sweep standing along +Y.
            let dorsal = find("dorsal fin", &|g| {
                material_is(g, &c.fins)
                    && match &g.kind {
                        GeneratorKind::Spine { points, .. } => {
                            points.last().is_some_and(|p| p.position.0[1] > 0.0)
                        }
                        _ => false,
                    }
            });
            close(b.max.y, part_bounds(dorsal[0]).max.y, 1e-4, "crown");
            // Keel: the bumper, or an ornate ship's keel airscrew - whose
            // disc's lowest point is read off her blades' own blocks, not
            // off the sweep the box is measured with, so a sweep that
            // stopped sweeping would shrink the box past it and fail here.
            if ornateness == OrnatenessTier::Ornate {
                let keel = find("keel airscrew", &|g| {
                    g.spin.is_some() && g.transform.translation.0[0].abs() < 1e-4
                });
                let hub = Vec3::from(keel[0].transform.translation.0);
                close(b.min.y, hub.y - airscrew_reach(keel[0]), 1e-4, "keel");
            } else {
                let bumper = find("bumper", &|g| {
                    matches!(g.kind, GeneratorKind::Sphere { .. }) && material_is(g, &c.dark)
                });
                close(b.min.y, part_bounds(bumper[0]).min.y, 1e-4, "keel");
            }
            // Sides: the forward pair's airscrews, their reach read the same
            // way.
            let forward = plan.z(engines::ENGINE_X[0]);
            let side = find("forward airscrew", &|g| {
                g.spin.is_some()
                    && g.transform.translation.0[0] > 0.0
                    && g.transform.translation.0[2] > forward - plan.len(0.4)
            });
            let hub = Vec3::from(side[0].transform.translation.0);
            close(b.max.x, hub.x + airscrew_reach(side[0]), 1e-4, "side");
            close(b.min.x, -b.max.x, 1e-4, "other side");
        }
    }
}

/// Her nav lights are on the aviation sides: red to port (her left, `+X`,
/// facing ahead), green to starboard - each on its own horizontal fin's tip.
#[test]
fn her_nav_lights_are_on_the_aviation_sides() {
    let ctx = ctx_at(OrnatenessTier::Plain);
    let c = airship_colours(&ctx);
    for bp in blueprints() {
        let plan = Zeppelin.plan(&bp);
        let built = Zeppelin.build(&ctx, &plan);
        let lit = |m: &crate::pds::texture::SovereignMaterialSettings| -> Vec<Vec3> {
            built
                .children
                .iter()
                .filter(|g| g.kind.common().is_some_and(|k| &k.material == m))
                .map(|g| Vec3::from(g.transform.translation.0))
                .collect()
        };
        let (port, starboard) = (lit(&c.port), lit(&c.starboard));
        assert_eq!((port.len(), starboard.len()), (1, 1), "one light a side");
        let reach = tail::FIN_REACH * plan.girth();
        assert!(port[0].x > reach * 0.9, "the red light is at {:?}", port[0]);
        assert!(
            starboard[0].x < -reach * 0.9,
            "the green light is at {:?}",
            starboard[0]
        );
        let mirrored = Vec3::new(-port[0].x, port[0].y, port[0].z);
        assert!(
            port[0].y.abs() < 1e-4 && (mirrored - starboard[0]).length() < 1e-4,
            "the two lights do not mirror each other: {port:?} / {starboard:?}"
        );
    }
}

/// Her identity is on her cheat line (#1365): a band laid on her hull down
/// EACH flank in the seeded accent - the one thing on her the owner's own
/// colour paints. A sweep drawn nose to tail mirrors its angular frame, so
/// the two flanks' wedges straddle `0.5` (`+X`) and `0` (`-X`).
#[test]
fn her_cheat_line_carries_her_identity_down_each_flank() {
    let ctx = ctx_at(OrnatenessTier::Plain);
    let c = airship_colours(&ctx);
    let plan = Zeppelin.plan(&AirshipBlueprint::NOMINAL);
    let built = Zeppelin.build(&ctx, &plan);
    let cuts: Vec<[f32; 2]> = built
        .children
        .iter()
        .filter(|g| g.kind.common().is_some_and(|k| k.material == c.stripe))
        .map(|g| g.kind.torture().expect("a primitive").path_cut.0)
        .collect();
    assert_eq!(cuts.len(), 2, "the cheat line is {} bands", cuts.len());
    let plus_x = cuts.iter().filter(|c| c[0] > 0.25 && c[1] < 0.5).count();
    let minus_x = cuts.iter().filter(|c| c[0] > 0.0 && c[1] < 0.25).count();
    assert_eq!(
        (plus_x, minus_x),
        (1, 1),
        "the bands are not one a flank, just over her widest line: {cuts:?}"
    );
}
