//! Shared primitives used across every PDS submodule: fixed-point wire
//! wrappers ([`Fp`], [`Fp2`], [`Fp3`], [`Fp4`], [`Fp64`]), common record
//! primitives ([`TransformData`], [`WaterRelation`], [`BiomeFilter`],
//! [`ScatterBounds`]), and the string-keyed serde helpers used by
//! [`crate::pds::Generator`] for `u64` seeds and `HashMap<u8|u16, _>` fields.

use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// DAG-CBOR float scale. Every `f32` goes to the wire as `i32` scaled by this.
pub(crate) const FP_SCALE: f32 = 10_000.0;

/// Serialize a `u64` as a JSON **string** rather than a number.
///
/// JSON has no native integer type — most parsers (including the ones in
/// front of ATProto PDSes) decode all numbers as IEEE-754 `f64`, which can
/// only represent integers exactly up to `2^53` (≈ 9.0e15). Our 64-bit FNV
/// seeds routinely run above that, and when the PDS rounds them through
/// `f64` its DAG-CBOR encoder either loses precision and rejects the
/// record or crashes outright with `500 InternalServerError`. Encoding as
/// a string side-steps the float hop entirely; the wire form is just a
/// decimal literal in quotes.
pub mod u64_as_string {
    use serde::{Deserialize, Deserializer, Serializer};

    pub fn serialize<S: Serializer>(value: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&value.to_string())
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        let s = String::deserialize(d)?;
        s.parse::<u64>().map_err(serde::de::Error::custom)
    }
}

/// Fixed-point `f32` wrapper — (de)serialises as `i32` scaled by 10_000.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp(pub f32);

impl Fp {
    pub const ZERO: Fp = Fp(0.0);
    pub const ONE: Fp = Fp(1.0);
}

impl From<f32> for Fp {
    fn from(v: f32) -> Self {
        Fp(v)
    }
}

impl From<Fp> for f32 {
    fn from(v: Fp) -> f32 {
        v.0
    }
}

impl Serialize for Fp {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((self.0 * FP_SCALE).round() as i32)
    }
}

impl<'de> Deserialize<'de> for Fp {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Fp(i32::deserialize(d)? as f32 / FP_SCALE))
    }
}

/// Fixed-point `[f32; 2]` wrapper.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp2(pub [f32; 2]);

impl From<[f32; 2]> for Fp2 {
    fn from(v: [f32; 2]) -> Self {
        Fp2(v)
    }
}

impl Serialize for Fp2 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ints = [
            (self.0[0] * FP_SCALE).round() as i32,
            (self.0[1] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Fp2 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ints = <[i32; 2]>::deserialize(d)?;
        Ok(Fp2([ints[0] as f32 / FP_SCALE, ints[1] as f32 / FP_SCALE]))
    }
}

/// Fixed-point `[f32; 3]` wrapper.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp3(pub [f32; 3]);

impl From<[f32; 3]> for Fp3 {
    fn from(v: [f32; 3]) -> Self {
        Fp3(v)
    }
}

impl Serialize for Fp3 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ints = [
            (self.0[0] * FP_SCALE).round() as i32,
            (self.0[1] * FP_SCALE).round() as i32,
            (self.0[2] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Fp3 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ints = <[i32; 3]>::deserialize(d)?;
        Ok(Fp3([
            ints[0] as f32 / FP_SCALE,
            ints[1] as f32 / FP_SCALE,
            ints[2] as f32 / FP_SCALE,
        ]))
    }
}

/// Fixed-point `[f32; 4]` wrapper (quaternions, RGBA colours).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp4(pub [f32; 4]);

impl From<[f32; 4]> for Fp4 {
    fn from(v: [f32; 4]) -> Self {
        Fp4(v)
    }
}

impl Serialize for Fp4 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ints = [
            (self.0[0] * FP_SCALE).round() as i32,
            (self.0[1] * FP_SCALE).round() as i32,
            (self.0[2] * FP_SCALE).round() as i32,
            (self.0[3] * FP_SCALE).round() as i32,
        ];
        ints.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Fp4 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let ints = <[i32; 4]>::deserialize(d)?;
        Ok(Fp4([
            ints[0] as f32 / FP_SCALE,
            ints[1] as f32 / FP_SCALE,
            ints[2] as f32 / FP_SCALE,
            ints[3] as f32 / FP_SCALE,
        ]))
    }
}

/// `f64` fixed-point wrapper — same scaling rules as [`Fp`]. Used for
/// noise frequency/scale fields in `bevy_symbios_texture` which operate
/// in double precision.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Fp64(pub f64);

impl From<f64> for Fp64 {
    fn from(v: f64) -> Self {
        Fp64(v)
    }
}

impl Serialize for Fp64 {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_i32((self.0 * FP_SCALE as f64).round() as i32)
    }
}

impl<'de> Deserialize<'de> for Fp64 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Ok(Fp64(i32::deserialize(d)? as f64 / FP_SCALE as f64))
    }
}

/// Rigid-body transform encoded as fixed-point arrays on the wire.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct TransformData {
    pub translation: Fp3,
    /// Quaternion in `[x, y, z, w]` order.
    pub rotation: Fp4,
    pub scale: Fp3,
}

impl Default for TransformData {
    fn default() -> Self {
        Self {
            translation: Fp3([0.0; 3]),
            rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
            scale: Fp3([1.0; 3]),
        }
    }
}

impl From<Transform> for TransformData {
    fn from(t: Transform) -> Self {
        Self {
            translation: Fp3(t.translation.to_array()),
            rotation: Fp4(t.rotation.to_array()),
            scale: Fp3(t.scale.to_array()),
        }
    }
}

pub(crate) fn default_true() -> bool {
    true
}

/// Whether a sampled point should be accepted above, below, or regardless of
/// the world's water surface. `Both` is the no-op default.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WaterRelation {
    /// No water constraint — keep points on either side of the surface.
    #[default]
    Both,
    /// Keep only points with world Y ≥ water surface.
    Above,
    /// Keep only points with world Y < water surface.
    Below,
}

/// Combined biome + water filter applied to each scatter sample. An empty
/// `biomes` list is "any biome"; `water = Both` is "any side". The
/// all-defaults filter is a no-op and accepts every sample.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct BiomeFilter {
    /// Allowed dominant splat layers, any-of. Empty = every biome passes.
    /// `0 = Grass, 1 = Dirt, 2 = Rock, 3 = Snow`.
    #[serde(default)]
    pub biomes: Vec<u8>,
    /// Water-surface relation. `Both` imposes no constraint.
    #[serde(default)]
    pub water: WaterRelation,
}

impl BiomeFilter {
    /// `true` when neither the biome allow-list nor the water relation
    /// imposes any constraint. Lets the caller skip expensive per-sample
    /// work when the filter is a no-op.
    pub fn is_noop(&self) -> bool {
        self.biomes.is_empty() && matches!(self.water, WaterRelation::Both)
    }

    /// Accept / reject a sample. `water_level` is `None` when the record has
    /// no water generator — in that case water-relative filters collapse to
    /// accept so a filter targeted at land-only biomes still behaves
    /// sensibly on dry-land records.
    pub fn accepts(&self, biome: u8, y: f32, water_level: Option<f32>) -> bool {
        if !self.biomes.is_empty() && !self.biomes.contains(&biome) {
            return false;
        }
        match (self.water, water_level) {
            (WaterRelation::Above, Some(wl)) => y >= wl,
            (WaterRelation::Below, Some(wl)) => y < wl,
            _ => true,
        }
    }
}

/// Scatter region shape for `Placement::Scatter`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ScatterBounds {
    #[serde(rename = "circle")]
    Circle { center: Fp2, radius: Fp },
    #[serde(rename = "rect")]
    Rect {
        center: Fp2,
        extents: Fp2,
        #[serde(default)]
        rotation: Fp,
    },
}

impl Default for ScatterBounds {
    fn default() -> Self {
        ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(64.0),
        }
    }
}

/// Trim `s` to at most `max_bytes`, walking back to the previous UTF-8
/// boundary so `String::truncate`'s boundary-panic can't be triggered by a
/// multi-byte character straddling the cap.
pub(crate) fn truncate_on_char_boundary(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
}

/// Serde helper for `HashMap<u8, V>` — JSON object keys must be strings.
pub mod map_u8_as_string {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S, V>(map: &HashMap<u8, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: serde::Serialize,
    {
        use serde::ser::SerializeMap;
        let mut map_ser = serializer.serialize_map(Some(map.len()))?;
        for (k, v) in map {
            map_ser.serialize_entry(&k.to_string(), v)?;
        }
        map_ser.end()
    }

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<HashMap<u8, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: serde::Deserialize<'de>,
    {
        let string_map = HashMap::<String, V>::deserialize(deserializer)?;
        let mut map = HashMap::new();
        for (k, v) in string_map {
            if let Ok(key) = k.parse::<u8>() {
                map.insert(key, v);
            }
        }
        Ok(map)
    }
}

/// Serde helper for `HashMap<u16, V>` — JSON object keys must be strings.
pub mod map_u16_as_string {
    use serde::{Deserialize, Deserializer, Serializer};
    use std::collections::HashMap;

    pub fn serialize<S, V>(map: &HashMap<u16, V>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: serde::Serialize,
    {
        use serde::ser::SerializeMap;
        let mut map_ser = serializer.serialize_map(Some(map.len()))?;
        for (k, v) in map {
            map_ser.serialize_entry(&k.to_string(), v)?;
        }
        map_ser.end()
    }

    pub fn deserialize<'de, D, V>(deserializer: D) -> Result<HashMap<u16, V>, D::Error>
    where
        D: Deserializer<'de>,
        V: serde::Deserialize<'de>,
    {
        let string_map = HashMap::<String, V>::deserialize(deserializer)?;
        let mut map = HashMap::new();
        for (k, v) in string_map {
            if let Ok(key) = k.parse::<u16>() {
                map.insert(key, v);
            }
        }
        Ok(map)
    }
}
