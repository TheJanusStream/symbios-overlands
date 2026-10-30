//! Humanoid preset - the "On foot" body: a capsule with locked-axes
//! uprighting, velocity-driven walk controller, jump impulse, swim/wading
//! modes. A person stands upright on its capsule; a four-legged beast lies
//! along it (#1802, [`OnFootShape`]).

use super::{LocomotionConfig, LocomotionPreset, clamp_pos, clamp_unit};
use crate::pds::types::Fp;
use serde::{Deserialize, Serialize};

/// Humanoid preset: capsule rigid body with `LockedAxes` keeping it
/// upright, velocity-driven walk controller, jump impulse, swim/wading
/// modes triggered by water depth.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidParams {
    /// Capsule collider radius (m).
    pub capsule_radius: Fp,
    /// Capsule cylindrical length (m): up the body for a person, so its
    /// total height is length + 2·radius; along it for a beast lying on its
    /// capsule ([`OnFootShape::Lying`]), so its total LENGTH is.
    pub capsule_length: Fp,
    pub mass: Fp,
    pub linear_damping: Fp,
    /// The travel speed Shift runs at (m/s) - the RUN since #1193, under
    /// its old wire name; the unshifted walk is derived from the body
    /// (`player::humanoid::WALK_FROUDE`). Required on the wire and never
    /// skipped, so every published record carries its own value: the
    /// default reaches freshly created and freshly seeded records only, and
    /// an owner moves their own in the Locomotion tab's Run speed.
    pub walk_speed: Fp,
    pub acceleration: Fp,
    pub jump_impulse: Fp,
    pub swim_speed: Fp,
    pub swim_vertical_speed: Fp,
    pub wading_speed_factor: Fp,
    /// Horizontal-velocity decay rate (1/s) once every movement key is
    /// released - higher stops harder ("stops on a dime" at the
    /// default). Promoted from a hard-coded constant by #876; field-level
    /// serde default so records published before it deserialize to the
    /// historical feel.
    #[serde(default = "default_stop_damping")]
    pub stop_damping: Fp,
    /// Facing slerp rate (1/s) toward the movement direction - how
    /// quickly the whole avatar turns to face where it walks or swims.
    #[serde(default = "default_turn_rate")]
    pub turn_rate: Fp,
    /// How the body stands over its capsule (#1802, owner decision D1):
    /// upright, a person's, or lying along it, a four-legged beast's.
    /// **Upright is left off the wire**, so every record published before
    /// the shape existed - and every person's since - writes exactly the
    /// bytes it did, with no new key; a client older than the field reads a
    /// lying record as upright and hangs the beast wrongly, which the owner
    /// accepted as cosmetic.
    #[serde(default, skip_serializing_if = "OnFootShape::leaves_no_key")]
    pub shape: OnFootShape,
}

/// How an on-foot body stands over its capsule (#1802) - an open union, like
/// [`super::LocomotionConfig`].
///
/// A person stands upright on it: the capsule's axis runs up the body. A
/// four-legged beast lies along it (owner decision D2): the axis runs along
/// the trunk, girdle to girdle, at a radius of half the trunk's depth, with
/// the head, the neck and the tail outside it. The capsule rests on the
/// ground under the trunk rather than at the trunk's own height, because the
/// walk controller holds a body up by its collider, which a beast's legs are
/// not: its bottom is where the feet stand, as a person's is, so the drawn
/// body still hangs with its feet at the collider's bottom. Its radius and
/// length are [`HumanoidParams::capsule_radius`] and
/// [`HumanoidParams::capsule_length`], as a person's are.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(tag = "$type")]
pub enum OnFootShape {
    /// A person's: the capsule stands up the body.
    #[default]
    #[serde(rename = "network.symbios.locomotion.humanoid#upright")]
    Upright,
    /// A four-legged beast's: the capsule lies along the body's own forward
    /// axis.
    #[serde(rename = "network.symbios.locomotion.humanoid#lying")]
    Lying {
        /// How tall the body stands (m): the top of its trunk over its feet.
        /// What its water depths are shares of
        /// ([`HumanoidParams::stand_height`]), as a person's are of the
        /// capsule they stand up in - a lying capsule is only the trunk's
        /// depth tall, and its legs are under it. Never under the capsule's
        /// own diameter: the trunk's top is the capsule's.
        height: Fp,
        /// How far ahead of the body's origin the capsule's middle lies (m):
        /// the girdles' midpoint, so the capsule runs from the hips to the
        /// shoulders whatever the body's proportions. Its origin is the rig's,
        /// which the drawn body hangs from; 0 for a record that names none.
        #[serde(default)]
        ahead: Fp,
    },
    /// A shape this build does not know: stood upright, and never written
    /// back (last variant: serde requires `other` to close the union).
    #[serde(other, skip_serializing)]
    Unknown,
}

impl OnFootShape {
    /// Whether the record leaves the key off: an upright body's, so its bytes
    /// are the ones it wrote before the shape existed - and one this build
    /// cannot write.
    pub fn leaves_no_key(&self) -> bool {
        !matches!(self, Self::Lying { .. })
    }
}

/// Serde fallback for records published before #876 - the constant the
/// walk controller hard-coded. Shared with `Default` so an old record and
/// a fresh preset agree.
fn default_stop_damping() -> Fp {
    Fp(20.0)
}

/// Serde fallback for records published before #876 (see
/// [`default_stop_damping`]).
fn default_turn_rate() -> Fp {
    Fp(12.0)
}

impl Default for HumanoidParams {
    fn default() -> Self {
        Self {
            capsule_radius: Fp(0.28),
            capsule_length: Fp(1.24),
            mass: Fp(80.0),
            linear_damping: Fp(0.3),
            // The owner's run (#1323), up from 4.0.
            walk_speed: Fp(5.0),
            acceleration: Fp(12.0),
            jump_impulse: Fp(450.0),
            swim_speed: Fp(2.5),
            swim_vertical_speed: Fp(1.8),
            wading_speed_factor: Fp(0.5),
            stop_damping: default_stop_damping(),
            turn_rate: default_turn_rate(),
            shape: OnFootShape::Upright,
        }
    }
}

impl HumanoidParams {
    /// How tall the collider stands (m): its bottom is the body's feet, half
    /// this below its middle. A person's whole capsule, length and both
    /// ends; a beast's lying capsule only its diameter, the trunk's depth
    /// ([`OnFootShape::Lying`]).
    pub fn total_height(&self) -> f32 {
        match self.shape {
            OnFootShape::Lying { .. } => 2.0 * self.capsule_radius.0,
            OnFootShape::Upright | OnFootShape::Unknown => {
                self.capsule_length.0 + 2.0 * self.capsule_radius.0
            }
        }
    }

    /// How tall the body stands over its feet (m), which its water depths
    /// are shares of: a person's capsule, which is as tall as they are; a
    /// beast's trunk top, which its lying capsule does not reach
    /// ([`OnFootShape::Lying`]).
    pub fn stand_height(&self) -> f32 {
        match self.shape {
            OnFootShape::Lying { height, .. } => height.0,
            OnFootShape::Upright | OnFootShape::Unknown => self.total_height(),
        }
    }

    /// Whether the body lies along its capsule - a beast's - rather than
    /// standing up in it.
    pub fn is_lying(&self) -> bool {
        matches!(self.shape, OnFootShape::Lying { .. })
    }

    /// How far ahead of the body's origin its capsule's middle lies (m): a
    /// lying capsule's offset along the body ([`OnFootShape::Lying`]), and 0
    /// for an upright one, which stands on the origin.
    pub fn capsule_ahead(&self) -> f32 {
        match self.shape {
            OnFootShape::Lying { ahead, .. } => ahead.0,
            OnFootShape::Upright | OnFootShape::Unknown => 0.0,
        }
    }
}

impl LocomotionPreset for HumanoidParams {
    const KIND_TAG: &'static str = "humanoid";
    /// "On foot", not "Humanoid" (#1803, owner decisions D1 and D7): the
    /// preset carries a four-legged beast as well as a person, lying along
    /// its capsule ([`OnFootShape::Lying`]).
    const DISPLAY_LABEL: &'static str = "On foot";

    fn sanitize(&mut self) {
        self.capsule_radius = clamp_pos(self.capsule_radius, 0.05, 2.0);
        self.capsule_length = clamp_pos(self.capsule_length, 0.1, 8.0);
        self.mass = clamp_pos(self.mass, 0.1, 10_000.0);
        self.linear_damping = clamp_pos(self.linear_damping, 0.0, 100.0);
        self.walk_speed = clamp_pos(self.walk_speed, 0.0, 50.0);
        self.acceleration = clamp_pos(self.acceleration, 0.0, 200.0);
        self.jump_impulse = clamp_pos(self.jump_impulse, 0.0, 50_000.0);
        self.swim_speed = clamp_pos(self.swim_speed, 0.0, 50.0);
        self.swim_vertical_speed = clamp_pos(self.swim_vertical_speed, 0.0, 50.0);
        self.wading_speed_factor = clamp_unit(self.wading_speed_factor);
        self.stop_damping = clamp_pos(self.stop_damping, 0.0, 100.0);
        self.turn_rate = clamp_pos(self.turn_rate, 0.0, 50.0);
        // Quantised as it is clamped: a clamp can land off the wire's grid,
        // and a peer decodes the record off it. An unknown shape stands
        // upright, which is how this build draws it.
        // A lying body stands at least as tall as its capsule, whose top is
        // the trunk's; its capsule's middle stays within its length of the
        // origin the body hangs from.
        self.shape = match self.shape {
            OnFootShape::Lying { height, ahead } => OnFootShape::Lying {
                height: clamp_pos(height, (2.0 * self.capsule_radius.0).max(0.05), 10.0).on_wire(),
                ahead: Fp(if ahead.0.is_finite() {
                    ahead.0.clamp(-self.capsule_length.0, self.capsule_length.0)
                } else {
                    0.0
                })
                .on_wire(),
            },
            OnFootShape::Upright | OnFootShape::Unknown => OnFootShape::Upright,
        };
    }

    fn into_config(self) -> LocomotionConfig {
        LocomotionConfig::Humanoid(Box::new(self))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::AvatarRecord;
    use serde_json::json;

    /// The default on-foot preset, as every release since #876 wrote it.
    const DEFAULT_WIRE: &str = r#"{"capsule_radius":2800,"capsule_length":12400,"mass":800000,"linear_damping":3000,"walk_speed":50000,"acceleration":120000,"jump_impulse":4500000,"swim_speed":25000,"swim_vertical_speed":18000,"wading_speed_factor":5000,"stop_damping":200000,"turn_rate":120000}"#;

    /// **An upright body writes the bytes it wrote before the shape existed**
    /// (#1802, D1): the default preset to the byte, and no seeded person's
    /// record names a shape at all.
    #[test]
    fn an_upright_body_writes_no_shape() {
        let wire = serde_json::to_string(&HumanoidParams::default()).expect("writes");
        assert_eq!(wire, DEFAULT_WIRE);
        let mut people = 0;
        for seed in 0u64..600 {
            if let LocomotionConfig::Humanoid(p) = AvatarRecord::default_for_seed(seed).locomotion {
                people += 1;
                let wire = serde_json::to_string(&p).expect("writes");
                assert!(
                    !wire.contains("shape"),
                    "seed {seed} writes a shape: {wire}"
                );
            }
        }
        assert!(people > 50, "the fixture: only {people} people under 600");
    }

    /// **A lying body writes its shape, on the wire's grid, and reads it
    /// back** (#1802): sanitising quantises its height and its offset as it
    /// clamps them.
    #[test]
    fn a_lying_body_round_trips_its_shape() {
        let mut p = HumanoidParams {
            shape: OnFootShape::Lying {
                height: Fp(0.704_83),
                ahead: Fp(0.031_27),
            },
            ..HumanoidParams::default()
        };
        p.sanitize();
        let OnFootShape::Lying { height, ahead } = p.shape else {
            panic!("sanitising stood the body up: {:?}", p.shape);
        };
        assert_eq!(height, Fp(0.704_83).on_wire(), "not quantised");
        assert_eq!(ahead, Fp(0.031_27).on_wire(), "not quantised");
        let wire = serde_json::to_value(&p).expect("writes");
        assert_eq!(
            wire["shape"],
            json!({"$type": "network.symbios.locomotion.humanoid#lying", "height": 7048, "ahead": 313})
        );
        let back: HumanoidParams = serde_json::from_value(wire).expect("reads");
        assert_eq!(back, p);
        // A shape naming no offset lies on the body's origin.
        let bare: OnFootShape = serde_json::from_value(
            json!({"$type": "network.symbios.locomotion.humanoid#lying", "height": 7048}),
        )
        .expect("reads");
        assert_eq!(
            bare,
            OnFootShape::Lying {
                height: Fp(0.7048),
                ahead: Fp(0.0)
            }
        );
        // A hostile height is held to a body's, and a hostile offset to the
        // capsule's length either way.
        let mut tall = HumanoidParams {
            shape: OnFootShape::Lying {
                height: Fp(80.0),
                ahead: Fp(-9.0),
            },
            ..HumanoidParams::default()
        };
        tall.sanitize();
        assert_eq!(
            tall.shape,
            OnFootShape::Lying {
                height: Fp(10.0),
                ahead: Fp(-1.24)
            }
        );
        let mut lost = HumanoidParams {
            shape: OnFootShape::Lying {
                height: Fp(0.7),
                ahead: Fp(f32::NAN),
            },
            ..HumanoidParams::default()
        };
        lost.sanitize();
        assert_eq!(lost.capsule_ahead(), 0.0, "a non-finite offset");
    }

    /// **A lying body stands no lower than its capsule** (#1802): its height
    /// is the top of its trunk, which is the capsule's top, so a record
    /// naming less - a hostile or a stale one - is held to the capsule's
    /// diameter, and its water depths stay shares of a body at least that
    /// tall.
    #[test]
    fn a_lying_body_stands_at_least_its_capsule() {
        let mut short = HumanoidParams {
            capsule_radius: Fp(0.3),
            shape: OnFootShape::Lying {
                height: Fp(0.2),
                ahead: Fp(0.0),
            },
            ..HumanoidParams::default()
        };
        short.sanitize();
        assert_eq!(short.stand_height(), 0.6);
        assert_eq!(short.stand_height(), short.total_height());
    }

    /// A record from before the shape reads upright, and a shape from a
    /// newer client reads, stands upright, and is never written back (#1802).
    #[test]
    fn an_old_record_and_an_unknown_shape_stand_upright() {
        let old: HumanoidParams = serde_json::from_str(DEFAULT_WIRE).expect("reads");
        assert_eq!(old.shape, OnFootShape::Upright);
        let mut wire: serde_json::Value = serde_json::from_str(DEFAULT_WIRE).expect("parses");
        wire["shape"] =
            json!({"$type": "network.symbios.locomotion.humanoid#sitting", "height": 5000});
        let mut newer: HumanoidParams = serde_json::from_value(wire).expect("reads");
        assert_eq!(newer.shape, OnFootShape::Unknown);
        assert_eq!(newer.total_height(), old.total_height());
        assert_eq!(
            serde_json::to_string(&newer).expect("writes"),
            DEFAULT_WIRE,
            "an unknown shape is written back"
        );
        newer.sanitize();
        assert_eq!(newer.shape, OnFootShape::Upright);
    }

    /// A lying capsule is its diameter tall and stands as tall as its shape
    /// says; an upright one is its whole capsule, both ways (#1802).
    #[test]
    fn a_lying_body_is_its_diameter_tall_and_stands_its_height() {
        let person = HumanoidParams::default();
        assert_eq!(person.total_height(), 1.24 + 2.0 * 0.28);
        assert_eq!(person.stand_height(), person.total_height());
        assert_eq!(person.capsule_ahead(), 0.0);
        let beast = HumanoidParams {
            shape: OnFootShape::Lying {
                height: Fp(0.7),
                ahead: Fp(0.05),
            },
            ..HumanoidParams::default()
        };
        assert_eq!(beast.total_height(), 2.0 * 0.28);
        assert_eq!(beast.stand_height(), 0.7);
        assert_eq!(beast.capsule_ahead(), 0.05);
    }
}
