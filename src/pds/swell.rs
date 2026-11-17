//! A generator node's swell (#1935): a breathing of its size every client
//! animates for itself, as it does a [`Spin`](super::spin::Spin), which
//! never touches the transform on record.
//!
//! The terms sum to one amount, a share of the authored size; each of the
//! node's own axes grows by that amount times its weight, so a node with
//! the default weights grows evenly and one weighted `[0, 1, 0]` grows only
//! upward. With `keep_volume` the other axes give back what one takes, so
//! the part keeps its volume: weighted `[0, 1, 0]` it stretches up as it
//! squeezes in - squash and stretch - and with equal weights it does not
//! change at all, every axis giving back exactly what it took.
//!
//! Each client reads the wall clock, with the same clock and the same copy
//! offset a spin reads, so a part that swings and swells with one period
//! peaks in both at once. The wind term ([`SwellTerm::Wind`]) adds the
//! room's own wind. Everything below the node grows with it.
//!
//! A swell is visual only: a part that swells, and everything below it, is
//! spawned without solid colliders, as one that turns is. The runtime is
//! [`world_builder::spin`](crate::world_builder::spin), which writes a
//! node's turn, slide and swell together.

use serde::{Deserialize, Serialize};

use super::sanitize::limits;
use super::spin::default_swing_period;
use super::types::{FP_SCALE, Fp, Fp3, from_wire};

/// How one node's size breathes: by the sum of its `terms`, shared among
/// its axes by `weights`.
///
/// Default-eliding on the wire as a [`Spin`](super::spin::Spin) is: the
/// default weights (`[1, 1, 1]`, an even swell) and a `keep_volume` of
/// false are left out, so a breathing cap is `{"terms": [...]}`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Swell {
    /// How much of the amount each of the node's own axes (X, Y, Z) takes,
    /// each in `[0, 1]`: at an amount `s` an axis is `1 + s x weight` times
    /// its authored size.
    pub weights: Fp3,
    /// Whether the axes share one volume: every axis is divided by the cube
    /// root of the three factors' product, so a part that stretches one
    /// way squeezes the others. Equal weights then change nothing.
    pub keep_volume: bool,
    /// Summed into the node's amount - at most
    /// [`MAX_SWELL_TERMS`](super::sanitize::limits::MAX_SWELL_TERMS).
    pub terms: Vec<SwellTerm>,
}

impl Default for Swell {
    fn default() -> Self {
        Self {
            weights: Fp3([1.0, 1.0, 1.0]),
            keep_volume: false,
            terms: Vec::new(),
        }
    }
}

// The weights elide by their wire form (#1565), as a spin's axis does.
crate::pds::serde_util::impl_default_eliding_serialize!(Swell {
    weights (wire),
    keep_volume,
    terms
});

impl Swell {
    /// An even swell of one term.
    pub fn even(term: SwellTerm) -> Self {
        Self {
            terms: vec![term],
            ..Self::default()
        }
    }

    /// Whether anything about this swell changes the node's size: some
    /// weight, at least one term that moves, and - keeping the volume -
    /// weights that differ, for equal ones give back what they take. A node
    /// whose swell does not is spawned exactly as one without a swell,
    /// colliders and all.
    pub fn moves(&self) -> bool {
        let w = self.weights.0;
        let weighted = w.iter().any(|v| v.is_finite() && *v != 0.0);
        let even = w[0] == w[1] && w[1] == w[2];
        weighted && !(self.keep_volume && even) && self.terms.iter().any(SwellTerm::moves)
    }

    /// The weights as the runtime reads them: each held in `[0, 1]`, a
    /// non-finite one taking no share.
    pub fn held_weights(&self) -> [f32; 3] {
        self.weights.0.map(|w| {
            if w.is_finite() {
                w.clamp(0.0, 1.0)
            } else {
                0.0
            }
        })
    }

    /// The lowest and the highest summed amount the terms can reach: a
    /// swing or a wobble either side of nothing, a wind term from nothing in
    /// a still room to its amount times [`limits::MAX_WIND_SCALE`] in the
    /// strongest wind. A term that does not move adds nothing, as the
    /// runtime skips it.
    pub fn amount_range(&self) -> (f32, f32) {
        let (mut lo, mut hi) = (0.0_f32, 0.0_f32);
        for term in self.terms.iter().filter(|t| t.moves()) {
            match *term {
                SwellTerm::Swing { amount, .. } | SwellTerm::Wobble { amount, .. }
                    if amount.0.is_finite() =>
                {
                    lo -= amount.0.abs();
                    hi += amount.0.abs();
                }
                SwellTerm::Wind { amount } if amount.0.is_finite() => {
                    let gale = amount.0 * limits::MAX_WIND_SCALE;
                    lo += gale.min(0.0);
                    hi += gale.max(0.0);
                }
                _ => {}
            }
        }
        (lo, hi)
    }

    /// The largest factor any of the node's axes reaches on its authored
    /// size, at the worst moment the terms can line up: what the avatar
    /// scale cap multiplies into a body's accumulated scale (#1221 f327).
    /// One for a swell that changes nothing.
    ///
    /// The extremes of [`Self::amount_range`] are enough. Without
    /// `keep_volume` every axis is linear in the amount. Keeping it, the
    /// widest-weighted axis only grows with the amount and the narrowest
    /// only shrinks, and at any amount the largest factor is one of those
    /// two - the widest above nothing, the narrowest below - so the largest
    /// of all is at one end or the other. The runtime's floor is monotonic
    /// too, so it keeps that so.
    pub fn peak_factor(&self) -> f32 {
        if !self.moves() {
            return 1.0;
        }
        let weights = self.held_weights();
        let (lo, hi) = self.amount_range();
        [lo, 0.0, hi]
            .into_iter()
            .flat_map(|amount| axis_factors(weights, self.keep_volume, amount))
            .fold(1.0, f32::max)
    }

    /// Scale the amounts down until [`Self::peak_factor`] is at most `most`,
    /// all the way to nothing when `most` leaves no room for any swell.
    ///
    /// Each amount lands on the wire's grid, rounded toward nothing, so the
    /// record saves and reloads to the same bytes and a reload never rounds
    /// one back over the bound. The peak only grows with the share kept, so
    /// halving the share converges on the largest that fits.
    pub fn fit_peak(&mut self, most: f32) {
        if self.peak_factor() <= most {
            return;
        }
        let kept = |share: f32| {
            let mut fitted = self.clone();
            fitted.scale_amounts(share);
            fitted
        };
        let (mut fits, mut over) = (0.0_f32, 1.0_f32);
        for _ in 0..32 {
            let mid = 0.5 * (fits + over);
            if kept(mid).peak_factor() <= most {
                fits = mid;
            } else {
                over = mid;
            }
        }
        self.scale_amounts(fits);
    }

    /// Every term's amount times `share`, on the wire's grid rounded toward
    /// nothing.
    fn scale_amounts(&mut self, share: f32) {
        let scaled = |amount: &mut Fp| {
            let wire = (f64::from(amount.0 * share) * f64::from(FP_SCALE)).trunc();
            *amount = Fp(from_wire(wire as i64));
        };
        for term in &mut self.terms {
            match term {
                SwellTerm::Swing { amount, .. }
                | SwellTerm::Wobble { amount, .. }
                | SwellTerm::Wind { amount } => scaled(amount),
                SwellTerm::Unknown => {}
            }
        }
    }
}

/// Each of a node's axes' factor on its authored size at swell `amount`,
/// for `weights` held as [`Swell::held_weights`] holds them: `1 + amount x
/// weight`, the amount held so no weighted axis falls under
/// [`limits::MIN_SWELL_FACTOR`], and - keeping the volume - divided by the
/// cube root of the three factors' product. The runtime draws by this, and
/// [`Swell::peak_factor`] bounds by it, so the bound is the drawing's own.
pub fn axis_factors(weights: [f32; 3], keep_volume: bool, amount: f32) -> [f32; 3] {
    let widest = weights[0].max(weights[1]).max(weights[2]);
    let amount = if !amount.is_finite() {
        0.0
    } else if widest > 0.0 {
        amount.max(-(1.0 - limits::MIN_SWELL_FACTOR) / widest)
    } else {
        amount
    };
    let factors = weights.map(|w| 1.0 + w * amount);
    if keep_volume {
        let volume = (factors[0] * factors[1] * factors[2]).cbrt();
        factors.map(|f| f / volume)
    } else {
        factors
    }
}

/// One contribution to a node's amount (a share of its authored size),
/// summed with its other terms.
///
/// An open union, like every `$type`-tagged record type: a term from a newer
/// engine decodes as [`Self::Unknown`], changes nothing, and - like every
/// `Unknown` arm (#1111) - stops this build saving the record rather than
/// dropping the term.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum SwellTerm {
    /// Breathe in and out: `amount` either side of the authored size, one
    /// full breath every `period` seconds, starting `phase` degrees into
    /// it. A beating heart, a pulsing mushroom cap.
    #[serde(rename = "network.symbios.swell.swing")]
    Swing {
        #[serde(default)]
        amount: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
        #[serde(default, skip_serializing_if = "Fp::is_wire_zero")]
        phase: Fp,
    },
    /// Breathe without settling into a beat: up to `amount` either side,
    /// about once every `period` seconds - a spin's wobble, read off the
    /// same clock. A jelly, a flame's glow.
    #[serde(rename = "network.symbios.swell.wobble")]
    Wobble {
        #[serde(default)]
        amount: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
    },
    /// Fill in the room's wind - the one its trees sway in: up to `amount`
    /// as the wind gusts at the foliage's own pace, and further in a
    /// stronger wind, whichever way it blows. A sail, a windsock, a
    /// balloon.
    #[serde(rename = "network.symbios.swell.wind")]
    Wind {
        #[serde(default)]
        amount: Fp,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

impl SwellTerm {
    /// The kind names the editor offers, in its order, each with a fresh
    /// term of that kind.
    pub const KINDS: [&'static str; 3] = ["Swing", "Wobble", "Wind"];

    /// A fresh term of the kind [`Self::label`] names, at values that show
    /// it working: a slow breath, a jelly's wobble, a sail's fill.
    pub fn fresh(kind: &str) -> Option<Self> {
        Some(match kind {
            "Swing" => Self::Swing {
                amount: Fp(0.1),
                period: Fp(default_swing_period().0),
                phase: Fp::ZERO,
            },
            "Wobble" => Self::Wobble {
                amount: Fp(0.08),
                period: Fp(3.0),
            },
            "Wind" => Self::Wind { amount: Fp(0.1) },
            _ => return None,
        })
    }

    /// The kind's name, as [`Self::KINDS`] spells it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Swing { .. } => "Swing",
            Self::Wobble { .. } => "Wobble",
            Self::Wind { .. } => "Wind",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether this term can change anything: a zero amount or period
    /// leaves the part as it is, and an unknown term is skipped.
    pub fn moves(&self) -> bool {
        match self {
            Self::Swing { amount, period, .. } | Self::Wobble { amount, period } => {
                amount.0 != 0.0 && period.0 > 0.0
            }
            Self::Wind { amount } => amount.0 != 0.0,
            Self::Unknown => false,
        }
    }

    /// Whether the term reads the room's wind.
    pub fn reads_wind(&self) -> bool {
        matches!(self, Self::Wind { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(swell: &Swell) -> String {
        serde_json::to_string(swell).expect("serialise")
    }

    #[test]
    fn an_even_swell_leaves_its_weights_and_keep_volume_out() {
        let swell = Swell {
            // A hair off one each, written as the default's integers.
            weights: Fp3([0.999_999, 1.0, 1.000_001]),
            keep_volume: false,
            terms: vec![SwellTerm::fresh("Swing").expect("offered")],
        };
        assert_eq!(
            wire(&swell),
            r#"{"terms":[{"$type":"network.symbios.swell.swing","amount":1000,"period":20000}]}"#
        );
        let squash = Swell {
            weights: Fp3([0.0, 1.0, 0.0]),
            keep_volume: true,
            ..swell
        };
        let text = wire(&squash);
        assert!(text.contains(r#""weights":[0,10000,0]"#), "{text}");
        assert!(text.contains(r#""keep_volume":true"#), "{text}");
    }

    #[test]
    fn every_swell_kind_round_trips_and_moves_when_fresh() {
        let swell = Swell {
            weights: Fp3([0.5, 1.0, 0.25]),
            keep_volume: true,
            terms: SwellTerm::KINDS
                .iter()
                .map(|k| SwellTerm::fresh(k).expect("a kind the editor offers"))
                .collect(),
        };
        let back: Swell = serde_json::from_str(&wire(&swell)).expect("decodes");
        assert_eq!(back, swell);
        for (term, kind) in back.terms.iter().zip(SwellTerm::KINDS) {
            assert_eq!(term.label(), kind);
            assert!(term.moves(), "a fresh {kind} shows itself working");
        }
        let phased = Swell::even(SwellTerm::Swing {
            amount: Fp(0.2),
            period: Fp(3.0),
            phase: Fp(-45.0),
        });
        assert!(wire(&phased).contains(r#""phase":-450000"#));
        assert_eq!(
            serde_json::from_str::<Swell>(&wire(&phased)).expect("decodes"),
            phased
        );
    }

    #[test]
    fn a_swell_term_from_a_newer_engine_decodes_changes_nothing_and_cannot_be_written() {
        let swell: Swell = serde_json::from_str(
            r#"{"terms":[{"$type":"network.symbios.swell.beat","bpm":600000}]}"#,
        )
        .expect("an open union tolerates a future term");
        assert_eq!(swell.terms, vec![SwellTerm::Unknown]);
        assert!(!swell.moves());
        // #1111: an arm this build cannot write back refuses the save.
        assert!(serde_json::to_string(&swell).is_err());
    }

    fn swing(amount: f32) -> SwellTerm {
        SwellTerm::Swing {
            amount: Fp(amount),
            period: Fp(2.0),
            phase: Fp::ZERO,
        }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    /// The worst moment each way a swell can grow a part: summed swings at
    /// their tops, a wind term in the strongest wind, a kept volume that
    /// grows the axes it does not weight as the weighted one shrinks to the
    /// runtime's floor.
    #[test]
    fn a_swells_peak_is_its_largest_factor_at_its_extremes() {
        let even = |terms: Vec<SwellTerm>| Swell {
            terms,
            ..Swell::default()
        };
        // Four deep swings: 1 + 3.6.
        assert!(close(even(vec![swing(0.9); 4]).peak_factor(), 4.6));
        // A wind term counts at the strongest wind; one that shrinks never
        // grows the part.
        let gale = SwellTerm::Wind { amount: Fp(0.9) };
        assert!(close(
            even(vec![gale.clone()]).peak_factor(),
            1.0 + 0.9 * limits::MAX_WIND_SCALE
        ));
        assert!(close(
            even(vec![SwellTerm::Wind { amount: Fp(-0.9) }]).peak_factor(),
            1.0
        ));
        // A negative swing grows as far as a positive one.
        assert!(close(even(vec![swing(-0.4)]).peak_factor(), 1.4));
        // Squash and stretch at the floor: the weighted axis at a tenth, the
        // other two at 1 / cbrt(0.1).
        let squash = Swell {
            weights: Fp3([0.0, 1.0, 0.0]),
            keep_volume: true,
            terms: vec![swing(0.9)],
        };
        assert!(
            close(squash.peak_factor(), 1.0 / 0.1_f32.cbrt()),
            "{}",
            squash.peak_factor()
        );
        // A middle weight under a kept volume grows both ways from nothing;
        // a dense sweep of the range finds nothing past the ends.
        let mixed = Swell {
            weights: Fp3([0.0, 0.5, 1.0]),
            keep_volume: true,
            terms: vec![swing(0.7), SwellTerm::Wind { amount: Fp(0.3) }],
        };
        let (lo, hi) = mixed.amount_range();
        assert!(close(lo, -0.7) && close(hi, 0.7 + 0.3 * limits::MAX_WIND_SCALE));
        let swept = (0..=2000)
            .map(|k| lo + (hi - lo) * k as f32 / 2000.0)
            .flat_map(|a| axis_factors(mixed.held_weights(), true, a))
            .fold(1.0, f32::max);
        assert!(swept <= mixed.peak_factor() + 1e-5, "{swept}");
        assert!(close(swept, mixed.peak_factor()));
        // A swell that changes nothing has no peak.
        assert_eq!(Swell::default().peak_factor(), 1.0);
        let even_kept = Swell {
            keep_volume: true,
            ..even(vec![swing(0.9)])
        };
        assert_eq!(even_kept.peak_factor(), 1.0);
    }

    #[test]
    fn fitting_a_peak_scales_the_amounts_onto_the_grid_or_to_nothing() {
        let mut deep = Swell {
            terms: vec![swing(0.9); 4],
            ..Swell::default()
        };
        deep.fit_peak(3.0);
        let peak = deep.peak_factor();
        assert!(peak <= 3.0 && peak > 2.99, "{peak}");
        for term in &deep.terms {
            let SwellTerm::Swing { amount, .. } = term else {
                panic!("still swings");
            };
            assert_eq!(amount.on_wire(), *amount, "on the grid");
        }
        let mut fits = Swell::even(swing(0.2));
        let before = fits.clone();
        fits.fit_peak(1.2);
        assert_eq!(fits, before, "one that fits is left alone");
        let mut no_room = Swell::even(swing(0.2));
        no_room.fit_peak(1.0);
        assert!(!no_room.moves(), "{no_room:?}");
    }

    #[test]
    fn a_swell_moves_only_with_a_weight_a_moving_term_and_uneven_kept_volume() {
        let breath = || SwellTerm::fresh("Swing").expect("offered");
        assert!(Swell::even(breath()).moves());
        assert!(!Swell::default().moves(), "no terms, no swell");
        let weighted = |weights: [f32; 3], keep_volume: bool| Swell {
            weights: Fp3(weights),
            keep_volume,
            terms: vec![breath()],
        };
        assert!(!weighted([0.0; 3], false).moves());
        assert!(
            !weighted([0.5; 3], true).moves(),
            "equal weights give back what they take"
        );
        assert!(weighted([0.0, 1.0, 0.0], true).moves());
        assert!(
            !Swell::even(SwellTerm::Swing {
                amount: Fp::ZERO,
                period: Fp(2.0),
                phase: Fp::ZERO
            })
            .moves()
        );
    }
}
