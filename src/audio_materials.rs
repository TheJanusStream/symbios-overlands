//! Hand-authored impact-sound recipes keyed off the texture variant
//! a surface carries.
//!
//! [`impact_recipe_for`] takes the [`SovereignTextureConfig`] of an
//! authored splat layer or construct material and returns a recipe whose
//! [`AudioPatch`] is tuned for that material's perceptual character —
//! rock is a bright sharp transient, grass a soft muffled thud, metal
//! a high-frequency ring, and so on.
//!
//! # Why authored, not seeded
//!
//! Impact sounds need consistency across rooms: hitting rock should
//! always sound like rock regardless of which DID owns the world. The
//! room seed varies the *ambient* track (see
//! [`crate::seeded_defaults::room::audio`]); impact patches are part
//! of the world-builder's invariant asset bank.
//!
//! # Patch shape
//!
//! Every impact uses the same four-node topology — a noise source, an
//! ADSR envelope, a biquad low-pass, and a final gain (VCA). The single
//! ADSR does double duty: it sweeps the filter cutoff (so the transient
//! opens bright then darkens) *and* drives the output gain (so the tail
//! also decays in level, not just in brightness — a cleaner one-shot
//! than the filter sweep alone):
//!
//! ```text
//! noise --in--> biquad LP --in--> gain (VCA) --> patch output
//!                 ^ cutoff_hz        ^ gain
//!                 |                  |
//!   adsr (gate=1) +------------------+
//!     attack ~5 ms, sustain 0, decay material-dependent
//!     · cutoff_hz: base 20 Hz, modulation amount = peak_cutoff_hz
//!     · gain:      base 0.0,   driven 0 → 1 by the envelope
//! ```
//!
//! With ADSR sustain at 0 and gate held high, the envelope ramps up to
//! 1.0 (filter fully open at `peak_cutoff_hz`, VCA at unity), decays to
//! 0 (filter back to its 20 Hz base and the VCA to silence), and stays
//! at 0. Bake-side `duration_secs` = `attack + decay + tail`.
//!
//! [`AudioPatch`]: bevy_symbios_audio::AudioPatch
//! [`SovereignTextureConfig`]: crate::pds::SovereignTextureConfig

use std::collections::BTreeMap;

use bevy_symbios_audio::{
    AdsrCurve, AdsrEnvelope, AudioPatch, BiquadLowpass, BrownNoise, Connection, Gain, GraphNode,
    Instrument, NodeGraph, NodeId, NodeKind, PinkNoise, SequenceRecipe, Track, WhiteNoise,
};

use crate::pds::SovereignTextureConfig;

const NOISE_ID: NodeId = NodeId(0);
const ADSR_ID: NodeId = NodeId(1);
const FILTER_ID: NodeId = NodeId(2);
const GAIN_ID: NodeId = NodeId(3);

/// Material-class identifier — the perceptual bucket a texture maps to.
/// One impact recipe per class; multiple texture variants can share a
/// class (e.g. `Pavers` / `Cobblestone` / `Ashlar` all sound like rock).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImpactMaterial {
    /// Bright sharp transient — exposed bedrock, dressed stone, paving.
    Rock,
    /// Muffled organic thud — packed soil, turf, dirt.
    Ground,
    /// Wood-like thud — planks, bark, shingles, panelling.
    Wood,
    /// Solid heavy thud — brick, concrete, marble, asphalt.
    Stone,
    /// Bright high-frequency ring — sheet metal, grilles, corrugated.
    Metal,
    /// Almost-silent rustle — leaves, thatch, twig piles.
    Soft,
    /// Catch-all for None / Referenced / Unknown / non-impactable
    /// decorative textures.
    Generic,
}

/// Tunable knobs that distinguish one impact material from another.
#[derive(Debug, Clone, Copy)]
struct ImpactParams {
    /// Noise colour — white for sharp transients, pink for balanced,
    /// brown for low-rumble.
    noise: NoiseKind,
    /// Noise amplitude before the filter. Bounded well below unity so
    /// the master mix never clips after the event volume compounds.
    noise_amplitude: f32,
    /// Peak filter cutoff (Hz) when the ADSR envelope is at 1.0.
    /// Higher = brighter; lower = more muffled.
    peak_cutoff_hz: f32,
    /// Filter resonance — bumps tonal character around the cutoff.
    /// Low for broad transients, slightly higher for ringing materials.
    filter_q: f32,
    /// ADSR attack time (s). Sub-millisecond for clicky impacts;
    /// ~10 ms for softer onsets.
    attack_s: f32,
    /// ADSR decay time (s). The dominant perceptual length of the
    /// impact — short for crisp materials, long for soft / lossy ones.
    decay_s: f32,
}

#[derive(Debug, Clone, Copy)]
enum NoiseKind {
    White,
    Pink,
    Brown,
}

impl ImpactMaterial {
    /// Tunable parameters for this material class. Hand-tuned; tweak
    /// here to retune every variant that maps to this class.
    fn params(self) -> ImpactParams {
        match self {
            ImpactMaterial::Rock => ImpactParams {
                noise: NoiseKind::White,
                noise_amplitude: 0.7,
                peak_cutoff_hz: 8_000.0,
                filter_q: 0.7,
                attack_s: 0.002,
                decay_s: 0.15,
            },
            ImpactMaterial::Ground => ImpactParams {
                noise: NoiseKind::Brown,
                noise_amplitude: 0.8,
                peak_cutoff_hz: 2_000.0,
                filter_q: 0.6,
                attack_s: 0.005,
                decay_s: 0.20,
            },
            ImpactMaterial::Wood => ImpactParams {
                noise: NoiseKind::Pink,
                noise_amplitude: 0.7,
                peak_cutoff_hz: 3_500.0,
                filter_q: 1.0,
                attack_s: 0.003,
                decay_s: 0.18,
            },
            ImpactMaterial::Stone => ImpactParams {
                noise: NoiseKind::Pink,
                noise_amplitude: 0.7,
                peak_cutoff_hz: 4_500.0,
                filter_q: 0.8,
                attack_s: 0.002,
                decay_s: 0.22,
            },
            ImpactMaterial::Metal => ImpactParams {
                noise: NoiseKind::White,
                noise_amplitude: 0.6,
                peak_cutoff_hz: 9_500.0,
                filter_q: 2.5,
                attack_s: 0.001,
                decay_s: 0.30,
            },
            ImpactMaterial::Soft => ImpactParams {
                noise: NoiseKind::Pink,
                noise_amplitude: 0.4,
                peak_cutoff_hz: 1_200.0,
                filter_q: 0.5,
                attack_s: 0.010,
                decay_s: 0.10,
            },
            ImpactMaterial::Generic => ImpactParams {
                noise: NoiseKind::Pink,
                noise_amplitude: 0.6,
                peak_cutoff_hz: 3_000.0,
                filter_q: 0.7,
                attack_s: 0.004,
                decay_s: 0.18,
            },
        }
    }

    /// Total bake duration (seconds) covering the full envelope plus a
    /// short tail. Sized exactly so the bake buffer ends at silence
    /// rather than mid-decay.
    pub fn duration_secs(self) -> f32 {
        let p = self.params();
        // Small safety pad past attack+decay so the very last sample
        // is reliably below the audible floor.
        p.attack_s + p.decay_s + 0.02
    }
}

/// Map a texture variant to the impact-material class it sounds like.
/// New variants in [`SovereignTextureConfig`] should be added here as
/// they're introduced; the compiler will catch any missing arm.
fn classify(texture: &SovereignTextureConfig) -> ImpactMaterial {
    match texture {
        SovereignTextureConfig::Rock(_) => ImpactMaterial::Rock,
        SovereignTextureConfig::Pavers(_)
        | SovereignTextureConfig::Cobblestone(_)
        | SovereignTextureConfig::Ashlar(_) => ImpactMaterial::Rock,
        SovereignTextureConfig::Ground(_) | SovereignTextureConfig::Sand(_) => {
            ImpactMaterial::Ground
        }
        SovereignTextureConfig::Plank(_)
        | SovereignTextureConfig::Bark(_)
        | SovereignTextureConfig::Twig(_)
        | SovereignTextureConfig::Shingle(_)
        | SovereignTextureConfig::Wainscoting(_)
        // A cut-log end is a wooden surface.
        | SovereignTextureConfig::LogEnd(_)
        // A cactus stem is a firm, fibrous succulent — closest to wood.
        | SovereignTextureConfig::CactusSkin(_) => ImpactMaterial::Wood,
        SovereignTextureConfig::Brick(_)
        | SovereignTextureConfig::Concrete(_)
        | SovereignTextureConfig::Stucco(_)
        | SovereignTextureConfig::Marble(_)
        | SovereignTextureConfig::Asphalt(_)
        | SovereignTextureConfig::Encaustic(_)
        // Frozen / molten rock crust both read as hard stone underfoot.
        | SovereignTextureConfig::Ice(_)
        | SovereignTextureConfig::Lava(_) => ImpactMaterial::Stone,
        SovereignTextureConfig::Metal(_)
        | SovereignTextureConfig::Corrugated(_)
        | SovereignTextureConfig::IronGrille(_)
        // A chain-link fence is woven steel wire.
        | SovereignTextureConfig::ChainLink(_) => ImpactMaterial::Metal,
        // Foliage / petal sprite cards sound like plant matter, same as the
        // leaf surface card.
        SovereignTextureConfig::Leaf(_)
        | SovereignTextureConfig::Thatch(_)
        | SovereignTextureConfig::LeafSprite(_)
        | SovereignTextureConfig::Petal(_)
        | SovereignTextureConfig::Flower(_)
        | SovereignTextureConfig::GrassTuft(_)
        | SovereignTextureConfig::Frond(_)
        // Reeds are plant matter; moss and lichen are a soft organic mat over
        // whatever they encrust.
        | SovereignTextureConfig::Reed(_)
        | SovereignTextureConfig::Needle(_)
        | SovereignTextureConfig::Broadleaf(_)
        | SovereignTextureConfig::Moss(_)
        | SovereignTextureConfig::Lichen(_)
        // Cloth and powder snow both give a muffled, soft footfall.
        | SovereignTextureConfig::Fabric(_)
        | SovereignTextureConfig::Snow(_) => ImpactMaterial::Soft,
        // Delicate / non-impactable decorative variants fall to the
        // generic thud — you don't normally walk on a window pane, but
        // the impact trigger may still fire on edge cases (clipping,
        // construct collisions) and silent is worse than a thud. The
        // intangible particle sprites (glows, sparks, flames, rings) have
        // no real impact sound, so they thud generically too.
        SovereignTextureConfig::Window(_)
        | SovereignTextureConfig::StainedGlass(_)
        | SovereignTextureConfig::SoftDisc(_)
        | SovereignTextureConfig::Spark(_)
        | SovereignTextureConfig::Snowflake(_)
        | SovereignTextureConfig::Puff(_)
        | SovereignTextureConfig::Ring(_)
        | SovereignTextureConfig::Shard(_)
        | SovereignTextureConfig::Flame(_) => ImpactMaterial::Generic,
        SovereignTextureConfig::None
        | SovereignTextureConfig::Referenced { .. }
        | SovereignTextureConfig::Unknown => ImpactMaterial::Generic,
    }
}

/// Resolve a texture variant + collision intensity to a one-event
/// [`SequenceRecipe`]. Sized at exactly one bake of the material's
/// natural duration so it fits the mixdown-baker's gate semantics
/// cleanly; volume scales by the caller-supplied intensity in `[0, 1]`.
///
/// This is the helper the impact-trigger system (#300) calls:
/// `play_terrain_impacts` (below) resolves the dominant splat layer at
/// each terrain contact, builds a recipe here, and feeds it to
/// `dispatch_one_shot_audio` for a cached bake and spatial one-shot
/// playback.
pub fn impact_recipe_for(texture: &SovereignTextureConfig, volume: f32) -> SequenceRecipe {
    let material = classify(texture);
    let patch = build_impact_patch(material.params());
    let duration_secs = material.duration_secs();
    // 60 BPM means one beat = one second, so duration_beats =
    // duration_secs is exact.
    let duration_beats = duration_secs;
    SequenceRecipe {
        bpm: 60.0,
        sample_rate: 44_100,
        duration_beats,
        loop_start_beats: None,
        loop_crossfade_beats: 0.0,
        instruments: vec![Instrument {
            id: "impact".to_string(),
            patch,
        }],
        tracks: vec![Track {
            events: vec![bevy_symbios_audio::Event {
                time_beats: 0.0,
                instrument_id: "impact".to_string(),
                pitch_multiplier: 1.0,
                volume: volume.clamp(0.0, 1.0),
                gate_beats: duration_beats,
                // Gate spans the whole timeline and the ADSR release is
                // ~1 ms, so no extra tail is needed past the gate close.
                release_beats: 0.0,
                // One-shot impact: keep the default resample behaviour.
                pitch_mode: bevy_symbios_audio::PitchMode::Varispeed,
            }],
        }],
    }
}

fn build_impact_patch(params: ImpactParams) -> AudioPatch {
    let noise_kind = match params.noise {
        NoiseKind::White => NodeKind::WhiteNoise(WhiteNoise {
            amplitude: params.noise_amplitude,
        }),
        NoiseKind::Pink => NodeKind::PinkNoise(PinkNoise {
            amplitude: params.noise_amplitude,
        }),
        NoiseKind::Brown => NodeKind::BrownNoise(BrownNoise {
            amplitude: params.noise_amplitude,
        }),
    };
    let noise_node = GraphNode {
        id: NOISE_ID,
        kind: noise_kind,
        inputs: BTreeMap::new(),
    };

    // ADSR gate is permanently asserted; with sustain=0 the envelope
    // shape is purely attack + decay before going (and staying) silent.
    let mut adsr_inputs = BTreeMap::new();
    adsr_inputs.insert(
        "gate".to_string(),
        vec![Connection::Constant { value: 1.0 }],
    );
    let adsr_node = GraphNode {
        id: ADSR_ID,
        kind: NodeKind::Adsr(AdsrEnvelope {
            attack_s: params.attack_s,
            decay_s: params.decay_s,
            sustain_level: 0.0,
            release_s: 0.001,
            curve: AdsrCurve::Exponential,
        }),
        inputs: adsr_inputs,
    };

    // Filter base cutoff at 20 Hz keeps the audible band suppressed
    // until the ADSR drives it up. ADSR output is [0, 1]; the
    // modulation `amount` scales that into Hz of cutoff sweep.
    let mut filter_inputs = BTreeMap::new();
    filter_inputs.insert("in".to_string(), vec![Connection::from_node(NOISE_ID)]);
    filter_inputs.insert(
        "cutoff_hz".to_string(),
        vec![Connection::modulation(ADSR_ID, params.peak_cutoff_hz)],
    );
    let filter_node = GraphNode {
        id: FILTER_ID,
        kind: NodeKind::BiquadLowpass(BiquadLowpass {
            cutoff_hz: 20.0,
            q: params.filter_q,
        }),
        inputs: filter_inputs,
    };

    // VCA: the same ADSR that sweeps the cutoff also shapes the output
    // amplitude. With base gain 0.0 the envelope on the `gain` port is
    // the whole amplitude contour, so the impact's tail decays in level
    // (not just in brightness) — a cleaner one-shot than relying on the
    // filter sweep alone. ADSR output is [0, 1], a clean VCA control.
    let mut gain_inputs = BTreeMap::new();
    gain_inputs.insert("in".to_string(), vec![Connection::from_node(FILTER_ID)]);
    gain_inputs.insert("gain".to_string(), vec![Connection::from_node(ADSR_ID)]);
    let gain_node = GraphNode {
        id: GAIN_ID,
        kind: NodeKind::Gain(Gain { gain: 0.0 }),
        inputs: gain_inputs,
    };

    AudioPatch {
        // Impact patches are authored, not seeded — a fixed seed gives
        // the same bit-identical impact every collision. The noise
        // colour distinguishes materials; bake-to-bake variation can be
        // achieved by the call site mutating the seed if jitter is
        // wanted later.
        seed: 0,
        graph: NodeGraph {
            nodes: vec![noise_node, adsr_node, filter_node, gain_node],
            output: GAIN_ID,
        },
    }
}

// ---------------------------------------------------------------------------
// Terrain-impact trigger system (#300)
// ---------------------------------------------------------------------------

/// Per-avatar cooldown clock so footsteps don't stack into a muddy
/// continuous noise burst. Inserted by the
/// [`InteractionPlugin`](crate::interaction::InteractionPlugin); read +
/// updated by [`play_terrain_impacts`].
#[derive(bevy::prelude::Resource, Default)]
pub struct ImpactCooldowns {
    last_play_secs: std::collections::HashMap<bevy::prelude::Entity, f64>,
}

/// Minimum seconds between two consecutive impact plays on the same
/// avatar. Below this, a fresh impact is dropped — keeps a stutter-step
/// or terrain glitch from queuing a wall of overlapping voices.
const IMPACT_COOLDOWN_SECS: f64 = 0.18;

/// Volume floor — below this scaled value an impact is dropped
/// entirely (saves the cost of a bake whose result will be inaudible).
const IMPACT_VOLUME_FLOOR: f32 = 0.05;

/// Read this frame's [`AvatarContacts`](crate::interaction::AvatarContacts)
/// and play a procedural terrain impact for every `Enter` sample. The
/// played sound's material is looked up from the dominant splat-layer at
/// the contact point via the room's
/// [`SovereignTerrainConfig::material`](crate::pds::SovereignTerrainConfig::material)
/// layers.
///
/// Scheduling: `.after(ContactProducerSet)` so it runs in the same
/// frame the classifier emits the `Enter` sample. Inert when no room
/// record / heightmap / terrain config is loaded yet.
pub fn play_terrain_impacts(
    mut commands: bevy::prelude::Commands,
    contacts: bevy::prelude::Res<crate::interaction::AvatarContacts>,
    room_record: Option<bevy::prelude::Res<crate::state::LiveRoomRecord>>,
    time: bevy::prelude::Res<bevy::prelude::Time>,
    mut cooldowns: bevy::prelude::ResMut<ImpactCooldowns>,
    mut bake_cache: bevy::prelude::ResMut<crate::world_builder::spatial_audio::BakedAudioCache>,
) {
    use crate::interaction::{ContactPhase, SurfaceContact, SurfaceKind};

    let Some(room) = room_record else {
        return;
    };
    let Some(terrain) = crate::pds::find_terrain_config(&room.0) else {
        return;
    };
    let now = time.elapsed_secs_f64();

    for sample in contacts.iter_kind(SurfaceKind::Terrain) {
        if sample.phase != ContactPhase::Enter {
            continue;
        }
        // Per-avatar cooldown — the same avatar can't trigger two
        // impacts within the cooldown window. Different avatars get
        // independent budgets so a crowded room still feels alive.
        if let Some(&last) = cooldowns.last_play_secs.get(&sample.avatar)
            && now - last < IMPACT_COOLDOWN_SECS
        {
            continue;
        }

        let SurfaceContact::Terrain { material_blend, .. } = sample.surface else {
            // SurfaceKind::Terrain filter should guarantee this branch
            // is unreachable, but the compiler can't see through the
            // filter — explicit unreachable! would be cleaner if the
            // unreachable-arm lint flags this in a future Rust.
            continue;
        };

        // Dominant material layer — argmax over the four splat weights.
        let dominant = crate::interaction::contact::dominant_layer(material_blend);
        let texture = &terrain.material.layers[dominant];

        // Volume scales with the sample's normalised intensity. Below
        // the floor we skip the bake entirely.
        let volume = sample.intensity.clamp(0.0, 1.0);
        if volume < IMPACT_VOLUME_FLOOR {
            continue;
        }

        // Bake at unit volume and scale at playback (the one-shot's
        // `PlaybackSettings` carries `Volume::Linear(volume)`): keeping
        // the recipe volume-independent makes the serialised config —
        // the bake-cache key — identical across impacts, so each
        // material bakes once per session instead of once per footstep.
        let recipe = impact_recipe_for(texture, 1.0);
        let audio = crate::pds::SovereignAudioConfig::from_sequence(&recipe);
        crate::world_builder::spatial_audio::dispatch_one_shot_audio(
            &mut commands,
            &mut bake_cache,
            sample.world_pos,
            &audio,
            volume,
        );
        cooldowns.last_play_secs.insert(sample.avatar, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_symbios_audio::bake;

    /// Helper: assert that a baked buffer is non-silent (sum of |sample|
    /// over the attack window exceeds a sensible floor).
    fn assert_audible(buffer: &[f32], context: &str) {
        let energy: f32 = buffer.iter().map(|s| s.abs()).sum();
        assert!(
            energy > 1.0,
            "{context}: buffer should contain audible energy; got total |sum| = {energy}, len = {}",
            buffer.len()
        );
    }

    #[test]
    fn every_classified_material_bakes_to_audible_output() {
        for (label, texture) in [
            ("Rock", SovereignTextureConfig::Rock(Default::default())),
            ("Ground", SovereignTextureConfig::Ground(Default::default())),
            ("Plank", SovereignTextureConfig::Plank(Default::default())),
            ("Brick", SovereignTextureConfig::Brick(Default::default())),
            ("Metal", SovereignTextureConfig::Metal(Default::default())),
            ("Leaf", SovereignTextureConfig::Leaf(Default::default())),
            ("Thatch", SovereignTextureConfig::Thatch(Default::default())),
        ] {
            let material = classify(&texture);
            let patch = build_impact_patch(material.params());
            let samples = bake(&patch, 44_100, material.duration_secs());
            assert_audible(&samples, label);
        }
    }

    #[test]
    fn generic_fallback_for_non_impactable_variants() {
        // None, Referenced, Unknown all map to Generic — the patch
        // must still be bakeable so a fire-and-forget impact trigger
        // never panics on a forward-compat variant.
        for texture in [
            SovereignTextureConfig::None,
            SovereignTextureConfig::Referenced {
                source: crate::pds::SovereignAssetReference::default(),
            },
            SovereignTextureConfig::Unknown,
            SovereignTextureConfig::Window(Default::default()),
        ] {
            let material = classify(&texture);
            let patch = build_impact_patch(material.params());
            assert_eq!(material, ImpactMaterial::Generic);
            let samples = bake(&patch, 44_100, material.duration_secs());
            assert!(
                !samples.is_empty(),
                "generic fallback bake must produce samples"
            );
        }
    }

    #[test]
    fn material_classes_have_distinct_decay_times() {
        // Metal rings longer than Rock; Soft is shorter; Ground is
        // medium. Pinning these prevents an accidental retune from
        // collapsing every material to the same length.
        let rock = ImpactMaterial::Rock.duration_secs();
        let metal = ImpactMaterial::Metal.duration_secs();
        let soft = ImpactMaterial::Soft.duration_secs();
        assert!(
            metal > rock,
            "metal should ring longer than rock; got metal={metal}, rock={rock}"
        );
        assert!(
            soft < rock,
            "soft should decay faster than rock; got soft={soft}, rock={rock}"
        );
    }

    #[test]
    fn impact_recipe_carries_event_with_clamped_volume() {
        let texture = SovereignTextureConfig::Rock(Default::default());
        let recipe = impact_recipe_for(&texture, 2.5); // Out-of-range
        assert_eq!(recipe.tracks.len(), 1);
        assert_eq!(recipe.tracks[0].events.len(), 1);
        let event = &recipe.tracks[0].events[0];
        assert_eq!(event.volume, 1.0, "volume must be clamped to [0,1]");
        assert_eq!(event.instrument_id, "impact");
    }

    #[test]
    fn impact_recipe_volume_zero_silences_event() {
        let texture = SovereignTextureConfig::Rock(Default::default());
        let recipe = impact_recipe_for(&texture, -0.5);
        assert_eq!(
            recipe.tracks[0].events[0].volume, 0.0,
            "negative volume must clamp to 0"
        );
    }
}
