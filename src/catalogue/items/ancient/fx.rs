//! AncientClassical "bring-it-to-life" helpers: small nested particle
//! emitters and a spatial-audio patch for the kit's one firelit element,
//! the [`brazier`](super::brazier) - a low altar flame, drifting embers,
//! and a fire crackle.
//!
//! Particle emitters are returned as [`Generator`] nodes positioned in the
//! prop's world frame, so they drop straight into an
//! [`assemble`](super::super::util::assemble) list. Counts stay small. The
//! audio patch returns a [`SovereignAudioConfig`] to assign to a node's
//! `audio` field; the world compiler plays it spatially at that node.

use crate::catalogue::items::fx::{Emitter, FireCrackle};
use crate::pds::{
    EmitterShape, Fp, Fp3, Generator, ParticleBlendMode, SovereignAudioConfig,
    SovereignFlameConfig, SovereignSparkConfig, SovereignTextureConfig,
};

/// A low altar flame licking up out of the brazier bowl.
pub(super) fn brazier_flame(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        burst: 0,
        shape: EmitterShape::Cone {
            half_angle: Fp(0.3),
            height: Fp(0.25),
        },
        rate: 18.0,
        max: 80,
        life: (0.5, 1.2),
        speed: (0.6, 1.4),
        gravity: -0.1,
        accel: [0.0, 0.3, 0.0],
        drag: 0.3,
        size: (0.32, 0.0),
        start_color: [1.0, 0.72, 0.24, 1.0],
        end_color: [0.80, 0.20, 0.05, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Flame(SovereignFlameConfig {
            seed: (seed ^ 0x00F1_A3E0) as u32,
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// Glowing embers drifting up off the coals into the warm air.
pub(super) fn brazier_embers(pos: [f32; 3], seed: u64) -> Generator {
    Emitter {
        burst: 0,
        shape: EmitterShape::Sphere { radius: Fp(0.22) },
        rate: 5.0,
        max: 44,
        life: (1.0, 2.4),
        speed: (0.5, 1.4),
        gravity: -0.22,
        accel: [0.04, 0.36, 0.0],
        drag: 0.22,
        size: (0.05, 0.0),
        start_color: [1.0, 0.80, 0.38, 1.0],
        end_color: [0.9, 0.30, 0.06, 0.0],
        blend: ParticleBlendMode::Additive,
        sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
            seed: (seed ^ 0x0E_3BE0) as u32,
            points: 4,
            color_core: Fp3([1.0, 0.95, 0.7]),
            color_tip: Fp3([1.0, 0.5, 0.12]),
            ..Default::default()
        }),
    }
    .at(pos, seed)
}

/// A warm, irregular fire crackle - band-passed noise pulsed by a slow LFO
/// over a low ember rumble. The voice of the brazier coals.
pub(super) fn fire_crackle() -> SovereignAudioConfig {
    FireCrackle {
        noise: 0.55,
        pulse_hz: 12.0,
        pulse: 0.485,
        pitch_hz: 1600.0,
        rumble_hz: 68.0,
        rumble: 0.16,
    }
    .patch()
}
