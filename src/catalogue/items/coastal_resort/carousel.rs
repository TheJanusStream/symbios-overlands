//! Carousel - a Coastal-Resort boardwalk ride (#1941), one of the three
//! showpieces of a part's slide (#1940): eight painted horses ride their
//! brass poles up and down while the whole top turns.
//!
//! A static round base stands on its footing - a concrete step and a
//! planked deck - and carries the ride's only solid parts. Everything above
//! it turns about +Y on the rotor, the turning platform, at a real
//! carousel's pace ([`ROTOR_DEG_S`]): the central column, the canopy with
//! its striped roof and its valance of warm bulbs, the eight poles and the
//! eight horses. Each horse is its own slide: its body, the root of the
//! horse, swings [`RISE`] metres either side of its rest along +Y once
//! every [`RISE_PERIOD`] seconds, and the horse at bearing `45k` degrees
//! starts `45k` degrees into its swing, so they rise one after another
//! round the ring. The body is the hub a horse hangs from - a hidden hub
//! under it would only have cost a drawn part per horse. The pole passes
//! through the saddle, as on a real carousel, and stays where it is while
//! the horse rides it.
//!
//! Every client turns and slides the parts for itself (`pds::spin`,
//! `pds::slide`), so nothing that turns has a collider. A still floor hidden
//! inside the turning platform ([`FLOOR_TOP`]) carries a visitor who steps
//! on at the height of the platform they see. Four of the twelve bulbs are
//! lights, short ones, and turn with the canopy.
//!
//! Built in the ride's ground frame: the base by [`assemble`], the rotor
//! and each horse by [`nest`], the horses authored in their own frame (nose
//! along +X) and then set on their poles, facing the way the ride carries
//! them.

use std::f32::consts::PI;

use crate::catalogue::items::fx::light::{Lamp, lit};
use crate::catalogue::items::util::{
    assemble, blob_capsule, blob_ellipsoid, blob_group, cone, cylinder_tapered, footing_disc, glow,
    id_quat, nest, prim, quat_y, quat_z, solid, sphere, steady, superellipsoid, torus, tube,
    turning,
};
use crate::catalogue::{CatalogueEntry, Footprint, StructureRole};
use crate::pds::{Fp, Generator, SlideTerm};
use crate::seeded_defaults::ThemeArchetype;

use super::{
    AWNING_RED, AWNING_WHITE, LAMP_WARM, PILING_GREY, SIGN_GOLD, canvas, concrete, enamel, plank,
};

/// The step's radius and top, the deck's radius and top (m).
const STEP_R: f32 = 4.95;
const STEP_TOP: f32 = 0.22;
const DECK_R: f32 = 4.6;
const DECK_TOP: f32 = 0.45;
/// The turning platform's radius, its underside (sunk into the deck) and
/// its top.
const PLATFORM_R: f32 = 4.4;
const PLATFORM_BOTTOM: f32 = 0.43;
pub(super) const PLATFORM_TOP: f32 = 0.58;
/// A still, solid floor hidden inside the turning platform, a little
/// narrower and its top 5 mm under the platform's. The platform turns, so it
/// has no collider (#1940): without this floor a visitor stepping on would
/// sink through the floor they see to the deck's top, 13 cm lower.
const FLOOR_R: f32 = PLATFORM_R - 0.05;
const FLOOR_TOP: f32 = PLATFORM_TOP - 0.005;
/// The canopy's ceiling: the underside the poles meet.
pub(super) const CEILING_Y: f32 = 3.86;
/// The valance round the canopy's rim: outer radius and middle height.
const VALANCE_R: f32 = 4.72;
const VALANCE_Y: f32 = 3.75;
/// The roof's eave radius, eave height and rise.
const ROOF_R: f32 = 4.85;
const ROOF_EAVE: f32 = 3.98;
const ROOF_RISE: f32 = 1.7;
/// The radius the poles and horses stand at.
const RING_R: f32 = 3.5;
/// How many horses ride the ring.
pub(super) const HORSES: usize = 8;
/// A horse's body centre at rest, over the ground (m).
pub(super) const HORSE_REST_Y: f32 = 1.65;
/// How far a horse rides either side of its rest (m), and how long one
/// rise and fall takes (s).
pub(super) const RISE: f32 = 0.3;
pub(super) const RISE_PERIOD: f32 = 3.0;
/// How fast the top turns (degrees a second): a turn every fifteen seconds,
/// four turns a minute, as a boardwalk carousel runs.
pub(super) const ROTOR_DEG_S: f32 = 24.0;
/// How many bulbs ring the valance, and every how many of them is a light.
const BULBS: usize = 12;
const LIGHT_EVERY: usize = 3;
/// A bulb's glow - a lamp's strength, so a lit one reads lit.
const BULB_GLOW: f32 = 2.4;
/// Spacing circle round the ride: the step's rim with room to walk round it.
pub(super) const CLEARANCE: f32 = 5.2;

/// The horses' coats, round the ring: cream, black, dapple grey, palomino,
/// white, chestnut, sky blue and rose - a fairground's paint box.
const COATS: [[f32; 3]; HORSES] = [
    [0.93, 0.88, 0.76],
    [0.10, 0.10, 0.11],
    [0.66, 0.67, 0.70],
    [0.86, 0.66, 0.30],
    [0.96, 0.96, 0.94],
    [0.55, 0.26, 0.14],
    [0.50, 0.70, 0.90],
    [0.92, 0.58, 0.66],
];
/// Each horse's saddle and plume, in fairground colours.
const SADDLES: [[f32; 3]; HORSES] = [
    [0.78, 0.14, 0.16],
    [0.16, 0.34, 0.78],
    [0.12, 0.56, 0.30],
    [0.52, 0.18, 0.62],
    [0.92, 0.46, 0.10],
    [0.10, 0.52, 0.60],
    [0.80, 0.16, 0.36],
    [0.20, 0.24, 0.62],
];
/// The column's cream paint, the turning floor's varnished boards and the
/// deck's red.
const COLUMN_CREAM: [f32; 3] = [0.95, 0.90, 0.78];
const PLATFORM_WOOD: [f32; 3] = [0.64, 0.46, 0.28];
const PLATFORM_RED: [f32; 3] = [0.62, 0.14, 0.12];

pub struct Carousel;

impl CatalogueEntry for Carousel {
    fn slug(&self) -> &'static str {
        "carousel"
    }
    fn name(&self) -> &'static str {
        "Carousel"
    }
    fn description(&self) -> &'static str {
        "Boardwalk carousel: eight painted horses rise and fall on brass poles \
         under a striped canopy that turns, ringed with warm bulbs."
    }
    fn role(&self) -> StructureRole {
        StructureRole::Secondary
    }
    fn themes(&self) -> &'static [ThemeArchetype] {
        &[ThemeArchetype::CoastalResort]
    }
    fn prosperity_band(&self) -> crate::seeded_defaults::ProsperityBand {
        super::RESORT_BAND
    }
    /// Placed from the Catalogue alone (#1940): a showpiece of motion, it
    /// changes no seeded world.
    fn seeds_worlds(&self) -> bool {
        false
    }
    fn footprint(&self) -> Footprint {
        Footprint {
            clearance: CLEARANCE,
            min_spawn_dist: 20.0,
        }
    }

    fn build(&self, _local_did: &str) -> Generator {
        build_tree()
    }
}

/// A small rounded piece of a horse's tack - a saddle, a mane, a plume - at
/// half the mesh a [`superellipsoid`] carries by default: twenty-four of
/// them turn on the rotor, and at a hand's breadth across the finer mesh
/// shows nothing but triangles.
fn tack_piece(
    half_extents: [f32; 3],
    exponent_ns: f32,
    exponent_ew: f32,
    material: crate::pds::SovereignMaterialSettings,
) -> crate::pds::GeneratorKind {
    let mut kind = superellipsoid(half_extents, exponent_ns, exponent_ew, material);
    if let crate::pds::GeneratorKind::Superellipsoid {
        latitudes,
        longitudes,
        ..
    } = &mut kind
    {
        *latitudes = 6;
        *longitudes = 10;
    }
    kind
}

/// One horse in its own frame, its body's centre at the origin and its
/// nose along +X: a body, neck, head, four galloping legs and a tail
/// melted into one painted skin, a saddle over the pole, a gilt mane and a
/// plume. Its body is its root, which carries its slide.
fn horse(coat: [f32; 3], tack: [f32; 3], phase_deg: f32) -> Generator {
    let gilt = enamel(SIGN_GOLD);
    let tilted = |mut e: crate::pds::generator::BlobElement, angle: f32| {
        e.rotation = quat_z(angle);
        e
    };
    let skin = vec![
        // Barrel and chest, a deeper chest forward.
        blob_ellipsoid([-0.04, 0.0, 0.0], [0.5, 0.19, 0.16], 0.08),
        blob_ellipsoid([0.28, 0.02, 0.0], [0.22, 0.2, 0.16], 0.08),
        // Neck, raised and reaching forward.
        blob_capsule([0.47, 0.27, 0.0], 0.1, 0.13, quat_z(-0.62), 0.08),
        // Head, its muzzle dipped.
        tilted(
            blob_ellipsoid([0.66, 0.46, 0.0], [0.19, 0.085, 0.08], 0.06),
            -0.7,
        ),
        // Forelegs reaching forward, one higher than the other.
        blob_capsule([0.46, -0.3, 0.08], 0.06, 0.22, quat_z(0.45), 0.05),
        blob_capsule([0.5, -0.24, -0.08], 0.06, 0.2, quat_z(0.9), 0.05),
        // Hind legs thrown back.
        blob_capsule([-0.48, -0.3, 0.08], 0.06, 0.22, quat_z(-0.5), 0.05),
        blob_capsule([-0.44, -0.31, -0.08], 0.06, 0.22, quat_z(-0.32), 0.05),
        // Tail, streaming.
        blob_capsule([-0.6, -0.05, 0.0], 0.06, 0.16, quat_z(-0.9), 0.05),
    ];
    let mut body = prim(blob_group(skin, 26, enamel(coat)), [0.0; 3], id_quat());
    body.slide = Some(crate::pds::Slide::along(
        [0.0, 1.0, 0.0],
        SlideTerm::Swing {
            amplitude: Fp(RISE),
            period: Fp(RISE_PERIOD),
            phase: Fp(phase_deg),
        },
    ));
    nest(
        body,
        vec![
            // The saddle sits over the pole, on the back.
            prim(
                tack_piece([0.17, 0.045, 0.18], 0.45, 0.7, enamel(tack)),
                [0.02, 0.19, 0.0],
                id_quat(),
            ),
            // A gilt mane along the back of the neck.
            prim(
                tack_piece([0.045, 0.19, 0.035], 0.6, 0.8, gilt),
                [0.41, 0.33, 0.0],
                quat_z(-0.62),
            ),
            // A plume between the ears, in the saddle's colour.
            prim(
                tack_piece([0.035, 0.09, 0.03], 0.8, 0.8, enamel(tack)),
                [0.6, 0.58, 0.0],
                quat_z(0.1),
            ),
        ],
    )
}

/// The bearing of horse `k` round the ring, and how far into its swing it
/// starts (both radians): the same, so they rise one after another.
pub(super) fn bearing(k: usize) -> f32 {
    k as f32 * 2.0 * PI / HORSES as f32
}

fn build_tree() -> Generator {
    let gilt = enamel(SIGN_GOLD);
    let brass = enamel([0.86, 0.72, 0.38]);

    // ---- what turns: the platform carries everything above the deck.
    let platform = prim(
        cylinder_tapered(
            PLATFORM_R,
            PLATFORM_TOP - PLATFORM_BOTTOM,
            32,
            0.0,
            plank(PLATFORM_WOOD),
        ),
        [0.0, (PLATFORM_TOP + PLATFORM_BOTTOM) * 0.5, 0.0],
        id_quat(),
    );
    let column_h = CEILING_Y + 0.06 - (PLATFORM_TOP - 0.01);
    let mut turning_parts = vec![
        // The central column, with a gilt band low and high.
        prim(
            cylinder_tapered(0.62, column_h, 16, 0.0, enamel(COLUMN_CREAM)),
            [0.0, PLATFORM_TOP - 0.01 + column_h * 0.5, 0.0],
            id_quat(),
        ),
        prim(torus(0.06, 0.62, gilt.clone()), [0.0, 1.0, 0.0], id_quat()),
        prim(torus(0.06, 0.62, gilt.clone()), [0.0, 3.5, 0.0], id_quat()),
        // The canopy's ceiling, reaching into the valance's wall.
        prim(
            cylinder_tapered(4.65, 0.08, 32, 0.0, enamel(AWNING_WHITE)),
            [0.0, CEILING_Y + 0.04, 0.0],
            id_quat(),
        ),
        // The valance, its bulbs set round it. Its bore is as wide as a
        // tube's may be: the sanitiser keeps a wall a twentieth of the
        // radius thick at the least.
        prim(
            tube(VALANCE_R, 4.48, 0.5, 32, enamel(AWNING_WHITE)),
            [0.0, VALANCE_Y, 0.0],
            id_quat(),
        ),
        // The striped roof, and its gilt finial.
        prim(
            cone(ROOF_R, ROOF_RISE, 32, canvas(AWNING_RED, AWNING_WHITE)),
            [0.0, ROOF_EAVE + ROOF_RISE * 0.5, 0.0],
            id_quat(),
        ),
        prim(
            sphere(0.18, 3, gilt.clone()),
            [0.0, ROOF_EAVE + ROOF_RISE, 0.0],
            id_quat(),
        ),
        prim(
            cone(0.07, 0.5, 8, gilt.clone()),
            [0.0, ROOF_EAVE + ROOF_RISE + 0.38, 0.0],
            id_quat(),
        ),
    ];
    // The poles, sunk a centimetre into the platform and two into the
    // ceiling, and a horse on each.
    let pole_bottom = PLATFORM_TOP - 0.01;
    let pole_top = CEILING_Y + 0.02;
    for k in 0..HORSES {
        let a = bearing(k);
        let (x, z) = (RING_R * a.sin(), RING_R * a.cos());
        turning_parts.push(prim(
            cylinder_tapered(0.035, pole_top - pole_bottom, 8, 0.0, brass.clone()),
            [x, (pole_top + pole_bottom) * 0.5, z],
            id_quat(),
        ));
        // Nose along +X turned by `a` about +Y points along (cos a, 0,
        // -sin a): the way a turn about +Y carries a horse standing at
        // (sin a, 0, cos a) round the ring.
        let mut rider = horse(COATS[k], SADDLES[k], a.to_degrees().round());
        rider.transform.translation = crate::pds::Fp3([x, HORSE_REST_Y, z]);
        rider.transform.rotation = quat_y(a);
        turning_parts.push(rider);
    }
    // The bulbs round the valance, every third a short light: enough to
    // warm the horses passing under it, and few enough for the browser.
    for k in 0..BULBS {
        let a = (k as f32 + 0.5) * 2.0 * PI / BULBS as f32;
        let r = VALANCE_R + 0.04;
        let bulb = prim(
            sphere(0.07, 3, glow(LAMP_WARM, BULB_GLOW)),
            [r * a.sin(), VALANCE_Y, r * a.cos()],
            id_quat(),
        );
        turning_parts.push(if k.is_multiple_of(LIGHT_EVERY) {
            lit(bulb, Lamp::Bulb { over: 2.0 })
        } else {
            bulb
        });
    }
    let rotor = turning(
        platform,
        [0.0, 1.0, 0.0],
        steady(ROTOR_DEG_S),
        turning_parts,
    );

    // ---- what stands: the footing, the step and the deck the rotor turns
    // on, the only parts with colliders.
    let deck = nest(
        prim(
            solid(cylinder_tapered(
                DECK_R,
                DECK_TOP - (STEP_TOP - 0.02),
                32,
                0.0,
                plank(PLATFORM_RED),
            )),
            [0.0, (DECK_TOP + STEP_TOP - 0.02) * 0.5, 0.0],
            id_quat(),
        ),
        vec![
            // A brass rail round the deck's edge.
            prim(
                torus(0.03, DECK_R - 0.05, brass.clone()),
                [0.0, DECK_TOP + 0.01, 0.0],
                id_quat(),
            ),
            // The floor visitors stand on, still and hidden inside the
            // platform that turns over it; its foot a centimetre into the
            // deck, so no face of it lies on the deck's.
            prim(
                solid(cylinder_tapered(
                    FLOOR_R,
                    FLOOR_TOP - (DECK_TOP - 0.01),
                    32,
                    0.0,
                    plank(PLATFORM_RED),
                )),
                [0.0, (FLOOR_TOP + DECK_TOP - 0.01) * 0.5, 0.0],
                id_quat(),
            ),
            rotor,
        ],
    );
    let step = nest(
        prim(
            solid(cylinder_tapered(
                STEP_R,
                STEP_TOP,
                32,
                0.0,
                concrete(PILING_GREY),
            )),
            [0.0, STEP_TOP * 0.5, 0.0],
            id_quat(),
        ),
        vec![deck],
    );
    assemble(vec![footing_disc(STEP_R, CLEARANCE), step])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::measure::Bounds;
    use crate::catalogue::items::overhaul::{assert_no_z_fighting, assert_nothing_floats};
    use crate::catalogue::items::util::{assert_sanitize_stable, motion_reach, placed, placed_at};
    use crate::pds::{SpinTerm, sanitize_generator};

    fn built() -> Generator {
        Carousel.build("")
    }

    /// The tree as a peer draws it: sanitised and through the wire.
    fn on_the_wire() -> Generator {
        let mut tree = built();
        sanitize_generator(&mut tree);
        serde_json::from_str(&serde_json::to_string(&tree).expect("writes")).expect("reads")
    }

    /// Each horse: its node path, its slide's phase (degrees) and its
    /// bearing round the rotor (degrees, from its place on the ring).
    fn horses(tree: &Generator) -> Vec<(Vec<usize>, f32, f32)> {
        placed(tree)
            .into_iter()
            .filter_map(|p| {
                let slide = p.node.slide.as_ref()?;
                let SlideTerm::Swing { phase, .. } = slide.terms[0] else {
                    panic!("a horse rides a swing");
                };
                let at = p.world.translation;
                let bearing = at.x.atan2(at.z).to_degrees().rem_euclid(360.0);
                Some((p.path, phase.0, bearing))
            })
            .collect()
    }

    /// The drawn bounds of everything under `path` at `now`.
    fn bounds_under(tree: &Generator, path: &[usize], now: f64, calm: f32) -> Bounds {
        placed_at(tree, now, calm)
            .into_iter()
            .filter(|p| p.path.starts_with(path))
            .filter_map(|p| p.drawn)
            .reduce(|a, b| Bounds {
                min: a.min.min(b.min),
                max: a.max.max(b.max),
            })
            .expect("a horse draws")
    }

    #[test]
    fn build_round_trips_through_sanitize() {
        assert_sanitize_stable(&built(), "carousel");
    }

    /// The floor a visitor stands on is the one they see: the turning
    /// platform has no collider, so a still, solid floor inside it carries
    /// them a few millimetres under its top - not 13 cm lower on the deck.
    #[test]
    fn the_floor_you_stand_on_is_the_floor_you_see() {
        let tree = built();
        let floor = placed(&tree)
            .into_iter()
            .filter(|p| p.node.kind.common().is_some_and(|c| c.solid))
            .filter_map(|p| p.drawn)
            .filter(|b| b.max.x >= PLATFORM_R - 0.1)
            .map(|b| b.max.y)
            .fold(f32::MIN, f32::max);
        assert!(
            (PLATFORM_TOP - 0.01..=PLATFORM_TOP).contains(&floor),
            "the solid floor's top is {floor} m, the platform drawn over it {PLATFORM_TOP} m"
        );
    }

    #[test]
    fn nothing_floats_and_nothing_z_fights() {
        let tree = built();
        assert_nothing_floats(&tree, "carousel");
        assert_no_z_fighting(&tree, "carousel");
    }

    /// The top turns about +Y at a carousel's pace, and is the only part
    /// that turns; the eight horses are the only parts that slide.
    #[test]
    fn the_top_turns_and_the_horses_ride() {
        let tree = on_the_wire();
        let turning: Vec<_> = placed(&tree)
            .into_iter()
            .filter_map(|p| p.node.spin.clone())
            .collect();
        assert_eq!(turning.len(), 1, "one rotor");
        assert_eq!(
            turning[0].terms,
            vec![SpinTerm::Constant {
                rate: Fp(ROTOR_DEG_S)
            }]
        );
        assert_eq!(turning[0].axis.0, [0.0, 1.0, 0.0]);
        assert_eq!(horses(&tree).len(), HORSES);
        assert!(placed(&tree).iter().all(|p| p.node.swell.is_none()));
    }

    /// The eight horses' swings start 45 degrees apart round the ring, each
    /// as far into its swing as it stands round the rotor, so they rise
    /// one after another - and drawn, each stands at its own height in
    /// that order.
    #[test]
    fn the_horses_rise_one_after_another_round_the_ring() {
        let tree = on_the_wire();
        let mut found = horses(&tree);
        found.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (k, (_, phase, bearing)) in found.iter().enumerate() {
            let want = k as f32 * 360.0 / HORSES as f32;
            assert_eq!(*phase, want, "horse {k}'s phase");
            assert!(
                (bearing - want).abs() < 1e-3,
                "horse {k} stands at {bearing} degrees, rides at phase {phase}"
            );
        }
        // Drawn: each horse's rise is the swing at its phase, its bearing
        // round the rotor whatever the rotor's turn.
        for now in [0.4, 7.25, 1.8e9 + 2.0] {
            let drawn = placed_at(&tree, now, 1.0);
            for (path, phase, _) in &found {
                let horse = drawn.iter().find(|p| &p.path == path).expect("drawn");
                let rise = horse.world.translation.y - HORSE_REST_Y;
                let cycle = (now / f64::from(RISE_PERIOD)).rem_euclid(1.0);
                let want = RISE as f64
                    * (std::f64::consts::TAU * cycle + f64::from(phase.to_radians())).sin();
                assert!(
                    (f64::from(rise) - want).abs() < 1e-4,
                    "at {now} s the horse at phase {phase} rises {rise}, not {want}"
                );
            }
        }
    }

    /// Over its whole ride a horse stays clear of the platform at the
    /// bottom of its swing and of the canopy at the top - at both extremes,
    /// for every horse, read off the drawn skin.
    #[test]
    fn every_horse_clears_the_platform_and_the_canopy_at_both_extremes() {
        let tree = on_the_wire();
        let mut seen = 0;
        for (path, phase, _) in horses(&tree) {
            // The swing is at its top a quarter period in, less its phase,
            // and at its bottom half a period later.
            let quarter = f64::from(RISE_PERIOD) * (0.25 - f64::from(phase) / 360.0);
            for (now, top) in [
                (quarter, true),
                (quarter + f64::from(RISE_PERIOD) * 0.5, false),
            ] {
                let b = bounds_under(&tree, &path, now, 1.0);
                let centre = placed_at(&tree, now, 1.0)
                    .into_iter()
                    .find(|p| p.path == path)
                    .expect("drawn")
                    .world
                    .translation
                    .y;
                let want = HORSE_REST_Y + if top { RISE } else { -RISE };
                assert!(
                    (centre - want).abs() < 1e-4,
                    "{centre} at {now}, not {want}"
                );
                if top {
                    assert!(
                        b.max.y < CEILING_Y - 0.9,
                        "at its top the horse at phase {phase} reaches {}",
                        b.max.y
                    );
                } else {
                    assert!(
                        b.min.y > PLATFORM_TOP + 0.15,
                        "at its bottom the horse at phase {phase} reaches down to {}",
                        b.min.y
                    );
                }
                seen += 1;
            }
        }
        assert_eq!(seen, 2 * HORSES);
    }

    /// Everything the ride draws, over a whole turn of the rotor and every
    /// horse's swing, stays inside its footprint.
    #[test]
    fn the_footprint_covers_the_ride_as_it_turns() {
        let times: Vec<f64> = (0..30).map(|k| f64::from(k) * 0.5).collect();
        let reach = motion_reach(&on_the_wire(), &times);
        assert!(reach <= CLEARANCE, "the ride reaches {reach} m");
        assert!(reach > STEP_R - 0.01, "the reach reads the step: {reach}");
    }
}
