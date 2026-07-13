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

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, SawtoothOsc, SineOsc, WhiteNoise,
};

use crate::catalogue::items::fx::Emitter;
use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::texture::{
    SovereignPuffConfig, SovereignSoftDiscConfig, SovereignSparkConfig, SovereignTextureConfig,
};
use crate::pds::types::{Fp, Fp3};
use crate::pds::{EmitterShape, ParticleBlendMode, SovereignAudioConfig};
use crate::seeded_defaults::{AvatarFx, AvatarVoice, ChassisFamily, ParticleAura};

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
    family: ChassisFamily,
    seed: u64,
) {
    if let Some(emitter) = aura_emitter(fx.aura, mount, accent, family, fx.intensity, seed) {
        root.children.push(emitter);
    }
    if let Some(audio) = voice_config(fx.voice, family, seed) {
        root.audio = audio;
    }
}

/// The fraction of the craft's own velocity a motion aura's particles inherit
/// at spawn, so the plume streams aft under way and just puffs at rest. `0`
/// for a static aura (a humanoid's motes hang around the figure) or a
/// non-vehicle. Runtime rate-coupling then thickens any `> 0` emitter with
/// speed — see `world_builder::particles`. Kept inside the particle
/// sanitiser's `[0, 2]` `inherit_velocity` band so the record round-trips
/// unchanged.
fn motion_inherit(aura: ParticleAura, is_vehicle: bool) -> f32 {
    match aura {
        // The chassis floors are always motion FX (only vehicles carry them).
        ParticleAura::Wake => 0.85,
        ParticleAura::Exhaust => 0.7,
        ParticleAura::Vent => 0.5,
        // A vehicle's steam / embers vent from working gear and trail aft too;
        // on a humanoid they hang around the figure.
        ParticleAura::Steam | ParticleAura::Embers if is_vehicle => 0.6,
        _ => 0.0,
    }
}

/// Set an already-built emitter node's `inherit_velocity` (no-op if the node
/// somehow isn't a `ParticleSystem`). [`Emitter::at`] hardcodes `0.0`, so a
/// motion aura patches it here rather than widening the shared catalogue
/// [`Emitter`] with a field every non-avatar call site would have to zero.
fn set_inherit_velocity(g: &mut Generator, inherit: f32) {
    if let GeneratorKind::ParticleSystem(p) = &mut g.kind {
        p.inherit_velocity = Fp(inherit);
    }
}

/// Build the aura emitter for `aura`, or `None` for [`ParticleAura::None`].
/// `intensity` scales the emit rate + population; `accent` colours the
/// decorative auras; `family` picks the chassis-signature recipes (wake /
/// vent / exhaust) and aims the steam / embers aft on a surface craft instead
/// of letting them chimney straight up.
fn aura_emitter(
    aura: ParticleAura,
    pos: [f32; 3],
    accent: [f32; 3],
    family: ChassisFamily,
    intensity: f32,
    seed: u64,
) -> Option<Generator> {
    let rate = |base: f32| base * intensity;
    let pop = |base: u32| ((base as f32 * intensity) as u32).min(120);
    let is_vehicle = family != ChassisFamily::Humanoid;
    let emitter = match aura {
        ParticleAura::None => return None,
        // Pale steam / exhaust. On a surface craft it vents from working gear
        // and streams aft (near-neutral buoyancy + velocity inheritance)
        // rather than rising like a chimney; on a humanoid it plumes upward.
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
            gravity: if is_vehicle { 0.05 } else { -0.04 },
            accel: if is_vehicle {
                [0.0, 0.05, 0.0]
            } else {
                [0.0, 0.2, 0.0]
            },
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
        // Boat chassis floor: a low, spreading whitewater wake-mist off the
        // stern. Non-glowing (it's water), it drifts down and back and thins
        // fast; velocity inheritance streams it into a proper trailing wake.
        ParticleAura::Wake => Emitter {
            shape: EmitterShape::Cone {
                half_angle: Fp(0.7),
                height: Fp(0.12),
            },
            rate: rate(6.0),
            burst: 0,
            max: pop(46),
            life: (0.7, 1.6),
            speed: (0.25, 0.7),
            gravity: 0.35,
            accel: [0.0, 0.0, 0.0],
            drag: 0.5,
            size: (0.12, 0.5),
            start_color: [0.86, 0.9, 0.94, 0.5],
            end_color: [0.94, 0.96, 0.98, 0.0],
            blend: ParticleBlendMode::Alpha,
            sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
                seed: (seed ^ 0x00A7_E600) as u32,
                color_base: Fp3([0.92, 0.95, 0.98]),
                color_shadow: Fp3([0.66, 0.72, 0.8]),
                ..Default::default()
            }),
        },
        // Airship chassis floor: a soft pale vapour puff venting under the
        // gondola, drifting gently down and thinning.
        ParticleAura::Vent => Emitter {
            shape: EmitterShape::Cone {
                half_angle: Fp(0.45),
                height: Fp(0.14),
            },
            rate: rate(4.5),
            burst: 0,
            max: pop(38),
            life: (1.2, 2.6),
            speed: (0.12, 0.4),
            gravity: 0.12,
            accel: [0.0, -0.05, 0.0],
            drag: 0.55,
            size: (0.14, 0.55),
            start_color: [0.78, 0.8, 0.83, 0.34],
            end_color: [0.85, 0.87, 0.9, 0.0],
            blend: ParticleBlendMode::Alpha,
            sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
                seed: (seed ^ 0x0056_E070) as u32,
                color_base: Fp3([0.82, 0.84, 0.87]),
                color_shadow: Fp3([0.56, 0.58, 0.62]),
                ..Default::default()
            }),
        },
        // Skiff chassis floor: a thin grey-brown exhaust wisp off the
        // tailpipe, small and short-lived; the wake of a healthy engine, not
        // a smoke-belching wreck.
        ParticleAura::Exhaust => Emitter {
            shape: EmitterShape::Cone {
                half_angle: Fp(0.28),
                height: Fp(0.12),
            },
            rate: rate(4.0),
            burst: 0,
            max: pop(30),
            life: (0.8, 1.8),
            speed: (0.2, 0.5),
            gravity: -0.01,
            accel: [0.0, 0.03, 0.0],
            drag: 0.5,
            size: (0.07, 0.28),
            start_color: [0.4, 0.4, 0.42, 0.34],
            end_color: [0.55, 0.55, 0.57, 0.0],
            blend: ParticleBlendMode::Alpha,
            sprite: SovereignTextureConfig::Puff(SovereignPuffConfig {
                seed: (seed ^ 0x00E8_0A57) as u32,
                color_base: Fp3([0.48, 0.47, 0.46]),
                color_shadow: Fp3([0.28, 0.27, 0.26]),
                ..Default::default()
            }),
        },
    };
    let mut node = emitter.at(pos, seed);
    // Motion auras inherit a fraction of the craft's velocity so the plume
    // streams aft under way (and the runtime thickens it with speed).
    let inherit = motion_inherit(aura, is_vehicle);
    if inherit > 0.0 {
        set_inherit_velocity(&mut node, inherit);
    }
    Some(node)
}

// ---------------------------------------------------------------------------
// Spatial-audio voices (#796)
//
// The three vehicle families no longer share one fixed 55 Hz drone, and a
// luminous style no longer *replaces* the engine (a cyberpunk skiff used to
// buzz like a sign with no machine underneath). Each family gets its own
// seeded engine voice — a boat's low water-washed rumble, an airship's rotor
// thump, a skiff's detuned putter — and on a luminous *vehicle* that engine is
// mixed in UNDER the neon / arcane voice at low gain instead of being dropped.
// The fundamental + LFO rate are detuned a few percent per avatar (quantised
// into a handful of buckets, so two skiffs rarely idle in unison without
// spawning an unbounded number of distinct bakes). Patch construction is pure
// data (no `std::time`), so it is wasm-safe.
// ---------------------------------------------------------------------------

/// Build the spatial-audio voice config for `voice` on `family`, seeded by
/// `seed`, or `None` for [`AvatarVoice::None`].
fn voice_config(
    voice: AvatarVoice,
    family: ChassisFamily,
    seed: u64,
) -> Option<SovereignAudioConfig> {
    voice_patch(voice, family, seed).map(|p| SovereignAudioConfig::from_patch(&p))
}

/// The raw [`AudioPatch`] for a voice: the family engine, or a luminous voice
/// (with the family engine mixed in underneath at low gain on a *vehicle*,
/// pure on a humanoid). Split from [`voice_config`] so tests can bake it.
fn voice_patch(voice: AvatarVoice, family: ChassisFamily, seed: u64) -> Option<AudioPatch> {
    let is_vehicle = family != ChassisFamily::Humanoid;
    let bucket = detune_bucket(seed);
    let detune = detune_factor(bucket);
    let mut g = GraphBuilder::new();
    let out = match voice {
        AvatarVoice::None => return None,
        AvatarVoice::EngineHum => family_engine(&mut g, family, detune),
        AvatarVoice::NeonBuzz => {
            let lum = neon_buzz(&mut g, detune);
            mix_engine_under(&mut g, lum, family, detune, is_vehicle)
        }
        AvatarVoice::ArcaneShimmer => {
            let lum = arcane_shimmer(&mut g, detune);
            mix_engine_under(&mut g, lum, family, detune, is_vehicle)
        }
    };
    Some(g.into_patch(out, bucket))
}

/// Number of detune buckets — small so a family's engine bakes into at most
/// this many distinct patches (bounded audio-cache footprint) while still
/// spreading avatars across audibly different pitches.
const DETUNE_BUCKETS: u32 = 7;

/// A stable per-avatar detune bucket in `0..DETUNE_BUCKETS`.
fn detune_bucket(seed: u64) -> u32 {
    ((seed ^ 0xEA57_1CE5_1DE0_0001) % DETUNE_BUCKETS as u64) as u32
}

/// The pitch multiplier for a bucket: ±3 % across the buckets, centred on 1.0.
fn detune_factor(bucket: u32) -> f32 {
    let centred = bucket as f32 - (DETUNE_BUCKETS - 1) as f32 * 0.5;
    1.0 + centred / ((DETUNE_BUCKETS - 1) as f32 * 0.5) * 0.03
}

/// The engine sub-voice for a family (a vehicle always has one; a humanoid
/// never reaches this via `EngineHum`, but maps to the skiff putter as a
/// harmless default).
fn family_engine(g: &mut GraphBuilder, family: ChassisFamily, detune: f32) -> NodeId {
    match family {
        ChassisFamily::Boat => boat_hum(g, detune),
        ChassisFamily::Airship => airship_rotor(g, detune),
        ChassisFamily::Skiff | ChassisFamily::Humanoid => skiff_putter(g, detune),
    }
}

/// Sum `luminous` with the family engine at low gain when `is_vehicle`, else
/// return the luminous voice alone.
fn mix_engine_under(
    g: &mut GraphBuilder,
    luminous: NodeId,
    family: ChassisFamily,
    detune: f32,
    is_vehicle: bool,
) -> NodeId {
    if !is_vehicle {
        return luminous;
    }
    let engine = family_engine(g, family, detune);
    // The machine sits quietly under the luminous voice — present, not
    // dominant. A `Gain` with several `"in"` connections sums them.
    let quiet = g.sink(NodeKind::Gain(Gain { gain: 0.14 }), &[engine]);
    g.sink(NodeKind::Gain(Gain { gain: 1.0 }), &[luminous, quiet])
}

/// Boat engine — a low water-washed rumble: a deep fundamental under a slow,
/// band-passed noise wash (the hull working through the water).
fn boat_hum(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let rumble = g.src(NodeKind::Sine(sine(40.0 * detune, 0.34)));
    let noise = g.src(NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let band = g.sink(
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 480.0,
            q: 0.8,
        }),
        &[noise],
    );
    let swell = g.src(NodeKind::Lfo(Lfo {
        rate_hz: 0.4 * detune,
        shape: LfoShape::Sine,
        depth: 0.6,
        offset: 0.35,
    }));
    let wash = g.vca(&[band], swell);
    let mix = g.sink(NodeKind::Gain(Gain { gain: 0.7 }), &[rumble, wash]);
    g.sink(
        NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 320.0,
            q: 0.9,
        }),
        &[mix],
    )
}

/// Airship engine — a hum amplitude-modulated by a 4–6 Hz rotor thump (the
/// beat of the props), matching the helicopter feel.
fn airship_rotor(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let fund = g.src(NodeKind::Sine(sine(52.0 * detune, 0.34)));
    let oct = g.src(NodeKind::Sine(sine(104.0 * detune, 0.14)));
    let body = g.sink(NodeKind::Gain(Gain { gain: 0.8 }), &[fund, oct]);
    let thump = g.src(NodeKind::Lfo(Lfo {
        rate_hz: 5.0 * detune,
        shape: LfoShape::Sine,
        depth: 0.7,
        offset: 0.35,
    }));
    let pumped = g.vca(&[body], thump);
    g.sink(
        NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 360.0,
            q: 1.0,
        }),
        &[pumped],
    )
}

/// Skiff engine — a detuned saw/sine putter around 78 Hz, chugged by a faster
/// LFO; the two slightly-detuned oscillators beat for an idling-motor waver.
fn skiff_putter(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let saw = g.src(NodeKind::Sawtooth(SawtoothOsc {
        freq_hz: 78.0 * detune,
        polarity: Default::default(),
        amplitude: 0.3,
        anti_alias: Default::default(),
    }));
    let sine = g.src(NodeKind::Sine(sine(78.0 * detune * 0.99, 0.22)));
    let body = g.sink(NodeKind::Gain(Gain { gain: 0.6 }), &[saw, sine]);
    let chug = g.src(NodeKind::Lfo(Lfo {
        rate_hz: 8.0 * detune,
        shape: LfoShape::Sine,
        depth: 0.5,
        offset: 0.5,
    }));
    let pumped = g.vca(&[body], chug);
    g.sink(
        NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 520.0,
            q: 1.0,
        }),
        &[pumped],
    )
}

/// A buzzing, faintly flickering neon hum — a sawtooth through a bandpass,
/// tremolo'd by a slow LFO.
fn neon_buzz(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let saw = g.src(NodeKind::Sawtooth(SawtoothOsc {
        freq_hz: 120.0 * detune,
        polarity: Default::default(),
        amplitude: 0.4,
        anti_alias: Default::default(),
    }));
    let band = g.sink(
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 900.0,
            q: 2.0,
        }),
        &[saw],
    );
    let lfo = g.src(NodeKind::Lfo(Lfo {
        rate_hz: 9.0,
        shape: LfoShape::Sine,
        depth: 0.25,
        offset: 0.7,
    }));
    g.vca(&[band], lfo)
}

/// A soft tonal shimmer — a high sine fifth slowly swelling under an LFO.
fn arcane_shimmer(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let s1 = g.src(NodeKind::Sine(sine(660.0 * detune, 0.22)));
    let s2 = g.src(NodeKind::Sine(sine(990.0 * detune, 0.14)));
    let lfo = g.src(NodeKind::Lfo(Lfo {
        rate_hz: 0.5,
        shape: LfoShape::Sine,
        depth: 0.5,
        offset: 0.5,
    }));
    g.vca(&[s1, s2], lfo)
}

/// A plain sine oscillator (no phase offset).
fn sine(freq_hz: f32, amplitude: f32) -> SineOsc {
    SineOsc {
        freq_hz,
        phase_offset: 0.0,
        amplitude,
    }
}

/// Assembles an audio node graph with monotonic ids, so a luminous voice and
/// an engine sub-voice can be built into the *same* graph (disjoint ids) and
/// summed — the "engine under the luminous voice" path — with no hand
/// renumbering.
struct GraphBuilder {
    nodes: Vec<GraphNode>,
    next: u32,
}

impl GraphBuilder {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            next: 0,
        }
    }

    fn push(&mut self, kind: NodeKind, inputs: BTreeMap<String, Vec<Connection>>) -> NodeId {
        let id = NodeId(self.next);
        self.next += 1;
        self.nodes.push(GraphNode { id, kind, inputs });
        id
    }

    /// A source node (oscillator / noise) with no inputs.
    fn src(&mut self, kind: NodeKind) -> NodeId {
        self.push(kind, BTreeMap::new())
    }

    /// A node fed `ins` on its `"in"` port — a filter, or (with several inputs)
    /// a summing bus.
    fn sink(&mut self, kind: NodeKind, ins: &[NodeId]) -> NodeId {
        let mut m = BTreeMap::new();
        m.insert(
            "in".to_string(),
            ins.iter().map(|&n| Connection::from_node(n)).collect(),
        );
        self.push(kind, m)
    }

    /// A VCA: the summed `signals` on `"in"`, amplitude-modulated by `ctrl` on
    /// the `"gain"` port (a `Gain { gain: 0.0 }` base, so the control LFO's
    /// offset sets the DC floor).
    fn vca(&mut self, signals: &[NodeId], ctrl: NodeId) -> NodeId {
        let mut m = BTreeMap::new();
        m.insert(
            "in".to_string(),
            signals.iter().map(|&n| Connection::from_node(n)).collect(),
        );
        m.insert("gain".to_string(), vec![Connection::from_node(ctrl)]);
        self.push(NodeKind::Gain(Gain { gain: 0.0 }), m)
    }

    /// Close the graph into an [`AudioPatch`]. `seed` only drives the noise /
    /// random-LFO draws, so it is the detune bucket — the noise varies per
    /// bucket, not unboundedly per avatar.
    fn into_patch(self, output: NodeId, seed: u32) -> AudioPatch {
        AudioPatch {
            seed,
            graph: NodeGraph {
                nodes: self.nodes,
                output,
            },
        }
    }
}

#[cfg(test)]
mod audio_tests {
    use super::*;
    use bevy_symbios_audio::bake;

    /// Bake a voice patch to a short buffer and assert it makes real,
    /// finite sound — a structural guard that the node graph is valid (no
    /// dangling refs / silence / NaN) before it ever reaches an ear.
    fn assert_audible(patch: &AudioPatch, label: &str) {
        let samples = bake(patch, 44_100, 0.4);
        assert!(!samples.is_empty(), "{label}: baked no samples");
        assert!(
            samples.iter().all(|s| s.is_finite()),
            "{label}: produced non-finite samples"
        );
        assert!(
            samples.iter().any(|s| s.abs() > 1e-3),
            "{label}: baked to silence"
        );
    }

    #[test]
    fn each_family_engine_bakes_to_real_sound() {
        for fam in [
            ChassisFamily::Boat,
            ChassisFamily::Airship,
            ChassisFamily::Skiff,
        ] {
            let patch = voice_patch(AvatarVoice::EngineHum, fam, 7).expect("engine voice");
            assert_audible(&patch, &format!("{fam:?} engine"));
        }
    }

    #[test]
    fn luminous_vehicle_voices_bake_with_engine_underneath() {
        // A luminous *vehicle* carries the family engine mixed in; a luminous
        // humanoid stays pure. Both must bake to sound, and the vehicle's
        // graph is strictly larger (the extra engine + mix nodes).
        for voice in [AvatarVoice::NeonBuzz, AvatarVoice::ArcaneShimmer] {
            let vehicle = voice_patch(voice, ChassisFamily::Skiff, 3).expect("vehicle voice");
            let humanoid = voice_patch(voice, ChassisFamily::Humanoid, 3).expect("humanoid voice");
            assert_audible(&vehicle, &format!("{voice:?} skiff"));
            assert_audible(&humanoid, &format!("{voice:?} humanoid"));
            assert!(
                vehicle.graph.nodes.len() > humanoid.graph.nodes.len(),
                "{voice:?}: vehicle should carry an engine under the luminous voice"
            );
        }
    }

    #[test]
    fn the_three_family_engines_are_distinct() {
        let boat = voice_patch(AvatarVoice::EngineHum, ChassisFamily::Boat, 7).unwrap();
        let airship = voice_patch(AvatarVoice::EngineHum, ChassisFamily::Airship, 7).unwrap();
        let skiff = voice_patch(AvatarVoice::EngineHum, ChassisFamily::Skiff, 7).unwrap();
        // Bake each and require the waveforms differ — they are genuinely
        // different voices, not one shared hum.
        let b = bake(&boat, 22_050, 0.3);
        let a = bake(&airship, 22_050, 0.3);
        let s = bake(&skiff, 22_050, 0.3);
        assert_ne!(b, a, "boat and airship engines are identical");
        assert_ne!(a, s, "airship and skiff engines are identical");
        assert_ne!(b, s, "boat and skiff engines are identical");
    }

    #[test]
    fn detune_is_bounded_and_bucketed() {
        for bucket in 0..DETUNE_BUCKETS {
            let f = detune_factor(bucket);
            assert!((0.97..=1.03).contains(&f), "detune {f} out of ±3%");
        }
        // Every seed lands in a valid bucket.
        for s in 0u64..500 {
            assert!(detune_bucket(s) < DETUNE_BUCKETS);
        }
    }

    #[test]
    fn humanoid_luminous_voice_has_no_engine() {
        // Pure neon / arcane on a humanoid == the same voice with no vehicle
        // engine: the mix step is a no-op, so the node graph is engine-free.
        let human = voice_patch(AvatarVoice::NeonBuzz, ChassisFamily::Humanoid, 9).unwrap();
        // neon_buzz alone is 4 nodes (saw, bandpass, lfo, vca).
        assert_eq!(human.graph.nodes.len(), 4);
    }
}
