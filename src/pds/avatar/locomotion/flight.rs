//! The on-foot preset's flight (#1861): the numbers a winged body flies by,
//! an optional section of [`super::HumanoidParams`].
//!
//! One chassis with a runtime mode, never a preset swap. Taking off on the
//! helicopter or the airplane preset would edit the published record at
//! every take-off - an unsaved avatar, an undo step, a Reliable avatar
//! update to everyone and a chassis stripped and rebuilt - so a body that
//! flies walks on the on-foot preset as a person does, and this section says
//! how it flies. What it is doing - on the ground, flapping, gliding - is
//! runtime state on the chassis (`player::flight`), never record state.
//!
//! **Absent, a body walks exactly as it always did**, and its record writes
//! the bytes it wrote before the section existed: the key is left off. A
//! client older than the section reads a record with it, drops the key it
//! does not know and walks; when it writes the record back, the section is
//! gone. A winged body gets it back on its owner's client, as its body flies
//! (#1875, `fit::restore_body_shape`): its kind is on its record, and its
//! section is fitted from its body (`fit::fit_on_foot`). Nothing gives one
//! back to a generator body: its visuals are no sound witness (a pair of
//! swaying lanterns swings as a pair of wings does - #1861's critic), so its
//! section is worn again by hand.
//!
//! Every number is a scaled int on the wire ([`Fp`]), as every record number
//! is (atproto has no floats), and the sanitiser quantises as it clamps.

use super::HumanoidParams;
use crate::pds::types::Fp;
use serde::{Deserialize, Serialize};

/// The height (m) the defaults are tuned at: the stand-in bird's, a little
/// under a big dog's back - avatar-sized, as owner decision (1) on #1859
/// asked for, about cat to dog.
pub const REFERENCE_HEIGHT_M: f32 = 0.75;

/// How a winged body flies (#1861) - arcade, not aerodynamic (research-11,
/// section 8.5): a glide polar, flaps that cost stamina, turns by banking.
/// Speeds in m/s, rates in degrees per second.
///
/// **A section that leaves a number out takes the reference body's**
/// ([`FlightParams::default`]): a hand-written record missing one flies on
/// the rest rather than losing the whole avatar to a decode error.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct FlightParams {
    /// How much the body can flap, 0 to 1 (research-11, section 8.4): 1 a
    /// bird, about 0.7 a bat, 0 a glider. It scales a beat's push and how
    /// many beats the body's stamina holds; under
    /// [`FlightParams::MIN_FLAP_POWER`] the body cannot flap at all, and its
    /// take-off is a stronger leap into a glide.
    pub flight_power: Fp,
    /// The glide polar's lift to drag: metres forward for each metre of
    /// height a hands-off glide gives up at its trim speed.
    pub glide_ratio: Fp,
    /// The speed a hands-off glide settles at (m/s): the polar's best, where
    /// it sinks at `trim_speed / glide_ratio`.
    pub trim_speed: Fp,
    /// Under this airspeed (m/s) the wings stall: the nose drops and the body
    /// dives until it is flying again. Held under the trim speed.
    pub stall_speed: Fp,
    /// What one full-power beat adds to the body's speed (m/s), forward and
    /// up along its wings.
    pub beat_speed: Fp,
    /// Beats a second while the flap key is held.
    pub wingbeat_rate: Fp,
    /// How many full-power beats a full stamina holds.
    pub stamina_beats: Fp,
    /// Seconds of gliding that bring an empty stamina back to full - three
    /// times as fast standing on something.
    pub stamina_recovery: Fp,
    /// The take-off's leap (m/s), up and forward.
    pub takeoff_speed: Fp,
    /// The steepest the body banks into a turn (degrees).
    pub max_bank: Fp,
    /// How fast it rolls into a bank and out of it (degrees per second).
    pub roll_rate: Fp,
    /// How fast it pitches its nose (degrees per second).
    pub pitch_rate: Fp,
}

impl FlightParams {
    /// Under this `flight_power` a body cannot flap at all (research-11,
    /// section 8.4: the Flap state unlocks at about 0.3-0.5).
    pub const MIN_FLAP_POWER: f32 = 0.25;

    /// The defaults for a body standing `height` metres tall: the stand-in
    /// bird's at [`REFERENCE_HEIGHT_M`], every speed scaled as research-11's
    /// section 8.2 has it - cruise as mass to the 0.13, so as length to about
    /// the 0.4 - and the wingbeat as mass to the -0.28, length to the -0.84.
    pub fn for_height(height: f32) -> Self {
        let size = if height.is_finite() {
            (height / REFERENCE_HEIGHT_M).clamp(0.2, 8.0)
        } else {
            1.0
        };
        let speed = size.powf(0.4);
        let mut flight = Self {
            flight_power: Fp(1.0),
            glide_ratio: Fp(5.0),
            trim_speed: Fp(6.5 * speed),
            stall_speed: Fp(3.5 * speed),
            beat_speed: Fp(2.0 * speed),
            wingbeat_rate: Fp(3.0 * size.powf(-0.84)),
            stamina_beats: Fp(24.0),
            stamina_recovery: Fp(8.0),
            takeoff_speed: Fp(3.0 * speed),
            max_bank: Fp(50.0),
            roll_rate: Fp(180.0),
            pitch_rate: Fp(90.0),
        };
        flight.sanitize();
        flight
    }

    /// The sink (m/s) a hands-off glide settles at: the polar's at its trim.
    pub fn trim_sink(&self) -> f32 {
        self.trim_speed.0 / self.glide_ratio.0.max(1.0)
    }

    /// Whether the body can flap at all ([`Self::MIN_FLAP_POWER`]).
    pub fn can_flap(&self) -> bool {
        self.flight_power.0 >= Self::MIN_FLAP_POWER
    }

    /// Keep `owned`'s handling over these numbers (#1875's critic): the ones
    /// the body does not size - how many beats a stamina holds and how fast
    /// it comes back, the glide polar's ratio, and the bank, roll and pitch
    /// the controller turns by - which are the owner's to tune. The rest -
    /// the flap power its families give, the trim and stall speeds, a beat's
    /// push, the wingbeat and the take-off leap - are the body's, and a
    /// winged body's refit sizes them afresh on every edit.
    pub fn keep_handling(&mut self, owned: &Self) {
        self.stamina_beats = owned.stamina_beats;
        self.stamina_recovery = owned.stamina_recovery;
        self.glide_ratio = owned.glide_ratio;
        self.max_bank = owned.max_bank;
        self.roll_rate = owned.roll_rate;
        self.pitch_rate = owned.pitch_rate;
        self.sanitize();
    }

    /// Clamp every number into a sane range and onto the wire's grid: a clamp
    /// can land off it, and a peer decodes the record off it.
    pub fn sanitize(&mut self) {
        fn held(v: Fp, lo: f32, hi: f32) -> Fp {
            v.clamped(lo, hi).on_wire()
        }
        self.flight_power = held(self.flight_power, 0.0, 1.0);
        self.glide_ratio = held(self.glide_ratio, 1.0, 20.0);
        self.trim_speed = held(self.trim_speed, 1.0, 40.0);
        // Held under the trim, or a hands-off glide would fly stalled.
        self.stall_speed = held(self.stall_speed, 0.5, 0.9 * self.trim_speed.0);
        self.beat_speed = held(self.beat_speed, 0.0, 10.0);
        self.wingbeat_rate = held(self.wingbeat_rate, 0.5, 12.0);
        self.stamina_beats = held(self.stamina_beats, 1.0, 500.0);
        self.stamina_recovery = held(self.stamina_recovery, 0.5, 120.0);
        self.takeoff_speed = held(self.takeoff_speed, 0.0, 15.0);
        self.max_bank = held(self.max_bank, 5.0, 80.0);
        self.roll_rate = held(self.roll_rate, 10.0, 720.0);
        self.pitch_rate = held(self.pitch_rate, 10.0, 360.0);
    }
}

impl Default for FlightParams {
    fn default() -> Self {
        Self::for_height(REFERENCE_HEIGHT_M)
    }
}

impl HumanoidParams {
    /// Whether the body flies (#1861): its record carries a flight section.
    pub fn flies(&self) -> bool {
        self.flight.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::AvatarRecord;
    use crate::pds::avatar::locomotion::LocomotionConfig;
    use crate::pds::avatar::locomotion::LocomotionPreset as _;
    use serde_json::{Value, json};

    /// The stand-in bird's whole avatar (`tests/fixtures/winged`).
    fn bird() -> AvatarRecord {
        let mut record: AvatarRecord = serde_json::from_str(include_str!(
            "../../../../tests/fixtures/winged/standin_bird_avatar.json"
        ))
        .expect("the bird decodes");
        record.sanitize();
        record
    }

    /// Whether any number anywhere in `value` is written with a decimal
    /// point: atproto records carry none.
    fn has_float(value: &Value) -> bool {
        match value {
            Value::Number(n) => !n.is_i64() && !n.is_u64(),
            Value::Array(items) => items.iter().any(has_float),
            Value::Object(fields) => fields.values().any(has_float),
            _ => false,
        }
    }

    /// **A body that does not fly writes the bytes it always did; one that
    /// flies writes its section, in scaled ints, and reads it back** (#1861).
    #[test]
    fn the_flight_section_round_trips_without_floats() {
        let walker = HumanoidParams::default();
        let wire = serde_json::to_value(&walker).expect("writes");
        assert!(wire.get("flight").is_none(), "{wire}");
        let flier = HumanoidParams {
            flight: Some(FlightParams::default()),
            ..HumanoidParams::default()
        };
        let wire = serde_json::to_value(&flier).expect("writes");
        assert!(!has_float(&wire), "{wire}");
        assert_eq!(
            wire["flight"],
            json!({
                "flight_power": 10000, "glide_ratio": 50000, "trim_speed": 65000,
                "stall_speed": 35000, "beat_speed": 20000, "wingbeat_rate": 30000,
                "stamina_beats": 240000, "stamina_recovery": 80000,
                "takeoff_speed": 30000, "max_bank": 500000, "roll_rate": 1800000,
                "pitch_rate": 900000
            })
        );
        let back: HumanoidParams = serde_json::from_value(wire).expect("reads");
        assert_eq!(back, flier);
        // The whole bird, through the record.
        let record = bird();
        let wire = serde_json::to_value(&record).expect("writes");
        assert!(!has_float(&wire));
        let back: AvatarRecord = serde_json::from_value(wire).expect("reads");
        assert_eq!(back, record);
    }

    /// **The sanitiser clamps and quantises** (#1861): a hostile or a stale
    /// number is held to a sane range, a stall over the trim under it, a
    /// non-finite one to its floor, and every number lands on the wire's grid.
    #[test]
    fn the_sanitiser_clamps_onto_the_grid() {
        let mut flight = FlightParams {
            flight_power: Fp(3.0),
            glide_ratio: Fp(f32::NAN),
            trim_speed: Fp(7.123_456),
            stall_speed: Fp(9.0),
            wingbeat_rate: Fp(0.0),
            max_bank: Fp(170.0),
            ..FlightParams::default()
        };
        flight.sanitize();
        assert_eq!(flight.flight_power, Fp(1.0));
        assert_eq!(flight.glide_ratio, Fp(1.0));
        assert_eq!(flight.trim_speed, Fp(7.123_456).on_wire());
        assert_eq!(flight.stall_speed, Fp(0.9 * flight.trim_speed.0).on_wire());
        assert!(flight.stall_speed.0 < flight.trim_speed.0);
        assert_eq!(flight.wingbeat_rate, Fp(0.5));
        assert_eq!(flight.max_bank, Fp(80.0));
        let again = {
            let mut again = flight.clone();
            again.sanitize();
            again
        };
        assert_eq!(again, flight, "a second sanitise moved it");
        // Through the preset, as a fetched record is.
        let mut p = HumanoidParams {
            flight: Some(FlightParams {
                takeoff_speed: Fp(99.0),
                ..FlightParams::default()
            }),
            ..HumanoidParams::default()
        };
        p.sanitize();
        assert_eq!(p.flight.expect("kept").takeoff_speed, Fp(15.0));
    }

    /// **The defaults scale with the body** (#1861): a bigger body flies
    /// faster and beats slower, as research-11's section 8.2 has it, and the
    /// stand-in's own height is the reference.
    #[test]
    fn the_defaults_scale_with_the_body() {
        let small = FlightParams::for_height(0.4);
        let bird = FlightParams::for_height(REFERENCE_HEIGHT_M);
        let big = FlightParams::for_height(1.5);
        assert_eq!(bird, FlightParams::default());
        assert!(small.trim_speed.0 < bird.trim_speed.0 && bird.trim_speed.0 < big.trim_speed.0);
        assert!(small.wingbeat_rate.0 > bird.wingbeat_rate.0);
        assert!(bird.wingbeat_rate.0 > big.wingbeat_rate.0);
        assert!((bird.trim_sink() - 1.3).abs() < 1e-3);
        assert_eq!(FlightParams::for_height(f32::NAN), bird);
    }

    /// **A section that leaves a number out takes the default for it, and
    /// keeps the rest** (#1861's critic): a hand-written record missing one
    /// number would otherwise lose its whole avatar to a decode error.
    #[test]
    fn a_section_that_leaves_a_number_out_takes_the_default_for_it() {
        let record = bird();
        let mut wire = serde_json::to_value(&record).expect("writes");
        wire["locomotion"]["flight"]
            .as_object_mut()
            .expect("a section")
            .remove("pitch_rate");
        let back: AvatarRecord = serde_json::from_value(wire).expect("a partial section reads");
        let (LocomotionConfig::Humanoid(given), LocomotionConfig::Humanoid(read)) =
            (&record.locomotion, &back.locomotion)
        else {
            panic!("on foot");
        };
        let (given, read) = (
            given.flight.as_ref().expect("the bird flies"),
            read.flight.as_ref().expect("the section survives"),
        );
        assert_eq!(read.pitch_rate, FlightParams::default().pitch_rate);
        assert_eq!(read.glide_ratio, given.glide_ratio, "the rest as given");
        assert_eq!(read.flight_power, given.flight_power, "the rest as given");
    }
}
