//! Shared primitives used across every PDS submodule: fixed-point wire
//! wrappers ([`Fp`], [`Fp2`], [`Fp3`], [`Fp4`], [`Fp64`]), common record
//! primitives ([`TransformData`], [`WaterRelation`], [`BiomeFilter`],
//! [`ScatterBounds`]), and the string-keyed serde helpers used by
//! [`crate::pds::Generator`] for `u64` seeds and `HashMap<u8|u16, _>` fields.

use bevy::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// DAG-CBOR float scale. Every `f32` goes to the wire as `i32` scaled by this.
pub const FP_SCALE: f32 = 10_000.0;

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

    /// Exactly zero — the `skip_serializing_if` predicate for optional
    /// scalar knobs whose "off" state is `0.0`.
    pub fn is_zero(&self) -> bool {
        self.0 == 0.0
    }

    /// `self` clamped into `[lo, hi]`, with any non-finite value collapsed
    /// to `lo`. `f32::clamp` propagates NaN, so a hostile record could
    /// otherwise carry a NaN straight through a sanitiser into a transform.
    pub(crate) fn clamped(self, lo: f32, hi: f32) -> Fp {
        if self.0.is_finite() {
            Fp(self.0.clamp(lo, hi))
        } else {
            Fp(lo)
        }
    }
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
///
/// Default-eliding wire format (#695): identity components (zero
/// translation, unit rotation, unit scale) are omitted on write and filled
/// back in by the container `#[serde(default)]`, so the identity transform
/// every child prim starts from serializes as `{}` — and callers holding a
/// fully-identity transform skip the field via [`TransformData::is_identity`].
#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TransformData {
    pub translation: Fp3,
    /// Quaternion in `[x, y, z, w]` order.
    pub rotation: Fp4,
    pub scale: Fp3,
}

crate::pds::serde_util::impl_default_eliding_serialize!(TransformData {
    translation,
    rotation,
    scale,
});

impl TransformData {
    /// `true` when the whole transform equals the identity default — the
    /// wire-format skip predicate for `transform` fields (#695).
    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }
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

/// Wire-format skip predicate for `#[serde(default = "default_true")]`
/// bool fields (#695): a still-true value is elided and restored on read.
pub(crate) fn is_true(b: &bool) -> bool {
    *b
}

/// Wire-format skip predicate for plain `#[serde(default)]` bool fields
/// (#695): a still-false value is elided and restored on read.
pub(crate) fn is_false(b: &bool) -> bool {
    !*b
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
    /// no water generator — in that case `Above` collapses to accept (all
    /// ground on a dry-land record *is* above water, so a land-targeted
    /// filter keeps behaving sensibly) while `Below` fails closed (#914):
    /// there is no below-water ground to stand on, and silently placing an
    /// aquatic species across dry land would be #335 all over again.
    ///
    /// `Above` demands a freeboard margin, not just `y >= wl`: a sample
    /// exactly at the waterline puts a tree trunk in the surf (and the
    /// water shader's wave displacement floods anything marginal), so
    /// "above water" means "far enough above to read as land".
    pub fn accepts(&self, biome: u8, y: f32, water_level: Option<f32>) -> bool {
        /// Required terrain clearance (m) over the water line for
        /// [`WaterRelation::Above`] — covers visual wave amplitude plus
        /// a believable dry bank.
        const ABOVE_FREEBOARD: f32 = 0.5;

        if !self.biomes.is_empty() && !self.biomes.contains(&biome) {
            return false;
        }
        match (self.water, water_level) {
            (WaterRelation::Above, Some(wl)) => y >= wl + ABOVE_FREEBOARD,
            (WaterRelation::Below, Some(wl)) => y < wl,
            (WaterRelation::Below, None) => false,
            _ => true,
        }
    }
}

/// Clamp a `[min, max]` band into `[lo, hi]` and swap the ends if they are
/// inverted. A `None` band is left alone — that is "no constraint", which is
/// different from an empty one.
fn sanitize_band(band: &mut Option<Fp2>, lo: f32, hi: f32) {
    if let Some(Fp2([a, b])) = band {
        let (mut x, mut y) = (
            if a.is_finite() { a.clamp(lo, hi) } else { lo },
            if b.is_finite() { b.clamp(lo, hi) } else { hi },
        );
        if x > y {
            std::mem::swap(&mut x, &mut y);
        }
        *a = x;
        *b = y;
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

/// Placement-naturalness knobs for `Placement::Scatter` (#912) — the dials
/// that turn a flat uniform sprinkle into something that reads as *grown*.
/// All-zero / `None` is the historical behaviour and is elided on the wire,
/// so records written before this struct existed decode unchanged.
///
/// The knobs are grouped into one sub-struct rather than added as five
/// sibling fields on the variant deliberately: an enum struct-variant has no
/// functional-update syntax, so every construction site must spell out every
/// field. One field means one line per site now, and WS5's microbiome dials
/// extend the struct without touching them again.
///
/// **Determinism.** Two of these (`clumping`, `edge_falloff`) are pure
/// remappings of an already-drawn sample and two (`scale_jitter`,
/// `tilt_jitter`) are drawn from a side stream keyed off the scatter's
/// `local_seed`; `max_slope_deg` only *rejects*. None of them consumes a
/// draw from the placement RNG, so changing any knob leaves every surviving
/// instance's position exactly where it was. The discipline that buys that
/// is documented in the `world_builder::compile::scatter` module (a private
/// module, so this is deliberately not a link).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq)]
pub struct ScatterNaturalness {
    /// How hard each sample is pulled toward its nearest cluster seed,
    /// `0` (flat uniform — the historical distribution) to `1` (every
    /// sample collapses onto a seed). Cluster seeds are themselves derived
    /// from `local_seed`, so a stand grows in patches rather than at an
    /// even density. `0.5` roughly halves each patch's radius, which reads
    /// as thickets with clearings between them.
    #[serde(default, skip_serializing_if = "Fp::is_zero")]
    pub clumping: Fp,
    /// Density falloff toward the bounds edge, as an exponent on the
    /// normalised radius. `0` is flat; `1` thins the rim noticeably; `2`+
    /// concentrates hard on the middle. Gives a stand a soft boundary
    /// instead of a mown circular edge.
    #[serde(default, skip_serializing_if = "Fp::is_zero")]
    pub edge_falloff: Fp,
    /// Per-instance uniform scale spread, as a half-width in log space:
    /// the scale factor lands in `[e^-j, e^+j]`. `0.18` gives roughly
    /// 0.84×–1.20×, which is what kills the pixel-identical-clone read
    /// without any instance looking mis-sized.
    #[serde(default, skip_serializing_if = "Fp::is_zero")]
    pub scale_jitter: Fp,
    /// Per-instance lean off vertical, in radians, applied in a random
    /// azimuth. `0.12` ≈ 7°, about right for grass and shrubs; trees want
    /// less (a visibly leaning trunk reads as damaged, not natural).
    #[serde(default, skip_serializing_if = "Fp::is_zero")]
    pub tilt_jitter: Fp,
    /// Reject samples on ground steeper than this, in **degrees** from
    /// horizontal. `None` (the default) imposes no slope limit, which is
    /// what every scatter did before this field existed — slope reached
    /// placement only indirectly, through whichever splat layer the
    /// terrain config's height+slope bands made dominant.
    ///
    /// Requires a heightmap and a terrain generator to evaluate; without
    /// them a scatter that sets this places nothing, matching how the
    /// biome allow-list fails closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_slope_deg: Option<Fp>,
    /// Accept only samples whose ground sits within this band **above the
    /// room's water line**, as `[min, max]` metres (#913).
    ///
    /// This is the moisture proxy, and it is deliberately a *height* band
    /// rather than a horizontal distance to water. On any shoreline that
    /// is not a cliff the two agree closely, a height band costs nothing
    /// (the sampler already knows the ground height and the water level),
    /// and the horizontal version would need a per-room distance field
    /// that has to be built, stored and freed. Where they disagree — a
    /// cliff edge — the answer a height band gives is the one you want
    /// anyway, since nothing riparian grows on a cliff.
    ///
    /// `[0, 6]` is a riparian band: reed beds and damp-loving cover that
    /// hug the shore. A high `min` gives the opposite — dry ridge species
    /// that should stay well clear of standing water.
    ///
    /// Supersedes what `BiomeFilter::water` can express: that is a
    /// half-space test (above / below), so before this there was no way to
    /// say "within N metres of the waterline" and reeds could only be
    /// scattered across all dry land (noted as a gap when the ground-cover
    /// tier landed).
    ///
    /// Requires a heightmap **and** a water line; on a record with no
    /// water generator a scatter that sets this places nothing, matching
    /// how the other terrain filters fail closed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub above_water_band: Option<Fp2>,
    /// Accept only samples whose ground height falls in this world-Y band,
    /// as `[min, max]` metres (#913).
    ///
    /// Altitude zonation: the treeline that keeps conifers off the peaks,
    /// the alpine cushion plants that only appear above one, the valley
    /// species that stop partway up. Unlike [`Self::above_water_band`]
    /// this needs no water line — only the heightmap.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub altitude_band: Option<Fp2>,
}

impl ScatterNaturalness {
    /// `true` when every knob is at its historical default, so the sampler
    /// can take the plain uniform path and the field is elided on the wire.
    pub fn is_noop(&self) -> bool {
        *self == Self::default()
    }

    /// Clamp every knob into its supported range. `clumping` stops short of
    /// `1.0` because a full collapse stacks every instance on a handful of
    /// points; the rest are generous bounds that only exist to keep a
    /// hostile record from producing NaN or absurd transforms.
    pub fn sanitize(&mut self) {
        self.clumping = self.clumping.clamped(0.0, 0.95);
        self.edge_falloff = self.edge_falloff.clamped(0.0, 8.0);
        self.scale_jitter = self.scale_jitter.clamped(0.0, 1.5);
        self.tilt_jitter = self.tilt_jitter.clamped(0.0, std::f32::consts::FRAC_PI_3);
        if let Some(deg) = &mut self.max_slope_deg {
            *deg = deg.clamped(0.0, 90.0);
        }
        // Bands are ordered and finite. An inverted band would accept
        // nothing, which is a silent "this scatter vanished" rather than an
        // error, so normalise instead of rejecting.
        sanitize_band(&mut self.above_water_band, -1_000.0, 10_000.0);
        sanitize_band(&mut self.altitude_band, -10_000.0, 10_000.0);
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
