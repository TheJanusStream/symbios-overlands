//! ATProto PDS integration: DID resolution, room-record fetch, and upsert,
//! plus the `RoomRecord` lexicon that describes a room as a data-driven
//! *recipe*.
//!
//! The record is stored at `collection = network.symbios.overlands.room,
//! rkey = self`.  A record is composed of three open unions:
//!
//! * `generators`  — named blueprints (terrain / water / shape / lsystem…)
//! * `placements`  — how and where those generators are instantiated
//! * `traits`      — ECS components attached to entities a generator spawns
//!
//! Every union uses `#[serde(other)] Unknown` so a client visiting a room
//! authored by a newer version of the engine skips the unrecognised variants
//! instead of crashing its deserializer. This is how the schema evolves
//! without breaking older clients.
//!
//! **DAG-CBOR float ban.** ATProto records are encoded as DAG-CBOR, which
//! forbids floats entirely — a PDS returns `400 InvalidRequest` the moment
//! it sees `0.98` in a record body. Every float-bearing field is therefore
//! wrapped in [`Fp`] (or its fixed-length array siblings [`Fp2`], [`Fp3`],
//! [`Fp4`]), which multiply by `FP_SCALE` and round to `i32` on the wire.
//! The wrappers are fully transparent in editor code (`.0` returns the
//! underlying `f32`), so the heightmap / splat / L-system callers never see
//! the fixed-point hop.

use bevy::prelude::*;
use bevy_symbios_multiuser::auth::AtprotoSession;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::collections::HashMap;

const COLLECTION: &str = "network.symbios.overlands.room";
const AVATAR_COLLECTION: &str = "network.symbios.overlands.avatar";

// ---------------------------------------------------------------------------
// Fixed-point serde wrapper types (DAG-CBOR float workaround)
// ---------------------------------------------------------------------------
//
// DAG-CBOR is strict about numeric types — any `0.98` in the record body
// earns a `400 InvalidRequest` from the PDS. `Fp` wraps an `f32` and
// serialises as `i32` (×10_000); `Fp2`/`Fp3`/`Fp4` do the same for small
// fixed arrays. Rust-side callers still work with plain floats via the
// public `.0` field / `From` conversions.

const FP_SCALE: f32 = 10_000.0;

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

// ---------------------------------------------------------------------------
// Primitives
// ---------------------------------------------------------------------------

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

/// Scatter region shape for `Placement::Scatter`.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "type")]
pub enum ScatterBounds {
    #[serde(rename = "circle")]
    Circle { center: Fp2, radius: Fp },
    #[serde(rename = "rect")]
    Rect { center: Fp2, extents: Fp2 },
}

impl Default for ScatterBounds {
    fn default() -> Self {
        ScatterBounds::Circle {
            center: Fp2([0.0, 0.0]),
            radius: Fp(64.0),
        }
    }
}

// ---------------------------------------------------------------------------
// Terrain generator payload (ported from symbios-ground-lab)
// ---------------------------------------------------------------------------

/// Which base terrain algorithm to run. Mirrors `ground-lab::GeneratorKind`
/// but tagged for lexicon-safe serde.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignGeneratorKind {
    FbmNoise,
    DiamondSquare,
    #[default]
    VoronoiTerracing,
}

/// Full terrain configuration stored inside a `Generator::Terrain` variant.
/// This is a serialisable mirror of `ground-lab::TerrainConfig` — all `f32`
/// fields are wrapped in [`Fp`] so the record stays DAG-CBOR compliant.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignTerrainConfig {
    // Grid / world
    pub grid_size: u32,
    pub cell_scale: Fp,
    pub height_scale: Fp,

    // Algorithm selection
    pub generator_kind: SovereignGeneratorKind,
    #[serde(with = "u64_as_string")]
    pub seed: u64,

    // FBM params
    pub octaves: u32,
    pub persistence: Fp,
    pub lacunarity: Fp,
    pub base_frequency: Fp,

    // Diamond Square params
    pub ds_roughness: Fp,

    // Voronoi params
    pub voronoi_num_seeds: u32,
    pub voronoi_num_terraces: u32,

    // Hydraulic erosion
    pub erosion_enabled: bool,
    pub erosion_drops: u32,
    pub inertia: Fp,
    pub erosion_rate: Fp,
    pub deposition_rate: Fp,
    pub evaporation_rate: Fp,
    pub capacity_factor: Fp,

    // Thermal erosion
    pub thermal_enabled: bool,
    pub thermal_iterations: u32,
    pub thermal_talus_angle: Fp,

    // Material (splat) config
    pub material: SovereignMaterialConfig,
}

impl Default for SovereignTerrainConfig {
    fn default() -> Self {
        Self {
            grid_size: 512,
            cell_scale: Fp(2.0),
            height_scale: Fp(50.0),

            generator_kind: SovereignGeneratorKind::VoronoiTerracing,
            seed: 42,

            octaves: 6,
            persistence: Fp(0.5),
            lacunarity: Fp(2.0),
            base_frequency: Fp(4.0),

            ds_roughness: Fp(0.5),

            voronoi_num_seeds: 1000,
            voronoi_num_terraces: 2,

            erosion_enabled: true,
            erosion_drops: 50_000,
            inertia: Fp(0.05),
            erosion_rate: Fp(0.3),
            deposition_rate: Fp(0.3),
            evaporation_rate: Fp(0.02),
            capacity_factor: Fp(8.0),

            thermal_enabled: true,
            thermal_iterations: 30,
            thermal_talus_angle: Fp(0.05),

            material: SovereignMaterialConfig::default(),
        }
    }
}

/// Splat rule for a single texture layer. `[0, 1]` normalised height; slope
/// is raw gradient magnitude in `[0, ∞)` (1.0 ≈ 45°).
#[derive(Serialize, Deserialize, Clone, Copy, Debug)]
pub struct SovereignSplatRule {
    pub height_min: Fp,
    pub height_max: Fp,
    pub slope_min: Fp,
    pub slope_max: Fp,
    pub sharpness: Fp,
}

/// Procedural "ground" texture parameters (grass / dirt / snow layers).
/// Mirrors `bevy_symbios_texture::ground::GroundConfig` with fixed-point wrappers.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignGroundConfig {
    pub seed: u32,
    pub macro_scale: Fp64,
    pub macro_octaves: u32,
    pub micro_scale: Fp64,
    pub micro_octaves: u32,
    pub micro_weight: Fp64,
    pub color_dry: Fp3,
    pub color_moist: Fp3,
    pub normal_strength: Fp,
}

/// Procedural "rock" texture parameters. Mirrors
/// `bevy_symbios_texture::rock::RockConfig`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignRockConfig {
    pub seed: u32,
    pub scale: Fp64,
    pub octaves: u32,
    pub attenuation: Fp64,
    pub color_light: Fp3,
    pub color_dark: Fp3,
    pub normal_strength: Fp,
}

/// Four-layer splat/texture configuration for a terrain generator.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignMaterialConfig {
    pub texture_size: u32,
    pub tile_scale: Fp,
    /// Splat rules for channels R, G, B, A — Grass / Dirt / Rock / Snow.
    pub rules: [SovereignSplatRule; 4],
    pub grass: SovereignGroundConfig,
    pub dirt: SovereignGroundConfig,
    pub rock: SovereignRockConfig,
    pub snow: SovereignGroundConfig,
}

impl Default for SovereignMaterialConfig {
    fn default() -> Self {
        Self {
            texture_size: 512,
            tile_scale: Fp(90.0),
            rules: [
                // R — Grass
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(0.45),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.30),
                    sharpness: Fp(0.5),
                },
                // G — Dirt
                SovereignSplatRule {
                    height_min: Fp(0.30),
                    height_max: Fp(0.65),
                    slope_min: Fp(0.0),
                    slope_max: Fp(0.55),
                    sharpness: Fp(0.5),
                },
                // B — Rock
                SovereignSplatRule {
                    height_min: Fp(0.0),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.25),
                    slope_max: Fp(1.0),
                    sharpness: Fp(0.5),
                },
                // A — Snow
                SovereignSplatRule {
                    height_min: Fp(0.88),
                    height_max: Fp(1.0),
                    slope_min: Fp(0.0),
                    slope_max: Fp(1.0),
                    sharpness: Fp(4.0),
                },
            ],
            grass: SovereignGroundConfig {
                seed: 1,
                macro_scale: Fp64(2.5),
                macro_octaves: 4,
                micro_scale: Fp64(10.0),
                micro_octaves: 3,
                micro_weight: Fp64(0.3),
                color_dry: Fp3([0.07, 0.12, 0.03]),
                color_moist: Fp3([0.03, 0.07, 0.01]),
                normal_strength: Fp(4.5),
            },
            dirt: SovereignGroundConfig {
                seed: 13,
                macro_scale: Fp64(2.0),
                macro_octaves: 5,
                micro_scale: Fp64(8.0),
                micro_octaves: 4,
                micro_weight: Fp64(0.35),
                color_dry: Fp3([0.52, 0.40, 0.26]),
                color_moist: Fp3([0.28, 0.20, 0.12]),
                normal_strength: Fp(2.0),
            },
            rock: SovereignRockConfig {
                seed: 7,
                scale: Fp64(3.0),
                octaves: 8,
                attenuation: Fp64(2.0),
                color_light: Fp3([0.37, 0.42, 0.36]),
                color_dark: Fp3([0.22, 0.20, 0.18]),
                normal_strength: Fp(4.0),
            },
            snow: SovereignGroundConfig {
                seed: 99,
                macro_scale: Fp64(4.0),
                macro_octaves: 3,
                micro_scale: Fp64(12.0),
                micro_octaves: 3,
                micro_weight: Fp64(0.4),
                color_dry: Fp3([0.95, 0.95, 0.98]),
                color_moist: Fp3([0.80, 0.82, 0.88]),
                normal_strength: Fp(0.8),
            },
        }
    }
}

// ---------------------------------------------------------------------------
// L-system generator payload (ported from lsystem-explorer)
// ---------------------------------------------------------------------------

/// Which procedural texture generator to bake for a given material slot.
/// Mirrors `bevy_symbios::materials::TextureType`, with only the foliage
/// variants the world builder currently supports getting async-baked on a
/// per-slot basis. The non-foliage variants stay here so a record authored
/// on a newer client still round-trips even if we don't bake them yet.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignTextureType {
    #[default]
    None,
    Grid,
    Noise,
    Checker,
    Leaf,
    Twig,
    Bark,
}

/// Fixed-point mirror of `bevy_symbios_texture::leaf::LeafConfig`. Every
/// `f64` field becomes an [`Fp64`] and every `[f32; 3]` becomes an [`Fp3`]
/// so the record stays DAG-CBOR clean. See `to_leaf_config` for the
/// conversion back into the upstream type consumed by `LeafGenerator`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignLeafConfig {
    pub seed: u32,
    pub color_base: Fp3,
    pub color_edge: Fp3,
    pub serration_strength: Fp64,
    pub vein_angle: Fp64,
    pub micro_detail: Fp64,
    pub normal_strength: Fp,
    pub lobe_count: Fp64,
    pub lobe_depth: Fp64,
    pub lobe_sharpness: Fp64,
    pub petiole_length: Fp64,
    pub petiole_width: Fp64,
    pub midrib_width: Fp64,
    pub vein_count: Fp64,
    pub venule_strength: Fp64,
}

impl Default for SovereignLeafConfig {
    fn default() -> Self {
        Self {
            seed: 0,
            color_base: Fp3([0.12, 0.19, 0.11]),
            color_edge: Fp3([0.35, 0.28, 0.05]),
            serration_strength: Fp64(0.12),
            vein_angle: Fp64(2.5),
            micro_detail: Fp64(0.3),
            normal_strength: Fp(1.0),
            lobe_count: Fp64(4.0),
            lobe_depth: Fp64(0.23),
            lobe_sharpness: Fp64(1.0),
            petiole_length: Fp64(0.12),
            petiole_width: Fp64(0.022),
            midrib_width: Fp64(0.12),
            vein_count: Fp64(6.0),
            venule_strength: Fp64(0.50),
        }
    }
}

/// Fixed-point mirror of `bevy_symbios_texture::twig::TwigConfig`. Shares a
/// nested `SovereignLeafConfig` so the twig's leaf appearance is edited
/// once and baked into both the twig card and any standalone leaf cards on
/// the same generator.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignTwigConfig {
    pub leaf: SovereignLeafConfig,
    pub stem_color: Fp3,
    pub stem_half_width: Fp64,
    pub leaf_pairs: u32,
    pub leaf_angle: Fp64,
    pub leaf_scale: Fp64,
    pub stem_curve: Fp64,
    pub sympodial: bool,
}

impl Default for SovereignTwigConfig {
    fn default() -> Self {
        // Mirrors upstream default: `FRAC_PI_2 - 0.35` ≈ 1.2208 radians.
        Self {
            leaf: SovereignLeafConfig::default(),
            stem_color: Fp3([0.18, 0.08, 0.06]),
            stem_half_width: Fp64(0.021),
            leaf_pairs: 4,
            leaf_angle: Fp64(std::f64::consts::FRAC_PI_2 - 0.35),
            leaf_scale: Fp64(0.38),
            stem_curve: Fp64(0.015),
            sympodial: true,
        }
    }
}

/// Fixed-point mirror of `bevy_symbios_texture::bark::BarkConfig`.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignBarkConfig {
    pub seed: u32,
    pub scale: Fp64,
    pub octaves: u32,
    pub warp_u: Fp64,
    pub warp_v: Fp64,
    pub color_light: Fp3,
    pub color_dark: Fp3,
    pub normal_strength: Fp,
    pub furrow_multiplier: Fp64,
    pub furrow_scale_u: Fp64,
    pub furrow_scale_v: Fp64,
    pub furrow_shape: Fp64,
}

impl Default for SovereignBarkConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            scale: Fp64(2.0),
            octaves: 6,
            warp_u: Fp64(0.15),
            warp_v: Fp64(0.55),
            color_light: Fp3([0.45, 0.28, 0.14]),
            color_dark: Fp3([0.09, 0.05, 0.03]),
            normal_strength: Fp(3.0),
            furrow_multiplier: Fp64(0.78),
            furrow_scale_u: Fp64(2.0),
            furrow_scale_v: Fp64(0.48),
            furrow_shape: Fp64(2.0),
        }
    }
}

impl SovereignLeafConfig {
    /// Convert this DAG-CBOR-safe mirror back into the upstream
    /// `bevy_symbios_texture::leaf::LeafConfig` the `LeafGenerator` consumes.
    pub fn to_leaf_config(&self) -> bevy_symbios_texture::leaf::LeafConfig {
        bevy_symbios_texture::leaf::LeafConfig {
            seed: self.seed,
            color_base: self.color_base.0,
            color_edge: self.color_edge.0,
            serration_strength: self.serration_strength.0,
            vein_angle: self.vein_angle.0,
            micro_detail: self.micro_detail.0,
            normal_strength: self.normal_strength.0,
            lobe_count: self.lobe_count.0,
            lobe_depth: self.lobe_depth.0,
            lobe_sharpness: self.lobe_sharpness.0,
            petiole_length: self.petiole_length.0,
            petiole_width: self.petiole_width.0,
            midrib_width: self.midrib_width.0,
            vein_count: self.vein_count.0,
            venule_strength: self.venule_strength.0,
        }
    }
}

impl SovereignTwigConfig {
    pub fn to_twig_config(&self) -> bevy_symbios_texture::twig::TwigConfig {
        bevy_symbios_texture::twig::TwigConfig {
            leaf: self.leaf.to_leaf_config(),
            stem_color: self.stem_color.0,
            stem_half_width: self.stem_half_width.0,
            leaf_pairs: self.leaf_pairs as usize,
            leaf_angle: self.leaf_angle.0,
            leaf_scale: self.leaf_scale.0,
            stem_curve: self.stem_curve.0,
            sympodial: self.sympodial,
        }
    }
}

impl SovereignBarkConfig {
    pub fn to_bark_config(&self) -> bevy_symbios_texture::bark::BarkConfig {
        bevy_symbios_texture::bark::BarkConfig {
            seed: self.seed,
            scale: self.scale.0,
            octaves: self.octaves as usize,
            warp_u: self.warp_u.0,
            warp_v: self.warp_v.0,
            color_light: self.color_light.0,
            color_dark: self.color_dark.0,
            normal_strength: self.normal_strength.0,
            furrow_multiplier: self.furrow_multiplier.0,
            furrow_scale_u: self.furrow_scale_u.0,
            furrow_scale_v: self.furrow_scale_v.0,
            furrow_shape: self.furrow_shape.0,
        }
    }
}

/// Per-slot material settings for an L-system generator — mirrors
/// `bevy_symbios::materials::MaterialSettings` with DAG-CBOR-safe numeric
/// fields. The three nested `*_config` structs hold procedural texture
/// parameters that `world_builder` hands to `bevy_symbios_texture`
/// generators whenever `texture_type` asks for them.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SovereignMaterialSettings {
    pub base_color: Fp3,
    pub emission_color: Fp3,
    pub emission_strength: Fp,
    pub roughness: Fp,
    pub metallic: Fp,
    #[serde(default = "default_uv_scale")]
    pub uv_scale: Fp,
    #[serde(default)]
    pub texture_type: SovereignTextureType,
    #[serde(default)]
    pub leaf_config: SovereignLeafConfig,
    #[serde(default)]
    pub twig_config: SovereignTwigConfig,
    #[serde(default)]
    pub bark_config: SovereignBarkConfig,
}

fn default_uv_scale() -> Fp {
    Fp(1.0)
}

impl Default for SovereignMaterialSettings {
    fn default() -> Self {
        Self {
            base_color: Fp3([0.6, 0.4, 0.2]),
            emission_color: Fp3([0.0, 0.0, 0.0]),
            emission_strength: Fp(0.0),
            roughness: Fp(0.5),
            metallic: Fp(0.0),
            uv_scale: Fp(1.0),
            texture_type: SovereignTextureType::None,
            leaf_config: SovereignLeafConfig::default(),
            twig_config: SovereignTwigConfig::default(),
            bark_config: SovereignBarkConfig::default(),
        }
    }
}

/// Prop mesh shapes for `PropMeshType` slots. Mirrors
/// `lsystem-explorer::PropMeshType`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum PropMeshType {
    #[default]
    Leaf,
    Twig,
    Sphere,
    Cone,
    Cylinder,
    Cube,
}

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

// ---------------------------------------------------------------------------
// Open unions: Generators and Placements
// ---------------------------------------------------------------------------

/// Blueprint for something that can be spawned into a room.  Open union:
/// unknown tags deserialize to `Unknown` instead of failing.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
// The Terrain variant carries a full `SovereignTerrainConfig` (~400 bytes);
// boxing it would force serde through a wrapping layer that breaks the
// current round-trip tests and the Raw JSON editor format. Generators are
// kept by owning HashMaps, not in hot paths, so the size penalty is fine.
#[allow(clippy::large_enum_variant)]
pub enum Generator {
    #[serde(rename = "network.symbios.gen.terrain")]
    Terrain(SovereignTerrainConfig),

    #[serde(rename = "network.symbios.gen.water")]
    Water { level_offset: Fp },

    #[serde(rename = "network.symbios.gen.shape")]
    Shape { style: String, floors: u32 },

    #[serde(rename = "network.symbios.gen.lsystem")]
    LSystem {
        source_code: String,
        finalization_code: String,
        iterations: u32,
        #[serde(with = "u64_as_string")]
        seed: u64,
        angle: Fp,
        step: Fp,
        width: Fp,
        elasticity: Fp,
        tropism: Option<Fp3>,
        /// Material slot id → PBR settings.
        #[serde(with = "map_u8_as_string")]
        materials: HashMap<u8, SovereignMaterialSettings>,
        /// Prop id → mesh shape.
        #[serde(with = "map_u16_as_string")]
        prop_mappings: HashMap<u16, PropMeshType>,
        prop_scale: Fp,
        mesh_resolution: u32,
    },

    #[serde(other)]
    Unknown,
}

/// Where and how a `Generator` is instantiated.
#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(tag = "$type")]
pub enum Placement {
    #[serde(rename = "network.symbios.place.absolute")]
    Absolute {
        generator_ref: String,
        transform: TransformData,
    },

    #[serde(rename = "network.symbios.place.scatter")]
    Scatter {
        generator_ref: String,
        bounds: ScatterBounds,
        count: u32,
        #[serde(with = "u64_as_string")]
        local_seed: u64,
        /// Optional biome filter — scatter points whose dominant splat
        /// channel does not match this id are discarded.
        /// `0 = Grass, 1 = Dirt, 2 = Rock, 3 = Snow`. `None` = everywhere.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        biome_filter: Option<u8>,
    },

    #[serde(other)]
    Unknown,
}

// ---------------------------------------------------------------------------
// Avatar record — player vessel / body definition
// ---------------------------------------------------------------------------
//
// Each player's avatar is published to their own PDS at
// `collection = network.symbios.overlands.avatar, rkey = self`. The body is
// an open union tagged by `$type` so future vessels (e.g. submarine,
// glider) can extend the schema without breaking older clients — unknown
// tags deserialize to `AvatarBody::Unknown`, which the player-side fallback
// converts to a default hover-rover.
//
// **Phenotype vs kinematics.** The body carries two disjoint sub-records:
//   - `phenotype` — shape/scales/colours. Remote peers render this.
//   - `kinematics` — physics tuning (spring stiffness, drive force, jump
//     impulse). Remote peers *deserialize but ignore* these so a malicious
//     PDS cannot crash guests by broadcasting pathological spring constants.

/// Rover chassis construction + material, DAG-CBOR safe via `Fp*` wrappers.
/// Serialisable mirror of [`crate::protocol::AirshipParams`] — use
/// [`RoverPhenotype::to_airship_params`] to hand a plain-float copy to the
/// existing `rebuild_airship_children` builder.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoverPhenotype {
    pub hull_length: Fp,
    pub hull_width: Fp,
    pub hull_depth: Fp,
    pub pontoon_spread: Fp,
    pub pontoon_length: Fp,
    pub pontoon_width: Fp,
    pub pontoon_height: Fp,
    pub pontoon_shape: crate::protocol::PontoonShape,
    pub strut_drop: Fp,
    pub mast_height: Fp,
    pub mast_radius: Fp,
    pub mast_offset: Fp2,
    pub sail_size: Fp,
    pub hull_color: Fp3,
    pub pontoon_color: Fp3,
    pub mast_color: Fp3,
    pub strut_color: Fp3,
    pub metallic: Fp,
    pub roughness: Fp,
}

impl Default for RoverPhenotype {
    fn default() -> Self {
        use crate::config::airship as cfg;
        Self {
            hull_length: Fp(cfg::HULL_LENGTH),
            hull_width: Fp(cfg::HULL_WIDTH),
            hull_depth: Fp(cfg::HULL_DEPTH),
            pontoon_spread: Fp(cfg::PONTOON_SPREAD),
            pontoon_length: Fp(cfg::PONTOON_LENGTH),
            pontoon_width: Fp(cfg::PONTOON_WIDTH),
            pontoon_height: Fp(cfg::PONTOON_HEIGHT),
            pontoon_shape: crate::protocol::PontoonShape::default(),
            strut_drop: Fp(cfg::STRUT_DROP),
            mast_height: Fp(cfg::MAST_HEIGHT),
            mast_radius: Fp(cfg::MAST_RADIUS),
            mast_offset: Fp2(cfg::MAST_OFFSET),
            sail_size: Fp(cfg::SAIL_SIZE),
            hull_color: Fp3(cfg::HULL_COLOR),
            pontoon_color: Fp3(cfg::PONTOON_COLOR),
            mast_color: Fp3(cfg::MAST_COLOR),
            strut_color: Fp3(cfg::STRUT_COLOR),
            metallic: Fp(cfg::METALLIC),
            roughness: Fp(cfg::ROUGHNESS),
        }
    }
}

impl RoverPhenotype {
    /// Bake into the plain-float struct consumed by
    /// [`crate::rover::rebuild_airship_children`].
    pub fn to_airship_params(&self) -> crate::protocol::AirshipParams {
        crate::protocol::AirshipParams {
            hull_length: self.hull_length.0,
            hull_width: self.hull_width.0,
            pontoon_spread: self.pontoon_spread.0,
            pontoon_length: self.pontoon_length.0,
            pontoon_width: self.pontoon_width.0,
            pontoon_height: self.pontoon_height.0,
            pontoon_shape: self.pontoon_shape,
            strut_drop: self.strut_drop.0,
            mast_height: self.mast_height.0,
            mast_radius: self.mast_radius.0,
            mast_offset: self.mast_offset.0,
            sail_size: self.sail_size.0,
            hull_depth: self.hull_depth.0,
            hull_color: self.hull_color.0,
            pontoon_color: self.pontoon_color.0,
            mast_color: self.mast_color.0,
            strut_color: self.strut_color.0,
            metallic: self.metallic.0,
            roughness: self.roughness.0,
        }
    }
}

/// Rover physics tuning. Deserialized on remote peers but ignored at apply
/// time — only the local player's kinematics drive the rigid body.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct RoverKinematics {
    pub suspension_rest_length: Fp,
    pub suspension_stiffness: Fp,
    pub suspension_damping: Fp,
    pub drive_force: Fp,
    pub turn_torque: Fp,
    pub lateral_grip: Fp,
    pub jump_force: Fp,
    pub uprighting_torque: Fp,
    pub linear_damping: Fp,
    pub angular_damping: Fp,
    pub mass: Fp,
    pub water_rest_length: Fp,
    pub buoyancy_strength: Fp,
    pub buoyancy_damping: Fp,
    pub buoyancy_max_depth: Fp,
}

impl Default for RoverKinematics {
    fn default() -> Self {
        use crate::config::rover as cfg;
        Self {
            suspension_rest_length: Fp(cfg::SUSPENSION_REST_LENGTH),
            suspension_stiffness: Fp(cfg::SUSPENSION_STIFFNESS),
            suspension_damping: Fp(cfg::SUSPENSION_DAMPING),
            drive_force: Fp(cfg::DRIVE_FORCE),
            turn_torque: Fp(cfg::TURN_TORQUE),
            lateral_grip: Fp(cfg::LATERAL_GRIP),
            jump_force: Fp(cfg::JUMP_FORCE),
            uprighting_torque: Fp(cfg::UPRIGHTING_TORQUE),
            linear_damping: Fp(cfg::LINEAR_DAMPING),
            angular_damping: Fp(cfg::ANGULAR_DAMPING),
            mass: Fp(cfg::MASS),
            water_rest_length: Fp(cfg::WATER_REST_LENGTH),
            buoyancy_strength: Fp(cfg::BUOYANCY_STRENGTH),
            buoyancy_damping: Fp(cfg::BUOYANCY_DAMPING),
            buoyancy_max_depth: Fp(cfg::BUOYANCY_MAX_DEPTH),
        }
    }
}

/// Humanoid body construction (blocky/robotic).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidPhenotype {
    /// Total standing height (m).
    pub height: Fp,
    /// Torso half-width in X (m).
    pub torso_half_width: Fp,
    /// Torso half-depth in Z (m).
    pub torso_half_depth: Fp,
    /// Head edge length (m).
    pub head_size: Fp,
    /// Limb thickness (m).
    pub limb_thickness: Fp,
    /// Arm length expressed as a ratio of torso height (≈0.5–1.5).
    #[serde(default = "default_arm_ratio")]
    pub arm_length_ratio: Fp,
    /// Leg length expressed as a ratio of total height (≈0.3–0.6).
    #[serde(default = "default_leg_ratio")]
    pub leg_length_ratio: Fp,
    /// Show the owner's ATProto profile picture on the chest.
    #[serde(default = "default_show_badge")]
    pub show_badge: bool,
    pub body_color: Fp3,
    pub head_color: Fp3,
    pub limb_color: Fp3,
    pub metallic: Fp,
    pub roughness: Fp,
}

fn default_arm_ratio() -> Fp {
    Fp(0.9)
}
fn default_leg_ratio() -> Fp {
    Fp(0.45)
}
fn default_show_badge() -> bool {
    true
}

impl Default for HumanoidPhenotype {
    fn default() -> Self {
        Self {
            height: Fp(1.8),
            torso_half_width: Fp(0.28),
            torso_half_depth: Fp(0.18),
            head_size: Fp(0.28),
            limb_thickness: Fp(0.12),
            arm_length_ratio: default_arm_ratio(),
            leg_length_ratio: default_leg_ratio(),
            show_badge: default_show_badge(),
            body_color: Fp3([0.25, 0.45, 0.75]),
            head_color: Fp3([0.85, 0.75, 0.65]),
            limb_color: Fp3([0.20, 0.20, 0.25]),
            metallic: Fp(0.2),
            roughness: Fp(0.7),
        }
    }
}

/// Humanoid movement tuning.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct HumanoidKinematics {
    /// Target linear speed when input is held (m/s).
    pub walk_speed: Fp,
    /// Per-second velocity correction applied toward the target (1/s).
    pub acceleration: Fp,
    /// Instantaneous upward impulse magnitude on jump (N·s).
    pub jump_impulse: Fp,
    pub mass: Fp,
    pub linear_damping: Fp,
}

impl Default for HumanoidKinematics {
    fn default() -> Self {
        Self {
            walk_speed: Fp(4.0),
            acceleration: Fp(12.0),
            jump_impulse: Fp(450.0),
            mass: Fp(80.0),
            linear_damping: Fp(0.3),
        }
    }
}

/// Open-union avatar body. Future vehicle types add new `#[serde(rename)]`
/// variants; older clients fall through to `Unknown`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum AvatarBody {
    #[serde(rename = "network.symbios.avatar.hover_rover")]
    HoverRover {
        phenotype: RoverPhenotype,
        kinematics: RoverKinematics,
    },

    #[serde(rename = "network.symbios.avatar.humanoid")]
    Humanoid {
        phenotype: HumanoidPhenotype,
        kinematics: HumanoidKinematics,
    },

    #[serde(other)]
    Unknown,
}

impl AvatarBody {
    /// Stable string tag used by hot-swap detection so a variant change
    /// (HoverRover → Humanoid) can be seen without a full `==` compare.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            AvatarBody::HoverRover { .. } => "hover_rover",
            AvatarBody::Humanoid { .. } => "humanoid",
            AvatarBody::Unknown => "unknown",
        }
    }
}

/// The top-level avatar record. Stored at
/// `network.symbios.overlands.avatar / self` on the player's PDS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Resource)]
pub struct AvatarRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub body: AvatarBody,
}

impl AvatarRecord {
    /// Synthesise a starting hover-rover with a deterministic palette derived
    /// from the owner's DID — every fresh player gets a unique-coloured
    /// vessel without ever touching the editor.
    pub fn default_for_did(did: &str) -> Self {
        // FNV-1a 64-bit, identical to `RoomRecord::default_for_did`.
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in did.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        // Derive three hue-shifted colours from the hash by taking 8-bit
        // slots in HSV-ish space — any deterministic expansion works, the
        // only requirement is stability across peers.
        let hue = |n: u32| -> [f32; 3] {
            let r = ((hash.rotate_left(n) & 0xFF) as f32) / 255.0;
            let g = ((hash.rotate_left(n + 8) & 0xFF) as f32) / 255.0;
            let b = ((hash.rotate_left(n + 16) & 0xFF) as f32) / 255.0;
            // Bias away from near-black so new players aren't invisible.
            [0.25 + r * 0.70, 0.25 + g * 0.70, 0.25 + b * 0.70]
        };

        let phenotype = RoverPhenotype {
            hull_color: Fp3(hue(0)),
            pontoon_color: Fp3(hue(3)),
            mast_color: Fp3(hue(7)),
            strut_color: Fp3(hue(11)),
            ..RoverPhenotype::default()
        };

        Self {
            lex_type: AVATAR_COLLECTION.into(),
            body: AvatarBody::HoverRover {
                phenotype,
                kinematics: RoverKinematics::default(),
            },
        }
    }

    /// Clamp every numeric field so a malicious PDS (or forward-compat
    /// client shipping a record we cannot fully model) cannot weaponise the
    /// record to panic Bevy primitive constructors.
    pub fn sanitize(&mut self) {
        const MIN_DIM: f32 = 0.01;
        const MAX_DIM: f32 = 50.0;
        let clamp = |v: f32| {
            if v.is_finite() {
                v.clamp(MIN_DIM, MAX_DIM)
            } else {
                MIN_DIM
            }
        };
        let clamp_unit = |v: f32| {
            if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                0.0
            }
        };
        let clamp_color =
            |c: Fp3| Fp3([clamp_unit(c.0[0]), clamp_unit(c.0[1]), clamp_unit(c.0[2])]);
        let clamp_offset = |v: f32| {
            if v.is_finite() {
                v.clamp(-MAX_DIM, MAX_DIM)
            } else {
                0.0
            }
        };
        let clamp_pos = |v: f32, hi: f32| {
            if v.is_finite() { v.clamp(0.0, hi) } else { 0.0 }
        };

        match &mut self.body {
            AvatarBody::HoverRover {
                phenotype: p,
                kinematics: k,
            } => {
                p.hull_length = Fp(clamp(p.hull_length.0));
                p.hull_width = Fp(clamp(p.hull_width.0));
                p.hull_depth = Fp(clamp(p.hull_depth.0));
                p.pontoon_spread = Fp(clamp(p.pontoon_spread.0));
                p.pontoon_length = Fp(clamp(p.pontoon_length.0));
                p.pontoon_width = Fp(clamp(p.pontoon_width.0));
                p.pontoon_height = Fp(clamp(p.pontoon_height.0));
                p.strut_drop = Fp(clamp_unit(p.strut_drop.0));
                p.mast_height = Fp(clamp(p.mast_height.0));
                p.mast_radius = Fp(clamp(p.mast_radius.0));
                p.mast_offset = Fp2([
                    clamp_offset(p.mast_offset.0[0]),
                    clamp_offset(p.mast_offset.0[1]),
                ]);
                p.sail_size = Fp(clamp(p.sail_size.0));
                p.hull_color = clamp_color(p.hull_color);
                p.pontoon_color = clamp_color(p.pontoon_color);
                p.mast_color = clamp_color(p.mast_color);
                p.strut_color = clamp_color(p.strut_color);
                p.metallic = Fp(clamp_unit(p.metallic.0));
                p.roughness = Fp(clamp_unit(p.roughness.0));

                k.suspension_rest_length = Fp(clamp_pos(k.suspension_rest_length.0, 5.0));
                k.suspension_stiffness = Fp(clamp_pos(k.suspension_stiffness.0, 50_000.0));
                k.suspension_damping = Fp(clamp_pos(k.suspension_damping.0, 5_000.0));
                k.drive_force = Fp(clamp_pos(k.drive_force.0, 50_000.0));
                k.turn_torque = Fp(clamp_pos(k.turn_torque.0, 50_000.0));
                k.lateral_grip = Fp(clamp_pos(k.lateral_grip.0, 50_000.0));
                k.jump_force = Fp(clamp_pos(k.jump_force.0, 50_000.0));
                k.uprighting_torque = Fp(clamp_pos(k.uprighting_torque.0, 50_000.0));
                k.linear_damping = Fp(clamp_pos(k.linear_damping.0, 100.0));
                k.angular_damping = Fp(clamp_pos(k.angular_damping.0, 100.0));
                k.mass = Fp(k.mass.0.clamp(0.1, 10_000.0));
                k.water_rest_length = Fp(clamp_pos(k.water_rest_length.0, 10.0));
                k.buoyancy_strength = Fp(clamp_pos(k.buoyancy_strength.0, 100_000.0));
                k.buoyancy_damping = Fp(clamp_pos(k.buoyancy_damping.0, 10_000.0));
                k.buoyancy_max_depth = Fp(clamp_pos(k.buoyancy_max_depth.0, 50.0));
            }
            AvatarBody::Humanoid {
                phenotype: p,
                kinematics: k,
            } => {
                p.height = Fp(p.height.0.clamp(0.5, 5.0));
                p.torso_half_width = Fp(clamp(p.torso_half_width.0));
                p.torso_half_depth = Fp(clamp(p.torso_half_depth.0));
                p.head_size = Fp(clamp(p.head_size.0));
                p.limb_thickness = Fp(clamp(p.limb_thickness.0));
                p.arm_length_ratio = Fp(if p.arm_length_ratio.0.is_finite() {
                    p.arm_length_ratio.0.clamp(0.5, 1.5)
                } else {
                    default_arm_ratio().0
                });
                p.leg_length_ratio = Fp(if p.leg_length_ratio.0.is_finite() {
                    p.leg_length_ratio.0.clamp(0.3, 0.6)
                } else {
                    default_leg_ratio().0
                });
                p.body_color = clamp_color(p.body_color);
                p.head_color = clamp_color(p.head_color);
                p.limb_color = clamp_color(p.limb_color);
                p.metallic = Fp(clamp_unit(p.metallic.0));
                p.roughness = Fp(clamp_unit(p.roughness.0));

                k.walk_speed = Fp(clamp_pos(k.walk_speed.0, 50.0));
                k.acceleration = Fp(clamp_pos(k.acceleration.0, 200.0));
                k.jump_impulse = Fp(clamp_pos(k.jump_impulse.0, 50_000.0));
                k.mass = Fp(k.mass.0.clamp(0.1, 10_000.0));
                k.linear_damping = Fp(clamp_pos(k.linear_damping.0, 100.0));
            }
            AvatarBody::Unknown => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Avatar record fetch / publish
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
struct GetAvatarResponse {
    value: AvatarRecord,
}

/// Fetch a player's avatar record from their PDS. Result semantics mirror
/// [`fetch_room_record`]: `Ok(None)` is a clean 404 ("no record yet"), and
/// any other failure returns an `Err` the caller distinguishes so it does
/// not silently overwrite a live record with the default.
pub async fn fetch_avatar_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<AvatarRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, AVATAR_COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        let body = resp.text().await.unwrap_or_default();
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: GetAvatarResponse = resp
        .json()
        .await
        .map_err(|e| FetchError::Decode(e.to_string()))?;
    let mut record = wrapper.value;
    record.sanitize();
    Ok(Some(record))
}

#[derive(Serialize)]
struct PutAvatarRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a AvatarRecord,
}

async fn try_put_avatar(
    client: &reqwest::Client,
    pds: &str,
    session: &AtprotoSession,
    record: &AvatarRecord,
) -> PutOutcome {
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutAvatarRequest {
        repo: &session.did,
        collection: AVATAR_COLLECTION,
        rkey: "self",
        record,
    };
    let resp = match client
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return PutOutcome::Transport(e.to_string()),
    };
    let status = resp.status();
    if status.is_success() {
        return PutOutcome::Ok;
    }
    let body = resp.text().await.unwrap_or_default();
    let msg = format!("putRecord (avatar) failed: {} — {}", status, body);
    if status.is_server_error() {
        PutOutcome::ServerError(msg)
    } else {
        PutOutcome::ClientError(msg)
    }
}

async fn delete_avatar_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    let url = format!("{}/xrpc/com.atproto.repo.deleteRecord", pds);
    let body = DeleteRecordRequest {
        repo: &session.did,
        collection: AVATAR_COLLECTION,
        rkey: "self",
    };
    let resp = client
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if resp.status().is_success() || resp.status().as_u16() == 404 {
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!(
            "deleteRecord (avatar) failed: {} — {}",
            status, body
        ))
    }
}

/// Upsert the avatar record to the authenticated user's own PDS. Uses the
/// same 5xx → delete-then-put recovery path as `publish_room_record`.
pub async fn publish_avatar_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    record: &AvatarRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;
    match try_put_avatar(client, &pds, session, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(msg) => Err(msg),
        PutOutcome::Transport(msg) => Err(msg),
        PutOutcome::ServerError(first_err) => {
            warn!("{first_err} — retrying via delete+put for avatar");
            delete_avatar_record(client, session)
                .await
                .map_err(|e| format!("{first_err}; fallback delete failed: {e}"))?;
            match try_put_avatar(client, &pds, session, record).await {
                PutOutcome::Ok => Ok(()),
                PutOutcome::ClientError(m)
                | PutOutcome::ServerError(m)
                | PutOutcome::Transport(m) => Err(format!("{first_err}; fallback put failed: {m}")),
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Root room record
// ---------------------------------------------------------------------------

/// Non-spatial environment state — sky / sun / fog tint.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Environment {
    pub sun_color: Fp3,
}

/// The full recipe: environment + generators + placements + traits. Acts as
/// a Bevy `Resource` so `world_builder.rs` can compile it into ECS entities.
#[derive(Serialize, Deserialize, Clone, Debug, Resource)]
pub struct RoomRecord {
    #[serde(rename = "$type")]
    pub lex_type: String,
    pub environment: Environment,
    pub generators: HashMap<String, Generator>,
    pub placements: Vec<Placement>,
    /// Maps a generator name to a list of trait strings (e.g.
    /// `"collider_heightfield"`, `"sensor"`) the world compiler should attach
    /// to every entity that generator spawns.
    pub traits: HashMap<String, Vec<String>>,
}

impl RoomRecord {
    /// Zero-configuration homeworld. When a client visits a DID whose owner
    /// has never saved a custom record, this builds the canonical default
    /// recipe on the fly — a base terrain plus a base water plane — so the
    /// world builder always has something valid to compile.
    pub fn default_for_did(did: &str) -> Self {
        // Synthesise a per-owner terrain seed from the DID so every freshly
        // visited overland has unique topography without requiring the owner
        // to touch their record. FNV-1a 64-bit — deterministic across peers.
        let did_seed = {
            let mut hash: u64 = 0xcbf29ce484222325;
            for byte in did.bytes() {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
            hash
        };
        let terrain_cfg = SovereignTerrainConfig {
            seed: did_seed,
            ..SovereignTerrainConfig::default()
        };

        let mut generators = HashMap::new();
        generators.insert("base_terrain".to_string(), Generator::Terrain(terrain_cfg));
        generators.insert(
            "base_water".to_string(),
            Generator::Water {
                level_offset: Fp(0.0),
            },
        );

        let placements = vec![
            Placement::Absolute {
                generator_ref: "base_terrain".to_string(),
                transform: TransformData::default(),
            },
            Placement::Absolute {
                generator_ref: "base_water".to_string(),
                transform: TransformData::default(),
            },
        ];

        let mut traits = HashMap::new();
        traits.insert(
            "base_terrain".to_string(),
            vec!["collider_heightfield".to_string(), "ground".to_string()],
        );

        Self {
            lex_type: COLLECTION.into(),
            environment: Environment {
                sun_color: Fp3(crate::config::lighting::SUN_COLOR),
            },
            generators,
            placements,
            traits,
        }
    }
}

impl Default for RoomRecord {
    fn default() -> Self {
        Self::default_for_did("")
    }
}

// ---------------------------------------------------------------------------
// Sanitisation — clamp any numeric field a malicious peer might inflate to
// crash the engine or exhaust host RAM. The limits mirror the ranges the
// World Editor UI already exposes, so a hand-crafted record cannot trigger
// behaviour the owner couldn't have requested via the normal interface.
// ---------------------------------------------------------------------------

/// Maximum values allowed in a `RoomRecord`. Record fields outside these
/// bounds are clamped rather than rejected so slightly exotic records from
/// forward-compatible clients still round-trip, but a weaponised payload
/// cannot force a runaway allocation.
pub mod limits {
    /// Heightmap edge length (cells per side). 2048² ≈ 4M f32 cells ≈ 16 MiB.
    pub const MAX_GRID_SIZE: u32 = 2048;
    /// FBM / noise octaves.
    pub const MAX_OCTAVES: u32 = 32;
    /// Voronoi seed-point count.
    pub const MAX_VORONOI_SEEDS: u32 = 10_000;
    /// Voronoi terrace-level count.
    pub const MAX_VORONOI_TERRACES: u32 = 64;
    /// Hydraulic erosion drop count.
    pub const MAX_EROSION_DROPS: u32 = 500_000;
    /// Thermal erosion iteration count.
    pub const MAX_THERMAL_ITERATIONS: u32 = 500;
    /// Splat texture resolution per side (pixels).
    pub const MAX_TEXTURE_SIZE: u32 = 4096;
    /// Ground / rock generator octaves.
    pub const MAX_GROUND_OCTAVES: u32 = 12;
    pub const MAX_ROCK_OCTAVES: u32 = 16;
    /// Scatter placement count.
    pub const MAX_SCATTER_COUNT: u32 = 100_000;
    /// L-system derivation iterations. 12 is already enough to blow out most
    /// lexical grammars — anything beyond this is almost certainly an attack.
    pub const MAX_LSYSTEM_ITERATIONS: u32 = 12;
    /// L-system source / finalization code length in bytes.
    pub const MAX_LSYSTEM_CODE_BYTES: usize = 16_384;
    /// L-system mesh resolution (stroke segments per twig).
    pub const MAX_LSYSTEM_MESH_RESOLUTION: u32 = 32;
    /// Shape generator floor count.
    pub const MAX_SHAPE_FLOORS: u32 = 64;
    /// Maximum number of `Placement` entries per `RoomRecord`. Clamping
    /// `Scatter.count` alone is not enough — a record with ten-thousand
    /// single-count scatter entries still weaponises `compile_room_record`.
    pub const MAX_PLACEMENTS: usize = 1_024;
    /// Maximum number of generators per `RoomRecord`. Every generator also
    /// materialises per-peer state (L-system material cache, lookup work
    /// in hot loops) so a record with a million generator entries would
    /// still inflate memory and slow every `compile_room_record` pass even
    /// if no placement referenced them.
    pub const MAX_GENERATORS: usize = 256;
}

impl RoomRecord {
    /// Clamp every numeric field to a safe upper bound. Every path that
    /// accepts a `RoomRecord` from the network (PDS fetch and peer-broadcast
    /// `RoomStateUpdate`) calls this before handing the record to the world
    /// compiler, so an attacker cannot weaponise an unbounded field to crash
    /// or OOM the victim.
    pub fn sanitize(&mut self) {
        // Bound the total number of generators before touching any of them.
        // Drop entries in lexicographic key order so the survivor set is
        // deterministic across peers — otherwise a record with 1000
        // generators and `MAX_GENERATORS = 256` would resolve to a
        // different 256 on every client (HashMap iteration is SipHash
        // randomised) and fracture the shared world.
        if self.generators.len() > limits::MAX_GENERATORS {
            let mut keys: Vec<String> = self.generators.keys().cloned().collect();
            keys.sort();
            for key in keys.into_iter().skip(limits::MAX_GENERATORS) {
                self.generators.remove(&key);
            }
        }
        for generator in self.generators.values_mut() {
            match generator {
                Generator::Terrain(cfg) => sanitize_terrain_cfg(cfg),
                Generator::LSystem {
                    source_code,
                    finalization_code,
                    iterations,
                    angle: _,
                    step: _,
                    width: _,
                    elasticity: _,
                    mesh_resolution,
                    ..
                } => {
                    truncate_on_char_boundary(source_code, limits::MAX_LSYSTEM_CODE_BYTES);
                    truncate_on_char_boundary(finalization_code, limits::MAX_LSYSTEM_CODE_BYTES);
                    *iterations = (*iterations).min(limits::MAX_LSYSTEM_ITERATIONS);
                    *mesh_resolution =
                        (*mesh_resolution).clamp(3, limits::MAX_LSYSTEM_MESH_RESOLUTION);
                }
                Generator::Shape { floors, .. } => {
                    *floors = (*floors).min(limits::MAX_SHAPE_FLOORS);
                }
                Generator::Water { .. } | Generator::Unknown => {}
            }
        }
        // Drop excess placements so a 1M-entry array can't force
        // `compile_room_record` to spawn tens of millions of entities in
        // a single frame. Keeping a prefix is order-stable (serde
        // round-trips `Vec` in order) so every peer truncates to the
        // same survivor set.
        if self.placements.len() > limits::MAX_PLACEMENTS {
            self.placements.truncate(limits::MAX_PLACEMENTS);
        }
        for placement in self.placements.iter_mut() {
            if let Placement::Scatter { count, .. } = placement {
                *count = (*count).min(limits::MAX_SCATTER_COUNT);
            }
        }
    }
}

/// Trim `s` to at most `max_bytes`, walking back to the previous UTF-8
/// boundary so `String::truncate`'s boundary-panic can't be triggered by a
/// multi-byte character straddling the cap.
fn truncate_on_char_boundary(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    s.truncate(end);
}

fn sanitize_terrain_cfg(cfg: &mut SovereignTerrainConfig) {
    cfg.grid_size = cfg.grid_size.clamp(2, limits::MAX_GRID_SIZE);
    cfg.octaves = cfg.octaves.clamp(1, limits::MAX_OCTAVES);
    cfg.voronoi_num_seeds = cfg.voronoi_num_seeds.clamp(1, limits::MAX_VORONOI_SEEDS);
    cfg.voronoi_num_terraces = cfg
        .voronoi_num_terraces
        .clamp(1, limits::MAX_VORONOI_TERRACES);
    cfg.erosion_drops = cfg.erosion_drops.min(limits::MAX_EROSION_DROPS);
    cfg.thermal_iterations = cfg.thermal_iterations.min(limits::MAX_THERMAL_ITERATIONS);
    cfg.material.texture_size = cfg
        .material
        .texture_size
        .clamp(16, limits::MAX_TEXTURE_SIZE);
    for ground in [
        &mut cfg.material.grass,
        &mut cfg.material.dirt,
        &mut cfg.material.snow,
    ] {
        ground.macro_octaves = ground.macro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
        ground.micro_octaves = ground.micro_octaves.clamp(1, limits::MAX_GROUND_OCTAVES);
    }
    cfg.material.rock.octaves = cfg.material.rock.octaves.clamp(1, limits::MAX_ROCK_OCTAVES);
}

// ---------------------------------------------------------------------------
// Deterministic generator lookup
// ---------------------------------------------------------------------------

/// Return the terrain generator with the lexicographically smallest key.
///
/// `HashMap::values()` iteration order is randomised per execution (SipHash),
/// so a record with more than one `Generator::Terrain` entry would otherwise
/// have every client picking a different one and landing on a different
/// heightmap — instantly fracturing the shared world. Every site that needs
/// "the terrain" for a record must go through this function (or its sibling)
/// so the choice is deterministic across peers.
pub fn find_terrain_config(record: &RoomRecord) -> Option<&SovereignTerrainConfig> {
    let mut keys: Vec<&String> = record.generators.keys().collect();
    keys.sort();
    for k in keys {
        if let Some(Generator::Terrain(cfg)) = record.generators.get(k) {
            return Some(cfg);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// DID Document types (shared with avatar.rs on WASM)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DidDocument {
    #[serde(default)]
    pub service: Vec<DidService>,
}

#[derive(Deserialize)]
pub struct DidService {
    pub id: String,
    #[serde(rename = "serviceEndpoint")]
    pub service_endpoint: String,
}

// ---------------------------------------------------------------------------
// PDS resolution
// ---------------------------------------------------------------------------

/// Build the DID-document URL for a `did:web` identifier, following the W3C
/// did:web spec rules for path-based identifiers and percent-encoded ports.
///
/// * `did:web:example.com`             → `https://example.com/.well-known/did.json`
/// * `did:web:example.com:u:alice`     → `https://example.com/u/alice/did.json`
/// * `did:web:example.com%3A8080`      → `https://example.com:8080/.well-known/did.json`
fn did_web_document_url(rest: &str) -> String {
    let (domain_enc, path) = match rest.split_once(':') {
        Some((d, p)) => (d, Some(p.replace(':', "/"))),
        None => (rest, None),
    };
    let domain = domain_enc.replace("%3A", ":");
    match path {
        Some(path) => format!("https://{}/{}/did.json", domain, path),
        None => format!("https://{}/.well-known/did.json", domain),
    }
}

/// Resolve a DID to its ATProto PDS endpoint by fetching the DID document.
pub async fn resolve_pds(client: &reqwest::Client, did: &str) -> Option<String> {
    let url = if did.starts_with("did:plc:") {
        format!("https://plc.directory/{}", did)
    } else if let Some(rest) = did.strip_prefix("did:web:") {
        did_web_document_url(rest)
    } else {
        return None;
    };
    let doc: DidDocument = client.get(&url).send().await.ok()?.json().await.ok()?;
    doc.service
        .iter()
        .find(|s| s.id == "#atproto_pds")
        .map(|s| s.service_endpoint.clone())
}

// ---------------------------------------------------------------------------
// Read: fetch room record from the room owner's PDS
// ---------------------------------------------------------------------------

/// Wrapper for the `getRecord` XRPC response.
#[derive(Deserialize)]
struct GetRecordResponse {
    value: RoomRecord,
}

/// Outcome of a `fetch_room_record` call. A 404 means the owner has never
/// saved a custom record (ok to substitute the default homeworld); any other
/// outcome is a genuine failure that the caller must distinguish so it does
/// not silently overwrite an existing record with the default on a transient
/// DNS/timeout/5xx blip.
#[derive(Debug)]
pub enum FetchError {
    /// DID could not be resolved to a PDS endpoint (DID doc missing/invalid).
    DidResolutionFailed,
    /// Network transport failure (DNS, connection refused, timeout, etc.).
    Network(String),
    /// PDS responded but with a non-404 error status.
    PdsError(u16),
    /// The response body could not be decoded as a `RoomRecord`.
    Decode(String),
}

/// Error envelope returned by ATProto XRPC endpoints on non-2xx responses,
/// e.g. `{"error":"RecordNotFound","message":"Could not locate record..."}`.
#[derive(Deserialize)]
struct XrpcError {
    error: Option<String>,
    #[allow(dead_code)]
    message: Option<String>,
}

/// Fetch the room customisation record from the given DID's PDS.
///
/// * `Ok(Some(record))` — the owner has published a record.
/// * `Ok(None)` — the PDS reported there is no record yet (the caller may
///   substitute the default homeworld).
/// * `Err(FetchError)` — transient or permanent failure; the caller must
///   **not** fall through to the default, because doing so risks the user
///   publishing the blank default over their real room on the next save.
///
/// Note: ATProto's `com.atproto.repo.getRecord` returns `400 RecordNotFound`
/// — NOT `404` — when the record does not exist. We detect that payload
/// explicitly and convert it to `Ok(None)` so the loading state can advance
/// onto the default homeworld instead of hammering the PDS with retries.
pub async fn fetch_room_record(
    client: &reqwest::Client,
    did: &str,
) -> Result<Option<RoomRecord>, FetchError> {
    let pds = resolve_pds(client, did)
        .await
        .ok_or(FetchError::DidResolutionFailed)?;
    let url = format!(
        "{}/xrpc/com.atproto.repo.getRecord?repo={}&collection={}&rkey=self",
        pds, did, COLLECTION
    );
    let resp = client
        .get(&url)
        .send()
        .await
        .map_err(|e| FetchError::Network(e.to_string()))?;
    let status = resp.status();
    if status.as_u16() == 404 {
        return Ok(None);
    }
    if !status.is_success() {
        // Inspect the error body before surfacing as PdsError — ATProto
        // signals "no such record" via 400 + `error: "RecordNotFound"` in
        // the body, and we must not treat that as a transient retry case.
        let body = resp.text().await.unwrap_or_default();
        if let Ok(xrpc) = serde_json::from_str::<XrpcError>(&body)
            && let Some(err) = xrpc.error.as_deref()
            && (err == "RecordNotFound"
                || (err == "InvalidRequest" && body.contains("RecordNotFound")))
        {
            return Ok(None);
        }
        return Err(FetchError::PdsError(status.as_u16()));
    }
    let wrapper: GetRecordResponse = resp
        .json()
        .await
        .map_err(|e| FetchError::Decode(e.to_string()))?;
    let mut record = wrapper.value;
    record.sanitize();
    Ok(Some(record))
}

// ---------------------------------------------------------------------------
// Write: publish room record to the authenticated user's PDS
// ---------------------------------------------------------------------------

/// Payload for `com.atproto.repo.putRecord`.
#[derive(Serialize)]
struct PutRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
    record: &'a RoomRecord,
}

/// Result of a single `putRecord` attempt. The `ServerError` variant
/// distinguishes "the PDS's own logic blew up" (transient-or-buggy; we can
/// retry with delete-then-put) from "the PDS rejected our request" (4xx;
/// retrying won't help and we should surface the error as-is).
enum PutOutcome {
    Ok,
    ServerError(String),
    ClientError(String),
    Transport(String),
}

async fn try_put_record(
    client: &reqwest::Client,
    pds: &str,
    session: &AtprotoSession,
    record: &RoomRecord,
) -> PutOutcome {
    let url = format!("{}/xrpc/com.atproto.repo.putRecord", pds);
    let body = PutRecordRequest {
        repo: &session.did,
        collection: COLLECTION,
        rkey: "self",
        record,
    };

    let resp = match client
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&body)
        .send()
        .await
    {
        Ok(r) => r,
        Err(e) => return PutOutcome::Transport(e.to_string()),
    };

    let status = resp.status();
    if status.is_success() {
        return PutOutcome::Ok;
    }
    let body = resp.text().await.unwrap_or_default();
    let msg = format!("putRecord failed: {} — {}", status, body);
    if status.is_server_error() {
        PutOutcome::ServerError(msg)
    } else {
        PutOutcome::ClientError(msg)
    }
}

/// Write (upsert) the room record to the authenticated user's own PDS.
///
/// Tries `com.atproto.repo.putRecord` first (the fast-path upsert). If the
/// PDS responds with a `5xx`, some implementations are choking on their
/// own update-diff logic against a stale or incompatible stored CID — we
/// recover by transparently falling back to `delete_room_record` followed
/// by a fresh `putRecord`. Client (`4xx`) errors are surfaced directly
/// because retrying won't help.
pub async fn publish_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    record: &RoomRecord,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    match try_put_record(client, &pds, session, record).await {
        PutOutcome::Ok => Ok(()),
        PutOutcome::ClientError(msg) => Err(msg),
        PutOutcome::Transport(msg) => Err(msg),
        PutOutcome::ServerError(first_err) => {
            // Fall back to the hard-reset path. This recovers the common
            // failure mode where the PDS's putRecord update path crashes on
            // a stale CID/commit but can still handle a fresh create.
            warn!("{first_err} — retrying via delete_room_record + putRecord");
            delete_room_record(client, session)
                .await
                .map_err(|e| format!("{first_err}; fallback delete failed: {e}"))?;
            match try_put_record(client, &pds, session, record).await {
                PutOutcome::Ok => Ok(()),
                PutOutcome::ClientError(m)
                | PutOutcome::ServerError(m)
                | PutOutcome::Transport(m) => Err(format!("{first_err}; fallback put failed: {m}")),
            }
        }
    }
}

/// Payload for `com.atproto.repo.deleteRecord`.
#[derive(Serialize)]
struct DeleteRecordRequest<'a> {
    repo: &'a str,
    collection: &'a str,
    rkey: &'a str,
}

/// Delete the room record from the authenticated user's PDS. A 404 response
/// is reported as `Ok(())` because the caller usually just wants to know the
/// row is gone — whether it was never there or just removed is immaterial.
pub async fn delete_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
) -> Result<(), String> {
    let pds = resolve_pds(client, &session.did)
        .await
        .ok_or_else(|| "Failed to resolve PDS".to_string())?;

    let url = format!("{}/xrpc/com.atproto.repo.deleteRecord", pds);
    let body = DeleteRecordRequest {
        repo: &session.did,
        collection: COLLECTION,
        rkey: "self",
    };

    let resp = client
        .post(&url)
        .bearer_auth(&session.access_jwt)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if resp.status().is_success() || resp.status().as_u16() == 404 {
        Ok(())
    } else {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        Err(format!("deleteRecord failed: {} — {}", status, body))
    }
}

/// Force-overwrite the room record by deleting first, then creating fresh.
///
/// The plain `putRecord` upsert path can trip on an incompatible stored
/// record: some PDS implementations try to diff the prior CID and return
/// `500 InternalServerError` when the old blob can't be validated against
/// the current lexicon. Deleting first gives the PDS a clean slate, so the
/// subsequent create is a simple new-record path with no diff logic.
pub async fn reset_room_record(
    client: &reqwest::Client,
    session: &AtprotoSession,
    record: &RoomRecord,
) -> Result<(), String> {
    delete_room_record(client, session).await?;
    publish_room_record(client, session, record).await
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Regression guard for issue #58: 64-bit seeds must serialize as JSON
    /// strings, not numbers. Numeric form would round-trip through `f64` in
    /// most parsers (including the ones in front of ATProto PDSes), losing
    /// precision above `2^53` and triggering `500 InternalServerError`
    /// from the DAG-CBOR encoder. The default DID-derived terrain seed
    /// is FNV-1a 64-bit, which routinely lands well above the safe range.
    #[test]
    fn u64_seeds_serialize_as_strings() {
        let r = RoomRecord::default_for_did("did:plc:z5yhcebtrvzblrojezn6pjgi");
        let json = serde_json::to_string(&r).expect("serialise");
        assert!(
            json.contains("\"seed\":\""),
            "terrain seed must be a string in JSON, got: {json}"
        );
        // Round-trip stays lossless.
        let back: RoomRecord = serde_json::from_str(&json).expect("deserialise");
        let original_seed = match r.generators.get("base_terrain") {
            Some(Generator::Terrain(cfg)) => cfg.seed,
            _ => panic!("expected base_terrain"),
        };
        let round_seed = match back.generators.get("base_terrain") {
            Some(Generator::Terrain(cfg)) => cfg.seed,
            _ => panic!("expected base_terrain"),
        };
        assert_eq!(original_seed, round_seed);
    }

    /// Regression guard for issue #48: a `RoomRecord` serialised via serde
    /// must contain zero JSON floating-point literals. DAG-CBOR forbids
    /// floats and the PDS returns `400 InvalidRequest` when it sees one,
    /// so any future field that forgets its `Fp*` wrapper will be caught
    /// here. Scans for a digit-dot-digit pattern so the test doesn't
    /// false-positive on the `$type` string sigil.
    #[test]
    fn default_record_serialises_without_floats() {
        let mut record = RoomRecord::default_for_did("did:plc:test");
        record.environment.sun_color = Fp3([0.98, 0.95, 0.82]);
        if let Some(Generator::Water { level_offset }) = record.generators.get_mut("base_water") {
            *level_offset = Fp(2.5);
        }
        record.placements.push(Placement::Scatter {
            generator_ref: "base_terrain".to_string(),
            bounds: ScatterBounds::Circle {
                center: Fp2([10.5, -3.25]),
                radius: Fp(7.75),
            },
            count: 4,
            local_seed: 42,
            biome_filter: Some(0),
        });

        let json = serde_json::to_string(&record).expect("serialise record");
        let bytes = json.as_bytes();
        for i in 1..bytes.len().saturating_sub(1) {
            if bytes[i] == b'.' && bytes[i - 1].is_ascii_digit() && bytes[i + 1].is_ascii_digit() {
                panic!("expected fixed-point integers, got float in `{json}`");
            }
        }
    }

    /// Round-trip sanity: every `f32` we put in must come back equal
    /// (within the quantisation error of `FP_SCALE`).
    #[test]
    fn fixed_point_round_trip_preserves_values() {
        let original = TransformData {
            translation: Fp3([1.5, -2.25, 3.125]),
            rotation: Fp4([0.0, 0.6, 0.0, 0.8]),
            scale: Fp3([1.0, 2.0, 0.5]),
        };
        let json = serde_json::to_string(&original).unwrap();
        let decoded: TransformData = serde_json::from_str(&json).unwrap();
        let eps = 1.0 / FP_SCALE;
        for (a, b) in original
            .translation
            .0
            .iter()
            .zip(decoded.translation.0.iter())
        {
            assert!((a - b).abs() < eps, "translation drift: {a} vs {b}");
        }
        for (a, b) in original.rotation.0.iter().zip(decoded.rotation.0.iter()) {
            assert!((a - b).abs() < eps, "rotation drift: {a} vs {b}");
        }
        for (a, b) in original.scale.0.iter().zip(decoded.scale.0.iter()) {
            assert!((a - b).abs() < eps, "scale drift: {a} vs {b}");
        }
    }
}
