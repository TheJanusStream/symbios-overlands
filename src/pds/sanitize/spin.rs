//! [`Spin`] sanitiser (#1604): every term clamped into what the editor
//! offers, the axis held finite, the list capped.
//!
//! Nothing here quantises or renormalises. A spin is read back exactly as it
//! is written, and the runtime normalises the axis itself, so a direction
//! the editor saved reloads unchanged (the #1565 rule a rotation follows).
//! Unknown terms are kept: a term from a newer engine refuses this build's
//! save (#1111) rather than vanishing from the owner's record.

use super::Sanitize;
use super::common::clamp_finite;
use super::limits;
use crate::pds::spin::{Spin, SpinTerm};
use crate::pds::types::{Fp, Fp3};

impl Sanitize for Spin {
    fn sanitize(&mut self) {
        // Only the direction counts, so a component past one is clamped
        // rather than the vector scaled: the editor writes unit axes, and
        // only a hand-written record can reach the clamp at all.
        self.axis = Fp3(self.axis.0.map(|v| clamp_finite(v, -1.0, 1.0, 0.0)));
        self.terms.truncate(limits::MAX_SPIN_TERMS);
        for term in &mut self.terms {
            term.sanitize();
        }
    }
}

impl Sanitize for SpinTerm {
    fn sanitize(&mut self) {
        let clamp = |v: &mut Fp, lo: f32, hi: f32, default: f32| {
            *v = Fp(clamp_finite(v.0, lo, hi, default));
        };
        match self {
            SpinTerm::Constant { rate } => {
                clamp(
                    rate,
                    -limits::MAX_SPIN_RATE_DEG,
                    limits::MAX_SPIN_RATE_DEG,
                    0.0,
                );
            }
            SpinTerm::Swing {
                amplitude,
                period,
                phase,
            } => {
                let reach = limits::MAX_SWING_AMPLITUDE_DEG;
                clamp(amplitude, -reach, reach, 0.0);
                clamp(
                    period,
                    limits::MIN_SWING_PERIOD_S,
                    limits::MAX_SWING_PERIOD_S,
                    2.0,
                );
                let turn = limits::MAX_SWING_PHASE_DEG;
                clamp(phase, -turn, turn, 0.0);
            }
            SpinTerm::Roll { radius } => {
                clamp(
                    radius,
                    limits::MIN_ROLL_RADIUS_M,
                    limits::MAX_ROLL_RADIUS_M,
                    0.5,
                );
            }
            SpinTerm::Steer { gain, limit } => {
                clamp(gain, -limits::MAX_STEER_GAIN, limits::MAX_STEER_GAIN, 0.0);
                clamp(limit, 0.0, limits::MAX_STEER_LIMIT_DEG, 30.0);
            }
            SpinTerm::Unknown => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitized(mut spin: Spin) -> Spin {
        spin.sanitize();
        spin
    }

    #[test]
    fn every_term_is_held_inside_what_the_editor_offers() {
        let spin = sanitized(Spin {
            axis: Fp3([f32::NAN, 3.0, -0.5]),
            terms: vec![
                SpinTerm::Constant { rate: Fp(1e9) },
                SpinTerm::Swing {
                    amplitude: Fp(-720.0),
                    period: Fp(0.0),
                    phase: Fp(f32::INFINITY),
                },
                SpinTerm::Roll { radius: Fp(0.0) },
                SpinTerm::Steer {
                    gain: Fp(f32::NAN),
                    limit: Fp(400.0),
                },
            ],
        });
        assert_eq!(spin.axis, Fp3([0.0, 1.0, -0.5]));
        assert_eq!(
            spin.terms,
            vec![
                SpinTerm::Constant {
                    rate: Fp(limits::MAX_SPIN_RATE_DEG)
                },
                SpinTerm::Swing {
                    amplitude: Fp(-limits::MAX_SWING_AMPLITUDE_DEG),
                    period: Fp(limits::MIN_SWING_PERIOD_S),
                    phase: Fp::ZERO,
                },
                SpinTerm::Roll {
                    radius: Fp(limits::MIN_ROLL_RADIUS_M)
                },
                SpinTerm::Steer {
                    gain: Fp::ZERO,
                    limit: Fp(limits::MAX_STEER_LIMIT_DEG),
                },
            ]
        );
    }

    #[test]
    fn a_long_list_keeps_its_first_terms_and_an_unknown_one_survives() {
        let mut terms = vec![SpinTerm::Unknown];
        terms.extend((1..10).map(|i| SpinTerm::Constant { rate: Fp(i as f32) }));
        let spin = sanitized(Spin {
            axis: Fp3([0.0, 1.0, 0.0]),
            terms,
        });
        assert_eq!(spin.terms.len(), limits::MAX_SPIN_TERMS);
        assert_eq!(spin.terms[0], SpinTerm::Unknown);
        assert_eq!(spin.terms[3], SpinTerm::Constant { rate: Fp(3.0) });
    }

    #[test]
    fn a_sanitised_spin_is_unchanged_by_a_second_pass_and_by_the_wire() {
        let once = sanitized(Spin {
            axis: Fp3([0.577_35, -0.577_35, 0.577_35]),
            terms: SpinTerm::KINDS
                .iter()
                .map(|kind| SpinTerm::fresh(kind).expect("offered"))
                .collect(),
        });
        assert_eq!(sanitized(once.clone()), once);
        let saved = serde_json::to_string(&once).expect("serialise");
        let reloaded: Spin = serde_json::from_str(&saved).expect("decode");
        let resaved = serde_json::to_string(&sanitized(reloaded)).expect("serialise");
        assert_eq!(resaved, saved, "a reload saves the same bytes");
    }
}
