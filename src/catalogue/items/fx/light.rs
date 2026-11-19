//! The lights the catalogue's lamps carry (#1923): a lamp head, a lantern,
//! a torch's flame, a beacon - each given an authored [`LightSource`] in one
//! line, [`lit`], so every theme's lamps light alike and a new one lights
//! the way its kin do. The seeded vehicles' lamps are lit by the same line
//! (#1924, `pds::avatar::default_visuals::lights`), with the mounts a
//! vehicle carries: a headlight ([`Lamp::Headlight`]), a running light
//! ([`Lamp::Running`]) and an anti-collision light ([`Lamp::Strobe`]).
//!
//! A light goes on the glowing part that is its source, and takes that
//! part's glow for its colour: the lamp's glass, the flame's core. Which
//! glowing parts are sources and which stay a glow alone (a lit window, a
//! neon tube, a screen, a rune) is decided item by item where the item is
//! built; only a part someone would call the lamp gets one, and only out of
//! doors: no light casts a shadow, so a lamp inside walls (a tavern
//! table's lantern, a garage's bench lamp) stays a glow: its light would
//! light the ground outside the walls as much as the room.
//!
//! # How far and how bright
//!
//! Each kind of mount ([`Lamp`]) is told how high it stands over what it
//! lights (`over`, metres), and reaches [`REACH_PER_HEIGHT`] times the
//! distance its light travels to it ([`Lamp::throw`]), never less than
//! [`MIN_REACH_M`]. For a lamp shining every way or straight down that is
//! its height; a raked floodlight's beam meets the ground further off, at
//! `over` divided by how steeply its aim falls, so it reaches twice the
//! distance along its aim. Bevy's range window has taken a light
//! to about a third of itself at four fifths of its reach, so a lamp whose
//! reach is only half again its height lights the ground under it at
//! barely two thirds of its power and draws a faint pool; at twice its
//! height it lights it at 0.88 of it, and a pool reads. Measured on lamp
//! posts of 1 to 8 m at 23:00 under a world's moon, on pale stone (lit over
//! unlit, under the head, at the brightest the sanitiser allows): a reach of
//! 1.5 times the height drew pools of x2.0 to x2.2 with radii of 0.6 to
//! 2.7 m; twice the height drew x3.5 to x3.8 under heads of 2 to 5 m, with
//! radii of 2.1 to 5.2 m; 2.5 times drew x4.1 to x5.3. Past 5 m the brightness
//! is held by the light's power (`sanitize::light::max_light_brightness`),
//! so a taller head draws a wider, fainter pool however it is set: an 8 m
//! head at twice its height drew x2.4.
//!
//! No reach is longer than the sanitiser's longest (20 m), and the power
//! cap holds a long one faint, so a spot throwing further than
//! [`LONGEST_THROW_M`] - a stadium's floodlights 12 m up and raked, a
//! crane's under a 15 m boom - would light its pool from three quarters of
//! its reach or more and draw nothing; its light hangs partway down its
//! own beam instead, that throw short of the pool, and the pool lands
//! where the lamp is aimed. With no shadows nothing shows where the light
//! stands but what it lights.
//!
//! Each mount's brightness is a share of that ceiling for its reach
//! ([`Lamp::brightness`]): a street light or a floodlight at the most it
//! may be, a lamp or a fire a little under it, a flame and a beacon less.
//!
//! # What a lamp costs
//!
//! WebGL2 keeps every light's place in each screen cluster it touches in
//! one list, and a client draws only the nearest lights the list holds
//! (`world_builder::light`): a long reach costs more of it than a short
//! one. So a dense item lights its lamps by the few that read - one light
//! for a bank of floodlight panels, a high floodlight's light hung down its
//! beam rather than reaching 20 m from its mast - and a catalogue test
//! holds every item's lights under `ITEM_CLUSTER_COST` places of the list
//! seen from beside it.
//!
//! # By day
//!
//! Every light here shows from dusk to dawn, the record's default: a lamp's
//! light is lost beside the sun anyway, and one lit by day would take one of
//! the few places in a client's budget for nothing. None of the catalogue's
//! lamps is on by day by design. The one mount on at every hour is a
//! vehicle's anti-collision light ([`Lamp::Strobe`]), as an aircraft's is
//! whenever her engine runs: short, so cheap in the budget, and its pulse
//! on her own skin shows close by under the sun.

use bevy::math::{Quat, Vec3};

use crate::pds::sanitize::light::max_light_brightness;
use crate::pds::sanitize::limits::MAX_LIGHT_RANGE_M;
use crate::pds::{Fp, Fp3, Generator, LightFlicker, LightShape, LightSource};

/// How many times its height over what it lights a lamp reaches - see the
/// module docs.
pub(crate) const REACH_PER_HEIGHT: f32 = 2.0;

/// The shortest reach (m) a lamp is given, however low it stands: a light
/// at the foot of a wall still lights a pool round it.
pub(crate) const MIN_REACH_M: f32 = 3.0;

/// The furthest a spot's light stands from what it lights (m), so it
/// reaches 16 m. A floodlight on a stadium mast or under a crane's boom
/// throws further than that, and its light hangs partway down its own beam
/// instead ([`Lamp::slide`]): the pool lands where the lamp is aimed, lit
/// as a lamp of this throw lights it. Not the 10 m half the longest reach
/// would allow: the power cap holds a 20 m reach to brightness 8 and a
/// 16 m one to 12.5, so a pool lit from 8 m is about two and a half times
/// as bright as one lit from 10 m, for less of the cluster list (the
/// stadium's four banks took 10,780 places from 10 m, 8,818 from 8, each
/// spot counted as the sphere of its reach; counted by their cones, as
/// Bevy counts them since #1926, they take 3,029 from 8).
pub(crate) const LONGEST_THROW_M: f32 = 8.0;

/// The least a floodlight's aim may fall or rise (the vertical part of its
/// unit aim): about 12 degrees off level.
const FLOOD_LEAST_FALL: f32 = 0.2;

/// The most places of WebGL2's cluster index list (16,384 in all) one
/// item's lights may take together, seen from beside it (the catalogue test
/// `every_lit_item_stays_in_its_cluster_cost`): under the 12,000 the budget
/// admits lights while, so every one of an item's lights can be drawn at
/// once. A bulb on a 5 m post takes about 3,300 of them from a couple of
/// metres off, a street light's cone down from a 5 m head about 2,400 (a
/// spot is counted by its cone, #1926); most items light by one or two,
/// and the dearest takes about 7,300.
#[cfg(test)]
pub(crate) const ITEM_CLUSTER_COST: u32 = 10_000;

/// How far over its glowing part a ground marker's light sits (m): a light
/// lying on the surface it lights grazes it and lights nothing.
pub(crate) const MARKER_LIFT_M: f32 = 0.35;

/// A ground marker's reach (m): a pool of its colour on the surface round
/// it.
pub(crate) const MARKER_REACH_M: f32 = 3.0;

/// How a lamp is mounted and what it throws, which sets its light's shape,
/// reach, brightness and flicker - see the module docs. `over` is how high
/// it stands over what it lights (m).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Lamp {
    /// A bulb, globe or lantern shining every way: a lamp post's head, a
    /// porch, wall or pier lamp.
    Bulb { over: f32 },
    /// A street light's head, a canopy's downlight: a wide cone down.
    Street { over: f32 },
    /// A floodlight: a narrower cone along `aim` (in the frame its part
    /// stands in - the item's, for a part of a flat assembly), at the
    /// most its reach allows. `over` is the height between it and what it
    /// lights, below it or, for an uplight, above it.
    Flood { over: f32, aim: [f32; 3] },
    /// A small flame - a torch, a candle, the flame in a lantern: a point
    /// light that wavers.
    Flame { over: f32 },
    /// A fire - a brazier, a bonfire, a beacon's fire, a forge's mouth: a
    /// stronger light that wavers deeper and slower.
    Fire { over: f32 },
    /// A beacon or a warning lamp: a point light that breathes, gently.
    Beacon { over: f32 },
    /// A light set in or on the ground - an edge light, a runway light: a
    /// small pool of its colour round it, lifted [`MARKER_LIFT_M`] off its
    /// part.
    Marker,
    /// A vehicle's headlight or landing light (#1924): a spot shining ahead,
    /// along `+Z` of the frame its part stands in (a vehicle's drawn
    /// forward), and a little down, onto the ground `ahead` metres in front
    /// of it from `over` metres up. Aimed so low it would light nothing as
    /// a floodlight, its throw is the length of its beam to that ground
    /// point, so it reaches twice that along its aim.
    Headlight { over: f32, ahead: f32 },
    /// A vehicle's running light (#1924) - a navigation light, a tail or
    /// stern light, a masthead light: a small light of its lamp's colour,
    /// lighting the vehicle's own skin and the ground just round it.
    Running,
    /// A vehicle's anti-collision beacon or strobe (#1924): a running light
    /// that pulses deep, about once a second, and is on at every hour.
    Strobe,
}

/// A street light's cone (inner, outer half angles in degrees): a wide
/// pool under the head.
const STREET_CONE_DEG: [f32; 2] = [35.0, 60.0];

/// A floodlight's cone.
const FLOOD_CONE_DEG: [f32; 2] = [20.0, 35.0];

/// A small flame's waver: shallow enough not to flash, so as quick as a
/// flame is (`sanitize::light::max_flicker_rate_hz`).
const FLAME_FLICKER: (f32, f32) = (0.15, 3.0);

/// A fire's waver: deeper, so held to the sanitiser's three flashes a
/// second (and under its two of before #1950).
const FIRE_FLICKER: (f32, f32) = (0.25, 1.5);

/// A beacon's breath: slow, and shallow - a beacon's glass glows steady
/// (a material does not pulse), so a deep breath in its pool would read as
/// a light at odds with its lamp; a third reads as the glass breathing
/// gently. Still a flash by the sanitiser's measure (deeper than a tenth),
/// and well under its three a second.
const BEACON_PULSE: (f32, f32) = (0.3, 0.5);

/// A fire's light is broader than a bulb's.
const FIRE_SIZE_M: f32 = 0.25;

/// A headlight's cone: the record's default, which a spot leaves off the
/// wire, so a vehicle's record writes no cone for its beam (a car's record
/// has few bytes to spare: 30 a beam against a cone of its own) - a pool
/// ahead, its lower edge reaching back toward the vehicle.
const HEADLIGHT_CONE_DEG: [f32; 2] = crate::pds::light::DEFAULT_SPOT_CONE_DEG;

/// An anti-collision light's pulse: deep, as a beacon's flash is, at the
/// once a second of a real one - under the three flashes a second the
/// sanitiser holds any light to.
const STROBE_PULSE: (f32, f32) = (0.8, 1.0);

impl Lamp {
    /// Which way a spot shines, as a unit vector in the frame its part
    /// stands in; `None` for a light that shines every way.
    ///
    /// Panics on a floodlight aimed within [`FLOOD_LEAST_FALL`] of level:
    /// its throw is read from how steeply it falls, and a beam that hardly
    /// falls meets nothing it stands over.
    pub(crate) fn aim(self) -> Option<Vec3> {
        match self {
            Lamp::Street { .. } => Some(Vec3::NEG_Y),
            Lamp::Headlight { over, ahead } => {
                Some(Vec3::new(0.0, -over.max(0.0), ahead.max(0.0)).normalize_or(Vec3::Z))
            }
            Lamp::Flood { aim, .. } => {
                let aim = Vec3::from(aim).normalize_or(Vec3::NEG_Y);
                assert!(
                    aim.y.abs() >= FLOOD_LEAST_FALL,
                    "a floodlight aimed {aim} lights nothing it stands over: aim it up or down"
                );
                Some(aim)
            }
            _ => None,
        }
    }

    /// How far its light travels to what it lights (m): `over` for a lamp
    /// shining every way or straight down, and for a floodlight the length
    /// of its beam down (or up) to the height `over` away - so a raked one
    /// throws further than it stands high.
    pub(crate) fn throw(self) -> f32 {
        match self {
            Lamp::Marker | Lamp::Running | Lamp::Strobe => MARKER_REACH_M / REACH_PER_HEIGHT,
            Lamp::Flood { over, .. } => over / self.aim().map_or(1.0, |aim| aim.y.abs()),
            Lamp::Headlight { over, ahead } => over.hypot(ahead),
            Lamp::Bulb { over }
            | Lamp::Street { over }
            | Lamp::Flame { over }
            | Lamp::Fire { over }
            | Lamp::Beacon { over } => over,
        }
    }

    /// How far down its own beam a spot's light hangs below its lamp (m):
    /// none, unless its throw is past [`LONGEST_THROW_M`], when the light
    /// stands that far short of its pool instead - see the module docs.
    pub(crate) fn slide(self) -> f32 {
        match self.aim() {
            Some(_) => (self.throw() - LONGEST_THROW_M).max(0.0),
            None => 0.0,
        }
    }

    /// How far it reaches (m): [`REACH_PER_HEIGHT`] times its throw (a
    /// spot's from where its light hangs), never less than [`MIN_REACH_M`]
    /// nor more than the longest reach.
    pub(crate) fn reach(self) -> f32 {
        match self {
            Lamp::Marker | Lamp::Running | Lamp::Strobe => MARKER_REACH_M,
            _ => ((self.throw() - self.slide()) * REACH_PER_HEIGHT)
                .clamp(MIN_REACH_M, MAX_LIGHT_RANGE_M),
        }
    }

    /// Its share of the brightest its reach allows.
    fn share(self) -> f32 {
        match self {
            Lamp::Street { .. } | Lamp::Flood { .. } | Lamp::Headlight { .. } => 1.0,
            Lamp::Bulb { .. } | Lamp::Fire { .. } => 0.75,
            Lamp::Flame { .. } | Lamp::Beacon { .. } | Lamp::Strobe => 0.5,
            Lamp::Marker | Lamp::Running => 0.25,
        }
    }

    /// How bright, on the wire's grid and inside the sanitiser's bound for
    /// its reach.
    pub(crate) fn brightness(self) -> f32 {
        let brightest = max_light_brightness(self.reach());
        Fp(brightest * self.share()).on_wire().0.min(brightest)
    }

    /// The light of this mount for a part glowing `color`, in the frame its
    /// part stands in: [`lit`] turns a spot's aim and a marker's lift into
    /// the part's own.
    pub(crate) fn light(self, color: [f32; 3]) -> LightSource {
        let spot = |[inner, outer]: [f32; 2]| LightShape::Spot {
            aim: Fp3(self.aim().unwrap_or(Vec3::NEG_Y).to_array()),
            inner: Fp(inner),
            outer: Fp(outer),
        };
        let flicker = |(amount, rate): (f32, f32)| LightFlicker::Flicker {
            amount: Fp(amount),
            rate: Fp(rate),
            shimmer_rate: None,
        };
        let mut light = LightSource {
            color: Fp3(color),
            brightness: Fp(self.brightness()),
            range: Fp(self.reach()),
            ..LightSource::default()
        };
        match self {
            Lamp::Bulb { .. } => {}
            Lamp::Street { .. } => light.shape = spot(STREET_CONE_DEG),
            Lamp::Flood { .. } => light.shape = spot(FLOOD_CONE_DEG),
            Lamp::Flame { .. } => light.flicker = Some(flicker(FLAME_FLICKER)),
            Lamp::Fire { .. } => {
                light.flicker = Some(flicker(FIRE_FLICKER));
                light.size = Fp(FIRE_SIZE_M);
            }
            Lamp::Beacon { .. } => {
                light.flicker = Some(LightFlicker::Pulse {
                    amount: Fp(BEACON_PULSE.0),
                    rate: Fp(BEACON_PULSE.1),
                });
            }
            Lamp::Marker => light.offset = Fp3([0.0, MARKER_LIFT_M, 0.0]),
            Lamp::Headlight { .. } => light.shape = spot(HEADLIGHT_CONE_DEG),
            Lamp::Running => {}
            Lamp::Strobe => {
                light.flicker = Some(LightFlicker::Pulse {
                    amount: Fp(STROBE_PULSE.0),
                    rate: Fp(STROBE_PULSE.1),
                });
                light.dusk_to_dawn = false;
            }
        }
        if let Some(aim) = self.aim() {
            light.offset = Fp3((aim * self.slide()).to_array());
        }
        light
    }
}

/// `part` - a glowing primitive - lit as `lamp`, its light's colour its
/// glow's and its light at its middle (or partway down its beam, for a spot
/// that throws further than [`LONGEST_THROW_M`]). A spot's aim and a
/// marker's lift are read in the frame `part` stands in (its parent's - the
/// item's, for a part of a flat assembly) and turned into the part's own,
/// its turn and its scale both undone, so a lamp head turned or stretched
/// on its part shines where the item means it to. A floodlight's throw is
/// read from how steeply its aim falls in that frame, so a raked lamp is
/// lit as a part of a level frame: a lens inside a tilted housing is
/// authored beside the housing, turned with it, not as its child.
///
/// Panics on a part that does not glow (a light is authored on its source)
/// or one scaled to nothing along an axis.
pub(crate) fn lit(part: Generator, lamp: Lamp) -> Generator {
    lit_from(part, lamp, None)
}

/// [`lit`], with the light at `at` - a point in the frame `part` stands in
/// (its parent's), as its translation is: the middle of a bank of panels
/// one light stands for.
pub(crate) fn lit_at(part: Generator, lamp: Lamp, at: [f32; 3]) -> Generator {
    lit_from(part, lamp, Some(at))
}

fn lit_from(mut part: Generator, lamp: Lamp, at: Option<[f32; 3]>) -> Generator {
    let material = part
        .kind
        .material()
        .filter(|m| m.emission_strength.0 > 0.0)
        .expect("a light is authored on a glowing part");
    let mut light = lamp.light(material.emission_color.0);
    let t = &part.transform;
    let turn = Quat::from_array(t.rotation.0).normalize().inverse();
    let scale = Vec3::from(t.scale.0);
    assert!(
        scale.is_finite() && scale.abs().min_element() > 0.0,
        "a light is authored on a part with a size: this one is scaled by {scale}"
    );
    // The parent's frame into the part's: undo its turn, then its scale.
    // A spot's aim is turned the same way, scale and all: the engine takes
    // a spot's direction from its whole frame, so on a part stretched more
    // one way than another an aim only turned back would lean toward the
    // stretch.
    let into_part = |v: Vec3| turn * v / scale;
    if let LightShape::Spot { aim, .. } = &mut light.shape {
        let local = into_part(Vec3::from(aim.0));
        *aim = Fp3(local.normalize_or(Vec3::NEG_Y).to_array());
    }
    let mut offset = into_part(Vec3::from(light.offset.0));
    if let Some(at) = at {
        offset += into_part(Vec3::from(at) - Vec3::from(t.translation.0));
    }
    light.offset = Fp3(offset.to_array());
    crate::pds::sanitize::Sanitize::sanitize(&mut light);
    part.light = Some(light);
    part
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalogue::items::util::{cuboid_tapered, glow, id_quat, prim, quat_x};

    fn head() -> Generator {
        prim(
            cuboid_tapered([0.3, 0.1, 0.3], 0.0, glow([1.0, 0.8, 0.5], 3.0)),
            [0.0, 5.0, 0.0],
            id_quat(),
        )
    }

    /// A lamp reaches twice its height, never less than the shortest
    /// reach nor more than the longest, and is as bright as its share of
    /// what that reach allows.
    #[test]
    fn a_lamp_reaches_twice_its_height_at_its_share_of_the_ceiling() {
        assert_eq!(Lamp::Street { over: 5.0 }.reach(), 10.0);
        assert_eq!(Lamp::Bulb { over: 0.5 }.reach(), MIN_REACH_M);
        assert_eq!(Lamp::Bulb { over: 15.0 }.reach(), MAX_LIGHT_RANGE_M);
        assert_eq!(Lamp::Street { over: 5.0 }.brightness(), 32.0);
        assert_eq!(Lamp::Bulb { over: 5.0 }.brightness(), 24.0);
        assert_eq!(Lamp::Bulb { over: 15.0 }.brightness(), 6.0);
        for lamp in [
            Lamp::Bulb { over: 3.3 },
            Lamp::Street { over: 6.1 },
            Lamp::Flame { over: 7.7 },
            Lamp::Fire { over: 2.2 },
            Lamp::Beacon { over: 9.9 },
            Lamp::Marker,
        ] {
            assert!(
                lamp.brightness() <= max_light_brightness(lamp.reach()),
                "{lamp:?}"
            );
            assert!(lamp.brightness() > 0.0, "{lamp:?}");
        }
    }

    /// A lit part's light takes its glow's colour, and stands as the
    /// sanitiser would leave it.
    #[test]
    fn a_lit_part_shines_its_glow_and_sanitises_to_itself() {
        let part = lit(head(), Lamp::Street { over: 5.0 });
        let light = part.light.clone().expect("lit");
        assert_eq!(light.color, Fp3([1.0, 0.8, 0.5]).on_wire());
        let mut again = light.clone();
        crate::pds::sanitize::Sanitize::sanitize(&mut again);
        assert_eq!(again, light);
        assert!(matches!(light.shape, LightShape::Spot { .. }));
        assert!(light.dusk_to_dawn);
    }

    /// A spot's aim is its parent's: a head turned on its part shines down
    /// all the same, and a light placed at a point of the item stands
    /// there whatever the part's turn.
    #[test]
    fn a_turned_part_aims_and_places_its_light_in_its_parents_frame() {
        let mut part = head();
        part.transform.rotation = quat_x(std::f32::consts::FRAC_PI_2);
        let part = lit_at(part, Lamp::Street { over: 5.0 }, [0.0, 5.0, 1.0]);
        let light = part.light.expect("lit");
        let turn = bevy::math::Quat::from_array(part.transform.rotation.0);
        let LightShape::Spot { aim, .. } = light.shape else {
            panic!("a spot");
        };
        let world = turn * bevy::math::Vec3::from(aim.0);
        assert!((world - bevy::math::Vec3::NEG_Y).length() < 1e-3, "{world}");
        let at = turn * bevy::math::Vec3::from(light.offset.0);
        assert!((at - bevy::math::Vec3::Z).length() < 1e-3, "{at}");
    }

    /// A raked floodlight reaches twice the length of its beam down to
    /// what it lights, not twice its height: one 5 m up aimed 45 degrees
    /// down meets the ground 7.07 m along its aim.
    #[test]
    fn a_raked_floodlight_reaches_twice_its_throw() {
        let raked = Lamp::Flood {
            over: 5.0,
            aim: [0.0, -1.0, -1.0],
        };
        assert!((raked.throw() - 5.0 * std::f32::consts::SQRT_2).abs() < 1e-4);
        assert!((raked.reach() - 10.0 * std::f32::consts::SQRT_2).abs() < 1e-4);
        assert_eq!(raked.slide(), 0.0);
        let LightShape::Spot { aim, .. } = raked.light([1.0; 3]).shape else {
            panic!("a spot");
        };
        let aim = Vec3::from(aim.0);
        assert!((aim - Vec3::new(0.0, -1.0, -1.0).normalize()).length() < 1e-4);
    }

    /// A spot throwing further than the longest throw hangs its light that
    /// far short of its pool, down its own beam: a floodlight 15 m up
    /// aimed straight down lights from 8 m, reaching 16.
    #[test]
    fn a_spot_too_high_for_its_reach_hangs_its_light_down_its_beam() {
        let high = Lamp::Flood {
            over: 15.0,
            aim: [0.0, -1.0, 0.0],
        };
        assert_eq!(high.slide(), 15.0 - LONGEST_THROW_M);
        assert_eq!(high.reach(), LONGEST_THROW_M * REACH_PER_HEIGHT);
        let part = lit(head(), high);
        let light = part.light.expect("lit");
        assert_eq!(light.offset, Fp3([0.0, LONGEST_THROW_M - 15.0, 0.0]));
        let street = Lamp::Street { over: 12.0 };
        assert_eq!(street.slide(), 12.0 - LONGEST_THROW_M);
        assert_eq!(street.reach(), LONGEST_THROW_M * REACH_PER_HEIGHT);
        assert_eq!(Lamp::Street { over: 6.0 }.slide(), 0.0);
        assert_eq!(Lamp::Bulb { over: 12.0 }.slide(), 0.0);
    }

    /// A spot on a part turned and stretched more one way than another
    /// shines where its parent means: the engine takes a spot's direction
    /// from its part's whole frame, scale and all.
    #[test]
    fn a_stretched_part_aims_its_spot_in_its_parents_frame() {
        let mut part = head();
        part.transform.rotation = quat_x(0.6);
        part.transform.scale = Fp3([1.0, 3.0, 0.5]);
        let want = Vec3::new(0.3, -1.0, -0.8).normalize();
        let part = lit(
            part,
            Lamp::Flood {
                over: 3.0,
                aim: want.to_array(),
            },
        );
        let LightShape::Spot { aim, .. } = part.light.as_ref().expect("lit").shape else {
            panic!("a spot");
        };
        let frame = Quat::from_array(part.transform.rotation.0);
        let world = (frame * (Vec3::from(part.transform.scale.0) * Vec3::from(aim.0))).normalize();
        assert!((world - want).length() < 1e-3, "{world} for {want}");
    }

    /// A floodlight aimed level lights nothing it stands over, and is
    /// refused.
    #[test]
    #[should_panic(expected = "lights nothing it stands over")]
    fn a_floodlight_aimed_level_is_refused() {
        lit(
            head(),
            Lamp::Flood {
                over: 3.0,
                aim: [0.0, -0.1, -1.0],
            },
        );
    }

    /// Flames waver, fires waver deeper, beacons breathe - gently, as their
    /// steady glass does not - each inside the sanitiser's flash cap, which
    /// a sanitised light keeps unchanged.
    #[test]
    fn flames_waver_and_beacons_breathe_inside_the_flash_cap() {
        let flicker = |lamp: Lamp| lamp.light([1.0; 3]).flicker;
        assert!(matches!(
            flicker(Lamp::Flame { over: 2.0 }),
            Some(LightFlicker::Flicker { .. })
        ));
        assert!(matches!(
            flicker(Lamp::Fire { over: 2.0 }),
            Some(LightFlicker::Flicker { .. })
        ));
        assert!(matches!(
            flicker(Lamp::Beacon { over: 2.0 }),
            Some(LightFlicker::Pulse { .. })
        ));
        assert_eq!(flicker(Lamp::Bulb { over: 2.0 }), None);
        let Some(LightFlicker::Pulse { amount, .. }) = flicker(Lamp::Beacon { over: 2.0 }) else {
            panic!("a pulse");
        };
        assert!(amount.0 <= 0.3, "a beacon breathes by {}", amount.0);
        for lamp in [
            Lamp::Flame { over: 2.0 },
            Lamp::Fire { over: 2.0 },
            Lamp::Beacon { over: 2.0 },
        ] {
            let light = lamp.light([1.0; 3]);
            let mut again = light.clone();
            crate::pds::sanitize::Sanitize::sanitize(&mut again);
            assert_eq!(again.flicker, light.flicker, "{lamp:?}");
        }
    }

    /// A light is authored on its source: a part that does not glow
    /// cannot be lit.
    #[test]
    #[should_panic(expected = "glowing part")]
    fn a_part_that_does_not_glow_cannot_be_lit() {
        let dark = prim(
            cuboid_tapered([0.3, 0.1, 0.3], 0.0, Default::default()),
            [0.0, 5.0, 0.0],
            id_quat(),
        );
        lit(dark, Lamp::Bulb { over: 5.0 });
    }
}
