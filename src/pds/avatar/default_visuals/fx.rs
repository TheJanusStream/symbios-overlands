//! Build-side avatar FX — turns an [`AvatarFx`] spec into the actual
//! `ParticleSystem` aura node and the spatial-audio voice, then hangs them
//! on a built avatar.
//!
//! The *selection* (which aura / voice, how dense) is the seeded
//! [`AvatarFx`] deriver; this module owns only the geometry/synth recipes,
//! reusing the shared catalogue FX toolkit ([`crate::catalogue::items::fx`])
//! so the avatar's steam/neon/ember emitters are built the exact same way
//! as the catalogue structures' signature FX. Emitter populations are kept
//! small (signature, not spectacle) and inside the particle sanitiser's
//! bounds so a built avatar round-trips [`crate::pds::sanitize_avatar_visuals`]
//! unchanged.

use bevy_symbios_audio::{
    BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape, NodeId, NodeKind,
    SawtoothOsc, SineOsc,
};

use crate::catalogue::items::fx::{Emitter, node, patch};
use crate::pds::generator::Generator;
use crate::pds::texture::{
    SovereignPuffConfig, SovereignSoftDiscConfig, SovereignSparkConfig, SovereignTextureConfig,
};
use crate::pds::types::{Fp, Fp3};
use crate::pds::{EmitterShape, ParticleBlendMode, SovereignAudioConfig};
use crate::seeded_defaults::{AvatarFx, AvatarVoice, ParticleAura};

/// Hang the FX on a freshly-built avatar root: push the aura emitter as a
/// child at `mount` (in the root's local frame) and set the body voice on
/// the root's `audio`. A no-op for `ParticleAura::None` / `AvatarVoice::None`.
///
/// `accent` is the avatar's primary accent — decorative auras (neon /
/// arcane motes) glow in it so the FX belongs to the avatar's palette.
pub(super) fn attach(
    root: &mut Generator,
    fx: &AvatarFx,
    mount: [f32; 3],
    accent: [f32; 3],
    seed: u64,
) {
    if let Some(emitter) = aura_emitter(fx.aura, mount, accent, fx.intensity, seed) {
        root.children.push(emitter);
    }
    if let Some(audio) = voice_audio(fx.voice) {
        root.audio = audio;
    }
}

/// Build the aura emitter for `aura`, or `None` for [`ParticleAura::None`].
/// `intensity` scales the emit rate + population; `accent` colours the
/// decorative auras.
fn aura_emitter(
    aura: ParticleAura,
    pos: [f32; 3],
    accent: [f32; 3],
    intensity: f32,
    seed: u64,
) -> Option<Generator> {
    let rate = |base: f32| base * intensity;
    let pop = |base: u32| ((base as f32 * intensity) as u32).min(120);
    let emitter = match aura {
        ParticleAura::None => return None,
        // Pale rising steam / exhaust.
        ParticleAura::Steam => Emitter {
            shape: EmitterShape::Cone {
                half_angle: Fp(0.3),
                height: Fp(0.3),
            },
            rate: rate(6.0),
            burst: 0,
            max: pop(48),
            life: (1.6, 3.2),
            speed: (0.3, 0.8),
            gravity: -0.04,
            accel: [0.0, 0.2, 0.0],
            drag: 0.6,
            size: (0.18, 0.7),
            start_color: [0.72, 0.74, 0.76, 0.28],
            end_color: [0.82, 0.84, 0.86, 0.0],
            blend: ParticleBlendMode::Alpha,
            sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
                seed: (seed ^ 0x0057_EA00) as u32,
                color_base: Fp3([0.80, 0.82, 0.85]),
                color_shadow: Fp3([0.55, 0.57, 0.60]),
                ..Default::default()
            }),
        },
        // Faint rising neon motes in the accent colour.
        ParticleAura::NeonHaze => Emitter {
            shape: EmitterShape::Box {
                half_extents: Fp3([0.4, 0.5, 0.4]),
            },
            rate: rate(7.0),
            burst: 0,
            max: pop(44),
            life: (1.8, 3.6),
            speed: (0.1, 0.4),
            gravity: -0.02,
            accel: [0.0, 0.12, 0.0],
            drag: 0.5,
            size: (0.06, 0.0),
            start_color: [accent[0], accent[1], accent[2], 0.9],
            end_color: [accent[0], accent[1], accent[2], 0.0],
            blend: ParticleBlendMode::Additive,
            sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
                seed: (seed ^ 0x0E0E_4A2E) as u32,
                color_core: Fp3(accent),
                color_halo: Fp3(accent),
                ..Default::default()
            }),
        },
        // Slow drifting arcane / biolume motes — bigger, softer than neon.
        ParticleAura::ArcaneMotes => Emitter {
            shape: EmitterShape::Box {
                half_extents: Fp3([0.5, 0.6, 0.5]),
            },
            rate: rate(5.0),
            burst: 0,
            max: pop(40),
            life: (2.5, 5.0),
            speed: (0.05, 0.25),
            gravity: -0.015,
            accel: [0.0, 0.08, 0.0],
            drag: 0.6,
            size: (0.09, 0.03),
            start_color: [accent[0], accent[1], accent[2], 0.85],
            end_color: [accent[0], accent[1], accent[2], 0.0],
            blend: ParticleBlendMode::Additive,
            sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
                seed: (seed ^ 0x00A2_C0DE) as u32,
                color_core: Fp3(accent),
                color_halo: Fp3(accent),
                ..Default::default()
            }),
        },
        // A bright downward jet wash beneath the craft.
        ParticleAura::Thruster => Emitter {
            shape: EmitterShape::Sphere { radius: Fp(0.12) },
            rate: rate(14.0),
            burst: 0,
            max: pop(60),
            life: (0.4, 0.9),
            speed: (0.4, 1.0),
            gravity: 0.8,
            accel: [0.0, -0.6, 0.0],
            drag: 0.2,
            size: (0.14, 0.0),
            start_color: [0.7, 0.85, 1.0, 0.9],
            end_color: [0.2, 0.4, 0.9, 0.0],
            blend: ParticleBlendMode::Additive,
            sprite: SovereignTextureConfig::SoftDisc(SovereignSoftDiscConfig {
                seed: (seed ^ 0x0741_05E7) as u32,
                color_core: Fp3([0.85, 0.92, 1.0]),
                color_halo: Fp3([0.35, 0.55, 1.0]),
                ..Default::default()
            }),
        },
        // Warm upward embers arcing back down — scorched / frontier gear.
        ParticleAura::Embers => Emitter {
            shape: EmitterShape::Sphere { radius: Fp(0.1) },
            rate: rate(5.0),
            burst: 0,
            max: pop(40),
            life: (0.8, 1.8),
            speed: (0.6, 1.4),
            gravity: 0.4,
            accel: [0.0, 0.0, 0.0],
            drag: 0.25,
            size: (0.05, 0.0),
            start_color: [1.0, 0.78, 0.34, 1.0],
            end_color: [0.8, 0.22, 0.06, 0.0],
            blend: ParticleBlendMode::Additive,
            sprite: SovereignTextureConfig::Spark(SovereignSparkConfig {
                seed: (seed ^ 0x00E3_B005) as u32,
                points: 4,
                color_core: Fp3([1.0, 0.95, 0.7]),
                color_tip: Fp3([1.0, 0.5, 0.12]),
                ..Default::default()
            }),
        },
    };
    Some(emitter.at(pos, seed))
}

/// Build the spatial-audio voice, or `None` for [`AvatarVoice::None`].
fn voice_audio(voice: AvatarVoice) -> Option<SovereignAudioConfig> {
    match voice {
        AvatarVoice::None => None,
        AvatarVoice::EngineHum => Some(engine_hum()),
        AvatarVoice::NeonBuzz => Some(neon_buzz()),
        AvatarVoice::ArcaneShimmer => Some(arcane_shimmer()),
    }
}

/// A low mechanical drone — a 60 Hz fundamental + octave through a lowpass.
fn engine_hum() -> SovereignAudioConfig {
    let s1 = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 55.0,
            phase_offset: 0.0,
            amplitude: 0.4,
        }),
    );
    let s2 = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 110.0,
            phase_offset: 0.0,
            amplitude: 0.18,
        }),
    );
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(1)),
        ],
    );
    let mix = GraphNode {
        id: NodeId(2),
        kind: NodeKind::Gain(Gain { gain: 0.6 }),
        inputs: mix_in,
    };
    let mut lp_in = std::collections::BTreeMap::new();
    lp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    let lp = GraphNode {
        id: NodeId(3),
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 350.0,
            q: 1.0,
        }),
        inputs: lp_in,
    };
    patch(vec![s1, s2, mix, lp], NodeId(3))
}

/// A buzzing, faintly flickering neon hum — a sawtooth through a bandpass,
/// tremolo'd by a slow LFO.
fn neon_buzz() -> SovereignAudioConfig {
    let saw = node(
        0,
        NodeKind::Sawtooth(SawtoothOsc {
            freq_hz: 120.0,
            polarity: Default::default(),
            amplitude: 0.4,
            anti_alias: Default::default(),
        }),
    );
    let lfo = node(
        1,
        NodeKind::Lfo(Lfo {
            rate_hz: 9.0,
            shape: LfoShape::Sine,
            depth: 0.25,
            offset: 0.7,
        }),
    );
    let mut bp_in = std::collections::BTreeMap::new();
    bp_in.insert("in".to_string(), vec![Connection::from_node(NodeId(0))]);
    let bp = GraphNode {
        id: NodeId(2),
        kind: NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 900.0,
            q: 2.0,
        }),
        inputs: bp_in,
    };
    let mut vca_in = std::collections::BTreeMap::new();
    vca_in.insert("in".to_string(), vec![Connection::from_node(NodeId(2))]);
    vca_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(1))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: vca_in,
    };
    patch(vec![saw, lfo, bp, vca], NodeId(3))
}

/// A soft tonal shimmer — a high sine fifth slowly swelling under an LFO.
fn arcane_shimmer() -> SovereignAudioConfig {
    let s1 = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 660.0,
            phase_offset: 0.0,
            amplitude: 0.22,
        }),
    );
    let s2 = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 990.0,
            phase_offset: 0.0,
            amplitude: 0.14,
        }),
    );
    let lfo = node(
        2,
        NodeKind::Lfo(Lfo {
            rate_hz: 0.5,
            shape: LfoShape::Sine,
            depth: 0.5,
            offset: 0.5,
        }),
    );
    let mut mix_in = std::collections::BTreeMap::new();
    mix_in.insert(
        "in".to_string(),
        vec![
            Connection::from_node(NodeId(0)),
            Connection::from_node(NodeId(1)),
        ],
    );
    mix_in.insert("gain".to_string(), vec![Connection::from_node(NodeId(2))]);
    let vca = GraphNode {
        id: NodeId(3),
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: mix_in,
    };
    patch(vec![s1, s2, lfo, vca], NodeId(3))
}
