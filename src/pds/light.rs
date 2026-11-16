//! A generator node's light source (#1921): an authored point or spot light
//! every client draws at the node, for the world round it to be lit by.
//!
//! The record says what the light is - its kind, colour, brightness, how far
//! it reaches, how big it is, where in the node's frame it sits and, for a
//! spot, which way it points; whether it shows only from dusk to dawn; and
//! whether it flickers or pulses. Nothing about it crosses the network
//! beyond the record: every client draws its own lights, the nearest few of
//! them, as [`world_builder::light`](crate::world_builder::light) says.
//!
//! The light hangs in the node's frame as a child would: its `offset` and a
//! spot's `aim` are in the node's frame after its rotation and scale, so a
//! lamp head tilted by its node shines where it points and a node that
//! turns (a [`Spin`](super::Spin)) carries its light round with it - a
//! lighthouse is a spot on a turning node. Its `range` and `size` are
//! metres in the world, whatever the node's scale.
//!
//! Every field is left off the wire at its default, so a fresh light is
//! `"light": {}` - a warm lamp of the helipad's reach, lit from dusk to
//! dawn - and a node without a light writes nothing at all, so a record
//! saved before lights existed keeps its bytes. A client built before them
//! ignores the key, draws no light, and drops it if it saves the node.

use serde::{Deserialize, Serialize};

use super::types::{Fp, Fp3, to_wire};

/// A node's light source - see the module docs.
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct LightSource {
    /// Point (the default, left off the wire) or spot, with a spot's aim
    /// and cone.
    pub shape: LightShape,
    /// The light's colour, sRGB, each channel 0 to 1 - as a material's
    /// colour is written. Its brightness is [`Self::brightness`]'s; the
    /// colour only tints it.
    pub color: Fp3,
    /// How bright, as a share of the reference lamp's: 1 lights what stands
    /// at four fifths of [`Self::range`] to a lamp's working 40 lx, whatever
    /// the range, so a longer reach is a stronger light and not a dimmer
    /// one. Zero is dark. Up to 32 at a short reach, its power held to that
    /// of 8 at the longest (`sanitize::light::max_light_brightness`).
    pub brightness: Fp,
    /// How far the light reaches (m): where it has fallen to nothing.
    pub range: Fp,
    /// The light's own radius (m) - a bigger source draws softer, wider
    /// highlights on what it shines on.
    pub size: Fp,
    /// Where the light sits, in the node's frame.
    pub offset: Fp3,
    /// Lit only while the world is dark, from dusk to dawn, as a material's
    /// glow of that name is - following the same darkness of the drawn
    /// world. On by default, so `false` is what the wire carries: a lamp's
    /// light is lost beside the sun anyway, and one lit by day would take a
    /// place among the few lights a client draws for nothing.
    pub dusk_to_dawn: bool,
    /// A flicker or a pulse; `None` (left off the wire) is a steady light.
    pub flicker: Option<LightFlicker>,
}

impl Default for LightSource {
    fn default() -> Self {
        Self {
            shape: LightShape::Point,
            color: Fp3(DEFAULT_COLOR),
            brightness: Fp(1.0),
            range: Fp(DEFAULT_RANGE_M),
            size: Fp(DEFAULT_SIZE_M),
            offset: Fp3([0.0; 3]),
            dusk_to_dawn: true,
            flicker: None,
        }
    }
}

// The vectors elide by their wire form (#1565), as a spin's axis does: an
// offset a hair off zero is written as zero, so it is left out as zero is.
crate::pds::serde_util::impl_default_eliding_serialize!(LightSource {
    shape,
    color (wire),
    brightness,
    range,
    size,
    offset (wire),
    dusk_to_dawn,
    flicker,
});

/// A fresh light's colour: a warm lamp's white, about 3,000 K, in sRGB.
pub const DEFAULT_COLOR: [f32; 3] = [1.0, 0.85, 0.65];

/// A fresh light's range (m): the helipad lamp's in the investigation's
/// prototype (#1919) - 4.5 m to a lamp's working light, and nothing from a
/// quarter past that.
pub const DEFAULT_RANGE_M: f32 = 5.6;

/// A fresh light's radius (m): a lamp's bulb.
pub const DEFAULT_SIZE_M: f32 = 0.05;

/// A spot's cone, inner and outer half angles (degrees), as fresh.
pub const DEFAULT_SPOT_CONE_DEG: [f32; 2] = [25.0, 40.0];

impl LightSource {
    /// A fresh spot ([`LightShape::fresh_spot`]): down its node's -Y, with
    /// [`DEFAULT_SPOT_CONE_DEG`], otherwise as a fresh light.
    pub fn spot() -> Self {
        Self {
            shape: LightShape::fresh_spot(),
            ..Self::default()
        }
    }

    /// Whether this light draws anything: a known shape and some
    /// brightness.
    pub fn shines(&self) -> bool {
        !matches!(self.shape, LightShape::Unknown) && self.brightness.0 > 0.0 && self.range.0 > 0.0
    }
}

/// What kind of light: one that shines every way, or a spot. An open union,
/// like every `$type`-tagged record type: a kind from a newer engine decodes
/// as [`Self::Unknown`], draws nothing, and - like every `Unknown` arm
/// (#1111) - stops this build saving the record rather than dropping it.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum LightShape {
    /// Shines every way from its point, like a bulb.
    #[serde(rename = "network.symbios.light.point")]
    #[default]
    Point,
    /// Shines along `aim`, a direction in the node's frame (down, -Y, by
    /// default): full inside `inner` degrees of it, fading to nothing at
    /// `outer` degrees - both half angles, from the aim to the cone's edge.
    /// Each of the three is left off the wire at its default
    /// ([`DEFAULT_SPOT_CONE_DEG`] for the cone), compared by its wire form.
    /// A streetlight's head, a stage light, a lighthouse's beam.
    #[serde(rename = "network.symbios.light.spot")]
    Spot {
        #[serde(default = "default_aim", skip_serializing_if = "is_default_aim")]
        aim: Fp3,
        #[serde(default = "default_inner", skip_serializing_if = "is_default_inner")]
        inner: Fp,
        #[serde(default = "default_outer", skip_serializing_if = "is_default_outer")]
        outer: Fp,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

impl LightShape {
    /// A fresh spot: down, with [`DEFAULT_SPOT_CONE_DEG`].
    pub fn fresh_spot() -> Self {
        Self::Spot {
            aim: default_aim(),
            inner: default_inner(),
            outer: default_outer(),
        }
    }
}

fn default_aim() -> Fp3 {
    Fp3([0.0, -1.0, 0.0])
}

fn is_default_aim(aim: &Fp3) -> bool {
    aim.wire() == default_aim().wire()
}

fn default_inner() -> Fp {
    Fp(DEFAULT_SPOT_CONE_DEG[0])
}

fn default_outer() -> Fp {
    Fp(DEFAULT_SPOT_CONE_DEG[1])
}

fn is_default_inner(inner: &Fp) -> bool {
    to_wire(inner.0) == to_wire(default_inner().0)
}

fn is_default_outer(outer: &Fp) -> bool {
    to_wire(outer.0) == to_wire(default_outer().0)
}

/// A flicker's fine shimmer moves this many times quicker than its broad
/// waver...
pub const FLICKER_FINE_RATE: f32 = 2.7;

/// ...and carries this share of its dip.
pub const FLICKER_FINE_SHARE: f32 = 1.0 / 3.0;

/// How a light's brightness moves: `amount` (0 to 1) is the share of it the
/// light dips by at most, and `rate` how many times a second it moves. Read
/// off the shared clock and the light's own place in the record, so every
/// peer sees one light do the same thing; a viewer's Effects setting calms
/// it at the drawing (gentler at Reduced, steady at Off). A light that dips
/// deep enough to flash may not flash faster than lightning does, at every
/// setting (`sanitize::light::max_flicker_rate_hz`).
///
/// An open union: a mode from a newer engine decodes as [`Self::Unknown`],
/// draws steady, and stops this build saving the record (#1111).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum LightFlicker {
    /// An uneven waver, like a flame's or a failing tube's: a smooth random
    /// walk of the brightness, about `rate` changes a second, with a finer
    /// shimmer over it ([`FLICKER_FINE_RATE`] times quicker, a
    /// [`FLICKER_FINE_SHARE`] of the dip).
    #[serde(rename = "network.symbios.light.flicker")]
    Flicker {
        #[serde(default = "default_flicker_amount")]
        amount: Fp,
        #[serde(default = "default_flicker_rate")]
        rate: Fp,
    },
    /// An even breathing, dimming and brightening `rate` times a second: a
    /// beacon, a warning lamp, a sign.
    #[serde(rename = "network.symbios.light.pulse")]
    Pulse {
        #[serde(default = "default_pulse_amount")]
        amount: Fp,
        #[serde(default = "default_pulse_rate")]
        rate: Fp,
    },

    #[serde(other, skip_serializing)]
    Unknown,
}

impl LightFlicker {
    /// The mode names the editor offers, in its order.
    pub const KINDS: [&'static str; 2] = ["Flicker", "Pulse"];

    /// A fresh one of the mode [`Self::label`] names: a candle's waver, a
    /// beacon's slow breath.
    pub fn fresh(kind: &str) -> Option<Self> {
        Some(match kind {
            "Flicker" => Self::Flicker {
                amount: default_flicker_amount(),
                rate: default_flicker_rate(),
            },
            "Pulse" => Self::Pulse {
                amount: default_pulse_amount(),
                rate: default_pulse_rate(),
            },
            _ => return None,
        })
    }

    /// The mode's name, as [`Self::KINDS`] spells it.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Flicker { .. } => "Flicker",
            Self::Pulse { .. } => "Pulse",
            Self::Unknown => "Unknown",
        }
    }
}

fn default_flicker_amount() -> Fp {
    Fp(0.12)
}

fn default_flicker_rate() -> Fp {
    Fp(4.0)
}

fn default_pulse_amount() -> Fp {
    Fp(0.6)
}

fn default_pulse_rate() -> Fp {
    Fp(0.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::Generator;

    fn wire<T: Serialize>(value: &T) -> String {
        serde_json::to_string(value).expect("serialise")
    }

    /// A fresh light writes nothing but its key, and every field left out
    /// reads back as fresh.
    #[test]
    fn a_fresh_light_is_an_empty_object() {
        assert_eq!(wire(&LightSource::default()), "{}");
        let back: LightSource = serde_json::from_str("{}").expect("decodes");
        assert_eq!(back, LightSource::default());
    }

    /// A node without a light writes no `light` key, so every record saved
    /// before lights keeps its bytes; and a node with one round-trips.
    #[test]
    fn a_node_writes_its_light_only_when_it_has_one() {
        let mut node = Generator::default_cuboid();
        let plain = wire(&node);
        assert!(!plain.contains("light"), "{plain}");
        node.light = Some(LightSource::default());
        let lit = wire(&node);
        assert!(lit.contains(r#""light":{}"#), "{lit}");
        let back: Generator = serde_json::from_str(&lit).expect("decodes");
        assert_eq!(back, node);
        let old: Generator = serde_json::from_str(&plain).expect("decodes");
        assert_eq!(old.light, None);
    }

    /// Every field written away from its default round-trips exactly, a
    /// spot's aim and cone and a flicker of each mode among them.
    #[test]
    fn every_field_round_trips() {
        for kind in LightFlicker::KINDS {
            let light = LightSource {
                shape: LightShape::Spot {
                    aim: Fp3([0.6, -0.8, 0.0]),
                    inner: Fp(10.0),
                    outer: Fp(30.0),
                },
                color: Fp3([0.2, 0.9, 0.4]),
                brightness: Fp(2.5),
                range: Fp(12.0),
                size: Fp(0.2),
                offset: Fp3([0.0, 2.5, -0.25]),
                dusk_to_dawn: false,
                flicker: LightFlicker::fresh(kind),
            };
            let text = wire(&light);
            assert!(text.contains(r#""dusk_to_dawn":false"#), "{text}");
            assert!(text.contains(r#""aim":[6000,-8000,0]"#), "{text}");
            let back: LightSource = serde_json::from_str(&text).expect("decodes");
            assert_eq!(back, light);
            assert_eq!(back.flicker.as_ref().map(LightFlicker::label), Some(kind));
        }
    }

    /// A spot pointing down on the default cone writes neither its aim nor
    /// its cone, nor one whose cone is a hair off the default (it is written
    /// as the default); each written away from its default is written; and
    /// one with its aim and cone left out reads them as fresh.
    #[test]
    fn a_spot_leaves_its_default_aim_and_cone_out() {
        let text = wire(&LightSource::spot());
        assert_eq!(text, r#"{"shape":{"$type":"network.symbios.light.spot"}}"#);
        let hair = LightShape::Spot {
            aim: Fp3([0.0, -1.000_01, 0.0]),
            inner: Fp(DEFAULT_SPOT_CONE_DEG[0] + 0.000_02),
            outer: Fp(DEFAULT_SPOT_CONE_DEG[1] - 0.000_02),
        };
        assert_eq!(wire(&hair), r#"{"$type":"network.symbios.light.spot"}"#);
        for (inner, outer, written) in [
            (20.0, 40.0, r#""inner":200000}"#),
            (25.0, 35.0, r#""outer":350000}"#),
        ] {
            let cone = LightShape::Spot {
                aim: default_aim(),
                inner: Fp(inner),
                outer: Fp(outer),
            };
            let text = wire(&cone);
            assert_eq!(
                text,
                format!(r#"{{"$type":"network.symbios.light.spot",{written}"#)
            );
            let back: LightShape = serde_json::from_str(&text).expect("decodes");
            assert_eq!(back, cone);
        }
        let bare: LightSource =
            serde_json::from_str(r#"{"shape":{"$type":"network.symbios.light.spot"}}"#)
                .expect("decodes");
        assert_eq!(bare, LightSource::spot());
    }

    /// A kind or a flicker from a newer engine decodes, draws nothing (or
    /// steady), and cannot be written back (#1111).
    #[test]
    fn a_shape_or_flicker_from_a_newer_engine_decodes_and_refuses_the_save() {
        let light: LightSource = serde_json::from_str(
            r#"{"shape":{"$type":"network.symbios.light.area","width":10000},
                "flicker":{"$type":"network.symbios.light.strobe"}}"#,
        )
        .expect("an open union tolerates a future arm");
        assert_eq!(light.shape, LightShape::Unknown);
        assert_eq!(light.flicker, Some(LightFlicker::Unknown));
        assert!(!light.shines());
        assert!(serde_json::to_string(&light).is_err());
    }

    #[test]
    fn a_light_shines_with_a_known_shape_and_some_brightness() {
        assert!(LightSource::default().shines());
        assert!(LightSource::spot().shines());
        let dark = LightSource {
            brightness: Fp::ZERO,
            ..LightSource::default()
        };
        assert!(!dark.shines());
    }

    /// What a light costs the record's budget: 10 bytes fresh, and 325 with
    /// every field set and each number as long as its bounds allow (#1921).
    #[test]
    fn a_light_costs_the_record_little() {
        assert_eq!(r#""light":{}"#.len(), 10);
        let full = LightSource {
            shape: LightShape::Spot {
                aim: Fp3([-0.7072, -0.7072, -0.1234]),
                inner: Fp(-88.8888),
                outer: Fp(-88.8888),
            },
            color: Fp3([0.1234, 0.5678, 0.9012]),
            brightness: Fp(-7.7777),
            range: Fp(-19.9999),
            size: Fp(-0.9999),
            offset: Fp3([-49.9999, -49.9999, -49.9999]),
            dusk_to_dawn: false,
            flicker: Some(LightFlicker::Flicker {
                amount: Fp(-0.9999),
                rate: Fp(-7.9999),
            }),
        };
        let bytes = format!(r#""light":{}"#, wire(&full)).len();
        assert!(bytes <= 335, "{bytes}: {}", wire(&full));
    }
}
