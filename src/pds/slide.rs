//! A generator node's slide (#1935): a move along an axis every client
//! animates for itself, as it does a [`Spin`](super::spin::Spin), which
//! never touches the transform on record.
//!
//! The record says only HOW a part moves - a buoy bobs, a ball bounces, a
//! crate rides a conveyor - and each client works out the offset from the
//! wall clock, with the same clock and the same copy offset a spin reads, so
//! a part that swings and slides with one period peaks in both at once.
//! The wind term ([`SlideTerm::Wind`]) adds the room's own wind. Nothing
//! about the motion crosses the network.
//!
//! The axis is in the node's frame AFTER its authored rotation and before
//! its scale, as a spin's is: a node turned a quarter round slides along its
//! turned axis, and the offset is in metres whatever the node's own scale.
//! It is in the parent's frame, though, so an ancestor's scale stretches it
//! as it does the node's place: a part inside a sub-assembly scaled 0.1
//! slides a tenth as far.
//! Everything below the node moves with it - its children, its light, its
//! sound.
//!
//! A slide is visual only: a part that slides, and everything below it, is
//! spawned without solid colliders, as one that turns is. The runtime is
//! [`world_builder::spin`](crate::world_builder::spin), which writes a
//! node's turn, slide and swell together.

use serde::{Deserialize, Serialize};

use super::spin::{default_swing_period, is_direction};
use super::types::{Fp, Fp3};

/// How one node moves along `axis`: by the sum of its `terms`, in metres.
///
/// Default-eliding on the wire as a [`Spin`](super::spin::Spin) is: the
/// default axis (`+Y`, up) is left out, so a bobbing buoy is
/// `{"terms": [...]}`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Slide {
    /// The axis the node moves along, in its own frame after its authored
    /// rotation. Only its direction counts; the sanitiser holds each
    /// component in `[-1, 1]`, and a zero axis moves nothing.
    pub axis: Fp3,
    /// Summed into the node's offset - at most
    /// [`MAX_SLIDE_TERMS`](super::sanitize::limits::MAX_SLIDE_TERMS).
    pub terms: Vec<SlideTerm>,
}

impl Default for Slide {
    fn default() -> Self {
        Self {
            axis: Fp3([0.0, 1.0, 0.0]),
            terms: Vec::new(),
        }
    }
}

// The axis elides by its wire form (#1565), as a spin's does.
crate::pds::serde_util::impl_default_eliding_serialize!(Slide { axis (wire), terms });

impl Slide {
    /// A slide of one term along `axis`.
    pub fn along(axis: [f32; 3], term: SlideTerm) -> Self {
        Self {
            axis: Fp3(axis),
            terms: vec![term],
        }
    }

    /// Whether anything about this slide moves the node: a non-zero axis
    /// and at least one term that moves. A node whose slide does not is
    /// spawned exactly as one without a slide, colliders and all.
    pub fn moves(&self) -> bool {
        is_direction(self.axis) && self.terms.iter().any(SlideTerm::moves)
    }

    /// The nearest and the furthest along its axis the terms can carry the
    /// node (m): a swing or a wobble either side of its place, a hop from
    /// its place up to its height, a belt over its whole run, a wind term
    /// either way in the strongest wind (it slides downwind, and the wind
    /// may blow along the axis either way). A term that does not move adds
    /// nothing, as the runtime skips it; calming a slide only shortens it,
    /// so this holds at every Effects setting. What a box that holds a
    /// sliding part reads (the airships' `swept_bounds`).
    pub fn offset_range(&self) -> (f32, f32) {
        let (mut lo, mut hi) = (0.0_f32, 0.0_f32);
        for term in self.terms.iter().filter(|t| t.moves()) {
            let (near, far) = match *term {
                SlideTerm::Swing { amplitude, .. } | SlideTerm::Wobble { amplitude, .. } => {
                    (-amplitude.0.abs(), amplitude.0.abs())
                }
                SlideTerm::Bounce { height, .. } => (height.0.min(0.0), height.0.max(0.0)),
                SlideTerm::Belt { length, .. } => (0.0, length.0),
                SlideTerm::Wind { amplitude } => {
                    let gale = amplitude.0.abs() * super::sanitize::limits::MAX_WIND_SCALE;
                    (-gale, gale)
                }
                SlideTerm::Unknown => (0.0, 0.0),
            };
            if near.is_finite() && far.is_finite() {
                lo += near;
                hi += far;
            }
        }
        (lo, hi)
    }
}

/// One contribution to a node's offset (m), summed with its other terms.
///
/// An open union, like every `$type`-tagged record type: a term from a newer
/// engine decodes as [`Self::Unknown`], moves nothing, and - like every
/// `Unknown` arm (#1111) - stops this build saving the record rather than
/// dropping the term.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum SlideTerm {
    /// Move to and fro: `amplitude` metres either side of the authored
    /// place, one full swing every `period` seconds, starting `phase`
    /// degrees into it. A bobbing buoy, a piston, a lift.
    #[serde(rename = "network.symbios.slide.swing")]
    Swing {
        #[serde(default)]
        amplitude: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
        #[serde(default, skip_serializing_if = "Fp::is_wire_zero")]
        phase: Fp,
    },
    /// Wander to and fro without settling into a beat: up to `amplitude`
    /// metres either side, about once every `period` seconds - a spin's
    /// wobble, read off the same clock. A hovering drone, a floating mote.
    #[serde(rename = "network.symbios.slide.wobble")]
    Wobble {
        #[serde(default)]
        amplitude: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
    },
    /// Move along the axis in the room's wind - the one its trees sway in:
    /// up to `amplitude` metres when the wind blows squarely along the
    /// axis, gusting at the foliage's own pace, and further in a stronger
    /// wind, as a spin's wind term does. Downwind is positive: a wind
    /// blowing the other way along the axis moves the part back.
    #[serde(rename = "network.symbios.slide.wind")]
    Wind {
        #[serde(default)]
        amplitude: Fp,
    },
    /// Hop: up to `height` metres along the axis and back, once every
    /// `period` seconds, touching the authored place between hops - a
    /// bouncing ball, a hopping frog. Starts `phase` degrees into the hop
    /// (180 is a whole hop).
    #[serde(rename = "network.symbios.slide.bounce")]
    Bounce {
        #[serde(default)]
        height: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
        #[serde(default, skip_serializing_if = "Fp::is_wire_zero")]
        phase: Fp,
    },
    /// Ride a conveyor: `speed` metres a second along the axis (negative
    /// runs the other way), jumping back to the node's place every `length`
    /// metres, starting `start` metres along the run. Several parts at ONE
    /// place, each with the same belt and starts spaced evenly over the
    /// length, read as one endless stream; the jump back is hidden by
    /// putting the ends of the run inside something. (Parts spaced along
    /// the run with one start would all jump back at once, leaving the run
    /// half empty in between.) The jump back is one frame of very fast
    /// travel to anything that reads the part's motion: a roll, speed or
    /// lean term on a belt part or under one feels it, so keep them off
    /// belt parts.
    #[serde(rename = "network.symbios.slide.belt")]
    Belt {
        #[serde(default)]
        speed: Fp,
        #[serde(default = "default_belt_length")]
        length: Fp,
        #[serde(default, skip_serializing_if = "Fp::is_wire_zero")]
        start: Fp,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

impl SlideTerm {
    /// The kind names the editor offers, in its order, each with a fresh
    /// term of that kind.
    pub const KINDS: [&'static str; 5] = ["Swing", "Wobble", "Wind", "Bounce", "Belt"];

    /// A fresh term of the kind [`Self::label`] names, at values that show
    /// it working: a buoy's bob, a drone's hover, a hanging sign's drift, a
    /// ball's hop, a slow conveyor.
    pub fn fresh(kind: &str) -> Option<Self> {
        Some(match kind {
            "Swing" => Self::Swing {
                amplitude: Fp(0.3),
                period: Fp(4.0),
                phase: Fp::ZERO,
            },
            "Wobble" => Self::Wobble {
                amplitude: Fp(0.2),
                period: Fp(3.0),
            },
            "Wind" => Self::Wind { amplitude: Fp(0.2) },
            "Bounce" => Self::Bounce {
                height: Fp(0.5),
                period: Fp(1.0),
                phase: Fp::ZERO,
            },
            "Belt" => Self::Belt {
                speed: Fp(0.5),
                length: default_belt_length(),
                start: Fp::ZERO,
            },
            _ => return None,
        })
    }

    /// The kind's name, as [`Self::KINDS`] spells it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Swing { .. } => "Swing",
            Self::Wobble { .. } => "Wobble",
            Self::Wind { .. } => "Wind",
            Self::Bounce { .. } => "Bounce",
            Self::Belt { .. } => "Belt",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether this term can move anything: a zero distance, speed or
    /// period leaves the part in place, and an unknown term is skipped.
    pub fn moves(&self) -> bool {
        match self {
            Self::Swing {
                amplitude, period, ..
            }
            | Self::Wobble { amplitude, period } => amplitude.0 != 0.0 && period.0 > 0.0,
            Self::Wind { amplitude } => amplitude.0 != 0.0,
            Self::Bounce { height, period, .. } => height.0 != 0.0 && period.0 > 0.0,
            Self::Belt { speed, length, .. } => speed.0 != 0.0 && length.0 > 0.0,
            Self::Unknown => false,
        }
    }

    /// Whether the term reads the room's wind - and so the way its part
    /// stands in the world.
    pub fn reads_wind(&self) -> bool {
        matches!(self, Self::Wind { .. })
    }
}

fn default_belt_length() -> Fp {
    Fp(2.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(slide: &Slide) -> String {
        serde_json::to_string(slide).expect("serialise")
    }

    #[test]
    fn a_slide_along_up_leaves_its_axis_out() {
        let slide = Slide::along(
            [0.0, 0.999_999, 0.0],
            SlideTerm::Swing {
                amplitude: Fp(0.3),
                period: Fp(4.0),
                phase: Fp(0.000_01),
            },
        );
        assert_eq!(
            wire(&slide),
            r#"{"terms":[{"$type":"network.symbios.slide.swing","amplitude":3000,"period":40000}]}"#
        );
        let sideways = Slide::along([1.0, 0.0, 0.0], SlideTerm::fresh("Belt").expect("offered"));
        assert!(wire(&sideways).contains(r#""axis":[10000,0,0]"#));
    }

    #[test]
    fn every_slide_kind_round_trips_and_moves_when_fresh() {
        let slide = Slide {
            axis: Fp3([0.0, 0.0, -1.0]),
            terms: SlideTerm::KINDS
                .iter()
                .map(|k| SlideTerm::fresh(k).expect("a kind the editor offers"))
                .collect(),
        };
        let back: Slide = serde_json::from_str(&wire(&slide)).expect("decodes");
        assert_eq!(back, slide);
        for (term, kind) in back.terms.iter().zip(SlideTerm::KINDS) {
            assert_eq!(term.label(), kind);
            assert!(term.moves(), "a fresh {kind} shows itself working");
        }
        let phased = Slide::along(
            [0.0, 1.0, 0.0],
            SlideTerm::Bounce {
                height: Fp(0.5),
                period: Fp(1.0),
                phase: Fp(90.0),
            },
        );
        assert!(wire(&phased).contains(r#""phase":900000"#));
        let started = Slide::along(
            [1.0, 0.0, 0.0],
            SlideTerm::Belt {
                speed: Fp(0.5),
                length: Fp(4.0),
                start: Fp(1.0),
            },
        );
        assert!(wire(&started).contains(r#""start":10000"#));
        assert!(
            !wire(&Slide::along(
                [1.0, 0.0, 0.0],
                SlideTerm::fresh("Belt").expect("offered")
            ))
            .contains("start")
        );
        assert_eq!(
            serde_json::from_str::<Slide>(&wire(&started)).expect("decodes"),
            started
        );
        let back: Slide = serde_json::from_str(&wire(&phased)).expect("decodes");
        assert_eq!(back, phased);
    }

    #[test]
    fn a_slide_term_missing_its_fields_decodes_at_their_defaults() {
        let slide: Slide = serde_json::from_str(
            r#"{"terms":[{"$type":"network.symbios.slide.belt","speed":5000},
                         {"$type":"network.symbios.slide.bounce","height":2000}]}"#,
        )
        .expect("decodes");
        assert_eq!(slide.axis, Fp3([0.0, 1.0, 0.0]));
        assert_eq!(
            slide.terms,
            vec![
                SlideTerm::Belt {
                    speed: Fp(0.5),
                    length: Fp(2.0),
                    start: Fp::ZERO,
                },
                SlideTerm::Bounce {
                    height: Fp(0.2),
                    period: Fp(2.0),
                    phase: Fp::ZERO
                },
            ]
        );
    }

    #[test]
    fn a_slide_term_from_a_newer_engine_decodes_moves_nothing_and_cannot_be_written() {
        let slide: Slide = serde_json::from_str(
            r#"{"terms":[{"$type":"network.symbios.slide.orbit","radius":5000},
                         {"$type":"network.symbios.slide.wind","amplitude":1000}]}"#,
        )
        .expect("an open union tolerates a future term");
        assert_eq!(slide.terms[0], SlideTerm::Unknown);
        assert!(!slide.terms[0].moves());
        assert!(slide.moves(), "the known term still moves the part");
        // #1111: an arm this build cannot write back refuses the save.
        assert!(serde_json::to_string(&slide).is_err());
    }

    #[test]
    fn a_slide_moves_only_with_an_axis_and_a_moving_term() {
        let swing = |amplitude: f32, period: f32| SlideTerm::Swing {
            amplitude: Fp(amplitude),
            period: Fp(period),
            phase: Fp::ZERO,
        };
        assert!(!Slide::along([0.0; 3], swing(0.3, 2.0)).moves());
        assert!(!Slide::along([0.0, 1.0, 0.0], swing(0.0, 2.0)).moves());
        assert!(!Slide::along([0.0, 1.0, 0.0], swing(0.3, 0.0)).moves());
        let belt = |speed: f32, length: f32| SlideTerm::Belt {
            speed: Fp(speed),
            length: Fp(length),
            start: Fp::ZERO,
        };
        assert!(!Slide::along([1.0, 0.0, 0.0], belt(0.5, 0.0)).moves());
        assert!(Slide::along([1.0, 0.0, 0.0], belt(-0.5, 2.0)).moves());
        assert!(!Slide::default().moves(), "no terms, no move");
    }

    /// How far a slide can carry its node, each term at its furthest: a
    /// swing and a wobble either side, a hop one way, a belt over its run,
    /// a wind term either way in the strongest wind; a still term nothing.
    #[test]
    fn a_slides_range_is_every_terms_reach_summed() {
        let slide = |terms: Vec<SlideTerm>| Slide {
            axis: Fp3([0.0, 1.0, 0.0]),
            terms,
        };
        let swing = SlideTerm::Swing {
            amplitude: Fp(-0.3),
            period: Fp(2.0),
            phase: Fp(90.0),
        };
        assert_eq!(slide(vec![swing.clone()]).offset_range(), (-0.3, 0.3));
        let hop = SlideTerm::Bounce {
            height: Fp(-0.5),
            period: Fp(1.0),
            phase: Fp::ZERO,
        };
        let belt = SlideTerm::Belt {
            speed: Fp(-0.5),
            length: Fp(4.0),
            start: Fp(1.0),
        };
        let gust = SlideTerm::Wind { amplitude: Fp(0.1) };
        let (lo, hi) = slide(vec![swing, hop, belt, gust]).offset_range();
        let gale = 0.1 * crate::pds::sanitize::limits::MAX_WIND_SCALE;
        assert!((lo - (-0.3 - 0.5 - gale)).abs() < 1e-6, "{lo}");
        assert!((hi - (0.3 + 4.0 + gale)).abs() < 1e-6, "{hi}");
        let still = SlideTerm::Swing {
            amplitude: Fp(0.3),
            period: Fp::ZERO,
            phase: Fp::ZERO,
        };
        assert_eq!(
            slide(vec![still, SlideTerm::Unknown]).offset_range(),
            (0.0, 0.0)
        );
    }

    /// A node without a slide or swell writes exactly what it wrote before
    /// they existed - no key, not even a null - and a typical one costs only
    /// its own bytes: measured with the publish path's counter, not assumed
    /// (#1935).
    #[test]
    fn a_node_pays_for_a_slide_or_swell_only_when_it_carries_one() {
        use crate::pds::Generator;
        use crate::pds::record_size::serialized_record_bytes;
        use crate::pds::swell::{Swell, SwellTerm};
        let plain = Generator::default_cuboid();
        let before = serde_json::to_string(&plain).expect("writes");
        assert!(
            !before.contains("slide") && !before.contains("swell"),
            "{before}"
        );
        let plain_bytes = serialized_record_bytes(&plain).expect("counts");
        assert_eq!(plain_bytes, before.len());

        // A buoy's bob: 0.3 m every 4 s, the default axis.
        let mut buoy = plain.clone();
        buoy.slide = Some(Slide::along(
            [0.0, 1.0, 0.0],
            SlideTerm::fresh("Swing").expect("offered"),
        ));
        let slide = r#","slide":{"terms":[{"$type":"network.symbios.slide.swing","amplitude":3000,"period":40000}]}"#;
        assert_eq!(slide.len(), 92);
        assert_eq!(
            serialized_record_bytes(&buoy),
            Some(plain_bytes + slide.len())
        );
        assert!(
            serde_json::to_string(&buoy)
                .expect("writes")
                .contains(slide)
        );

        // A breath: a tenth either way every 2 s, even weights.
        let mut cap = plain.clone();
        cap.swell = Some(Swell::even(SwellTerm::fresh("Swing").expect("offered")));
        let swell = r#","swell":{"terms":[{"$type":"network.symbios.swell.swing","amount":1000,"period":20000}]}"#;
        assert_eq!(swell.len(), 89);
        assert_eq!(
            serialized_record_bytes(&cap),
            Some(plain_bytes + swell.len())
        );

        // Read back, both are what was written.
        let mut both = buoy.clone();
        both.swell = cap.swell.clone();
        let back: Generator =
            serde_json::from_str(&serde_json::to_string(&both).expect("writes")).expect("reads");
        assert_eq!(back, both);
    }
}
