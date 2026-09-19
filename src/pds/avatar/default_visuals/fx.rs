//! Build-side avatar FX - turns an [`AvatarFx`] spec into the actual
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
    AudioPatch, BiquadBandpass, BiquadHighpass, BiquadLowpass, Connection, Gain, GraphNode, Lfo,
    LfoShape, NodeGraph, NodeId, NodeKind, PinkNoise, SawtoothOsc, SineOsc, WhiteNoise,
};

use crate::catalogue::items::fx::Emitter;
use crate::pds::generator::{Generator, GeneratorKind};
use crate::pds::texture::{
    SovereignPuffConfig, SovereignSoftDiscConfig, SovereignSparkConfig, SovereignTextureConfig,
};
use crate::pds::types::{Fp, Fp3};
use crate::pds::{EmitterShape, ParticleBlendMode, SovereignAudioConfig};
use crate::seeded_defaults::{AvatarFx, AvatarVoice, ChassisFamily, ParticleAura};

use super::boats::Propulsion;

/// Hang the FX on a freshly-built avatar root: push the aura emitter as a
/// child at `mount` (in the root's local frame) and set the body voice on
/// the root's `audio`. A no-op for `ParticleAura::None` / `AvatarVoice::None`.
///
/// `accent` is the avatar's primary accent - decorative auras (neon /
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
/// speed - see `world_builder::particles`. Kept inside the particle
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

/// Grow an emitter's particle sprites by the visual root's uniform scale
/// (#1361), so a craft built at airship class puffs steam to match.
///
/// Everything else about the plume already rides that scale, because the
/// emitter node hangs under the scaled root and `world_builder::particles`
/// pushes both the emission volume and each particle's launch velocity through
/// the emitter's affine. The sprite is the exception: a particle spawns into
/// WORLD space ([`SimulationSpace::World`](crate::pds::generator::SimulationSpace),
/// so it is never parented to the
/// emitter) with its transform scale set straight from `start_size`. Left
/// alone, a funnel twice the size would vent the same small puffs.
///
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
        // Slow drifting arcane / biolume motes - bigger, softer than neon.
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
        // Warm upward embers arcing back down - scorched / frontier gear.
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
// Spatial-audio voices (#796, #1383, #1385)
//
// The three vehicle families no longer share one fixed 55 Hz drone, and a
// luminous style no longer *replaces* the drive (a cyberpunk skiff used to
// buzz like a sign with no machine underneath). Each craft speaks with its own
// DRIVE - an airship's rotor thump, a skiff's detuned putter, a motor boat's
// water-washed rumble, and for a boat under sail no engine at all, only the
// wash along her hull and the wind in her rig - and on a luminous *vehicle*
// that drive is mixed in UNDER the neon / arcane voice at low gain instead of
// being dropped. Which drive a boat has is asked of the craft that is DRAWN
// (`boats::propulsion`), never of the type a seed picked, so the voice is
// right before every type is built.
//
// A voice is a construct patch: baked to ONE second and looped whole
// (`world_builder::spatial_audio::CONSTRUCT_PATCH_SECS`). So every pitch and
// every modulation rate here is a whole number of hertz, through [`hz`], or
// the loop seam steps once a second (#1385). Pitches are detuned a few
// percent per avatar and rounded after the detune, so the low voices keep
// three to five distinct pitches over the buckets and the high ones all
// seven; a modulator's rate rounds back to its own. The buckets also bound
// the number of distinct bakes. Patch construction is pure data (no
// `std::time`), so it is wasm-safe.
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

/// The raw [`AudioPatch`] for a voice on the craft `seed` draws. Split from
/// [`voice_config`] so tests can bake it.
fn voice_patch(voice: AvatarVoice, family: ChassisFamily, seed: u64) -> Option<AudioPatch> {
    driven_voice_patch(voice, family, drive_of(family, seed), detune_bucket(seed))
}

/// How the avatar for `seed` on `family` is driven. A boat answers for the
/// craft she is drawn as; every other family has an engine.
fn drive_of(family: ChassisFamily, seed: u64) -> Propulsion {
    match family {
        ChassisFamily::Boat => super::boats::propulsion(seed),
        ChassisFamily::Airship | ChassisFamily::Skiff | ChassisFamily::Humanoid => {
            Propulsion::Engine
        }
    }
}

/// The voice for an explicit drive and detune bucket: the family's drive, or
/// a luminous voice (with the drive mixed in underneath at low gain on a
/// *vehicle*, pure on a humanoid). Split from [`voice_patch`] so the tests
/// reach an engine boat before any boat type with an engine is drawn.
fn driven_voice_patch(
    voice: AvatarVoice,
    family: ChassisFamily,
    drive: Propulsion,
    bucket: u32,
) -> Option<AudioPatch> {
    let is_vehicle = family != ChassisFamily::Humanoid;
    let detune = detune_factor(bucket);
    let mut g = GraphBuilder::new();
    let out = match voice {
        AvatarVoice::None => return None,
        AvatarVoice::Drive => family_drive(&mut g, family, drive, detune),
        AvatarVoice::NeonBuzz => {
            let lum = neon_buzz(&mut g, detune);
            mix_drive_under(&mut g, lum, family, drive, detune, is_vehicle)
        }
        AvatarVoice::ArcaneShimmer => {
            let lum = arcane_shimmer(&mut g, detune);
            mix_drive_under(&mut g, lum, family, drive, detune, is_vehicle)
        }
    };
    Some(g.into_patch(out, bucket))
}

/// What the seeded avatar for `seed` sounds like, in words (#1383) - the
/// render tool's `--outfit` line, so a survey can find a seed by its voice.
pub(super) fn voice_label(seed: u64) -> String {
    let family = ChassisFamily::for_seed(seed);
    if family == ChassisFamily::Humanoid {
        // The rigged family returns before any FX is attached.
        return "silent (a rigged body carries no seeded voice)".to_string();
    }
    let voice = AvatarFx::for_seed(seed).voice;
    let drive = drive_of(family, seed);
    let under = match (drive, family) {
        (Propulsion::Sail, _) => "wash and rig wind",
        (Propulsion::Engine, ChassisFamily::Boat) => "boat engine hum",
        (Propulsion::Engine, ChassisFamily::Airship) => "rotor thump",
        (Propulsion::Engine, ChassisFamily::Skiff | ChassisFamily::Humanoid) => "putter",
    };
    let bucket = detune_bucket(seed);
    match voice {
        AvatarVoice::None => "silent".to_string(),
        AvatarVoice::Drive => format!(
            "{}, {} ({under}), detune bucket {bucket}",
            voice.label(),
            drive.label()
        ),
        AvatarVoice::NeonBuzz | AvatarVoice::ArcaneShimmer => format!(
            "{} over the {under} ({}), detune bucket {bucket}",
            voice.label(),
            drive.label()
        ),
    }
}

/// Number of detune buckets - small so a family's drive bakes into at most
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

/// A pitch or a rate, detuned and then rounded to a whole number of hertz,
/// so it closes the one-second loop it is baked into (#1385). Every voice
/// reaches its oscillators and modulators through this, and the rounding
/// comes AFTER the detune: 40 Hz x 0.97 is 38.8, which stepped at the seam.
fn hz(base: f32, detune: f32) -> f32 {
    (base * detune).round()
}

/// The drive sub-voice for a family (a vehicle always has one; a humanoid
/// never reaches this via `Drive`, but maps to the skiff putter as a harmless
/// default). A sailing craft has no engine note, whatever her family.
fn family_drive(
    g: &mut GraphBuilder,
    family: ChassisFamily,
    drive: Propulsion,
    detune: f32,
) -> NodeId {
    match (drive, family) {
        (Propulsion::Sail, _) => under_sail(g),
        (Propulsion::Engine, ChassisFamily::Boat) => boat_hum(g, detune),
        (Propulsion::Engine, ChassisFamily::Airship) => airship_rotor(g, detune),
        (Propulsion::Engine, ChassisFamily::Skiff | ChassisFamily::Humanoid) => {
            skiff_putter(g, detune)
        }
    }
}

/// Sum `luminous` with the family's drive at low gain when `is_vehicle`, else
/// return the luminous voice alone.
fn mix_drive_under(
    g: &mut GraphBuilder,
    luminous: NodeId,
    family: ChassisFamily,
    drive: Propulsion,
    detune: f32,
    is_vehicle: bool,
) -> NodeId {
    if !is_vehicle {
        return luminous;
    }
    let under = family_drive(g, family, drive, detune);
    // The craft sits quietly under the luminous voice - present, not
    // dominant. A `Gain` with several `"in"` connections sums them.
    let quiet = g.sink(NodeKind::Gain(Gain { gain: 0.14 }), &[under]);
    g.sink(NodeKind::Gain(Gain { gain: 1.0 }), &[luminous, quiet])
}

/// A boat under sail (#1383, owner decision C1; the owner's ear picked the
/// steady candidate): no engine and no tone, only the wash along her hull -
/// band-passed noise, softened - and a thin wind in the rig - pink noise
/// high-passed and banded up where rigging sings. Nothing in it oscillates,
/// so there is no pitch to detune and nothing to close at the loop seam; the
/// detune bucket still seeds the noise, so seven textures remain. RMS about
/// 0.11, near the skiff putter's.
fn under_sail(g: &mut GraphBuilder) -> NodeId {
    let noise = g.src(NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let band = g.sink(
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 480.0,
            q: 0.8,
        }),
        &[noise],
    );
    let wash = g.sink(NodeKind::Gain(Gain { gain: 0.62 }), &[band]);
    let wash = g.sink(
        NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 700.0,
            q: 0.7,
        }),
        &[wash],
    );
    let air = g.src(NodeKind::PinkNoise(PinkNoise { amplitude: 0.5 }));
    let high = g.sink(
        NodeKind::BiquadHighpass(BiquadHighpass {
            cutoff_hz: 900.0,
            q: 0.7,
        }),
        &[air],
    );
    let rig = g.sink(
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 1600.0,
            q: 1.2,
        }),
        &[high],
    );
    let wind = g.sink(NodeKind::Gain(Gain { gain: 0.30 }), &[rig]);
    g.sink(NodeKind::Gain(Gain { gain: 3.0 }), &[wash, wind])
}

/// A motor boat's engine - a low water-washed rumble: a deep fundamental over
/// a band-passed noise wash (the hull working through the water). Heard by no
/// seed yet: the sloop sails, and the engine types (#1370, #1372) are the
/// ones that will declare [`Propulsion::Engine`].
///
/// The wash used to swell at 0.4 Hz, which a one-second loop cannot hold: it
/// snapped back 4 dB every second (#1385). It is steady now, at the swell's
/// own RMS level (0.459 x sqrt(1.5)), matching the steady wash the owner
/// picked for the sail.
fn boat_hum(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let rumble = g.src(NodeKind::Sine(sine(hz(40.0, detune), 0.34)));
    let noise = g.src(NodeKind::WhiteNoise(WhiteNoise { amplitude: 0.5 }));
    let band = g.sink(
        NodeKind::BiquadBandpass(BiquadBandpass {
            center_hz: 480.0,
            q: 0.8,
        }),
        &[noise],
    );
    let wash = g.sink(NodeKind::Gain(Gain { gain: 0.56 }), &[band]);
    let mix = g.sink(NodeKind::Gain(Gain { gain: 0.7 }), &[rumble, wash]);
    g.sink(
        NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 320.0,
            q: 0.9,
        }),
        &[mix],
    )
}

/// Airship engine - a hum amplitude-modulated by a 5 Hz rotor thump (the
/// beat of the props), matching the helicopter feel. The octave is twice the
/// rounded fundamental, so it stays a true octave on every bucket.
fn airship_rotor(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let pitch = hz(52.0, detune);
    let fund = g.src(NodeKind::Sine(sine(pitch, 0.34)));
    let oct = g.src(NodeKind::Sine(sine(2.0 * pitch, 0.14)));
    let body = g.sink(NodeKind::Gain(Gain { gain: 0.8 }), &[fund, oct]);
    let thump = g.src(NodeKind::Lfo(Lfo {
        rate_hz: hz(5.0, detune),
        shape: LfoShape::Sine,
        depth: 0.495,
        offset: 0.495,
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

/// Skiff engine - a saw/sine putter around 78 Hz, chugged by a faster LFO;
/// the two oscillators sit one hertz apart, so they beat once a second for an
/// idling-motor waver. (They used to sit 1 % apart, a pair that can never
/// both be whole numbers of hertz, #1385.)
fn skiff_putter(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let pitch = hz(78.0, detune);
    let saw = g.src(NodeKind::Sawtooth(SawtoothOsc {
        freq_hz: pitch,
        polarity: Default::default(),
        amplitude: 0.3,
        anti_alias: Default::default(),
    }));
    let sine = g.src(NodeKind::Sine(sine(pitch - 1.0, 0.22)));
    let body = g.sink(NodeKind::Gain(Gain { gain: 0.6 }), &[saw, sine]);
    let chug = g.src(NodeKind::Lfo(Lfo {
        rate_hz: hz(8.0, detune),
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

/// A buzzing, faintly flickering neon hum - a sawtooth through a bandpass,
/// tremolo'd by a 9 Hz LFO.
fn neon_buzz(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let saw = g.src(NodeKind::Sawtooth(SawtoothOsc {
        freq_hz: hz(120.0, detune),
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

/// A soft tonal shimmer - a high sine fifth swelling once a second.
///
/// The swell was 0.5 Hz, and a one-second loop played only its upper half: a
/// rise from 0.5 to 1 and back, every second. It is that, as a whole cycle
/// now (#1385): 1 Hz between 0.55 and 1.05, a level within 0.2 dB of what
/// played.
fn arcane_shimmer(g: &mut GraphBuilder, detune: f32) -> NodeId {
    let s1 = g.src(NodeKind::Sine(sine(hz(660.0, detune), 0.22)));
    let s2 = g.src(NodeKind::Sine(sine(hz(990.0, detune), 0.14)));
    let lfo = g.src(NodeKind::Lfo(Lfo {
        rate_hz: 1.0,
        shape: LfoShape::Sine,
        depth: 0.25,
        offset: 0.8,
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
/// summed - the "engine under the luminous voice" path - with no hand
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

    /// A node fed `ins` on its `"in"` port - a filter, or (with several inputs)
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
    /// random-LFO draws, so it is the detune bucket - the noise varies per
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

    const VOICES: [AvatarVoice; 3] = [
        AvatarVoice::Drive,
        AvatarVoice::NeonBuzz,
        AvatarVoice::ArcaneShimmer,
    ];
    const DRIVES: [Propulsion; 2] = [Propulsion::Sail, Propulsion::Engine];

    /// Every voice patch there is: each voice on each chassis under each
    /// drive, on every detune bucket, labelled for a failure message. The
    /// drive is walked explicitly, because no boat is DRAWN with an engine
    /// yet and the engine boat's voice must still hold every rule for the
    /// types that will be (#1370, #1372).
    fn every_voice() -> Vec<(String, AudioPatch)> {
        let mut all = Vec::new();
        for voice in VOICES {
            for family in ChassisFamily::ALL {
                for drive in DRIVES {
                    for bucket in 0..DETUNE_BUCKETS {
                        if let Some(patch) = driven_voice_patch(voice, family, drive, bucket) {
                            all.push((
                                format!("{voice:?} on {family:?} {drive:?}, bucket {bucket}"),
                                patch,
                            ));
                        }
                    }
                }
            }
        }
        assert!(
            !all.is_empty(),
            "no voice built a patch - a walk would prove nothing"
        );
        all
    }

    /// The oscillators in a patch - the nodes that carry a pitch.
    fn oscillators(patch: &AudioPatch) -> Vec<&NodeKind> {
        patch
            .graph
            .nodes
            .iter()
            .map(|n| &n.kind)
            .filter(|k| {
                matches!(
                    k,
                    NodeKind::Sine(_)
                        | NodeKind::Square(_)
                        | NodeKind::Sawtooth(_)
                        | NodeKind::Triangle(_)
                )
            })
            .collect()
    }

    /// A seed on each detune bucket, in bucket order.
    fn a_seed_per_bucket() -> Vec<u64> {
        (0..DETUNE_BUCKETS)
            .map(|b| (0u64..).find(|&s| detune_bucket(s) == b).unwrap())
            .collect()
    }

    /// Bake a voice patch to a short buffer and assert it makes real,
    /// finite sound - a structural guard that the node graph is valid (no
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

    /// No avatar voice drives a `Gain` below zero (#1348): the VCA has no
    /// floor, so a trough below zero flips the voice's phase and it keeps
    /// sounding where the swell or the thump means to fall away. Every voice
    /// on every chassis and drive, over every detune bucket, since the drive
    /// a luminous vehicle carries is built into the same graph.
    #[test]
    fn no_avatar_voice_inverts_through_a_gain_trough() {
        use crate::catalogue::items::fx::gain_troughs;
        for (label, patch) in every_voice() {
            let troughs = gain_troughs(&patch);
            assert!(troughs.is_empty(), "{label} flips phase: {troughs:?}");
        }
    }

    /// Every avatar voice closes its one-second loop (#1385): no oscillator
    /// and no LFO in it runs a fractional number of cycles a second, on any
    /// chassis, drive or detune bucket. A voice is a construct patch, baked
    /// to one second and looped whole, so a pitch of 40 Hz x 0.97 left a step
    /// at the seam 51-86 times the largest one inside the loop - a thump once
    /// a second on six buckets of seven. The bake now rounds such a rate, but
    /// a voice states the pitch it will be heard at.
    #[test]
    fn every_avatar_voice_closes_its_one_second_loop() {
        use crate::catalogue::items::fx::off_loop_rates;
        let found: Vec<String> = every_voice()
            .iter()
            .flat_map(|(label, patch)| {
                off_loop_rates(patch)
                    .into_iter()
                    .map(move |(node, hz)| format!("{label}: node {} at {hz} Hz", node.0))
            })
            .collect();
        assert!(
            found.is_empty(),
            "{} rates leave the loop open:\n{}",
            found.len(),
            found.join("\n")
        );
    }

    /// The seam as the world plays it (#1385): a tonal voice baked through
    /// the construct bake job - the app's own rate, length and warm-up -
    /// steps across its loop seam by no more than the largest step inside
    /// the loop. The airship's rotor and the skiff's putter on the worst
    /// detune bucket, since a noise voice has no pitch to close and the
    /// luminous voices carry no filter to settle. With the warm-up at zero
    /// the rotor steps 5.4x (the control, run by hand in #1385).
    #[test]
    fn a_tonal_voice_meets_itself_at_the_loop_seam() {
        let seed = a_seed_per_bucket()[0];
        for family in [ChassisFamily::Airship, ChassisFamily::Skiff] {
            let patch = voice_patch(AvatarVoice::Drive, family, seed).expect("a drive voice");
            let (wav, _) = crate::world_builder::spatial_audio::bake_construct_wav_bytes(
                &SovereignAudioConfig::from_patch(&patch),
            )
            .expect("a patch bakes");
            // Mono 16-bit PCM behind a 44-byte header.
            let s: Vec<f32> = wav[44..]
                .chunks_exact(2)
                .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0)
                .collect();
            let inner = s
                .windows(2)
                .map(|p| (p[1] - p[0]).abs())
                .fold(0.0f32, f32::max);
            let seam = (s[0] - s[s.len() - 1]).abs();
            assert!(
                seam <= inner,
                "{family:?}: the seam steps {seam:.4}, {:.1}x the largest step inside the loop ({inner:.4})",
                seam / inner
            );
        }
    }

    /// A craft under sail makes no engine note (#1383, owner decision C1):
    /// her voice holds no oscillator at all, on any family or bucket - the
    /// wash and the rig wind are noise. And it is still real sound.
    #[test]
    fn a_sailing_craft_voice_holds_no_oscillator() {
        for family in ChassisFamily::ALL {
            for bucket in 0..DETUNE_BUCKETS {
                let patch =
                    driven_voice_patch(AvatarVoice::Drive, family, Propulsion::Sail, bucket)
                        .expect("a drive voice");
                assert!(
                    oscillators(&patch).is_empty(),
                    "{family:?} under sail, bucket {bucket}: {:?}",
                    oscillators(&patch)
                );
                assert_audible(&patch, &format!("{family:?} under sail"));
            }
        }
    }

    /// A luminous boat under sail mixes the sail under her neon or arcane
    /// voice, not an engine: the only oscillators in her graph are the
    /// luminous voice's own - exactly as many as the same voice carries pure,
    /// on a humanoid.
    #[test]
    fn a_luminous_sailing_boat_holds_only_the_luminous_voices_oscillators() {
        for voice in [AvatarVoice::NeonBuzz, AvatarVoice::ArcaneShimmer] {
            let boat = driven_voice_patch(voice, ChassisFamily::Boat, Propulsion::Sail, 0).unwrap();
            let pure =
                driven_voice_patch(voice, ChassisFamily::Humanoid, Propulsion::Engine, 0).unwrap();
            assert_eq!(
                oscillators(&boat),
                oscillators(&pure),
                "{voice:?}: a sailing boat carries an oscillator the luminous voice does not"
            );
            assert!(
                boat.graph.nodes.len() > pure.graph.nodes.len(),
                "{voice:?}: the sail wash is missing from under the luminous voice"
            );
        }
    }

    /// The voice follows the DRAWN craft (#1383): every boat seed, whatever
    /// type it picked, is drawn as a sailing sloop today and so sails - a
    /// Runabout-picked seed included, which is what keying the voice to the
    /// picked type would have got wrong.
    #[test]
    fn every_drawn_boat_sails_whatever_type_it_picked() {
        use crate::seeded_defaults::BoatType;
        let boats: Vec<u64> = (0u64..400)
            .filter(|&s| ChassisFamily::for_seed(s) == ChassisFamily::Boat)
            .collect();
        assert!(
            boats.iter().any(|&s| !BoatType::for_seed(s).implemented()),
            "no seed picked an unbuilt type - the test proves nothing"
        );
        for s in boats {
            assert_eq!(
                drive_of(ChassisFamily::Boat, s),
                Propulsion::Sail,
                "seed {s}"
            );
            let patch = voice_patch(AvatarVoice::Drive, ChassisFamily::Boat, s).unwrap();
            assert!(oscillators(&patch).is_empty(), "boat seed {s} hums");
        }
    }

    #[test]
    fn each_family_drive_bakes_to_real_sound() {
        for (family, drive) in [
            (ChassisFamily::Boat, Propulsion::Sail),
            (ChassisFamily::Boat, Propulsion::Engine),
            (ChassisFamily::Airship, Propulsion::Engine),
            (ChassisFamily::Skiff, Propulsion::Engine),
        ] {
            let patch = driven_voice_patch(AvatarVoice::Drive, family, drive, 3).expect("a drive");
            assert_audible(&patch, &format!("{family:?} {drive:?}"));
        }
    }

    #[test]
    fn luminous_vehicle_voices_bake_with_drive_underneath() {
        // A luminous *vehicle* carries its drive mixed in; a luminous
        // humanoid stays pure. Both must bake to sound, and the vehicle's
        // graph is strictly larger (the extra drive + mix nodes).
        for voice in [AvatarVoice::NeonBuzz, AvatarVoice::ArcaneShimmer] {
            let vehicle = voice_patch(voice, ChassisFamily::Skiff, 3).expect("vehicle voice");
            let humanoid = voice_patch(voice, ChassisFamily::Humanoid, 3).expect("humanoid voice");
            assert_audible(&vehicle, &format!("{voice:?} skiff"));
            assert_audible(&humanoid, &format!("{voice:?} humanoid"));
            assert!(
                vehicle.graph.nodes.len() > humanoid.graph.nodes.len(),
                "{voice:?}: vehicle should carry a drive under the luminous voice"
            );
        }
    }

    #[test]
    fn the_family_drives_are_distinct() {
        // The four drive voices - sail, motor boat, rotor, putter - are
        // genuinely different voices, not one shared hum.
        let baked: Vec<Vec<f32>> = [
            (ChassisFamily::Boat, Propulsion::Sail),
            (ChassisFamily::Boat, Propulsion::Engine),
            (ChassisFamily::Airship, Propulsion::Engine),
            (ChassisFamily::Skiff, Propulsion::Engine),
        ]
        .iter()
        .map(|&(family, drive)| {
            let patch = driven_voice_patch(AvatarVoice::Drive, family, drive, 3).unwrap();
            bake(&patch, 22_050, 0.3)
        })
        .collect();
        for i in 0..baked.len() {
            for j in i + 1..baked.len() {
                assert_ne!(baked[i], baked[j], "drives {i} and {j} are identical");
            }
        }
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

    /// Rounding to whole hertz (#1385) keeps the detune audible: the lowest
    /// tonal drive (the motor boat's 40 Hz) still spreads over three pitches
    /// across the buckets, and the skiff's 78 Hz over five.
    #[test]
    fn whole_hertz_keeps_the_detune_spread() {
        for (family, base, fewest) in [
            (ChassisFamily::Boat, 40.0, 3),
            (ChassisFamily::Airship, 52.0, 5),
            (ChassisFamily::Skiff, 78.0, 5),
        ] {
            let mut pitches: Vec<i32> = (0..DETUNE_BUCKETS)
                .map(|b| hz(base, detune_factor(b)) as i32)
                .collect();
            pitches.dedup();
            assert!(
                pitches.len() >= fewest,
                "{family:?}: {pitches:?} over {DETUNE_BUCKETS} buckets"
            );
        }
    }

    #[test]
    fn humanoid_luminous_voice_has_no_drive() {
        // Pure neon / arcane on a humanoid == the same voice with no vehicle
        // drive: the mix step is a no-op, so the node graph is drive-free.
        let human = voice_patch(AvatarVoice::NeonBuzz, ChassisFamily::Humanoid, 9).unwrap();
        // neon_buzz alone is 4 nodes (saw, bandpass, lfo, vca).
        assert_eq!(human.graph.nodes.len(), 4);
    }
}
