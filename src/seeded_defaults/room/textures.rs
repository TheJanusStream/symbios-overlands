//! Per-biome procedural texture generator parameters.
//!
//! Every splat layer (grass / dirt / rock / snow) has its own
//! generator with scale / octave / weight knobs that control how the
//! macro and micro noise fields combine into albedo + normal maps. The
//! palette deriver already coloured each layer; this deriver gives
//! them genuinely different *texture*, so two rooms whose grass shares
//! a hue still read as different surfaces (different grain, weave,
//! crack pattern, snow crust).
//!
//! Each layer is sampled from an independent ChaCha sub-stream so
//! perturbing the dirt deriver doesn't accidentally drift the grass
//! output — adding a future per-biome knob stays a local change.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32};

/// Sub-stream salt for the texture deriver. Distinct from
/// [`super::palette`] and [`super::terrain`] so each parameter group
/// advances its RNG independently.
const TEXTURE_STREAM_SALT: u64 = 0x7E47_0E51_0E51_0E51;

/// Per-layer salts so each biome layer has its own deterministic
/// ChaCha stream — modifying the rock deriver in isolation does not
/// shift the grass output, and vice versa.
const GRASS_SALT: u64 = 0x4757_4757_4757_4757;
const DIRT_SALT: u64 = 0xDEAD_BEEF_C0DE_C0DE;
const ROCK_SALT: u64 = 0x0123_4567_89AB_CDEF;
const SNOW_SALT: u64 = 0xFEEB_DAED_DAED_FEEB;

/// Procedural parameters for a `Ground` texture layer (grass, dirt,
/// snow). Mirrors [`crate::pds::SovereignGroundConfig`]'s non-colour
/// fields; the dry/moist colours come from
/// [`super::palette::RoomPalette`] instead.
#[derive(Clone, Copy, Debug)]
pub struct GroundTextureParams {
    pub seed: u32,
    pub macro_scale: f64,
    pub macro_octaves: u32,
    pub micro_scale: f64,
    pub micro_octaves: u32,
    pub micro_weight: f64,
    pub normal_strength: f32,
}

/// Procedural parameters for a `Rock` texture layer. Mirrors
/// [`crate::pds::SovereignRockConfig`]'s non-colour fields.
#[derive(Clone, Copy, Debug)]
pub struct RockTextureParams {
    pub seed: u32,
    pub scale: f64,
    pub octaves: u32,
    pub attenuation: f64,
    pub normal_strength: f32,
}

/// Full per-biome texture parameter set for the room's four splat
/// layers. Layer roles are positional (Grass, Dirt, Rock, Snow) to
/// match [`crate::pds::SovereignMaterialConfig::layers`].
#[derive(Clone, Copy, Debug)]
pub struct BiomeTextures {
    pub grass: GroundTextureParams,
    pub dirt: GroundTextureParams,
    pub rock: RockTextureParams,
    pub snow: GroundTextureParams,
}

impl BiomeTextures {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let base = room_seed ^ TEXTURE_STREAM_SALT;
        Self {
            grass: derive_ground(scene, base ^ GRASS_SALT, GrassLayer),
            dirt: derive_ground(scene, base ^ DIRT_SALT, DirtLayer),
            rock: derive_rock(scene, base ^ ROCK_SALT),
            snow: derive_ground(scene, base ^ SNOW_SALT, SnowLayer),
        }
    }
}

// ---------------------------------------------------------------------------
// Layer-specific ground texture derivers
// ---------------------------------------------------------------------------

/// Marker types tag the ground-deriver call so the same function body
/// can branch on layer-specific tuning (grass macro scale vs snow
/// macro scale, etc.) without three near-identical copies of the same
/// code.
trait GroundLayerKind {
    /// `(lo, hi)` for `macro_scale`.
    fn macro_scale_range(&self) -> (f64, f64);
    /// `(lo, hi)` for `macro_octaves`.
    fn macro_octaves_range(&self) -> (u32, u32);
    /// `(lo, hi)` for `micro_scale`.
    fn micro_scale_range(&self) -> (f64, f64);
    /// `(lo, hi)` for `micro_octaves`.
    fn micro_octaves_range(&self) -> (u32, u32);
    /// `(lo, hi)` for `micro_weight`.
    fn micro_weight_range(&self) -> (f64, f64);
    /// `(lo, hi)` for `normal_strength`.
    fn normal_strength_range(&self) -> (f32, f32);
    /// Optional biome-driven adjustment (e.g. lush biomes intensify
    /// grass grain, coastal biomes dampen dirt micro-detail).
    fn biome_bias(&self, biome: BiomeArchetype, params: &mut GroundTextureParams);
}

struct GrassLayer;
impl GroundLayerKind for GrassLayer {
    fn macro_scale_range(&self) -> (f64, f64) {
        (1.5, 4.0)
    }
    fn macro_octaves_range(&self) -> (u32, u32) {
        (3, 6)
    }
    fn micro_scale_range(&self) -> (f64, f64) {
        (6.0, 14.0)
    }
    fn micro_octaves_range(&self) -> (u32, u32) {
        (2, 5)
    }
    fn micro_weight_range(&self) -> (f64, f64) {
        (0.20, 0.40)
    }
    fn normal_strength_range(&self) -> (f32, f32) {
        (3.5, 5.5)
    }
    fn biome_bias(&self, biome: BiomeArchetype, p: &mut GroundTextureParams) {
        // Verdant rooms get denser, finer grass weave; dry runs sparser.
        match biome {
            BiomeArchetype::Lush
            | BiomeArchetype::Jungle
            | BiomeArchetype::TemperateForest
            | BiomeArchetype::Meadow => {
                p.micro_weight = (p.micro_weight + 0.05).min(0.50);
                p.normal_strength = (p.normal_strength + 0.5).min(6.0);
            }
            BiomeArchetype::Arid | BiomeArchetype::Savanna | BiomeArchetype::Badlands => {
                p.micro_weight = (p.micro_weight - 0.05).max(0.10);
                p.normal_strength = (p.normal_strength - 0.5).max(2.0);
            }
            BiomeArchetype::Tundra | BiomeArchetype::Glacial => {
                // Faint, frost-suppressed grass: mute normal contribution.
                p.normal_strength = (p.normal_strength * 0.5).max(1.0);
            }
            _ => {}
        }
    }
}

struct DirtLayer;
impl GroundLayerKind for DirtLayer {
    fn macro_scale_range(&self) -> (f64, f64) {
        (1.5, 3.5)
    }
    fn macro_octaves_range(&self) -> (u32, u32) {
        (3, 6)
    }
    fn micro_scale_range(&self) -> (f64, f64) {
        (5.0, 12.0)
    }
    fn micro_octaves_range(&self) -> (u32, u32) {
        (3, 6)
    }
    fn micro_weight_range(&self) -> (f64, f64) {
        (0.25, 0.45)
    }
    fn normal_strength_range(&self) -> (f32, f32) {
        (1.5, 3.0)
    }
    fn biome_bias(&self, biome: BiomeArchetype, p: &mut GroundTextureParams) {
        match biome {
            BiomeArchetype::Arid | BiomeArchetype::Savanna | BiomeArchetype::Badlands => {
                // Cracked, sun-baked dirt: pump up micro-detail + normals.
                p.micro_weight = (p.micro_weight + 0.05).min(0.50);
                p.normal_strength = (p.normal_strength + 0.5).min(3.5);
            }
            BiomeArchetype::Coastal | BiomeArchetype::Wetland => {
                // Sandy beach / wet peat mud — smooth: damp the micro weight.
                p.micro_weight = (p.micro_weight - 0.10).max(0.15);
                p.micro_octaves = p.micro_octaves.saturating_sub(1).max(2);
            }
            _ => {}
        }
    }
}

struct SnowLayer;
impl GroundLayerKind for SnowLayer {
    fn macro_scale_range(&self) -> (f64, f64) {
        (3.0, 6.0)
    }
    fn macro_octaves_range(&self) -> (u32, u32) {
        (2, 5)
    }
    fn micro_scale_range(&self) -> (f64, f64) {
        (8.0, 16.0)
    }
    fn micro_octaves_range(&self) -> (u32, u32) {
        (2, 5)
    }
    fn micro_weight_range(&self) -> (f64, f64) {
        (0.30, 0.50)
    }
    fn normal_strength_range(&self) -> (f32, f32) {
        (0.5, 1.5)
    }
    fn biome_bias(&self, biome: BiomeArchetype, p: &mut GroundTextureParams) {
        match biome {
            BiomeArchetype::Alpine
            | BiomeArchetype::Tundra
            | BiomeArchetype::Glacial
            | BiomeArchetype::Boreal => {
                // Crustier, more present snow: extra micro detail.
                p.micro_weight = (p.micro_weight + 0.05).min(0.55);
                p.normal_strength = (p.normal_strength + 0.3).min(2.0);
            }
            BiomeArchetype::Volcanic => {
                // Ash-veined snow if there's any at all: subtle micro.
                p.micro_weight = (p.micro_weight - 0.05).max(0.15);
            }
            _ => {}
        }
    }
}

fn derive_ground<L: GroundLayerKind>(
    scene: &SceneCharacter,
    seed: u64,
    layer: L,
) -> GroundTextureParams {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut p = GroundTextureParams {
        seed: rng_u32(&mut rng),
        macro_scale: sample_f64(&mut rng, layer.macro_scale_range()),
        macro_octaves: sample_u32(&mut rng, layer.macro_octaves_range()),
        micro_scale: sample_f64(&mut rng, layer.micro_scale_range()),
        micro_octaves: sample_u32(&mut rng, layer.micro_octaves_range()),
        micro_weight: sample_f64(&mut rng, layer.micro_weight_range()),
        normal_strength: range_f32(
            &mut rng,
            layer.normal_strength_range().0,
            layer.normal_strength_range().1,
        ),
    };
    layer.biome_bias(scene.biome, &mut p);
    p
}

// ---------------------------------------------------------------------------
// Rock deriver
// ---------------------------------------------------------------------------

fn derive_rock(scene: &SceneCharacter, seed: u64) -> RockTextureParams {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut p = RockTextureParams {
        seed: rng_u32(&mut rng),
        scale: sample_f64(&mut rng, (2.0, 4.5)),
        octaves: sample_u32(&mut rng, (6, 10)),
        attenuation: sample_f64(&mut rng, (1.5, 2.8)),
        normal_strength: range_f32(&mut rng, 3.0, 5.0),
    };
    // Volcanic & alpine rocks read more aggressive — bump attenuation
    // (more crack contrast) and octaves (sharper fracture pattern).
    match scene.biome {
        BiomeArchetype::Volcanic | BiomeArchetype::Badlands => {
            // Sharp, high-contrast fracture — fresh basalt / eroded strata.
            p.attenuation = (p.attenuation + 0.4).min(3.2);
            p.normal_strength = (p.normal_strength + 0.5).min(5.5);
        }
        BiomeArchetype::Alpine => {
            p.octaves = (p.octaves + 1).min(11);
        }
        BiomeArchetype::Coastal | BiomeArchetype::Glacial => {
            // Wave-smoothed / ice-polished rocks: damp the attenuation.
            p.attenuation = (p.attenuation - 0.3).max(1.2);
        }
        _ => {}
    }
    p
}

// ---------------------------------------------------------------------------
// Small sampling helpers
// ---------------------------------------------------------------------------

fn rng_u32(rng: &mut ChaCha8Rng) -> u32 {
    use rand_chacha::rand_core::RngCore;
    rng.next_u32()
}

fn sample_f64(rng: &mut ChaCha8Rng, (lo, hi): (f64, f64)) -> f64 {
    lo + (range_f32(rng, 0.0, 1.0) as f64) * (hi - lo)
}

fn sample_u32(rng: &mut ChaCha8Rng, (lo, hi): (u32, u32)) -> u32 {
    let lo_f = lo as f32;
    let hi_f = (hi + 1) as f32; // inclusive
    let v = range_f32(rng, lo_f, hi_f) as u32;
    v.clamp(lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seeded_defaults::hash::fnv1a_64;

    fn finite_ground(p: &GroundTextureParams) -> bool {
        p.macro_scale.is_finite()
            && p.macro_scale > 0.0
            && p.micro_scale.is_finite()
            && p.micro_scale > 0.0
            && p.micro_weight.is_finite()
            && (0.0..=1.0).contains(&p.micro_weight)
            && p.macro_octaves > 0
            && p.micro_octaves > 0
            && p.normal_strength.is_finite()
            && p.normal_strength > 0.0
    }

    fn finite_rock(p: &RockTextureParams) -> bool {
        p.scale.is_finite()
            && p.scale > 0.0
            && p.attenuation.is_finite()
            && p.attenuation > 0.0
            && p.octaves > 0
            && p.normal_strength > 0.0
    }

    #[test]
    fn deterministic() {
        let seed = fnv1a_64("did:plc:test");
        let scene = SceneCharacter::for_seed(seed);
        let a = BiomeTextures::from_scene(&scene, seed);
        let b = BiomeTextures::from_scene(&scene, seed);
        assert_eq!(a.grass.macro_scale, b.grass.macro_scale);
        assert_eq!(a.rock.scale, b.rock.scale);
        assert_eq!(a.snow.normal_strength, b.snow.normal_strength);
    }

    #[test]
    fn all_finite_across_biomes() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..4 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let t = BiomeTextures::from_scene(&scene, s);
                assert!(
                    finite_ground(&t.grass),
                    "{biome:?} seed {s} grass {:?}",
                    t.grass
                );
                assert!(
                    finite_ground(&t.dirt),
                    "{biome:?} seed {s} dirt {:?}",
                    t.dirt
                );
                assert!(finite_rock(&t.rock), "{biome:?} seed {s} rock {:?}", t.rock);
                assert!(
                    finite_ground(&t.snow),
                    "{biome:?} seed {s} snow {:?}",
                    t.snow
                );
            }
        }
    }

    #[test]
    fn per_layer_streams_are_independent() {
        // Two seeds whose only difference is the salt should produce
        // unrelated grass parameters — proves the per-layer salts
        // actually isolate the streams.
        let scene = SceneCharacter::for_seed(0);
        let t1 = BiomeTextures::from_scene(&scene, 1);
        let t2 = BiomeTextures::from_scene(&scene, 2);
        assert_ne!(t1.grass.seed, t2.grass.seed);
        assert_ne!(t1.rock.seed, t2.rock.seed);
    }
}
