//! [`Slide`] sanitiser (#1935): every term clamped into what the editor
//! offers, the axis held finite, the list capped - and every distance held
//! to the tree's own ceiling on a part's size, so a part on an avatar never
//! travels further than a part of one may be long.
//!
//! Nothing here quantises or renormalises, as with a spin
//! ([`super::spin`]): the runtime normalises the axis itself, so a
//! direction the editor saved reloads unchanged. Unknown terms are kept: a
//! term from a newer engine refuses this build's save (#1111) rather than
//! vanishing from the owner's record.

use super::common::clamp_finite;
use super::limits;
use crate::pds::slide::{Slide, SlideTerm};
use crate::pds::types::{Fp, Fp3, to_wire};

/// Clamp `slide` in place, its distances to `max_dim` metres either way -
/// the tree's ceiling on a part's size ([`limits::MAX_PRIM_DIM_M`] in a
/// room, [`limits::MAX_AVATAR_PRIM_DIM_M`] on an avatar).
pub(super) fn sanitize_slide(slide: &mut Slide, max_dim: f32) {
    // Only the direction counts, as with a spin's axis: clamped, never
    // scaled.
    slide.axis = Fp3(slide.axis.0.map(|v| clamp_finite(v, -1.0, 1.0, 0.0)));
    slide.terms.truncate(limits::MAX_SLIDE_TERMS);
    for term in &mut slide.terms {
        sanitize_term(term, max_dim);
    }
}

fn sanitize_term(term: &mut SlideTerm, max_dim: f32) {
    let clamp = |v: &mut Fp, lo: f32, hi: f32, default: f32| {
        *v = Fp(clamp_finite(v.0, lo, hi, default));
    };
    let period = |v: &mut Fp| {
        clamp(
            v,
            limits::MIN_SWING_PERIOD_S,
            limits::MAX_SWING_PERIOD_S,
            2.0,
        );
    };
    let phase = |v: &mut Fp| {
        let turn = limits::MAX_SWING_PHASE_DEG;
        clamp(v, -turn, turn, 0.0);
    };
    match term {
        SlideTerm::Swing {
            amplitude,
            period: p,
            phase: f,
        } => {
            clamp(amplitude, -max_dim, max_dim, 0.0);
            period(p);
            phase(f);
        }
        SlideTerm::Wobble {
            amplitude,
            period: p,
        } => {
            clamp(amplitude, -max_dim, max_dim, 0.0);
            period(p);
        }
        SlideTerm::Wind { amplitude } => clamp(amplitude, -max_dim, max_dim, 0.0),
        SlideTerm::Bounce {
            height,
            period: p,
            phase: f,
        } => {
            clamp(height, -max_dim, max_dim, 0.0);
            period(p);
            phase(f);
        }
        SlideTerm::Belt {
            speed,
            length,
            start,
        } => {
            let fastest = limits::MAX_BELT_SPEED_M_S;
            clamp(speed, -fastest, fastest, 0.0);
            clamp(length, 0.0, max_dim, 0.0);
            // The runtime wraps it into the length, so it is wrapped here
            // the same way - a hand-written -1 m on a 4 m run starts 3 m
            // along, not at the place - and a start a hair short of the
            // length, which the wire would round onto it, is the place.
            // Without a length the belt moves nothing, and the start is
            // only held finite and in range.
            if length.0 > 0.0 {
                clamp(start, f32::MIN, f32::MAX, 0.0);
                let wrapped = start.0.rem_euclid(length.0);
                *start = Fp(if to_wire(wrapped) >= to_wire(length.0) {
                    0.0
                } else {
                    wrapped
                });
            } else {
                clamp(start, 0.0, max_dim, 0.0);
            }
        }
        SlideTerm::Unknown => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sanitized(mut slide: Slide, max_dim: f32) -> Slide {
        sanitize_slide(&mut slide, max_dim);
        slide
    }

    #[test]
    fn every_slide_term_is_held_inside_what_the_editor_offers() {
        let room = limits::MAX_PRIM_DIM_M;
        let slide = sanitized(
            Slide {
                axis: Fp3([f32::NAN, 3.0, -0.5]),
                terms: vec![
                    SlideTerm::Swing {
                        amplitude: Fp(-1e6),
                        period: Fp(0.0),
                        phase: Fp(f32::INFINITY),
                    },
                    SlideTerm::Wobble {
                        amplitude: Fp(f32::NAN),
                        period: Fp(1e9),
                    },
                    SlideTerm::Bounce {
                        height: Fp(500.0),
                        period: Fp(-3.0),
                        phase: Fp(-720.0),
                    },
                    SlideTerm::Belt {
                        speed: Fp(-1e4),
                        length: Fp(-2.0),
                        start: Fp(1e9),
                    },
                ],
            },
            room,
        );
        assert_eq!(slide.axis, Fp3([0.0, 1.0, -0.5]));
        assert_eq!(
            slide.terms,
            vec![
                SlideTerm::Swing {
                    amplitude: Fp(-room),
                    period: Fp(limits::MIN_SWING_PERIOD_S),
                    phase: Fp::ZERO,
                },
                SlideTerm::Wobble {
                    amplitude: Fp::ZERO,
                    period: Fp(limits::MAX_SWING_PERIOD_S),
                },
                SlideTerm::Bounce {
                    height: Fp(room),
                    period: Fp(limits::MIN_SWING_PERIOD_S),
                    phase: Fp(-limits::MAX_SWING_PHASE_DEG),
                },
                SlideTerm::Belt {
                    speed: Fp(-limits::MAX_BELT_SPEED_M_S),
                    length: Fp::ZERO,
                    start: Fp(room),
                },
            ]
        );
        let wind = sanitized(
            Slide::along([0.0, 1.0, 0.0], SlideTerm::Wind { amplitude: Fp(1e9) }),
            room,
        );
        assert_eq!(
            wind.terms,
            vec![SlideTerm::Wind {
                amplitude: Fp(room)
            }]
        );
    }

    /// An avatar's ceiling is its own: the same record clamps tighter on a
    /// body than in a room.
    #[test]
    fn an_avatars_slide_travels_no_further_than_its_parts_may_be_long() {
        let far = || Slide {
            axis: Fp3([0.0, 1.0, 0.0]),
            terms: vec![
                SlideTerm::Swing {
                    amplitude: Fp(60.0),
                    period: Fp(2.0),
                    phase: Fp::ZERO,
                },
                SlideTerm::Belt {
                    speed: Fp(1.0),
                    length: Fp(60.0),
                    start: Fp(30.0),
                },
            ],
        };
        let body = sanitized(far(), limits::MAX_AVATAR_PRIM_DIM_M);
        let room = sanitized(far(), limits::MAX_PRIM_DIM_M);
        assert_eq!(
            body.terms[0],
            SlideTerm::Swing {
                amplitude: Fp(limits::MAX_AVATAR_PRIM_DIM_M),
                period: Fp(2.0),
                phase: Fp::ZERO,
            }
        );
        assert_eq!(
            body.terms[1],
            SlideTerm::Belt {
                speed: Fp(1.0),
                length: Fp(limits::MAX_AVATAR_PRIM_DIM_M),
                // 30 m along a run cut to 16 m is 14 m along it, as the
                // runtime would have read the 30.
                start: Fp(30.0_f32.rem_euclid(limits::MAX_AVATAR_PRIM_DIM_M)),
            }
        );
        assert_eq!(room, far(), "60 m is inside a room's ceiling");
    }

    /// A belt's start means where along the run it starts, wrapped as the
    /// runtime wraps it: a negative one counts back from the far end.
    #[test]
    fn a_belts_start_is_wrapped_into_its_run_as_the_runtime_reads_it() {
        let start_of = |start: f32, length: f32| {
            let slide = sanitized(
                Slide::along(
                    [1.0, 0.0, 0.0],
                    SlideTerm::Belt {
                        speed: Fp(0.5),
                        length: Fp(length),
                        start: Fp(start),
                    },
                ),
                limits::MAX_PRIM_DIM_M,
            );
            let SlideTerm::Belt { start, .. } = slide.terms[0] else {
                panic!("still a belt");
            };
            start.0
        };
        assert_eq!(start_of(-1.0, 4.0), 3.0, "a metre back from the far end");
        assert_eq!(start_of(5.0, 4.0), 1.0);
        assert_eq!(start_of(2.5, 4.0), 2.5, "inside the run, left alone");
        assert_eq!(start_of(4.0, 4.0), 0.0, "the far end is the place");
        assert_eq!(start_of(-1e-6, 4.0), 0.0, "a hair short of the end is too");
        assert_eq!(start_of(f32::NAN, 4.0), 0.0);
        assert_eq!(start_of(-3.0, 0.0), 0.0, "no run, no wrap");
    }

    #[test]
    fn a_long_slide_keeps_its_first_terms_and_an_unknown_one_survives() {
        let mut terms = vec![SlideTerm::Unknown];
        terms.extend((1..10).map(|i| SlideTerm::Wind {
            amplitude: Fp(i as f32 / 10.0),
        }));
        let slide = sanitized(
            Slide {
                axis: Fp3([0.0, 1.0, 0.0]),
                terms,
            },
            limits::MAX_PRIM_DIM_M,
        );
        assert_eq!(slide.terms.len(), limits::MAX_SLIDE_TERMS);
        assert_eq!(slide.terms[0], SlideTerm::Unknown);
        assert_eq!(slide.terms[3], SlideTerm::Wind { amplitude: Fp(0.3) });
    }

    #[test]
    fn a_sanitised_slide_is_unchanged_by_a_second_pass_and_by_the_wire() {
        let once = sanitized(
            Slide {
                axis: Fp3([0.6, -0.48, 0.64]),
                terms: SlideTerm::KINDS[..limits::MAX_SLIDE_TERMS]
                    .iter()
                    .map(|kind| SlideTerm::fresh(kind).expect("offered"))
                    .collect(),
            },
            limits::MAX_PRIM_DIM_M,
        );
        assert_eq!(sanitized(once.clone(), limits::MAX_PRIM_DIM_M), once);
        let saved = serde_json::to_string(&once).expect("serialise");
        let reloaded: Slide = serde_json::from_str(&saved).expect("decode");
        let resaved =
            serde_json::to_string(&sanitized(reloaded, limits::MAX_PRIM_DIM_M)).expect("serialise");
        assert_eq!(resaved, saved, "a reload saves the same bytes");
    }
}
