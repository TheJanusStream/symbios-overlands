//! The seeded vehicles' lights (#1924): the lamps a craft already draws, lit
//! as a real one carries them, through the catalogue's own helper
//! (`catalogue::items::fx::light`, [`lit`] and its [`Lamp`] mounts) so a
//! vehicle's lamp lights as a lamp post's does.
//!
//! What each family carries:
//!
//! * **Road vehicles** (the skiffs): headlights, one forward spot for the
//!   pair onto the road [`HEADLIGHT_AHEAD_M`] ahead ([`headlights`]), and
//!   red tail lights, one running light for the pair ([`running`]), each
//!   from one lamp of its pair. A carriage's lanterns are one flame, and
//!   the rover - which carries no headlamp - lights the road from her
//!   camera head's lit pane. A dune buggy's roof bar stays a glow: a second
//!   beam would double her cost in the list (below).
//! * **Aircraft** (airplanes, helicopters, airships): navigation lights -
//!   red to port, green to starboard, white astern ([`running`]) - and the
//!   anti-collision beacons and strobes as pulses ([`strobe`]), with a
//!   landing light where one is drawn ([`landing_light`]); an airship's
//!   gondola lamps and lanterns, and a balloon's burner as a fire.
//! * **Boats**: a cabin's light where its windows glow ([`cabin`]), a white
//!   light astern and at the masthead where they are drawn, the lanterns of
//!   the craft that hang them as flames, and a skimmer's thrusters as her
//!   glow astern.
//!
//! # What a craft costs
//!
//! Each light takes its own places in WebGL2's cluster list
//! (`world_builder::light`), and the player's own vehicle is always the
//! nearest thing to the camera, so its lights always take budget places. So
//! lights share where a pair of lamps sits close together - a car's two
//! headlamps throw one beam, its two tail lamps one glow - and a beam that
//! lights nothing for most of a craft's life is short: an aircraft's landing
//! light reaches only the ground just ahead of her parked
//! ([`LANDING_AHEAD_M`]). Seen by the chase camera a road vehicle's beam,
//! reaching past 7 m, fills the screen and takes about 3,400 places, a
//! running light about 300; every seeded craft stays under a third of the
//! line the budget admits lights by (the tests' bound), so three parked
//! side by side by the camera draw whole.
//!
//! Every light but the anti-collision beacon shows from dusk to dawn; the
//! beacon is on at every hour, as an aircraft's is whenever her engine runs.
//! Each takes its colour from its lamp's glow. A craft's drawn frame faces
//! `+Z`, with port at `+X` (her left, facing ahead).
//!
//! Lights are authored on parts hung on the craft's root, in its drawn frame,
//! so every aim and point here is read in that frame. A part drawn in no
//! paint - a craft drawn in the default material for its shape alone, as
//! some guards measure her - does not glow and carries no light: every
//! helper here hands such a part back as it came ([`glows`]).
//!
//! # What a car's record holds
//!
//! A road vehicle's record has few bytes to spare (the skiffs' own budget
//! guard), so each of its lights is written as short as it can be: from
//! the lamp's own place (a light standing off its part writes an offset)
//! and, for a beam, on the record's default cone, which is left off the
//! wire. A beam a lamp's width off the middle draws the same pool as one
//! from between the pair. The rover alone, with bytes to spare, lights her
//! beam from the front of her camera head rather than from its pane.

use crate::catalogue::items::fx::light::{Lamp, lit, lit_at};
use crate::pds::Generator;

/// How far ahead of its headlamps a road vehicle's beam meets the road (m):
/// a pool a couple of car lengths ahead. The beam reaches twice its length
/// to there, as any raked spot's, so about 10 m from a lamp under a metre
/// up, at the brightest the power cap allows for that reach.
pub(crate) const HEADLIGHT_AHEAD_M: f32 = 5.0;

/// How far ahead an aircraft's landing light meets the ground below her
/// (m), parked or setting down: close, so its beam stays short - in the
/// air it lights nothing, and a long one would hold a third of the list.
pub(crate) const LANDING_AHEAD_M: f32 = 2.0;

/// Whether `part` glows - what a light is authored on (`lit` refuses any
/// other). A craft drawn in the default material for her shape alone has
/// no lamp that glows, and so no light.
fn glows(part: &Generator) -> bool {
    part.kind
        .material()
        .is_some_and(|m| m.emission_strength.0 > 0.0)
}

/// `part` lit as `lamp` (at `at`, else at its own place) if it glows; any
/// other as it came.
fn lit_if(part: Generator, lamp: Lamp, at: Option<[f32; 3]>) -> Generator {
    match (glows(&part), at) {
        (false, _) => part,
        (true, Some(at)) => lit_at(part, lamp, at),
        (true, None) => lit(part, lamp),
    }
}

/// A pair of headlamps lit by one beam: `lens` (one of the pair's glowing
/// lenses) carries a spot at its own place, its height over `ground`
/// ([`headlight_at`]). At the lens, not between the pair: a light standing
/// off its part writes an offset into the record (the module docs).
pub(super) fn headlights(lens: Generator, ground: f32) -> Generator {
    let at = lens.transform.translation.0;
    headlight_at(lens, at, ground)
}

/// A headlight's beam on `lens` from `at`, its height over `ground`: aimed
/// onto the road [`HEADLIGHT_AHEAD_M`] ahead.
pub(super) fn headlight_at(lens: Generator, at: [f32; 3], ground: f32) -> Generator {
    beam(lens, at, ground, HEADLIGHT_AHEAD_M)
}

/// A spot from `at`, `at.y - ground` up, onto the ground `ahead` metres in
/// front - a headlight's, a roof bar's, a camera head's work light.
pub(super) fn beam(lens: Generator, at: [f32; 3], ground: f32, ahead: f32) -> Generator {
    lit_if(
        lens,
        Lamp::Headlight {
            over: at[1] - ground,
            ahead,
        },
        Some(at),
    )
}

/// An aircraft's landing light on `lens`, from its own place, onto the
/// ground [`LANDING_AHEAD_M`] ahead of it.
pub(super) fn landing_light(lens: Generator, ground: f32) -> Generator {
    let at = lens.transform.translation.0;
    beam(lens, at, ground, LANDING_AHEAD_M)
}

/// A running light - a navigation light, a stern or masthead light, a road
/// vehicle's tail light - on `part`, at its own place.
pub(super) fn running(part: Generator) -> Generator {
    lit_if(part, Lamp::Running, None)
}

/// A running light on `part` standing at `at`: the middle of a pair one
/// light stands for, or of a lamp bar whose part stands at the craft's
/// origin, where its part stands elsewhere.
pub(super) fn running_at(part: Generator, at: [f32; 3]) -> Generator {
    lit_if(part, Lamp::Running, Some(at))
}

/// The anti-collision beacon or strobe on `part`: a running light that
/// pulses, on at every hour.
pub(super) fn strobe(part: Generator) -> Generator {
    lit_if(part, Lamp::Strobe, None)
}

/// A lantern's flame on `part`, standing at `at` (the middle of a pair one
/// light stands for), `at.y - ground` over the ground.
pub(super) fn lantern_at(part: Generator, at: [f32; 3], ground: f32) -> Generator {
    lit_if(
        part,
        Lamp::Flame {
            over: at[1] - ground,
        },
        Some(at),
    )
}

/// How high over what it lights a lamp a craft carries for itself stands
/// (m) - a cabin light, a gondola's lamp or lantern, a balloon's burner:
/// what it lights is the craft round it and the ground or water just under
/// her, so it reaches the shortest reach there is.
const OWN_LAMP_OVER_M: f32 = 1.5;

/// A cabin's light, standing at `at` inside it, lit from one of its glowing
/// windows `pane` (#1924): the deck round the cabin and the water under her
/// lit as by the light through its windows.
pub(super) fn cabin(pane: Generator, at: [f32; 3]) -> Generator {
    lit_if(
        pane,
        Lamp::Bulb {
            over: OWN_LAMP_OVER_M,
        },
        Some(at),
    )
}

/// A lamp a craft carries for herself on `part`, shining every way - a
/// gondola's or a basket's lamp.
pub(super) fn own_lamp(part: Generator) -> Generator {
    lit_if(
        part,
        Lamp::Bulb {
            over: OWN_LAMP_OVER_M,
        },
        None,
    )
}

/// A lantern's flame a craft carries for herself on `part`.
pub(super) fn own_lantern(part: Generator) -> Generator {
    lit_if(
        part,
        Lamp::Flame {
            over: OWN_LAMP_OVER_M,
        },
        None,
    )
}

/// A burner's fire on `part` - a balloon's.
pub(super) fn own_fire(part: Generator) -> Generator {
    lit_if(
        part,
        Lamp::Fire {
            over: OWN_LAMP_OVER_M,
        },
        None,
    )
}

#[cfg(test)]
mod tests {
    use bevy::math::{Affine3A, UVec2, Vec3};

    use super::*;
    use crate::pds::{LightFlicker, LightShape, LightSource};
    use crate::seeded_defaults::{
        AirplaneType, AirshipType, BoatType, ChassisFamily, CraftType, HelicopterType, SkiffType,
    };
    use crate::world_builder::light::ClusterView;

    /// Up to `n` seeds of `craft`.
    fn seeds_of(craft: CraftType, n: usize) -> Vec<u64> {
        (0u64..60_000)
            .filter(|&s| CraftType::for_seed(s) == Some(craft))
            .take(n)
            .collect()
    }

    /// The seeded craft for `seed`'s visuals.
    fn craft(seed: u64) -> Generator {
        let (body, _) = super::super::build_for_seed(seed);
        body.visuals().expect("a vehicle draws a tree").clone()
    }

    /// Each light of `root`'s tree with its node's whole frame, in the
    /// craft's drawn frame (the root's own pose left out).
    fn framed(root: &Generator) -> Vec<(&LightSource, Affine3A)> {
        fn visit<'a>(
            g: &'a Generator,
            parent: Affine3A,
            out: &mut Vec<(&'a LightSource, Affine3A)>,
        ) {
            let world = parent
                * crate::catalogue::items::measure::transform_of(&g.transform).compute_affine();
            if let Some(light) = &g.light {
                out.push((light, world));
            }
            for child in &g.children {
                visit(child, world, out);
            }
        }
        let mut out = Vec::new();
        for child in &root.children {
            visit(child, Affine3A::IDENTITY, &mut out);
        }
        out
    }

    /// Where a light stands in the craft's drawn frame.
    fn at(light: &LightSource, frame: Affine3A) -> Vec3 {
        frame.transform_point3(Vec3::from(light.offset.0))
    }

    /// A light's kind: `H` a beam (a headlight, a landing light), `S` an
    /// anti-collision light, `F` a flame or fire, `B` a lamp the craft
    /// carries for herself (a cabin light, a gondola's lamp), `R` a running
    /// light.
    fn kind(light: &LightSource) -> char {
        match (&light.shape, &light.flicker) {
            (LightShape::Spot { .. }, _) => 'H',
            (_, Some(LightFlicker::Pulse { .. })) => 'S',
            (_, Some(LightFlicker::Flicker { .. })) => 'F',
            _ if light.brightness.0 > 8.0 => 'B',
            _ => 'R',
        }
    }

    /// `root`'s lights as a count before each kind, in the order H R S F B.
    fn signature(root: &Generator) -> String {
        let kinds: Vec<char> = framed(root).iter().map(|(l, _)| kind(l)).collect();
        "HRSFB"
            .chars()
            .filter_map(|k| {
                let n = kinds.iter().filter(|&&c| c == k).count();
                (n > 0).then(|| format!("{n}{k}"))
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The places of WebGL2's cluster list `lit` (each light's centre in the
    /// world and its reach) take, seen from `eye` looking at `look` on a
    /// 1920 x 1080 screen, counted as Bevy counts them.
    fn cost_from(lit: &[(Vec3, f32)], eye: Vec3, look: Vec3) -> u32 {
        let view = ClusterView::of(
            &bevy::camera::Projection::Perspective(Default::default()),
            UVec2::new(1920, 1080),
        );
        let to_view = bevy::transform::components::GlobalTransform::from(
            bevy::transform::components::Transform::from_translation(eye).looking_at(look, Vec3::Y),
        )
        .affine()
        .inverse();
        let in_view: Vec<(Vec3, f32)> = lit
            .iter()
            .map(|&(at, range)| (to_view.transform_point3(at), range))
            .collect();
        let far = in_view
            .iter()
            .filter_map(|&(c, r)| view.reach(c, r))
            .fold(0.0, f32::max);
        let slices = view.slices(far);
        in_view
            .iter()
            .map(|&(c, r)| view.count(&slices, c, r))
            .sum()
    }

    /// Each of `root`'s lights in the world, with its reach, for the craft
    /// standing with its drawn frame's `ground` on `y = 0` at `x` along the
    /// line of a parked fleet.
    fn parked(root: &Generator, ground: f32, x: f32) -> Vec<(Vec3, f32)> {
        framed(root)
            .into_iter()
            .map(|(l, f)| (at(l, f) + Vec3::new(x, -ground, 0.0), l.range.0))
            .collect()
    }

    /// The craft's drawn ground: the lowest point it draws.
    fn ground_of(root: &Generator) -> f32 {
        let mut drawn = root.clone();
        drawn.transform = crate::pds::TransformData::default();
        crate::catalogue::items::measure::tree_bounds(&drawn)
            .expect("a craft draws")
            .min
            .y
    }

    /// The most of the cluster list `lit` takes seen by the game's chase
    /// camera - its orbit's radius and pitch round `look` - from any of
    /// eight sides.
    fn play_cost(lit: &[(Vec3, f32)], look: Vec3) -> u32 {
        let (r, pitch) = (
            crate::config::camera::ORBIT_RADIUS,
            crate::config::camera::ORBIT_PITCH,
        );
        (0..8)
            .map(|k| {
                let yaw = k as f32 * std::f32::consts::TAU / 8.0;
                let eye = look
                    + Vec3::new(
                        yaw.sin() * pitch.cos(),
                        pitch.sin(),
                        yaw.cos() * pitch.cos(),
                    ) * r;
                cost_from(lit, eye, look)
            })
            .max()
            .unwrap_or(0)
    }

    /// The most of the cluster list one seeded craft's lights may take seen
    /// by the chase camera: a third of the line the budget admits lights by,
    /// so three craft parked side by side by the camera - the player's own
    /// and two beside it - draw whole.
    const VEHICLE_CLUSTER_COST: u32 = crate::world_builder::light::budget::ADMIT_LIMIT / 3;

    /// How many seeds of each type the tests read.
    const SEEDS: usize = 40;

    /// How many of them a test that measures a craft's drawn ground reads:
    /// the ground is every part meshed.
    const MEASURED: usize = 12;

    /// Every craft type and the lights its seeds carry, as [`signature`]
    /// spells them: one entry for each set of lamps a seed of it draws (a
    /// lantern or a beacon only some tiers hang). A type with `""` among
    /// them draws some seeds with no lamp at all; the longship and the
    /// floater draw none - the longship's one lit part is her vane, the
    /// floater's a creature's glow.
    const LIT: &[(CraftType, &[&str])] = &[
        (CraftType::Skiff(SkiffType::Roadster), &["1H 1R"]),
        (CraftType::Skiff(SkiffType::DuneBuggy), &["1H 1R"]),
        (CraftType::Skiff(SkiffType::ArmouredCar), &["1H 1R"]),
        (CraftType::Skiff(SkiffType::Cyclecar), &["1H 1R"]),
        (CraftType::Skiff(SkiffType::Wagon), &["", "1F"]),
        (CraftType::Skiff(SkiffType::Rover), &["1H"]),
        (CraftType::Boat(BoatType::Sloop), &["1B", "1R 1B"]),
        (CraftType::Boat(BoatType::Longship), &[""]),
        (CraftType::Boat(BoatType::SteamTug), &["1B", "1R 1B"]),
        (CraftType::Boat(BoatType::Junk), &["1B", "1F 1B"]),
        (CraftType::Boat(BoatType::Runabout), &["", "1R"]),
        (CraftType::Boat(BoatType::Scow), &["1B"]),
        (
            CraftType::Helicopter(HelicopterType::Utility),
            &["1H 2R 1S", "2R 1S"],
        ),
        (
            CraftType::Helicopter(HelicopterType::Trainer),
            &["1H 3R 1S", "3R 1S"],
        ),
        (
            CraftType::Helicopter(HelicopterType::Autogyro),
            &["3R", "3R 1S"],
        ),
        (
            CraftType::Helicopter(HelicopterType::Coaxial),
            &["1H 3R 1S", "3R"],
        ),
        (
            CraftType::Helicopter(HelicopterType::Tandem),
            &["3R", "3R 1S"],
        ),
        (
            CraftType::Airplane(AirplaneType::HighWing),
            &["1H 3R", "1H 3R 1S"],
        ),
        (
            CraftType::Airplane(AirplaneType::Biplane),
            &["1H 3R", "1H 3R 1S"],
        ),
        (CraftType::Airplane(AirplaneType::Twin), &["3R", "3R 1S"]),
        (CraftType::Airplane(AirplaneType::Racer), &["3R"]),
        (CraftType::Airship(AirshipType::Zeppelin), &["3R"]),
        (CraftType::Airship(AirshipType::Blimp), &["3R", "3R 1S"]),
        (CraftType::Airship(AirshipType::SteamDirigible), &["", "2F"]),
        (CraftType::Airship(AirshipType::SkyGalleon), &["", "2F"]),
        (
            CraftType::Airship(AirshipType::HotAirBalloon),
            &["1F", "1F 1B"],
        ),
        (CraftType::Airship(AirshipType::Hybrid), &["2R", "2R 2S"]),
        (CraftType::Airship(AirshipType::Floater), &[""]),
    ];

    /// Every craft type of every vehicle family, and the seeds the tests
    /// read of it.
    fn fleet() -> Vec<(CraftType, Vec<u64>)> {
        [
            ChassisFamily::Skiff,
            ChassisFamily::Boat,
            ChassisFamily::Helicopter,
            ChassisFamily::Airplane,
            ChassisFamily::Airship,
        ]
        .into_iter()
        .flat_map(|family| CraftType::all_of(family).iter().copied())
        .map(|c| (c, seeds_of(c, SEEDS)))
        .collect()
    }

    /// Every seeded craft type carries the lights of the lamps it draws, and
    /// a seed of it draws each set [`LIT`] names: a lamp that loses its
    /// light, or a glow that gains one, shows up here.
    #[test]
    fn every_seeded_craft_type_carries_the_lights_of_its_lamps() {
        let fleet = fleet();
        assert_eq!(fleet.len(), LIT.len(), "a craft type is missing from LIT");
        for (c, seeds) in fleet {
            assert_eq!(seeds.len(), SEEDS, "{c:?}: too few seeds");
            let want = LIT
                .iter()
                .find(|(t, _)| *t == c)
                .unwrap_or_else(|| panic!("{c:?} is not in LIT"))
                .1;
            let mut seen: Vec<String> = seeds.iter().map(|&s| signature(&craft(s))).collect();
            seen.sort();
            seen.dedup();
            let mut want: Vec<String> = want.iter().map(|s| s.to_string()).collect();
            want.sort();
            assert_eq!(seen, want, "{c:?}'s lights");
        }
    }

    /// Each light is where and what a real one is, in its craft's drawn frame
    /// (ahead `+Z`, port `+X`): an aircraft's red light to port and its
    /// green to starboard, mirrored, and its white one astern; a road
    /// vehicle's beam ahead from its nose and its red light at its tail; a
    /// boat's running light astern or at her masthead. Every one shines its
    /// own lamp's glow, and from dusk to dawn but the anti-collision lights,
    /// which pulse at every hour.
    #[test]
    fn every_light_is_where_a_real_one_is_and_its_lamps_colour() {
        let red = |c: [f32; 3]| c[0] > 2.0 * c[1] && c[0] > 2.0 * c[2];
        let green = |c: [f32; 3]| c[1] > 2.0 * c[0] && c[1] > 2.0 * c[2];
        let white = |c: [f32; 3]| c.iter().all(|&v| v > 0.6);
        let mut navs = 0;
        for (c, seeds) in fleet() {
            let family = c.family();
            for seed in seeds {
                let root = craft(seed);
                let what = format!("{c:?} seed {seed}");
                let glowing = glow_of(&root);
                let lights = framed(&root);
                let mut ports = Vec::new();
                let mut starboards = Vec::new();
                for (light, frame) in &lights {
                    let p = at(light, *frame);
                    let colour = light.color.0;
                    assert!(
                        glowing.contains(&light.color),
                        "{what}: a light at {p} shines {colour:?}, no lamp's glow"
                    );
                    let k = kind(light);
                    assert_eq!(
                        light.dusk_to_dawn,
                        k != 'S',
                        "{what}: the light at {p} is on by day or off by night"
                    );
                    match (family, k) {
                        (ChassisFamily::Skiff, 'H') => {
                            let aim = beam_of(light, *frame);
                            assert!(
                                p.z > 0.0 && aim.z > -aim.y,
                                "{what}: a beam at {p} along {aim}"
                            );
                        }
                        (ChassisFamily::Skiff, 'R') => {
                            assert!(red(colour) && p.z < 0.0, "{what}: a tail light at {p}");
                        }
                        (ChassisFamily::Boat, 'R') => {
                            let top = lights
                                .iter()
                                .map(|(l, f)| at(l, *f).y)
                                .fold(f32::MIN, f32::max);
                            assert!(
                                p.z < 0.0 || p.y >= top,
                                "{what}: a running light at {p} is neither astern nor aloft"
                            );
                        }
                        (
                            ChassisFamily::Airplane
                            | ChassisFamily::Helicopter
                            | ChassisFamily::Airship,
                            'R',
                        ) => {
                            if red(colour) {
                                assert!(p.x > 0.05, "{what}: the red light is at {p}");
                                ports.push(p);
                            } else if green(colour) {
                                assert!(p.x < -0.05, "{what}: the green light is at {p}");
                                starboards.push(p);
                            } else {
                                assert!(
                                    white(colour) && p.z < 0.0,
                                    "{what}: a running light at {p}"
                                );
                            }
                        }
                        (_, 'S') => {
                            assert!(
                                family != ChassisFamily::Skiff && family != ChassisFamily::Boat,
                                "{what}: a strobe on a {family:?}"
                            );
                            assert!(
                                red(colour) || white(colour),
                                "{what}: a strobe of {colour:?}"
                            );
                        }
                        _ => {}
                    }
                }
                assert_eq!(ports.len(), starboards.len(), "{what}: unpaired nav lights");
                for (port, starboard) in ports.iter().zip(&starboards) {
                    navs += 1;
                    let mirror = Vec3::new(-port.x, port.y, port.z);
                    assert!(
                        (mirror - *starboard).length() < 0.01,
                        "{what}: nav lights at {port} and {starboard} do not mirror"
                    );
                }
            }
        }
        assert!(navs > 300, "{navs} pairs of nav lights read");
    }

    /// Every beam - a headlight, a landing light, a camera head's - meets
    /// the ground its craft stands on along its aim, ahead of it, inside
    /// [`POOL_SHARE`] of its reach: a beam aimed low travels its throw to the
    /// ground, not its height (`fx::light`'s rule for a raked spot), and one
    /// whose pool lay past three quarters of its reach would light nothing.
    #[test]
    fn every_beam_meets_the_ground_ahead_inside_its_reach() {
        /// The furthest share of its reach a beam's ground point may lie.
        const POOL_SHARE: f32 = 0.6;
        let mut beams = 0;
        for (c, seeds) in fleet() {
            for seed in seeds.into_iter().take(MEASURED) {
                let root = craft(seed);
                if !framed(&root).iter().any(|(l, _)| kind(l) == 'H') {
                    continue;
                }
                let ground = ground_of(&root);
                for (light, frame) in framed(&root) {
                    if kind(light) != 'H' {
                        continue;
                    }
                    beams += 1;
                    let p = at(light, frame);
                    let aim = beam_of(light, frame);
                    let throw = (p.y - ground) / -aim.y;
                    assert!(
                        aim.y < 0.0
                            && aim.z > 0.0
                            && throw > 0.0
                            && throw <= POOL_SHARE * light.range.0,
                        "{c:?} seed {seed}: from {p} along {aim} it meets the ground {ground} \
                         {throw:.2} m off, {:.2} of its reach {}",
                        throw / light.range.0,
                        light.range.0
                    );
                }
            }
        }
        assert!(beams >= 80, "{beams} beams read");
    }

    /// Seen by the game's chase camera from any side, no seeded craft's
    /// lights take more of WebGL2's cluster list than
    /// [`VEHICLE_CLUSTER_COST`], and three of each type parked side by side
    /// stay under the line the budget admits lights by - so the player's own
    /// craft and two beside it draw whole, as every seed's lights do.
    #[test]
    fn every_seeded_craft_stays_in_its_cluster_cost() {
        use crate::world_builder::light::budget::ADMIT_LIMIT;
        let look = Vec3::new(0.0, 0.8, 0.0);
        let mut worst: Vec<String> = Vec::new();
        for (c, seeds) in fleet() {
            let (mut one, mut three) = (0, 0);
            for seed in seeds.into_iter().take(MEASURED / 2) {
                let root = craft(seed);
                let ground = ground_of(&root);
                let cost = play_cost(&parked(&root, ground, 0.0), look);
                assert!(
                    cost <= VEHICLE_CLUSTER_COST,
                    "{c:?} seed {seed}: its lights take {cost} places of the list"
                );
                let side_by_side: Vec<(Vec3, f32)> = (-1..=1)
                    .flat_map(|i| parked(&root, ground, i as f32 * PARKED_APART_M))
                    .collect();
                let parked_cost = play_cost(&side_by_side, look);
                assert!(
                    parked_cost <= ADMIT_LIMIT,
                    "{c:?} seed {seed}: three parked take {parked_cost} places"
                );
                one = one.max(cost);
                three = three.max(parked_cost);
            }
            worst.push(format!("{c:?} {one} (three parked {three})"));
        }
        println!(
            "cluster cost by craft type, the most over its seeds:\n{}",
            worst.join("\n")
        );
    }

    /// How far apart craft parked side by side stand (m, middle to middle):
    /// a little over the widest seeded craft's beam that is not an
    /// aircraft's span.
    const PARKED_APART_M: f32 = 3.5;

    /// Every seeded craft's lights are as the avatar sanitiser leaves them,
    /// and cross the wire a peer reads a body from unchanged: what one client
    /// authors, every other draws.
    #[test]
    fn every_seeded_crafts_lights_cross_the_wire_and_the_sanitiser_unchanged() {
        let lights_of = |record: &crate::pds::AvatarRecord| -> Vec<LightSource> {
            let root = record.body.visuals().expect("a vehicle draws a tree");
            framed(root).into_iter().map(|(l, _)| l.clone()).collect()
        };
        let mut lit = 0;
        for (c, seeds) in fleet() {
            for seed in seeds.into_iter().take(4) {
                let record = crate::pds::AvatarRecord::default_for_seed(seed);
                let mine = lights_of(&record);
                lit += mine.len();
                let wire = crate::protocol::OverlandsMessage::avatar_state_update(&record)
                    .expect("a seeded record broadcasts");
                let crate::protocol::OverlandsMessage::AvatarStateUpdate { record_json } = wire
                else {
                    panic!("an avatar update");
                };
                let mut theirs =
                    crate::protocol::OverlandsMessage::decode_avatar_state(&record_json)
                        .expect("a peer reads it");
                theirs.sanitize();
                assert_eq!(lights_of(&theirs), mine, "{c:?} seed {seed}");
            }
        }
        assert!(lit > 100, "{lit} lights read");
    }

    /// Every glow `root`'s parts shine, on the wire's grid as a light's
    /// colour is.
    fn glow_of(root: &Generator) -> Vec<crate::pds::Fp3> {
        fn visit(g: &Generator, out: &mut Vec<crate::pds::Fp3>) {
            if let Some(m) = g.kind.material().filter(|m| m.emission_strength.0 > 0.0) {
                out.push(m.emission_color.on_wire());
            }
            for child in &g.children {
                visit(child, out);
            }
        }
        let mut out = Vec::new();
        visit(root, &mut out);
        out
    }

    /// The way a spot shines in the craft's drawn frame, as the engine takes
    /// it from its node's whole frame.
    fn beam_of(light: &LightSource, frame: Affine3A) -> Vec3 {
        let LightShape::Spot { aim, .. } = light.shape else {
            panic!("a beam is a spot");
        };
        frame.transform_vector3(Vec3::from(aim.0)).normalize()
    }
}
