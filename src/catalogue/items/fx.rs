//! Shared "bring-it-to-life" fx kit for catalogue entries: the small
//! ambient [`Emitter`] particle builder and the spatial-audio [`node`] /
//! [`patch`] helpers that every theme's `fx.rs` hangs its signature smoke,
//! flame, ember and crackle on.
//!
//! These were copy-pasted verbatim into each per-theme `fx.rs`; the copies
//! had drifted (several kits ran an older [`Emitter`] missing the `burst`
//! field), so they live here once. A theme's `fx.rs` keeps only its own
//! emitter *recipes* (the per-prop colours, rates and shapes) and builds
//! them through this shared [`Emitter`].
//!
//! Particle emitters are returned as [`Generator`] nodes (a
//! `GeneratorKind::ParticleSystem`) positioned in the prop's world frame, so
//! they drop straight into an [`assemble`](super::util::assemble) list.
//! Counts stay small (signature, not spectacle) and well within the particle
//! sanitiser's bounds. The audio helpers wrap a node graph into a
//! mute-defaulted [`SovereignAudioConfig`] the world compiler plays spatially
//! at the owning node's position.
//!
//! Sound more than one kit plays lives here too. [`FireCrackle`] is a builder
//! carrying the six numbers each kit tuned its fire by: five kits had grown
//! near-copies of one graph that differed only in those numbers. [`WaterDrip`]
//! carries the pitch, rate and echo of a dripping pool. [`water_trickle`] and
//! [`ballast_buzz`] are shared outright, and [`wired`] builds a node together
//! with its inputs, which is most of the text of any patch.
//!
//! Two properties of the audio graph shape every patch here. A construct's
//! patch is baked to a one-second buffer and looped
//! (`world_builder::spatial_audio::CONSTRUCT_PATCH_SECS`), so anything that
//! modulates a new patch runs at a whole number of hertz or the loop seam
//! stutters. And a `Gain` node multiplies by `gain + input("gain")` with no
//! floor: an LFO whose trough dips below zero flips the signal's phase rather
//! than silencing it. A sound that must fall quiet between events keeps its
//! LFO's `offset` at or above its `depth`, or decays on a sawtooth as
//! [`WaterDrip`] does.

use bevy_symbios_audio::{
    AudioPatch, BiquadBandpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo, LfoShape,
    NodeGraph, NodeId, NodeKind, Reverb, SineOsc, WhiteNoise,
};

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, Generator, GeneratorKind, ParticleBlendMode,
    ParticleParams, SimulationSpace, SovereignAudioConfig, SovereignTextureConfig, TextureFilter,
    TransformData,
};

/// The varying parameters of a small ambient emitter; the rest are filled
/// with shared defaults by [`Emitter::at`].
///
/// Used by the per-theme catalogue `fx.rs` kits and by the avatar FX
/// builder ([`crate::pds::avatar::default_visuals`]), hence `pub(crate)`.
pub(crate) struct Emitter {
    pub(crate) shape: EmitterShape,
    pub(crate) rate: f32,
    pub(crate) burst: u32,
    pub(crate) max: u32,
    pub(crate) life: (f32, f32),
    pub(crate) speed: (f32, f32),
    pub(crate) gravity: f32,
    pub(crate) accel: [f32; 3],
    pub(crate) drag: f32,
    pub(crate) size: (f32, f32),
    pub(crate) start_color: [f32; 4],
    pub(crate) end_color: [f32; 4],
    pub(crate) blend: ParticleBlendMode,
    pub(crate) sprite: SovereignTextureConfig,
}

impl Emitter {
    /// Finish the emitter into a positioned [`Generator`] node, seeded for
    /// determinism.
    pub(crate) fn at(self, pos: [f32; 3], seed: u64) -> Generator {
        Generator {
            kind: GeneratorKind::ParticleSystem(Box::new(ParticleParams {
                emitter_shape: self.shape,
                rate_per_second: Fp(self.rate),
                burst_count: self.burst,
                max_particles: self.max,
                looping: true,
                duration: Fp(2.0),
                lifetime_min: Fp(self.life.0),
                lifetime_max: Fp(self.life.1),
                speed_min: Fp(self.speed.0),
                speed_max: Fp(self.speed.1),
                gravity_multiplier: Fp(self.gravity),
                acceleration: Fp3(self.accel),
                linear_drag: Fp(self.drag),
                start_size: Fp(self.size.0),
                end_size: Fp(self.size.1),
                start_color: Fp4(self.start_color),
                end_color: Fp4(self.end_color),
                blend_mode: self.blend,
                billboard: true,
                simulation_space: SimulationSpace::World,
                inherit_velocity: Fp(0.0),
                collide_terrain: false,
                collide_water: false,
                collide_colliders: false,
                bounce: Fp(0.3),
                friction: Fp(0.5),
                seed,
                texture: None,
                texture_atlas: None,
                frame_mode: AnimationFrameMode::RandomFrame,
                texture_filter: TextureFilter::Linear,
                procedural_texture: self.sprite,
            })),
            transform: TransformData {
                translation: Fp3(pos),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                scale: Fp3([1.0, 1.0, 1.0]),
            },
            children: Vec::new(),
            audio: SovereignAudioConfig::None,
        }
    }
}

/// A graph node with the given id and kind and no inputs wired yet.
pub(crate) fn node(id: u32, kind: NodeKind) -> GraphNode {
    GraphNode {
        id: NodeId(id),
        kind,
        inputs: std::collections::BTreeMap::new(),
    }
}

/// Wrap a node list + output into a mute-defaulted spatial audio config.
pub(crate) fn patch(nodes: Vec<GraphNode>, output: NodeId) -> SovereignAudioConfig {
    SovereignAudioConfig::from_patch(&AudioPatch {
        seed: 0,
        graph: NodeGraph { nodes, output },
    })
}

/// A graph node wired to its inputs: each `(port, sources)` pair connects
/// every node id in `sources` to `port`, in order.
pub(crate) fn wired(id: u32, kind: NodeKind, ports: &[(&str, &[u32])]) -> GraphNode {
    let mut inputs = std::collections::BTreeMap::new();
    for (port, sources) in ports {
        inputs.insert(
            (*port).to_string(),
            sources
                .iter()
                .map(|source| Connection::from_node(NodeId(*source)))
                .collect(),
        );
    }
    GraphNode {
        id: NodeId(id),
        kind,
        inputs,
    }
}

/// A warm, irregular fire crackle: band-passed noise broken into bursts by
/// an LFO, over a low ember rumble.
///
/// The fields are what each kit tuned, since a forge hearth is brighter than
/// coals buried in a drum. The graph around them is fixed, so a kit's fire is
/// one of these and its bytes did not move when the copies became one.
pub(crate) struct FireCrackle {
    /// Level of the white noise the crackle is cut from.
    pub noise: f32,
    /// Rate of the pulse that breaks the noise into bursts, in hertz.
    pub pulse_hz: f32,
    /// Offset of that pulse. It sits below the pulse's 0.8 depth, so each
    /// trough flips phase instead of falling silent: every cycle is one loud
    /// burst and one softer one.
    pub pulse_floor: f32,
    /// Centre of the band the crackle is heard in, in hertz.
    pub pitch_hz: f32,
    /// Frequency of the ember rumble under it, in hertz.
    pub rumble_hz: f32,
    /// Level of that rumble.
    pub rumble: f32,
}

impl FireCrackle {
    /// The crackle as a spatial patch.
    pub(crate) fn patch(&self) -> SovereignAudioConfig {
        let noise = node(
            0,
            NodeKind::WhiteNoise(WhiteNoise {
                amplitude: self.noise,
            }),
        );
        let pulse = node(
            1,
            NodeKind::Lfo(Lfo {
                rate_hz: self.pulse_hz,
                shape: LfoShape::Sine,
                depth: 0.8,
                offset: self.pulse_floor,
            }),
        );
        let band = wired(
            2,
            NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: self.pitch_hz,
                q: 2.0,
            }),
            &[("in", &[0])],
        );
        let crackle = wired(
            3,
            NodeKind::Gain(Gain { gain: 0.0 }),
            &[("in", &[2]), ("gain", &[1])],
        );
        let rumble = node(
            4,
            NodeKind::Sine(SineOsc {
                freq_hz: self.rumble_hz,
                phase_offset: 0.0,
                amplitude: self.rumble,
            }),
        );
        let mix = wired(5, NodeKind::Gain(Gain { gain: 0.7 }), &[("in", &[3, 4])]);
        patch(vec![noise, pulse, band, crackle, rumble, mix], NodeId(5))
    }
}

/// Water dripping into a still pool: a resonant plink struck `per_sec` times
/// a second, each decaying to silence before the next lands and each at a
/// random level, so some are all but lost.
///
/// `per_sec` is a whole number because the patch loops every second. `echo`
/// is the reverb's room size, so a well shaft rings where an open basin does
/// not.
pub(crate) struct WaterDrip {
    /// Pitch of the plink, in hertz. Low for a fish breaking a pond, high
    /// for a drop off stone.
    pub pitch_hz: f32,
    /// Drips a second.
    pub per_sec: u8,
    /// Reverb room size, in `[0, 1]`.
    pub echo: f32,
}

impl WaterDrip {
    /// The drip as a spatial patch.
    pub(crate) fn patch(&self) -> SovereignAudioConfig {
        let rate_hz = f32::from(self.per_sec);
        let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 2.0 }));
        let plink = wired(
            1,
            NodeKind::BiquadBandpass(BiquadBandpass {
                center_hz: self.pitch_hz,
                q: 7.0,
            }),
            &[("in", &[0])],
        );
        // A falling sawtooth from 1 at the strike to 0 at the next: offset
        // equals depth, so the level never dips below silence.
        let decay = node(
            2,
            NodeKind::Lfo(Lfo {
                rate_hz,
                shape: LfoShape::Saw,
                depth: -0.5,
                offset: 0.5,
            }),
        );
        // Applied twice, the linear fall becomes a quadratic one: a sharp
        // strike and a long quiet before the next.
        let struck = wired(
            3,
            NodeKind::Gain(Gain { gain: 0.0 }),
            &[("in", &[1]), ("gain", &[2])],
        );
        let fading = wired(
            4,
            NodeKind::Gain(Gain { gain: 0.0 }),
            &[("in", &[3]), ("gain", &[2])],
        );
        // A fresh level in [0, 1] for each drip, drawn on the same cycle.
        let level = node(
            5,
            NodeKind::Lfo(Lfo {
                rate_hz,
                shape: LfoShape::Random,
                depth: 0.5,
                offset: 0.5,
            }),
        );
        let landed = wired(
            6,
            NodeKind::Gain(Gain { gain: 0.0 }),
            &[("in", &[4]), ("gain", &[5])],
        );
        let room = wired(
            7,
            NodeKind::Reverb(Reverb {
                room_size: self.echo,
                damping: 0.5,
                mix: 0.3,
            }),
            &[("in", &[6])],
        );
        patch(
            vec![noise, plink, decay, struck, fading, level, landed, room],
            NodeId(7),
        )
    }
}

/// Running water: a bright spill band and a hollow body band of noise,
/// rippled to a random level eight times a second and softened by a lowpass.
/// A rill over a weir.
pub(crate) fn water_trickle() -> SovereignAudioConfig {
    let noise = node(0, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let spill = wired(
        1,
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1800.0,
            q: 0.9,
        }),
        &[("in", &[0])],
    );
    let body = wired(
        2,
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 650.0,
            q: 1.2,
        }),
        &[("in", &[0])],
    );
    let ripple = node(
        3,
        NodeKind::Lfo(Lfo {
            rate_hz: 8.0,
            shape: LfoShape::Random,
            depth: 0.18,
            offset: 0.0,
        }),
    );
    let level = wired(
        4,
        NodeKind::Gain(Gain { gain: 0.5 }),
        &[("in", &[1, 2]), ("gain", &[3])],
    );
    let soften = wired(
        5,
        NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 3200.0,
            q: 0.7,
        }),
        &[("in", &[4])],
    );
    patch(vec![noise, spill, body, ripple, level, soften], NodeId(5))
}

/// The buzz of a lamp ballast: the mains tone doubled, because the arc
/// restrikes every half cycle, with its octave, and a thin fizz flickering
/// to a random level twenty times a second. A floodlight bank.
pub(crate) fn ballast_buzz() -> SovereignAudioConfig {
    let tone = node(
        0,
        NodeKind::Sine(SineOsc {
            freq_hz: 100.0,
            phase_offset: 0.0,
            amplitude: 0.07,
        }),
    );
    let octave = node(
        1,
        NodeKind::Sine(SineOsc {
            freq_hz: 200.0,
            phase_offset: 0.0,
            amplitude: 0.035,
        }),
    );
    let noise = node(2, NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.25 }));
    let fizz = wired(
        3,
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 3600.0,
            q: 2.0,
        }),
        &[("in", &[2])],
    );
    let flicker = node(
        4,
        NodeKind::Lfo(Lfo {
            rate_hz: 20.0,
            shape: LfoShape::Random,
            depth: 0.08,
            offset: 0.0,
        }),
    );
    let flickering = wired(
        5,
        NodeKind::Gain(Gain { gain: 0.12 }),
        &[("in", &[3]), ("gain", &[4])],
    );
    let mix = wired(6, NodeKind::Gain(Gain { gain: 0.8 }), &[("in", &[0, 1, 5])]);
    patch(
        vec![tone, octave, noise, fizz, flicker, flickering, mix],
        NodeId(6),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A drip has to fall silent before the next one lands, or it is a hiss
    /// with a pulse in it. `Gain` has no floor, so this is the property a
    /// retune breaks without anyone hearing it in a diff: a decay LFO dipping
    /// below zero flips the plink's phase and rings on through the gap.
    #[test]
    fn a_water_drip_falls_silent_before_the_next_lands() {
        const RATE: u32 = 22_050;
        const PER_SEC: u8 = 3;
        let drip = WaterDrip {
            pitch_hz: 1200.0,
            per_sec: PER_SEC,
            echo: 0.0,
        };
        let SovereignAudioConfig::Patch { patch } = drip.patch() else {
            panic!("a drip is a patch");
        };
        let samples = bevy_symbios_audio::bake(&patch.to_native(), RATE, 1.0);
        let cycle = RATE as usize / usize::from(PER_SEC);
        let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();

        let mut heard = 0;
        for k in 0..usize::from(PER_SEC) {
            let one = &samples[k * cycle..(k + 1) * cycle];
            let strike = rms(&one[..cycle / 10]);
            let gap = rms(&one[cycle - cycle / 20..]);
            // A drip whose random level came up near zero proves nothing
            // either way; the others must be near silent at their tail.
            if strike < 1e-3 {
                continue;
            }
            heard += 1;
            assert!(
                gap < strike * 0.05,
                "drip {k} still rings at {gap} against a strike of {strike}"
            );
        }
        assert!(
            heard > 0,
            "every drip came up silent — the test measured nothing"
        );
    }
}
