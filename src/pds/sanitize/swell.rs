//! [`Swell`] sanitiser (#1935): every term clamped into what the editor
//! offers, each weight held in `[0, 1]`, the list capped.
//!
//! Nothing here quantises, as with a spin ([`super::spin`]). Unknown terms
//! are kept: a term from a newer engine refuses this build's save (#1111)
//! rather than vanishing from the owner's record.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::swell::{Swell, SwellTerm};
use crate::pds::types::{Fp, Fp3};

impl Sanitize for Swell {
    fn sanitize(&mut self) {
        // A non-finite weight takes no share: that axis keeps its size.
        self.weights = Fp3(self.weights.0.map(|v| clamp_finite(v, 0.0, 1.0, 0.0)));
        self.terms.truncate(limits::MAX_SWELL_TERMS);
        for term in &mut self.terms {
            term.sanitize();
        }
    }
}

impl Sanitize for SwellTerm {
    fn sanitize(&mut self) {
        let clamp = |v: &mut Fp, lo: f32, hi: f32, default: f32| {
            *v = Fp(clamp_finite(v.0, lo, hi, default));
        };
        let most = limits::MAX_SWELL_AMOUNT;
        let periods = (limits::MIN_SWING_PERIOD_S, limits::MAX_SWING_PERIOD_S);
        match self {
            SwellTerm::Swing {
                amount,
                period,
                phase,
            } => {
                clamp(amount, -most, most, 0.0);
                clamp(period, periods.0, periods.1, 2.0);
                let turn = limits::MAX_SWING_PHASE_DEG;
                clamp(phase, -turn, turn, 0.0);
            }
            SwellTerm::Wobble { amount, period } => {
                clamp(amount, -most, most, 0.0);
                clamp(period, periods.0, periods.1, 2.0);
            }
            SwellTerm::Wind { amount } => clamp(amount, -most, most, 0.0),
            SwellTerm::Unknown => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitized(mut swell: Swell) -> Swell {
        swell.sanitize();
        swell
    }

    #[test]
    fn every_swell_term_is_held_inside_what_the_editor_offers() {
        let swell = sanitized(Swell {
            weights: Fp3([f32::NAN, 3.0, -0.5]),
            keep_volume: true,
            terms: vec![
                SwellTerm::Swing {
                    amount: Fp(-5.0),
                    period: Fp(0.1),
                    phase: Fp(1e6),
                },
                SwellTerm::Wobble {
                    amount: Fp(f32::INFINITY),
                    period: Fp(f32::NAN),
                },
                SwellTerm::Wind { amount: Fp(2.0) },
            ],
        });
        assert_eq!(swell.weights, Fp3([0.0, 1.0, 0.0]));
        assert!(swell.keep_volume);
        let most = limits::MAX_SWELL_AMOUNT;
        assert_eq!(
            swell.terms,
            vec![
                SwellTerm::Swing {
                    amount: Fp(-most),
                    period: Fp(limits::MIN_SWING_PERIOD_S),
                    phase: Fp(limits::MAX_SWING_PHASE_DEG),
                },
                SwellTerm::Wobble {
                    amount: Fp::ZERO,
                    period: Fp(2.0),
                },
                SwellTerm::Wind { amount: Fp(most) },
            ]
        );
    }

    #[test]
    fn a_long_swell_keeps_its_first_terms_and_an_unknown_one_survives() {
        let mut terms = vec![SwellTerm::Unknown];
        terms.extend((1..10).map(|i| SwellTerm::Wind {
            amount: Fp(i as f32 / 100.0),
        }));
        let swell = sanitized(Swell {
            terms,
            ..Swell::default()
        });
        assert_eq!(swell.terms.len(), limits::MAX_SWELL_TERMS);
        assert_eq!(swell.terms[0], SwellTerm::Unknown);
        assert_eq!(swell.terms[3], SwellTerm::Wind { amount: Fp(0.03) });
    }

    #[test]
    fn a_sanitised_swell_is_unchanged_by_a_second_pass_and_by_the_wire() {
        let once = sanitized(Swell {
            weights: Fp3([0.25, 1.0, 0.5]),
            keep_volume: true,
            terms: SwellTerm::KINDS
                .iter()
                .map(|kind| SwellTerm::fresh(kind).expect("offered"))
                .collect(),
        });
        assert_eq!(sanitized(once.clone()), once);
        let saved = serde_json::to_string(&once).expect("serialise");
        let reloaded: Swell = serde_json::from_str(&saved).expect("decode");
        let resaved = serde_json::to_string(&sanitized(reloaded)).expect("serialise");
        assert_eq!(resaved, saved, "a reload saves the same bytes");
    }
}
