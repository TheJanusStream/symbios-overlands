//! The sloop's own discrete picks: which rig she carries and which hull form
//! she is built on (#1366).
//!
//! The sloop is the boat family's universal floor, so every theme can draw
//! her, and without a pick of her own a Nordic sloop and a cyberpunk one are
//! the same boat in two liveries. The RIG is the biggest thing in her
//! silhouette at play distance, so it is the pick that carries the theme;
//! the hull form is the quieter one and reads broadside.
//!
//! Both follow [`super::craft`]'s idiom exactly: an own salted sub-stream, a
//! universal floor at [`FLOOR`] on every theme, [`AT_HOME`] on the mood
//! groups a rig belongs to, and [`pick_weighted`]. On top of that sits a
//! **stance bonus**, because a stance predicts a rig better than a theme
//! does: a long narrow Sleek hull wants a tall narrow rig, a beamy Heavy one
//! a low spread of canvas.
//!
//! The affinity table is the owner's, agreed on the phase-1 renders of #1366.
//! The hull-form weights are NOT - the owner agreed which forms ship but not
//! how a seed picks one, so [`SloopHull::weight`] is a proposal keyed to
//! stance alone, for the owner to move at the in-app gate (#1368).

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use super::character::AvatarCharacter;
use super::craft::{AT_HOME, FLOOR};
use super::mood::{BUCCANEER, COASTAL, GRUBBY, HISTORIC, MARTIAL, NEON, REGAL, WORKING, holds};
use super::vehicle_blueprint::{VehicleBlueprint, VehicleStance};
use crate::seeded_defaults::scene::{ThemeArchetype, pick_weighted};

/// Sub-stream salts for the two draws - distinct from every sibling avatar
/// deriver salt and from each other, so a seed's rig tells you nothing about
/// her hull form, her craft type or her livery.
const AVATAR_RIG_SALT: u64 = 0x5100_9716_5100_9716;
const AVATAR_HULL_FORM_SALT: u64 = 0x4011_F0A4_4011_F0A4;

/// What a stance adds to the rigs and hull forms it favours. Two thirds of
/// [`AT_HOME`]: enough that a Sleek hull usually carries a tall narrow rig,
/// not so much that a theme's own rig stops being its likeliest answer.
const STANCE_BONUS: u32 = 4;

/// The agreed transom hull's weight on every stance - twice a variant's
/// floor, so she stays the likeliest single answer everywhere and each
/// variant only overtakes her on the stance it suits.
const AGREED_HULL: u32 = 4;

/// The sloop's rig.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SloopRig {
    /// Gaff main and a single jib on a bowsprit - the rig she was drawn with,
    /// and the floor every theme can reach.
    Gaff,
    /// The same spars with two headsails: a staysail set inboard of the jib
    /// on its own stay.
    GaffCutter,
    /// A short mast with a yard hoisted nearly vertical up it - one triangle
    /// of sail whose head stands above the masthead.
    Gunter,
    /// A stumpy bermudan: one triangle on one spar.
    Bermuda,
    /// A gaff main with a square topsail crossed on a topmast above it.
    SquareTopsail,
}

impl SloopRig {
    pub const ALL: [Self; 5] = [
        Self::Gaff,
        Self::GaffCutter,
        Self::Gunter,
        Self::Bermuda,
        Self::SquareTopsail,
    ];

    /// The rig every theme can reach - see [`FLOOR`].
    pub const UNIVERSAL: Self = Self::Gaff;

    /// Human-readable display name. Public, like
    /// [`BoatType::label`](super::BoatType::label), because the readouts that
    /// print it are native-only.
    pub fn label(self) -> &'static str {
        match self {
            Self::Gaff => "Gaff",
            Self::GaffCutter => "Gaff cutter",
            Self::Gunter => "Gunter",
            Self::Bermuda => "Stumpy bermudan",
            Self::SquareTopsail => "Square topsail",
        }
    }

    /// Whether this rig is at home on `style` - the owner's table (#1366).
    fn at_home(self, style: ThemeArchetype) -> bool {
        match self {
            // The labouring rig: a gaff main is what working boats carried.
            Self::Gaff => holds(WORKING, style) || holds(GRUBBY, style),
            // The historically apt pirate craft, and the fighting and working
            // themes that would carry the extra headsail.
            Self::GaffCutter => {
                holds(BUCCANEER, style) || holds(MARTIAL, style) || holds(WORKING, style)
            }
            // A light modern-looking rig for the seaside and the neon moods.
            Self::Gunter => holds(COASTAL, style) || holds(NEON, style),
            // The yacht rig: the seaside and the stately moods.
            Self::Bermuda => holds(COASTAL, style) || holds(REGAL, style),
            Self::SquareTopsail => holds(HISTORIC, style),
        }
    }

    /// Whether `stance` favours this rig: a tall narrow rig on a long narrow
    /// hull, a low spread of canvas on a beamy one.
    fn suits(self, stance: VehicleStance) -> bool {
        match stance {
            VehicleStance::Sleek => matches!(self, Self::Gunter | Self::Bermuda),
            VehicleStance::Heavy => matches!(self, Self::Gaff | Self::SquareTopsail),
            VehicleStance::Compact => false,
        }
    }

    /// Sampling weight of this rig for an avatar of `style` on a hull of
    /// `stance` (`None` where the seed has no boat blueprint to read one off).
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

    /// The rig for a seed, weighted by the avatar's style and its hull's
    /// stance.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_style(AvatarCharacter::for_seed(seed).style, stance_of(seed), seed)
    }

    /// The rig for an already-resolved style and stance.
    pub fn for_style(style: ThemeArchetype, stance: Option<VehicleStance>, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_RIG_SALT);
        pick_weighted(&Self::ALL, |r| r.weight(style, stance), &mut rng).unwrap_or(Self::UNIVERSAL)
    }
}

/// The sloop's hull form - a different plan station list and nothing else.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SloopHull {
    /// A moderately raked stem and a real transom - the hull she was agreed
    /// on in #1363.
    Transom,
    /// Beam held full to within a twentieth of the stem: a deep forefoot and
    /// a near-vertical entry, a workboat's bow.
    PlumbStem,
    /// Beam falling away early: a cut-away forefoot under a long fine entry.
    RakedStem,
    /// A pointed stern - a double-ender.
    CanoeStern,
}

impl SloopHull {
    pub const ALL: [Self; 4] = [
        Self::Transom,
        Self::PlumbStem,
        Self::RakedStem,
        Self::CanoeStern,
    ];

    /// Human-readable display name.
    pub fn label(self) -> &'static str {
        match self {
            Self::Transom => "Transom stern",
            Self::PlumbStem => "Plumb stem",
            Self::RakedStem => "Raked stem",
            Self::CanoeStern => "Canoe stern",
        }
    }

    /// Sampling weight on a hull of `stance`. The agreed hull is the likeliest
    /// answer everywhere; each variant is favoured by the stance whose hull it
    /// suits - the full workboat bow on the beamy Heavy hull, the fine entry
    /// on the long Sleek one, and the double-ender on the short Compact one,
    /// where a transom would cut off what little length she has.
    pub fn weight(self, stance: Option<VehicleStance>) -> u32 {
        let suited = match self {
            Self::Transom => return AGREED_HULL,
            Self::PlumbStem => VehicleStance::Heavy,
            Self::RakedStem => VehicleStance::Sleek,
            Self::CanoeStern => VehicleStance::Compact,
        };
        FLOOR
            + if stance == Some(suited) {
                STANCE_BONUS
            } else {
                0
            }
    }

    /// The hull form for a seed.
    pub fn for_seed(seed: u64) -> Self {
        Self::for_stance(stance_of(seed), seed)
    }

    /// The hull form for an already-resolved stance.
    pub fn for_stance(stance: Option<VehicleStance>, seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(seed ^ AVATAR_HULL_FORM_SALT);
        pick_weighted(&Self::ALL, |h| h.weight(stance), &mut rng).unwrap_or(Self::Transom)
    }
}

/// The stance of a seed's boat hull, or `None` for a seed that is not a boat.
fn stance_of(seed: u64) -> Option<VehicleStance> {
    VehicleBlueprint::from_seed(seed).and_then(|b| b.boat().map(|b| b.stance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        for s in 0u64..50 {
            assert_eq!(SloopRig::for_seed(s), SloopRig::for_seed(s));
            assert_eq!(SloopHull::for_seed(s), SloopHull::for_seed(s));
        }
    }

    /// The floor keeps its promise: every theme on every stance can reach the
    /// gaff rig and the agreed hull.
    #[test]
    fn the_floor_is_reachable_everywhere() {
        let stances = [
            None,
            Some(VehicleStance::Compact),
            Some(VehicleStance::Sleek),
            Some(VehicleStance::Heavy),
        ];
        for style in ThemeArchetype::ALL {
            for stance in stances {
                assert!(SloopRig::UNIVERSAL.weight(style, stance) > 0);
                assert!(SloopHull::Transom.weight(stance) > 0);
            }
        }
    }

    /// Every rig is some theme's own answer, or it is a rig nobody sees.
    #[test]
    fn every_rig_is_at_home_somewhere() {
        for r in SloopRig::ALL {
            assert!(
                ThemeArchetype::ALL.iter().any(|&s| r.at_home(s)),
                "{r:?} is at home in no theme"
            );
        }
    }

    /// The sampler reaches every rig and every hull form over the boat seeds
    /// that actually exist.
    #[test]
    fn the_sampler_reaches_every_rig_and_every_hull() {
        let (mut rigs, mut hulls) = (Vec::new(), Vec::new());
        for s in 0u64..3000 {
            let Some(stance) = stance_of(s) else { continue };
            let style = AvatarCharacter::for_seed(s).style;
            let r = SloopRig::for_style(style, Some(stance), s);
            let h = SloopHull::for_stance(Some(stance), s);
            if !rigs.contains(&r) {
                rigs.push(r);
            }
            if !hulls.contains(&h) {
                hulls.push(h);
            }
        }
        assert_eq!(rigs.len(), SloopRig::ALL.len(), "reached only {rigs:?}");
        assert_eq!(hulls.len(), SloopHull::ALL.len(), "reached only {hulls:?}");
    }

    /// The stance bonus is live: on a theme where neither rig is at home, a
    /// Sleek hull draws the tall narrow rigs far more often than a Heavy one
    /// does. ModernCity is at home with none of the five, so only the floor
    /// and the stance speak.
    #[test]
    fn a_sleek_hull_favours_the_tall_narrow_rigs() {
        let style = ThemeArchetype::ModernCity;
        let tall = |stance| {
            (0u64..3000)
                .filter(|&s| {
                    matches!(
                        SloopRig::for_style(style, Some(stance), s),
                        SloopRig::Gunter | SloopRig::Bermuda
                    )
                })
                .count()
        };
        let (sleek, heavy) = (tall(VehicleStance::Sleek), tall(VehicleStance::Heavy));
        // Sleek: 8 of 10 weight is gunter or bermudan; Heavy: none of it.
        assert!(sleek > 2100, "only {sleek} of 3000 Sleek hulls went tall");
        assert_eq!(heavy, 0, "a Heavy ModernCity hull drew a tall rig");
    }

    /// The theme speaks: a historic theme's own rig dominates its population.
    #[test]
    fn a_historic_theme_crosses_a_square_topsail() {
        let style = ThemeArchetype::Mesoamerican;
        let n = (0u64..3000)
            .filter(|&s| SloopRig::for_style(style, None, s) == SloopRig::SquareTopsail)
            .count();
        // 6 of 8: at home, against the gaff floor's 2.
        assert!(
            (2100..=2400).contains(&n),
            "{n} of 3000, expected about 2250"
        );
    }
}
