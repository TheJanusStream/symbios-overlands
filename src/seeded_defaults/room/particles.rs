//! Seeded ambient-particle spec — the room's "air".
//!
//! One looping emitter per room, mood-matched to the biome: fireflies
//! drifting through lush valleys, snowfall on tundra and alpine
//! ridges, rising embers over volcanic rock, wind-blown dust motes in
//! deserts, and faint sea-mist motes on coasts. Numbers stay well
//! inside the particle sanitiser budget (`MAX_PARTICLES = 512`) so
//! the layer reads as atmosphere, not weather-system spectacle.
//!
//! The wiring layer ([`RoomRecord::default_for_did`](crate::pds::RoomRecord::default_for_did))
//! maps the spec 1:1 onto a `GeneratorKind::ParticleSystem` generator
//! with a terrain-snapped Absolute placement at the spawn origin.

use rand_chacha::ChaCha8Rng;
use rand_chacha::rand_core::SeedableRng;

use crate::seeded_defaults::scene::{BiomeArchetype, SceneCharacter, range_f32};

/// Sub-stream salt distinct from every sibling room deriver.
const PARTICLE_STREAM_SALT: u64 = 0xFA17_1C1E_5EED_0001;

/// Biome-fixed particle mood. Carried on the spec so tests (and a
/// future debug HUD) can name what a room is emitting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleMood {
    Fireflies,
    Snowfall,
    Embers,
    DustMotes,
    MistMotes,
}

/// Fully-derived ambient emitter parameters. Field names mirror their
/// namesakes on `GeneratorKind::ParticleSystem`.
#[derive(Clone, Copy, Debug)]
pub struct AmbientParticles {
    pub mood: ParticleMood,
    /// Box-emitter half extents.
    pub emitter_half_extents: [f32; 3],
    /// Emitter altitude above the (terrain-snapped) placement origin.
    pub emitter_y: f32,
    pub rate_per_second: f32,
    pub max_particles: u32,
    pub lifetime: (f32, f32),
    pub speed: (f32, f32),
    pub gravity_multiplier: f32,
    /// Constant wind / lift acceleration (m/s²).
    pub acceleration: [f32; 3],
    pub linear_drag: f32,
    pub start_size: f32,
    pub end_size: f32,
    pub start_color: [f32; 4],
    pub end_color: [f32; 4],
    /// `true` = additive blend (glows); `false` = alpha (matter).
    pub additive: bool,
    /// Emitter determinism seed.
    pub seed: u64,
}

impl AmbientParticles {
    pub fn from_scene(scene: &SceneCharacter, room_seed: u64) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(room_seed ^ PARTICLE_STREAM_SALT);
        derive(scene, &mut rng, room_seed)
    }
}

fn derive(scene: &SceneCharacter, rng: &mut ChaCha8Rng, room_seed: u64) -> AmbientParticles {
    let seed = room_seed ^ 0x00AB_1E47;
    match scene.biome {
        BiomeArchetype::Lush => AmbientParticles {
            mood: ParticleMood::Fireflies,
            emitter_half_extents: [70.0, 5.0, 70.0],
            emitter_y: 3.0,
            rate_per_second: range_f32(rng, 10.0, 18.0),
            max_particles: 200,
            lifetime: (5.0, 10.0),
            speed: (0.2, 0.6),
            gravity_multiplier: 0.0,
            acceleration: [0.0, 0.0, 0.0],
            linear_drag: 0.6,
            start_size: 0.07,
            end_size: 0.02,
            start_color: [0.95, 1.0, 0.45, 1.0],
            end_color: [0.4, 0.8, 0.2, 0.0],
            additive: true,
            seed,
        },
        BiomeArchetype::Tundra | BiomeArchetype::Alpine => AmbientParticles {
            mood: ParticleMood::Snowfall,
            emitter_half_extents: [90.0, 3.0, 90.0],
            emitter_y: 30.0,
            rate_per_second: range_f32(rng, 100.0, 160.0),
            max_particles: 512,
            lifetime: (12.0, 20.0),
            speed: (0.2, 0.6),
            gravity_multiplier: 0.02,
            acceleration: [range_f32(rng, -0.4, 0.4), 0.0, range_f32(rng, -0.4, 0.4)],
            linear_drag: 0.8,
            start_size: 0.10,
            end_size: 0.08,
            start_color: [1.0, 1.0, 1.0, 0.9],
            end_color: [1.0, 1.0, 1.0, 0.7],
            additive: false,
            seed,
        },
        BiomeArchetype::Volcanic => AmbientParticles {
            mood: ParticleMood::Embers,
            emitter_half_extents: [60.0, 4.0, 60.0],
            emitter_y: 1.0,
            rate_per_second: range_f32(rng, 20.0, 35.0),
            max_particles: 300,
            lifetime: (4.0, 8.0),
            speed: (0.3, 0.8),
            // Negative gravity: embers rise on their own thermals.
            gravity_multiplier: -0.05,
            acceleration: [range_f32(rng, -0.2, 0.2), 0.0, range_f32(rng, -0.2, 0.2)],
            linear_drag: 0.4,
            start_size: 0.08,
            end_size: 0.0,
            start_color: [1.0, 0.55, 0.15, 1.0],
            end_color: [0.7, 0.1, 0.05, 0.0],
            additive: true,
            seed,
        },
        BiomeArchetype::Arid => AmbientParticles {
            mood: ParticleMood::DustMotes,
            // Hug the ground: a tall emitter band put motes against
            // the open sky where they read as glitter specks instead
            // of haze. Low band + larger, fainter quads keeps them in
            // front of the dunes.
            emitter_half_extents: [80.0, 2.5, 80.0],
            emitter_y: 1.8,
            rate_per_second: range_f32(rng, 25.0, 40.0),
            max_particles: 350,
            lifetime: (8.0, 15.0),
            speed: (0.1, 0.4),
            gravity_multiplier: 0.0,
            // Steady prevailing wind so the dust streams one way.
            acceleration: [range_f32(rng, 0.3, 0.7), 0.0, range_f32(rng, -0.2, 0.2)],
            linear_drag: 0.3,
            start_size: 0.32,
            end_size: 0.45,
            start_color: [0.82, 0.72, 0.52, 0.12],
            end_color: [0.82, 0.72, 0.52, 0.0],
            additive: false,
            seed,
        },
        BiomeArchetype::Coastal => AmbientParticles {
            mood: ParticleMood::MistMotes,
            emitter_half_extents: [80.0, 4.0, 80.0],
            emitter_y: 2.0,
            rate_per_second: range_f32(rng, 15.0, 25.0),
            max_particles: 250,
            lifetime: (6.0, 12.0),
            speed: (0.1, 0.4),
            gravity_multiplier: 0.0,
            acceleration: [range_f32(rng, -0.3, 0.3), 0.05, 0.0],
            linear_drag: 0.5,
            start_size: 0.18,
            end_size: 0.25,
            start_color: [0.85, 0.92, 1.0, 0.18],
            end_color: [0.85, 0.92, 1.0, 0.0],
            additive: false,
            seed,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic() {
        let scene = SceneCharacter::for_seed(3);
        let a = AmbientParticles::from_scene(&scene, 3);
        let b = AmbientParticles::from_scene(&scene, 3);
        assert_eq!(a.mood, b.mood);
        assert_eq!(a.rate_per_second, b.rate_per_second);
        assert_eq!(a.acceleration, b.acceleration);
        assert_eq!(a.seed, b.seed);
    }

    #[test]
    fn mood_tracks_biome_and_budget_holds() {
        for biome in BiomeArchetype::ALL {
            for s in 0u64..8 {
                let mut scene = SceneCharacter::for_seed(s);
                scene.biome = biome;
                let p = AmbientParticles::from_scene(&scene, s);
                let expected = match biome {
                    BiomeArchetype::Lush => ParticleMood::Fireflies,
                    BiomeArchetype::Tundra | BiomeArchetype::Alpine => ParticleMood::Snowfall,
                    BiomeArchetype::Volcanic => ParticleMood::Embers,
                    BiomeArchetype::Arid => ParticleMood::DustMotes,
                    BiomeArchetype::Coastal => ParticleMood::MistMotes,
                };
                assert_eq!(p.mood, expected);
                // Stay inside the particle sanitiser budget so the
                // record round-trips unchanged.
                assert!(p.max_particles <= 512);
                assert!(p.rate_per_second <= 256.0);
                assert!(p.lifetime.1 <= 30.0);
                assert!(p.emitter_half_extents.iter().all(|h| *h <= 100.0));
                assert!(p.lifetime.0 <= p.lifetime.1);
                assert!(p.speed.0 <= p.speed.1);
            }
        }
    }
}
