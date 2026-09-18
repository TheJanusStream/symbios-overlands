//! The roadster's own discrete picks: which body it is built on, whether it is
//! open or closed, and which wheels it rolls on (#1367).
//!
//! The roadster is the skiff family's universal floor, so every theme can draw
//! it, and without picks of its own a cyberpunk roadster and a pastoral one are
//! the same car in two liveries. The TOP is the pick that carries the theme -
//! a closed car is a town, night and weather car, and its lit window band is
//! the most luminous thing a car can carry - while the body and the wheels
//! follow the stance, because a stance predicts a machine's shape better than
//! a theme does.
//!
//! All three follow [`super::craft`]'s idiom: an own salted sub-stream, a
//! universal floor at [`FLOOR`] on every theme, [`AT_HOME`] on the mood groups
//! a variant belongs to, and [`pick_weighted`] - with the sloop's
//! [`STANCE_BONUS`] on top. One wheel is not drawn at all: every Heavy car
//! rolls on balloon tyres, because a fat low-pressure tyre is a proportion of
//! the hauler, not a choice made about it.
//!
//! The owner agreed these tables on the phase-1 renders of #1367 (session
//! 790). The BODY weights are keyed to stance alone, in the shape of the
//! sloop's hull forms ([`super::sloop::SloopHull`]), and were proposed as mine.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::character::AvatarCharacter;
use super::craft::{AT_HOME, FLOOR};
use super::mood::{AGRARIAN, COASTAL, NEON, REGAL, SEPULCHRAL, STEAM, holds};
use super::vehicle_blueprint::{VehicleBlueprint, VehicleStance};
use crate::seeded_defaults::scene::{ThemeArchetype, pick_weighted};

/// Sub-stream salts for the three draws - distinct from every sibling avatar
/// deriver salt and from each other, so a seed's body tells you nothing about
/// its top, its wheels, its craft type or its livery.
const AVATAR_ROADSTER_BODY_SALT: u64 = 0xB0B7_A11B_B0B7_A11B;
const AVATAR_ROADSTER_TOP_SALT: u64 = 0x4A2D_7092_4A2D_7092;
const AVATAR_ROADSTER_WHEELS_SALT: u64 = 0x5B0C_3EE1_5B0C_3EE1;

/// What a stance adds to the variant it suits - the sloop's own figure: two
/// thirds of [`AT_HOME`], enough that a stance usually wins, not so much that a
/// theme's own answer stops being its likeliest.
const STANCE_BONUS: u32 = 4;

/// The agreed boat-tail's weight on every stance - twice a variant's floor, as
/// the sloop's agreed transom hull is, so it stays the likeliest single answer
/// on the stances no variant suits.
const AGREED_BODY: u32 = 4;

/// The roadster's body - a different plan station list and layout, and
/// nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoadsterBody {
    /// The tail runs out to a point behind the rear wheels - the body the
    /// roadster was agreed on in #1364.
    BoatTail,
    /// The tail cut off just behind the rear wheels and rounded over - the
    /// stubby racer's back, with the spare standing upright on it.
    Bobtail,
    /// Full beam carried aft over a second row of seats to a blunt back.
    Tourer,
}

impl RoadsterBody {
    pub const ALL: [Self; 3] = [Self::BoatTail, Self::Bobtail, Self::Tourer];

    /// Human-readable display name. Public, like
    /// [`SkiffType::label`](super::SkiffType::label), because the readouts that
    /// print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::BoatTail => "Boat-tail",
            Self::Bobtail => "Bobtail",
            Self::Tourer => "Tourer",
        }
    }

    /// The stance whose machine this body suits: the racer's pointed tail on
    /// the long low Sleek car, the short tail on the short Compact one, the
    /// second row on the wide Heavy hauler.
    fn suited(self) -> VehicleStance {
        match self {
            Self::BoatTail => VehicleStance::Sleek,
            Self::Bobtail => VehicleStance::Compact,
            Self::Tourer => VehicleStance::Heavy,
        }
    }

    /// Sampling weight on a machine of `stance` (`None` where the seed has no
    /// skiff blueprint to read one off).
    pub fn weight(self, stance: Option<VehicleStance>) -> u32 {
        let base = if self == Self::BoatTail {
            AGREED_BODY
        } else {
            FLOOR
        };
        base + if stance == Some(self.suited()) {
            STANCE_BONUS
        } else {
            0
        }
    }

    /// The body for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_stance(stance_of(seed), seed)
    }

    /// The body for an already-resolved stance.
    pub fn for_stance(stance: Option<VehicleStance>, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_ROADSTER_BODY_SALT);
        pick_weighted(&Self::ALL, |b| b.weight(stance), &mut rng).unwrap_or(Self::BoatTail)
    }
}

/// Open, or closed under a hardtop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoadsterTop {
    /// The open two-seater, screen and steering wheel in the air - the floor
    /// every theme can reach.
    Open,
    /// A body-coloured cabin on the canopy seat, glazed with a lit window band.
    Hardtop,
}

impl RoadsterTop {
    pub const ALL: [Self; 2] = [Self::Open, Self::Hardtop];

    /// The top every theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::Open;

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::Hardtop => "Hardtop",
        }
    }

    /// Whether this top is at home on `style` - the owner's table (#1367).
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // Where a smart open car belongs on the street: the seaside, the
            // stately moods and the ordinary country and suburban roads.
            Self::Open => holds(COASTAL, style) || holds(REGAL, style) || holds(AGRARIAN, style),
            // A town, night and weather car: the lit band is the most luminous
            // thing a car can carry, the smoky cities close their cars, and a
            // closed carriage is the funereal one.
            Self::Hardtop => holds(NEON, style) || holds(STEAM, style) || holds(SEPULCHRAL, style),
        }
    }

    /// Whether `stance` favours this top: a racer runs open, a hauler is
    /// closed.
    fn suits(self, stance: VehicleStance) -> bool {
        match stance {
            VehicleStance::Sleek => self == Self::Open,
            VehicleStance::Heavy => self == Self::Hardtop,
            VehicleStance::Compact => false,
        }
    }

    /// Sampling weight of this top for an avatar of `style` on a machine of
    /// `stance`.
    pub fn weight(self, style: ThemeArchetype, stance: Option<VehicleStance>) -> u32 {
        let floor = if self == Self::UNIVERSAL { FLOOR } else { 0 };
        let home = if self.at_home(style) { AT_HOME } else { 0 };
        let bonus = if stance.is_some_and(|s| self.suits(s)) {
            STANCE_BONUS
        } else {
            0
        };
        floor + home + bonus
    }

    /// The top for a seed, weighted by the avatar's style and its machine's
    /// stance.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style, stance_of(seed), seed)
    }

    /// The top for an already-resolved style and stance.
    pub fn for_style(style: ThemeArchetype, stance: Option<VehicleStance>, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_ROADSTER_TOP_SALT);
        pick_weighted(&Self::ALL, |t| t.weight(style, stance), &mut rng).unwrap_or(Self::UNIVERSAL)
    }
}

/// The wheels a roadster rolls on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoadsterWheels {
    /// Pressed-steel discs with a proud cap - the wheel the roadster was built
    /// with, and the floor.
    Disc,
    /// Fat low-pressure tyres on a smaller rim - every Heavy car, and only a
    /// Heavy car.
    Balloon,
    /// Eight wire spokes to a turned hub.
    Wire,
}

impl RoadsterWheels {
    pub const ALL: [Self; 3] = [Self::Disc, Self::Balloon, Self::Wire];

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Disc => "Pressed disc",
            Self::Balloon => "Balloon",
            Self::Wire => "Wire",
        }
    }

    /// Sampling weight on a machine of `stance` for an avatar of `style`.
    ///
    /// A Heavy machine has exactly one answer - the balloon - and no other
    /// machine ever draws it. Everywhere else the disc is the floor, and the
    /// sporting wire wheel is at home where a smart open car is, and on the
    /// long low racer.
    pub fn weight(self, style: ThemeArchetype, stance: Option<VehicleStance>) -> u32 {
        let heavy = stance == Some(VehicleStance::Heavy);
        match self {
            Self::Balloon => {
                if heavy {
                    FLOOR
                } else {
                    0
                }
            }
            _ if heavy => 0,
            Self::Disc => FLOOR,
            Self::Wire => {
                let home = if holds(REGAL, style) || holds(COASTAL, style) {
                    AT_HOME
                } else {
                    0
                };
                home + if stance == Some(VehicleStance::Sleek) {
                    STANCE_BONUS
                } else {
                    0
                }
            }
        }
    }

    /// The wheels for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style, stance_of(seed), seed)
    }

    /// The wheels for an already-resolved style and stance.
    pub fn for_style(style: ThemeArchetype, stance: Option<VehicleStance>, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_ROADSTER_WHEELS_SALT);
        pick_weighted(&Self::ALL, |w| w.weight(style, stance), &mut rng).unwrap_or(Self::Disc)
    }
}

/// The stance of a seed's land-skiff, or `None` for a seed that is not a
/// skiff.
fn stance_of(seed: u64) -> Option<VehicleStance> {
    VehicleBlueprint::from_seed(seed).and_then(|b| b.skiff().map(|s| s.stance))
}

#[cfg(test)]
mod tests {
    use super::*;

    const STANCES: [Option<VehicleStance>; 4] = [
        None,
        Some(VehicleStance::Compact),
        Some(VehicleStance::Sleek),
        Some(VehicleStance::Heavy),
    ];

    #[test]
    fn deterministic() {
        for s in 0u64..50 {
            assert_eq!(RoadsterBody::for_seed(s), RoadsterBody::for_seed(s));
            assert_eq!(RoadsterTop::for_seed(s), RoadsterTop::for_seed(s));
            assert_eq!(RoadsterWheels::for_seed(s), RoadsterWheels::for_seed(s));
        }
    }

    /// The floors keep their promise: every theme on every stance can reach
    /// the open car and the agreed boat-tail, and every machine has at least
    /// one set of wheels to roll on.
    #[test]
    fn the_floor_is_reachable_everywhere() {
        for style in ThemeArchetype::ALL {
            for stance in STANCES {
                assert!(RoadsterTop::UNIVERSAL.weight(style, stance) > 0);
                assert!(RoadsterBody::BoatTail.weight(stance) > 0);
                assert!(
                    RoadsterWheels::ALL
                        .iter()
                        .any(|w| w.weight(style, stance) > 0),
                    "{style:?} on {stance:?} has no wheels"
                );
            }
        }
    }

    /// Every Heavy machine rolls on balloons, and nothing else ever does -
    /// a proportion of the stance, not a draw.
    #[test]
    fn a_heavy_machine_and_only_a_heavy_one_rolls_on_balloons() {
        for style in ThemeArchetype::ALL {
            for stance in STANCES {
                for s in 0u64..40 {
                    let heavy = stance == Some(VehicleStance::Heavy);
                    let w = RoadsterWheels::for_style(style, stance, s);
                    assert_eq!(
                        w == RoadsterWheels::Balloon,
                        heavy,
                        "{style:?} on {stance:?}, seed {s}: {w:?}"
                    );
                }
            }
        }
    }

    /// The hardtop is a pick a theme or a stance has to ask for: a theme at
    /// home in neither closes only its Heavy machines.
    #[test]
    fn a_car_is_closed_only_where_a_theme_or_a_stance_asks() {
        for style in ThemeArchetype::ALL {
            let home = RoadsterTop::Hardtop.at_home(style);
            for stance in STANCES {
                let can_close = RoadsterTop::Hardtop.weight(style, stance) > 0;
                assert_eq!(
                    can_close,
                    home || stance == Some(VehicleStance::Heavy),
                    "{style:?} on {stance:?}"
                );
            }
        }
    }

    /// Every top and every wheel variant is some theme's own answer, or it is a
    /// variant nobody sees.
    #[test]
    fn every_top_and_wheel_is_at_home_somewhere() {
        for t in RoadsterTop::ALL {
            assert!(
                ThemeArchetype::ALL.iter().any(|&s| t.at_home(s)),
                "{t:?} is at home in no theme"
            );
        }
        assert!(
            ThemeArchetype::ALL
                .iter()
                .any(|&s| RoadsterWheels::Wire.weight(s, None) > 0),
            "the wire wheel is at home in no theme"
        );
    }

    /// The sampler reaches every body, top and wheel over the skiff seeds that
    /// actually exist.
    #[test]
    fn the_sampler_reaches_every_variant() {
        let (mut bodies, mut tops, mut wheels) = (Vec::new(), Vec::new(), Vec::new());
        for s in 0u64..4000 {
            let Some(stance) = stance_of(s) else { continue };
            let style = AvatarCharacter::for_seed(s).style;
            let b = RoadsterBody::for_stance(Some(stance), s);
            let t = RoadsterTop::for_style(style, Some(stance), s);
            let w = RoadsterWheels::for_style(style, Some(stance), s);
            if !bodies.contains(&b) {
                bodies.push(b);
            }
            if !tops.contains(&t) {
                tops.push(t);
            }
            if !wheels.contains(&w) {
                wheels.push(w);
            }
        }
        assert_eq!(
            bodies.len(),
            RoadsterBody::ALL.len(),
            "reached only {bodies:?}"
        );
        assert_eq!(tops.len(), RoadsterTop::ALL.len(), "reached only {tops:?}");
        assert_eq!(
            wheels.len(),
            RoadsterWheels::ALL.len(),
            "reached only {wheels:?}"
        );
    }

    /// The stance bonus is live on the body: each stance draws the body it
    /// suits more often than any other stance does.
    #[test]
    fn each_stance_favours_its_own_body() {
        let share = |body: RoadsterBody, stance: VehicleStance| {
            (0u64..3000)
                .filter(|&s| RoadsterBody::for_stance(Some(stance), s) == body)
                .count()
        };
        for body in RoadsterBody::ALL {
            let own = share(body, body.suited());
            for other in [
                VehicleStance::Compact,
                VehicleStance::Sleek,
                VehicleStance::Heavy,
            ] {
                if other != body.suited() {
                    assert!(
                        own > share(body, other),
                        "{body:?} is not likelier on its own stance than on {other:?}"
                    );
                }
            }
        }
    }

    /// The theme speaks through the top: a neon theme on a stance that favours
    /// neither closes its car about half the time (6 of 12 against the open
    /// floor's 2 plus its own 4 on Sleek), a coastal one never.
    #[test]
    fn a_neon_theme_closes_its_car_and_a_coastal_one_does_not() {
        let closed = |style: ThemeArchetype| {
            (0u64..3000)
                .filter(|&s| {
                    RoadsterTop::for_style(style, Some(VehicleStance::Sleek), s)
                        == RoadsterTop::Hardtop
                })
                .count()
        };
        let neon = closed(ThemeArchetype::Cyberpunk);
        assert!(
            (1300..=1700).contains(&neon),
            "{neon} of 3000 Sleek cyberpunk roadsters closed, expected about 1500"
        );
        assert_eq!(closed(ThemeArchetype::CoastalResort), 0);
    }
}
