//! A generator node's spin (#1604): a turn every client animates for itself,
//! about an axis in the node's own frame, which never touches the transform
//! on record.
//!
//! The record says only HOW a part moves - a windmill's sails turn at a
//! steady rate, a pendulum swings, a wheel rolls with the vehicle carrying
//! it - and each client works out the angle. Nothing about the motion
//! crosses the network, so two clients may show a rolling wheel at slightly
//! different angles; the clock terms ([`SpinTerm::Constant`],
//! [`SpinTerm::Swing`], [`SpinTerm::Wobble`]) and the wind terms
//! ([`SpinTerm::Wind`], [`SpinTerm::Vane`]), which add the room's own wind,
//! read the wall clock, so clients agree on them as closely as their clocks
//! agree.
//!
//! The node turns about its own origin, and everything below it turns with
//! it, so compound motion is built by nesting: a steering node holding a
//! rolling one is a front wheel. The axis is in the node's frame AFTER its
//! authored rotation and before its scale, which turns a scaled part
//! rigidly, as a whole.
//!
//! A spin is visual only: a part that turns, and everything below it, is
//! spawned without solid colliders (a portal's or a gateway's trigger keeps
//! working). The runtime is
//! [`world_builder::spin`](crate::world_builder::spin).

use serde::{Deserialize, Serialize};

use super::types::{Fp, Fp3};

/// How one node turns: about `axis`, by the sum of its `terms`.
///
/// Default-eliding on the wire like [`TransformData`](super::TransformData):
/// the default axis (`+Y`, up) is left out, so a turntable is
/// `{"terms": [...]}`.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Spin {
    /// The axis the node turns about, in its own frame. Only its direction
    /// counts; the sanitiser holds each component in `[-1, 1]`, and a zero
    /// axis turns nothing.
    pub axis: Fp3,
    /// Summed into the node's angle - at most
    /// [`MAX_SPIN_TERMS`](super::sanitize::limits::MAX_SPIN_TERMS).
    pub terms: Vec<SpinTerm>,
}

impl Default for Spin {
    fn default() -> Self {
        Self {
            axis: Fp3([0.0, 1.0, 0.0]),
            terms: Vec::new(),
        }
    }
}

// The axis elides by its wire form (#1565), as a transform's fields do: an
// axis a hair off `+Y` is written as `+Y`, so it must be left out as `+Y` is.
crate::pds::serde_util::impl_default_eliding_serialize!(Spin { axis (wire), terms });

impl Spin {
    /// A spin of one term about `axis`.
    pub fn about(axis: [f32; 3], term: SpinTerm) -> Self {
        Self {
            axis: Fp3(axis),
            terms: vec![term],
        }
    }

    /// Whether anything about this spin turns the node: a non-zero axis and
    /// at least one term that moves. A node whose spin does not is spawned
    /// exactly as one without a spin, colliders and all.
    pub fn moves(&self) -> bool {
        is_direction(self.axis) && self.terms.iter().any(SpinTerm::moves)
    }
}

/// One contribution to a node's angle, summed with the node's other terms.
///
/// An open union, like every `$type`-tagged record type: a term from a newer
/// engine decodes as [`Self::Unknown`], turns nothing, and - like every
/// `Unknown` arm (#1111) - stops this build saving the record rather than
/// dropping the term.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum SpinTerm {
    /// Turn at a steady rate, in degrees per second: a windmill's sails, a
    /// fan, a turntable. Negative turns the other way.
    #[serde(rename = "network.symbios.spin.constant")]
    Constant {
        #[serde(default)]
        rate: Fp,
    },
    /// Swing to and fro: `amplitude` degrees either side of the authored
    /// pose, one full swing every `period` seconds, starting `phase` degrees
    /// into it. A pendulum, a hanging sign, a bobbing head.
    #[serde(rename = "network.symbios.spin.swing")]
    Swing {
        #[serde(default)]
        amplitude: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
        #[serde(default, skip_serializing_if = "is_wire_zero")]
        phase: Fp,
    },
    /// Roll along the ground like a wheel of this radius (metres), with the
    /// part's own motion: the turn rate is `((up x velocity) . axis) /
    /// radius`, so a wheel on the left and one on the right whose axes point
    /// opposite ways both roll forwards.
    #[serde(rename = "network.symbios.spin.roll")]
    Roll {
        #[serde(default = "default_roll_radius")]
        radius: Fp,
    },
    /// Turn with the turning of the part this one hangs from: `gain` degrees
    /// for each degree per second its parent turns about this node's axis,
    /// held within `limit` degrees either side. Front wheels following a
    /// vehicle into a bend; a negative gain steers against it. Travelling
    /// backwards - toward its parent's -Z, since every vehicle in Overlands
    /// is drawn facing +Z - it steers the other way, as front wheels do when
    /// a vehicle reverses through a bend.
    #[serde(rename = "network.symbios.spin.steer")]
    Steer {
        #[serde(default)]
        gain: Fp,
        #[serde(default = "default_steer_limit")]
        limit: Fp,
    },
    /// Turn at a rate that follows the part's speed: `idle` degrees per
    /// second standing still, and `gain` degrees more for each metre it
    /// travels along its parent's +Z - forward, as every vehicle in
    /// Overlands is drawn. A propeller that idles, spins up with its craft
    /// and backs as it reverses.
    #[serde(rename = "network.symbios.spin.speed")]
    Speed {
        #[serde(default)]
        idle: Fp,
        #[serde(default)]
        gain: Fp,
    },
    /// Lean against the part's acceleration and swing back, as a hanging
    /// lantern does when its carrier sets off or stops: `gain` degrees for
    /// each m/s^2 of acceleration along the way a turn about the axis would
    /// swing the part, reached through a spring that swings once every
    /// `period` seconds. Positive for a part that
    /// HANGS from its axis, whose bottom lags (5.84 is a real pendulum's: one
    /// over gravity, in degrees); negative for one that stands on it, whose
    /// top lags - a whip aerial, a mast.
    #[serde(rename = "network.symbios.spin.lean")]
    Lean {
        #[serde(default = "default_lean_gain")]
        gain: Fp,
        #[serde(default = "default_lean_period")]
        period: Fp,
    },
    /// Sway in the room's wind - the one its trees sway in: up to
    /// `amplitude` degrees about the axis when the wind crosses it squarely,
    /// gusting at the foliage's own pace, and further in a stronger wind.
    /// Positive for a part that HANGS from its axis, whose bottom swings
    /// downwind (a sign, a banner); negative for one that stands on it,
    /// whose top bends downwind.
    #[serde(rename = "network.symbios.spin.wind")]
    Wind {
        #[serde(default)]
        amplitude: Fp,
    },
    /// Wander to and fro without settling into a beat: up to `amplitude`
    /// degrees either side, about once every `period` seconds - three
    /// incommensurate swings summed. A bobbing buoy, a hovering drone. Read
    /// off the clock, as a swing is, so clients agree on it.
    #[serde(rename = "network.symbios.spin.wobble")]
    Wobble {
        #[serde(default)]
        amplitude: Fp,
        #[serde(default = "default_swing_period")]
        period: Fp,
    },
    /// Turn about the axis until `facing` - a direction in the part's own
    /// frame - points downwind in the room's wind, hunting a few degrees as
    /// it gusts: a weather vane, a windmill's head with its tail vane behind
    /// it, a flag on its pole. In a still room it keeps its authored pose.
    #[serde(rename = "network.symbios.spin.vane")]
    Vane {
        #[serde(default = "default_vane_facing")]
        facing: Fp3,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

impl SpinTerm {
    /// The kind names the editor offers, in its order, each with a fresh
    /// term of that kind.
    pub const KINDS: [&'static str; 9] = [
        "Constant", "Swing", "Roll", "Steer", "Speed", "Lean", "Wind", "Wobble", "Vane",
    ];

    /// A fresh term of the kind [`Self::label`] names, at values that show
    /// it working: a slow turn, a gentle swing, a wheel of half a metre, a
    /// steer that follows a vehicle's turning, a propeller's idle, a hanging
    /// lantern's lean, a hanging sign's sway, a buoy's wander, a vane's tail.
    pub fn fresh(kind: &str) -> Option<Self> {
        Some(match kind {
            "Constant" => Self::Constant { rate: Fp(30.0) },
            "Swing" => Self::Swing {
                amplitude: Fp(20.0),
                period: Fp(default_swing_period().0),
                phase: Fp::ZERO,
            },
            "Roll" => Self::Roll {
                radius: default_roll_radius(),
            },
            "Steer" => Self::Steer {
                gain: Fp(0.5),
                limit: default_steer_limit(),
            },
            "Speed" => Self::Speed {
                idle: Fp(120.0),
                gain: Fp(60.0),
            },
            "Lean" => Self::Lean {
                gain: default_lean_gain(),
                period: default_lean_period(),
            },
            "Wind" => Self::Wind {
                amplitude: Fp(15.0),
            },
            "Wobble" => Self::Wobble {
                amplitude: Fp(10.0),
                period: Fp(3.0),
            },
            "Vane" => Self::Vane {
                facing: default_vane_facing(),
            },
            _ => return None,
        })
    }

    /// The kind's name, as [`Self::KINDS`] spells it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Constant { .. } => "Constant",
            Self::Swing { .. } => "Swing",
            Self::Roll { .. } => "Roll",
            Self::Steer { .. } => "Steer",
            Self::Speed { .. } => "Speed",
            Self::Lean { .. } => "Lean",
            Self::Wind { .. } => "Wind",
            Self::Wobble { .. } => "Wobble",
            Self::Vane { .. } => "Vane",
            Self::Unknown => "Unknown",
        }
    }

    /// Whether this term can turn anything: a zero rate, amplitude or gain
    /// leaves the part at rest, and an unknown term is skipped.
    pub fn moves(&self) -> bool {
        match self {
            Self::Constant { rate } => rate.0 != 0.0,
            Self::Swing {
                amplitude, period, ..
            } => amplitude.0 != 0.0 && period.0 > 0.0,
            Self::Roll { radius } => radius.0 > 0.0,
            Self::Steer { gain, limit } => gain.0 != 0.0 && limit.0 > 0.0,
            Self::Speed { idle, gain } => idle.0 != 0.0 || gain.0 != 0.0,
            Self::Lean { gain, period } => gain.0 != 0.0 && period.0 > 0.0,
            Self::Wind { amplitude } => amplitude.0 != 0.0,
            Self::Wobble { amplitude, period } => amplitude.0 != 0.0 && period.0 > 0.0,
            Self::Vane { facing } => is_direction(*facing),
            Self::Unknown => false,
        }
    }

    /// Whether the term reads the part's motion rather than the clock: a
    /// part that only turns by the clock needs no motion sampled.
    pub fn follows_motion(&self) -> bool {
        matches!(
            self,
            Self::Roll { .. } | Self::Steer { .. } | Self::Speed { .. } | Self::Lean { .. }
        )
    }

    /// Whether the term reads the room's wind - and so, like a motion term,
    /// the way its part stands in the world.
    pub fn reads_wind(&self) -> bool {
        matches!(self, Self::Wind { .. } | Self::Vane { .. })
    }
}

fn default_swing_period() -> Fp {
    Fp(2.0)
}

fn default_roll_radius() -> Fp {
    Fp(0.5)
}

fn default_steer_limit() -> Fp {
    Fp(30.0)
}

/// A real pendulum's lean: one over gravity, in degrees per m/s^2 - 5.84,
/// written as the wire holds it, so a fresh term saves and reloads exactly.
fn default_lean_gain() -> Fp {
    Fp(5.84)
}

fn default_lean_period() -> Fp {
    Fp(1.2)
}

fn default_vane_facing() -> Fp3 {
    Fp3([0.0, 0.0, 1.0])
}

/// Whether `v` names a direction at all: finite and not zero. Lenient about
/// length, as the runtime is when it normalises one - a direction written
/// as `[0, 0, 1]` on the wire (a ten-thousandth long) still points along Z.
fn is_direction(v: Fp3) -> bool {
    let len_sq = v.0[0] * v.0[0] + v.0[1] * v.0[1] + v.0[2] * v.0[2];
    len_sq.is_finite() && len_sq > 1e-12
}

/// Whether `v` is written as zero: the skip predicate for an optional
/// scalar, compared as the wire holds it (#1565), so a value a hair off zero
/// is left out as zero is.
fn is_wire_zero(v: &Fp) -> bool {
    (v.0 * super::types::FP_SCALE).round() as i32 == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(spin: &Spin) -> String {
        serde_json::to_string(spin).expect("serialise")
    }

    #[test]
    fn a_turntable_leaves_its_default_axis_out() {
        let spin = Spin::about([0.0, 1.0, 0.0], SpinTerm::Constant { rate: Fp(30.0) });
        assert_eq!(
            wire(&spin),
            r#"{"terms":[{"$type":"network.symbios.spin.constant","rate":300000}]}"#
        );
    }

    #[test]
    fn an_axis_a_hair_off_up_is_left_out_as_up_is() {
        // 1e-6 below one is written as 10000, the default's integer.
        let spin = Spin::about([0.0, 0.999_999, 0.0], SpinTerm::Constant { rate: Fp(1.0) });
        assert!(!wire(&spin).contains("axis"), "{}", wire(&spin));
    }

    #[test]
    fn a_swing_leaves_a_zero_phase_out_and_writes_one_otherwise() {
        let mut spin = Spin::about(
            [1.0, 0.0, 0.0],
            SpinTerm::Swing {
                amplitude: Fp(20.0),
                period: Fp(2.0),
                phase: Fp(0.000_01),
            },
        );
        let text = wire(&spin);
        assert!(text.contains(r#""axis":[10000,0,0]"#), "{text}");
        assert!(!text.contains("phase"), "{text}");
        spin.terms[0] = SpinTerm::Swing {
            amplitude: Fp(20.0),
            period: Fp(2.0),
            phase: Fp(90.0),
        };
        assert!(wire(&spin).contains(r#""phase":900000"#));
    }

    #[test]
    fn every_kind_round_trips() {
        let spin = Spin {
            axis: Fp3([0.0, 0.0, -1.0]),
            terms: SpinTerm::KINDS
                .iter()
                .map(|k| SpinTerm::fresh(k).expect("a kind the editor offers"))
                .collect(),
        };
        let back: Spin = serde_json::from_str(&wire(&spin)).expect("decodes");
        assert_eq!(back, spin);
        for (term, kind) in back.terms.iter().zip(SpinTerm::KINDS) {
            assert_eq!(term.label(), kind);
            assert!(term.moves(), "a fresh {kind} shows itself working");
        }
    }

    #[test]
    fn a_term_missing_its_fields_decodes_at_their_defaults() {
        let spin: Spin = serde_json::from_str(
            r#"{"terms":[{"$type":"network.symbios.spin.roll"},
                         {"$type":"network.symbios.spin.swing","amplitude":100000}]}"#,
        )
        .expect("decodes");
        assert_eq!(spin.axis, Fp3([0.0, 1.0, 0.0]));
        assert_eq!(spin.terms[0], SpinTerm::Roll { radius: Fp(0.5) });
        assert_eq!(
            spin.terms[1],
            SpinTerm::Swing {
                amplitude: Fp(10.0),
                period: Fp(2.0),
                phase: Fp::ZERO
            }
        );
    }

    #[test]
    fn a_term_from_a_newer_engine_decodes_turns_nothing_and_cannot_be_written() {
        let spin: Spin = serde_json::from_str(
            r#"{"terms":[{"$type":"network.symbios.spin.follow","ratio":5000},
                         {"$type":"network.symbios.spin.constant","rate":100000}]}"#,
        )
        .expect("an open union tolerates a future term");
        assert_eq!(spin.terms[0], SpinTerm::Unknown);
        assert!(!spin.terms[0].moves());
        assert!(spin.moves(), "the known term still turns the part");
        // #1111: an arm this build cannot write back refuses the save
        // instead of dropping the owner's newer content.
        assert!(serde_json::to_string(&spin).is_err());
    }

    #[test]
    fn a_spin_moves_only_with_an_axis_and_a_moving_term() {
        let still = |axis: [f32; 3], term: SpinTerm| !Spin::about(axis, term).moves();
        assert!(still([0.0; 3], SpinTerm::Constant { rate: Fp(30.0) }));
        assert!(still(
            [0.0, 1.0, 0.0],
            SpinTerm::Constant { rate: Fp::ZERO }
        ));
        assert!(still(
            [0.0, 1.0, 0.0],
            SpinTerm::Swing {
                amplitude: Fp(20.0),
                period: Fp::ZERO,
                phase: Fp::ZERO
            }
        ));
        assert!(still([0.0, 1.0, 0.0], SpinTerm::Unknown));
        assert!(!still(
            [0.0, 1.0, 0.0],
            SpinTerm::Constant { rate: Fp(-5.0) }
        ));
        assert!(!Spin::default().moves(), "no terms, no turn");
    }
}
