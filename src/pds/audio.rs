//! Sovereign (DAG-CBOR-safe) mirrors of the [`bevy_symbios_audio`]
//! crate's authoring types. Every `f32` field is wrapped in [`Fp`] so
//! the wire stream carries fixed-point integers — DAG-CBOR forbids
//! floats and the PDS would reject any record carrying them otherwise.
//!
//! # Type hierarchy
//!
//! - [`SovereignAudioConfig`] is the top-level enum users drop into a
//!   slot. Variants: `None` / `Referenced{source}` /
//!   `Patch{patch}` / `Sequence{recipe}` / `Unknown`.
//! - [`SovereignAudioPatch`] mirrors `bevy_symbios_audio::AudioPatch`.
//! - [`SovereignNodeGraph`] mirrors `NodeGraph` — the DAG topology.
//! - [`SovereignGraphNode`] mirrors `GraphNode` — one node placed in
//!   the graph.
//! - [`SovereignNodeKind`] mirrors the closed `NodeKind` enum, with a
//!   forward-compat `Unknown` arm that maps to `Silence` on
//!   `to_native`.
//! - [`SovereignConnection`] mirrors `Connection` (constant / wired
//!   output).
//! - [`SovereignSequenceRecipe`] mirrors `SequenceRecipe`,
//!   [`SovereignInstrument`] mirrors `Instrument`, [`SovereignTrack`]
//!   mirrors `Track`, [`SovereignEvent`] mirrors `Event`.
//!
//! # Conversion
//!
//! Every Sovereign type carries `to_native` (returns the
//! `bevy_symbios_audio` equivalent) and `from_native` (builds the
//! sovereign mirror from a native value). The round-trip is loss-free
//! modulo `Fp` quantisation (each float quantises to its nearest
//! `FP_SCALE` tick — ~0.0001 precision, well below audio-rate
//! perceptual thresholds for any field these types carry).
//!
//! [`Fp`]: super::types::Fp
//! [`bevy_symbios_audio`]: bevy_symbios_audio

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::asset_reference::SovereignAssetReference;
use super::serde_util::define_sovereign_mirror;
use super::types::Fp;

// ===========================================================================
// Top-level config
// ===========================================================================

/// Open-union describing where audio data for a slot comes from.
/// Mirrors the structural shape of
/// [`crate::pds::SovereignTextureConfig`] so the editor bridges behave
/// identically across asset classes.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "$type")]
pub enum SovereignAudioConfig {
    /// No audio for this slot.
    #[default]
    None,
    /// External asset pointer — fetched bytes are decoded by the
    /// audio resolver into a `Handle<AudioSource>`.
    Referenced { source: SovereignAssetReference },
    /// Procedural single-voice patch — full structured mirror of
    /// [`bevy_symbios_audio::AudioPatch`].
    Patch { patch: SovereignAudioPatch },
    /// Procedural multi-voice mixdown — full structured mirror of
    /// [`bevy_symbios_audio::SequenceRecipe`].
    Sequence { recipe: SovereignSequenceRecipe },
    /// Forward-compat seam — a record from a future engine version
    /// decodes here rather than failing the whole load.
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignAudioConfig {
    /// `true` for the silent [`Self::None`] slot — the wire-format skip
    /// predicate for generator `audio` fields (#695).
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    /// Human-readable variant name for UI combo boxes.
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Referenced { .. } => "Referenced",
            Self::Patch { .. } => "Patch",
            Self::Sequence { .. } => "Sequence",
            Self::Unknown => "Unknown",
        }
    }

    /// Build a `Patch` variant from a native
    /// [`bevy_symbios_audio::AudioPatch`]. Conversion is infallible —
    /// the structural walk wraps every float in [`Fp`] without losing
    /// data outside `FP_SCALE` quantisation.
    pub fn from_patch(patch: &bevy_symbios_audio::AudioPatch) -> Self {
        SovereignAudioConfig::Patch {
            patch: SovereignAudioPatch::from_native(patch),
        }
    }

    /// Build a `Sequence` variant from a native
    /// [`bevy_symbios_audio::SequenceRecipe`].
    pub fn from_sequence(recipe: &bevy_symbios_audio::SequenceRecipe) -> Self {
        SovereignAudioConfig::Sequence {
            recipe: SovereignSequenceRecipe::from_native(recipe),
        }
    }

    /// If this is a `Patch` variant, convert it back to the native
    /// [`bevy_symbios_audio::AudioPatch`]. Returns `None` for every
    /// other variant.
    pub fn parse_patch(&self) -> Option<bevy_symbios_audio::AudioPatch> {
        match self {
            SovereignAudioConfig::Patch { patch } => Some(patch.to_native()),
            _ => None,
        }
    }

    /// If this is a `Sequence` variant, convert it back to the native
    /// [`bevy_symbios_audio::SequenceRecipe`].
    pub fn parse_sequence(&self) -> Option<bevy_symbios_audio::SequenceRecipe> {
        match self {
            SovereignAudioConfig::Sequence { recipe } => Some(recipe.to_native()),
            _ => None,
        }
    }
}

// ===========================================================================
// AudioPatch + graph topology
// ===========================================================================

/// Mirror of [`bevy_symbios_audio::AudioPatch`].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SovereignAudioPatch {
    pub seed: u32,
    pub graph: SovereignNodeGraph,
}

impl SovereignAudioPatch {
    pub fn to_native(&self) -> bevy_symbios_audio::AudioPatch {
        bevy_symbios_audio::AudioPatch {
            seed: self.seed,
            graph: self.graph.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::AudioPatch) -> Self {
        Self {
            seed: n.seed,
            graph: SovereignNodeGraph::from_native(&n.graph),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::NodeGraph`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignNodeGraph {
    pub nodes: Vec<SovereignGraphNode>,
    pub output: SovereignNodeId,
}

impl Default for SovereignNodeGraph {
    fn default() -> Self {
        // Match the native default — one Silence node at NodeId(0).
        Self {
            nodes: vec![SovereignGraphNode::default()],
            output: SovereignNodeId::default(),
        }
    }
}

impl SovereignNodeGraph {
    pub fn to_native(&self) -> bevy_symbios_audio::NodeGraph {
        bevy_symbios_audio::NodeGraph {
            nodes: self
                .nodes
                .iter()
                .map(SovereignGraphNode::to_native)
                .collect(),
            output: self.output.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::NodeGraph) -> Self {
        Self {
            nodes: n
                .nodes
                .iter()
                .map(SovereignGraphNode::from_native)
                .collect(),
            output: SovereignNodeId::from_native(n.output),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::GraphNode`].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SovereignGraphNode {
    pub id: SovereignNodeId,
    pub kind: SovereignNodeKind,
    /// Wired inputs, keyed by port name. Each port holds a *list* of
    /// connections whose resolved values are summed at bake time, so
    /// several sources can feed one port (signal mixing, modulation
    /// stacking) — mirrors `GraphNode::inputs` after the audio crate's
    /// single-`Connection` → `Vec<Connection>` change.
    #[serde(default)]
    pub inputs: BTreeMap<String, Vec<SovereignConnection>>,
}

impl SovereignGraphNode {
    pub fn to_native(&self) -> bevy_symbios_audio::GraphNode {
        bevy_symbios_audio::GraphNode {
            id: self.id.to_native(),
            kind: self.kind.to_native(),
            inputs: self
                .inputs
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.iter().map(SovereignConnection::to_native).collect(),
                    )
                })
                .collect(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::GraphNode) -> Self {
        Self {
            id: SovereignNodeId::from_native(n.id),
            kind: SovereignNodeKind::from_native(&n.kind),
            inputs: n
                .inputs
                .iter()
                .map(|(k, v)| {
                    (
                        k.clone(),
                        v.iter().map(SovereignConnection::from_native).collect(),
                    )
                })
                .collect(),
        }
    }
}

/// Transparent newtype mirroring [`bevy_symbios_audio::NodeId`].
#[derive(
    Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord,
)]
#[serde(transparent)]
pub struct SovereignNodeId(pub u32);

impl SovereignNodeId {
    pub fn to_native(self) -> bevy_symbios_audio::NodeId {
        bevy_symbios_audio::NodeId(self.0)
    }

    pub fn from_native(n: bevy_symbios_audio::NodeId) -> Self {
        Self(n.0)
    }
}

/// Mirror of [`bevy_symbios_audio::Connection`] with [`Fp`] floats.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum SovereignConnection {
    Constant {
        value: Fp,
    },
    Node {
        id: SovereignNodeId,
        #[serde(default = "default_connection_amount")]
        amount: Fp,
    },
    /// Forward-compat — a future Connection variant decodes here.
    /// Mapped to `Constant { value: 0.0 }` (silent) on `to_native`.
    #[serde(other, skip_serializing)]
    Unknown,
}

fn default_connection_amount() -> Fp {
    Fp(1.0)
}

impl Default for SovereignConnection {
    fn default() -> Self {
        Self::Constant { value: Fp(0.0) }
    }
}

impl SovereignConnection {
    pub fn to_native(&self) -> bevy_symbios_audio::Connection {
        match self {
            Self::Constant { value } => bevy_symbios_audio::Connection::Constant { value: value.0 },
            Self::Node { id, amount } => bevy_symbios_audio::Connection::Node {
                id: id.to_native(),
                amount: amount.0,
            },
            Self::Unknown => bevy_symbios_audio::Connection::Constant { value: 0.0 },
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Connection) -> Self {
        match n {
            bevy_symbios_audio::Connection::Constant { value } => {
                Self::Constant { value: Fp(*value) }
            }
            bevy_symbios_audio::Connection::Node { id, amount } => Self::Node {
                id: SovereignNodeId::from_native(*id),
                amount: Fp(*amount),
            },
        }
    }
}

// ===========================================================================
// NodeKind (closed enum mirror)
// ===========================================================================

/// Mirror of [`bevy_symbios_audio::NodeKind`]. `Unknown` is the
/// forward-compat seam — a future variant added in a newer audio
/// crate version decodes here and maps to `Silence` on `to_native`
/// (mute fallback).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(tag = "kind")]
pub enum SovereignNodeKind {
    #[default]
    Silence,
    Sine(SovereignSineOsc),
    Square(SovereignSquareOsc),
    Sawtooth(SovereignSawtoothOsc),
    Triangle(SovereignTriangleOsc),
    WhiteNoise(SovereignWhiteNoise),
    PinkNoise(SovereignPinkNoise),
    BrownNoise(SovereignBrownNoise),
    Adsr(SovereignAdsrEnvelope),
    BiquadLowpass(SovereignBiquadLowpass),
    BiquadHighpass(SovereignBiquadHighpass),
    BiquadBandpass(SovereignBiquadBandpass),
    Lfo(SovereignLfo),
    Mix(SovereignMix),
    Gain(SovereignGain),
    Gate(SovereignGate),
    Chorus(SovereignChorus),
    Reverb(SovereignReverb),
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignNodeKind {
    pub fn to_native(&self) -> bevy_symbios_audio::NodeKind {
        use bevy_symbios_audio::NodeKind as N;
        match self {
            Self::Silence | Self::Unknown => N::Silence,
            Self::Sine(c) => N::Sine(c.to_native()),
            Self::Square(c) => N::Square(c.to_native()),
            Self::Sawtooth(c) => N::Sawtooth(c.to_native()),
            Self::Triangle(c) => N::Triangle(c.to_native()),
            Self::WhiteNoise(c) => N::WhiteNoise(c.to_native()),
            Self::PinkNoise(c) => N::PinkNoise(c.to_native()),
            Self::BrownNoise(c) => N::BrownNoise(c.to_native()),
            Self::Adsr(c) => N::Adsr(c.to_native()),
            Self::BiquadLowpass(c) => N::BiquadLowpass(c.to_native()),
            Self::BiquadHighpass(c) => N::BiquadHighpass(c.to_native()),
            Self::BiquadBandpass(c) => N::BiquadBandpass(c.to_native()),
            Self::Lfo(c) => N::Lfo(c.to_native()),
            Self::Mix(c) => N::Mix(c.to_native()),
            Self::Gain(c) => N::Gain(c.to_native()),
            Self::Gate(c) => N::Gate(c.to_native()),
            Self::Chorus(c) => N::Chorus(c.to_native()),
            Self::Reverb(c) => N::Reverb(c.to_native()),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::NodeKind) -> Self {
        use bevy_symbios_audio::NodeKind as N;
        match n {
            N::Silence => Self::Silence,
            N::Sine(c) => Self::Sine(SovereignSineOsc::from_native(c)),
            N::Square(c) => Self::Square(SovereignSquareOsc::from_native(c)),
            N::Sawtooth(c) => Self::Sawtooth(SovereignSawtoothOsc::from_native(c)),
            N::Triangle(c) => Self::Triangle(SovereignTriangleOsc::from_native(c)),
            N::WhiteNoise(c) => Self::WhiteNoise(SovereignWhiteNoise::from_native(c)),
            N::PinkNoise(c) => Self::PinkNoise(SovereignPinkNoise::from_native(c)),
            N::BrownNoise(c) => Self::BrownNoise(SovereignBrownNoise::from_native(c)),
            N::Adsr(c) => Self::Adsr(SovereignAdsrEnvelope::from_native(c)),
            N::BiquadLowpass(c) => Self::BiquadLowpass(SovereignBiquadLowpass::from_native(c)),
            N::BiquadHighpass(c) => Self::BiquadHighpass(SovereignBiquadHighpass::from_native(c)),
            N::BiquadBandpass(c) => Self::BiquadBandpass(SovereignBiquadBandpass::from_native(c)),
            N::Lfo(c) => Self::Lfo(SovereignLfo::from_native(c)),
            N::Mix(c) => Self::Mix(SovereignMix::from_native(c)),
            N::Gain(c) => Self::Gain(SovereignGain::from_native(c)),
            N::Gate(c) => Self::Gate(SovereignGate::from_native(c)),
            N::Chorus(c) => Self::Chorus(SovereignChorus::from_native(c)),
            N::Reverb(c) => Self::Reverb(SovereignReverb::from_native(c)),
            // NodeKind is `#[non_exhaustive]` — a future variant added
            // in the audio crate is decoded as Unknown by mirror clients
            // that don't yet know it.
            _ => Self::Unknown,
        }
    }
}

// ===========================================================================
// Node configs
// ===========================================================================

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::SineOsc`].
    SovereignSineOsc => bevy_symbios_audio::SineOsc {
        fp: freq_hz = 440.0,
        fp: phase_offset = 0.0,
        #[serde(default = "default_amplitude")]
        fp: amplitude = 1.0,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::SquareOsc`].
    SovereignSquareOsc => bevy_symbios_audio::SquareOsc {
        fp: freq_hz = 440.0,
        fp: duty = 0.5,
        #[serde(default = "default_amplitude")]
        fp: amplitude = 1.0,
        /// Band-limiting mode. `#[serde(default)]` so records authored
        /// before this field existed decode to `Naive` — matching the audio
        /// crate's own back-compat default.
        #[serde(default)]
        mirror(SovereignAntiAlias): anti_alias = SovereignAntiAlias::Naive,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::SawtoothOsc`].
    SovereignSawtoothOsc => bevy_symbios_audio::SawtoothOsc {
        fp: freq_hz = 440.0,
        mirror(SovereignSawPolarity): polarity = SovereignSawPolarity::Up,
        #[serde(default = "default_amplitude")]
        fp: amplitude = 1.0,
        /// Band-limiting mode. `#[serde(default)]` so pre-existing records
        /// decode to `Naive`.
        #[serde(default)]
        mirror(SovereignAntiAlias): anti_alias = SovereignAntiAlias::Naive,
});

/// Mirror of [`bevy_symbios_audio::SawPolarity`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignSawPolarity {
    #[default]
    Up,
    Down,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignSawPolarity {
    pub fn to_native(self) -> bevy_symbios_audio::SawPolarity {
        match self {
            // Unknown -> Up matches the audio crate's Default impl.
            Self::Up | Self::Unknown => bevy_symbios_audio::SawPolarity::Up,
            Self::Down => bevy_symbios_audio::SawPolarity::Down,
        }
    }

    pub fn from_native(n: bevy_symbios_audio::SawPolarity) -> Self {
        match n {
            bevy_symbios_audio::SawPolarity::Up => Self::Up,
            bevy_symbios_audio::SawPolarity::Down => Self::Down,
        }
    }
}

/// Mirror of [`bevy_symbios_audio::AntiAlias`] — band-limiting mode for
/// the discontinuous oscillators (square / saw / triangle).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignAntiAlias {
    /// Raw generator — aliased, the historical default.
    #[default]
    Naive,
    /// PolyBLEP / polyBLAMP band-limited generator.
    PolyBlep,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignAntiAlias {
    pub fn to_native(self) -> bevy_symbios_audio::AntiAlias {
        match self {
            // Unknown -> Naive matches the audio crate's Default impl.
            Self::Naive | Self::Unknown => bevy_symbios_audio::AntiAlias::Naive,
            Self::PolyBlep => bevy_symbios_audio::AntiAlias::PolyBlep,
        }
    }

    pub fn from_native(n: bevy_symbios_audio::AntiAlias) -> Self {
        match n {
            bevy_symbios_audio::AntiAlias::Naive => Self::Naive,
            bevy_symbios_audio::AntiAlias::PolyBlep => Self::PolyBlep,
        }
    }
}

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::TriangleOsc`].
    SovereignTriangleOsc => bevy_symbios_audio::TriangleOsc {
        fp: freq_hz = 440.0,
        #[serde(default = "default_amplitude")]
        fp: amplitude = 1.0,
        /// Band-limiting mode. `#[serde(default)]` so pre-existing records
        /// decode to `Naive`.
        #[serde(default)]
        mirror(SovereignAntiAlias): anti_alias = SovereignAntiAlias::Naive,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::WhiteNoise`].
    SovereignWhiteNoise => bevy_symbios_audio::WhiteNoise {
        fp: amplitude = 0.5,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::PinkNoise`].
    SovereignPinkNoise => bevy_symbios_audio::PinkNoise {
        fp: amplitude = 0.5,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::BrownNoise`].
    SovereignBrownNoise => bevy_symbios_audio::BrownNoise {
        fp: amplitude = 0.5,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::AdsrEnvelope`].
    SovereignAdsrEnvelope => bevy_symbios_audio::AdsrEnvelope {
        fp: attack_s = 0.01,
        fp: decay_s = 0.1,
        fp: sustain_level = 0.7,
        fp: release_s = 0.2,
        mirror(SovereignAdsrCurve): curve = SovereignAdsrCurve::Linear,
});

/// Mirror of [`bevy_symbios_audio::AdsrCurve`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignAdsrCurve {
    #[default]
    Linear,
    Exponential,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignAdsrCurve {
    pub fn to_native(self) -> bevy_symbios_audio::AdsrCurve {
        match self {
            Self::Linear | Self::Unknown => bevy_symbios_audio::AdsrCurve::Linear,
            Self::Exponential => bevy_symbios_audio::AdsrCurve::Exponential,
        }
    }

    pub fn from_native(n: bevy_symbios_audio::AdsrCurve) -> Self {
        match n {
            bevy_symbios_audio::AdsrCurve::Linear => Self::Linear,
            bevy_symbios_audio::AdsrCurve::Exponential => Self::Exponential,
        }
    }
}

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::BiquadLowpass`].
    SovereignBiquadLowpass => bevy_symbios_audio::BiquadLowpass {
        fp: cutoff_hz = 1_000.0,
        /// Butterworth, as upstream. A hand-typed `0.707` sat one wire
        /// tick below it (7070, not 7071) until #1160's parity test.
        fp: q = std::f32::consts::FRAC_1_SQRT_2,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::BiquadHighpass`].
    SovereignBiquadHighpass => bevy_symbios_audio::BiquadHighpass {
        fp: cutoff_hz = 1_000.0,
        /// Butterworth, as upstream (see the lowpass note).
        fp: q = std::f32::consts::FRAC_1_SQRT_2,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::BiquadBandpass`].
    SovereignBiquadBandpass => bevy_symbios_audio::BiquadBandpass {
        fp: center_hz = 1_000.0,
        fp: q = 1.0,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::Lfo`].
    SovereignLfo => bevy_symbios_audio::Lfo {
        fp: rate_hz = 1.0,
        mirror(SovereignLfoShape): shape = SovereignLfoShape::Sine,
        fp: depth = 1.0,
        fp: offset = 0.0,
});

/// Mirror of [`bevy_symbios_audio::LfoShape`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignLfoShape {
    #[default]
    Sine,
    Triangle,
    Square,
    Saw,
    Random,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignLfoShape {
    pub fn to_native(self) -> bevy_symbios_audio::LfoShape {
        match self {
            Self::Sine | Self::Unknown => bevy_symbios_audio::LfoShape::Sine,
            Self::Triangle => bevy_symbios_audio::LfoShape::Triangle,
            Self::Square => bevy_symbios_audio::LfoShape::Square,
            Self::Saw => bevy_symbios_audio::LfoShape::Saw,
            Self::Random => bevy_symbios_audio::LfoShape::Random,
        }
    }

    pub fn from_native(n: bevy_symbios_audio::LfoShape) -> Self {
        match n {
            bevy_symbios_audio::LfoShape::Sine => Self::Sine,
            bevy_symbios_audio::LfoShape::Triangle => Self::Triangle,
            bevy_symbios_audio::LfoShape::Square => Self::Square,
            bevy_symbios_audio::LfoShape::Saw => Self::Saw,
            bevy_symbios_audio::LfoShape::Random => Self::Random,
        }
    }
}

fn default_amplitude() -> Fp {
    Fp(1.0)
}

/// Default gain (`1.0`) for [`SovereignMix`] / [`SovereignGain`] —
/// unity pass-through.
fn default_gain() -> Fp {
    Fp(1.0)
}

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::Mix`] — additive bus, sums all wired
    /// input ports scaled by `gain`.
    SovereignMix => bevy_symbios_audio::Mix {
        #[serde(default = "default_gain")]
        fp: gain = 1.0,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::Gain`] — voltage-controlled
    /// amplifier, `in * (gain + input("gain"))`.
    SovereignGain => bevy_symbios_audio::Gain {
        #[serde(default = "default_gain")]
        fp: gain = 1.0,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::Gate`] — note-gate signal driven by
    /// the sequencer's gate window. `invert` is a plain `bool` (no `Fp`).
    SovereignGate => bevy_symbios_audio::Gate {
        #[serde(default)]
        bool: invert = false,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::Chorus`] — internally-modulated
    /// fractional-delay chorus effect.
    SovereignChorus => bevy_symbios_audio::Chorus {
        fp: rate_hz = 0.8,
        fp: depth_ms = 2.0,
        fp: base_delay_ms = 8.0,
        fp: feedback = 0.0,
        fp: mix = 0.5,
});

define_sovereign_mirror!(verbatim
    /// Mirror of [`bevy_symbios_audio::Reverb`] — mono Freeverb
    /// reverberator.
    SovereignReverb => bevy_symbios_audio::Reverb {
        fp: room_size = 0.5,
        fp: damping = 0.5,
        fp: mix = 0.3,
});

// ===========================================================================
// SequenceRecipe + Instrument + Track + Event
// ===========================================================================

/// Mirror of [`bevy_symbios_audio::SequenceRecipe`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignSequenceRecipe {
    pub bpm: Fp,
    pub sample_rate: u32,
    pub duration_beats: Fp,
    /// `None` = play once, no loop.
    #[serde(default)]
    pub loop_start_beats: Option<Fp>,
    pub loop_crossfade_beats: Fp,
    pub instruments: Vec<SovereignInstrument>,
    pub tracks: Vec<SovereignTrack>,
}

impl Default for SovereignSequenceRecipe {
    fn default() -> Self {
        Self {
            bpm: Fp(120.0),
            sample_rate: 44_100,
            duration_beats: Fp(4.0),
            loop_start_beats: None,
            loop_crossfade_beats: Fp(0.0),
            instruments: Vec::new(),
            tracks: Vec::new(),
        }
    }
}

impl SovereignSequenceRecipe {
    pub fn to_native(&self) -> bevy_symbios_audio::SequenceRecipe {
        bevy_symbios_audio::SequenceRecipe {
            bpm: self.bpm.0,
            sample_rate: self.sample_rate,
            duration_beats: self.duration_beats.0,
            loop_start_beats: self.loop_start_beats.map(|fp| fp.0),
            loop_crossfade_beats: self.loop_crossfade_beats.0,
            instruments: self
                .instruments
                .iter()
                .map(SovereignInstrument::to_native)
                .collect(),
            tracks: self.tracks.iter().map(SovereignTrack::to_native).collect(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::SequenceRecipe) -> Self {
        Self {
            bpm: Fp(n.bpm),
            sample_rate: n.sample_rate,
            duration_beats: Fp(n.duration_beats),
            loop_start_beats: n.loop_start_beats.map(Fp),
            loop_crossfade_beats: Fp(n.loop_crossfade_beats),
            instruments: n
                .instruments
                .iter()
                .map(SovereignInstrument::from_native)
                .collect(),
            tracks: n.tracks.iter().map(SovereignTrack::from_native).collect(),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::Instrument`].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SovereignInstrument {
    pub id: String,
    pub patch: SovereignAudioPatch,
}

impl SovereignInstrument {
    pub fn to_native(&self) -> bevy_symbios_audio::Instrument {
        bevy_symbios_audio::Instrument {
            id: self.id.clone(),
            patch: self.patch.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Instrument) -> Self {
        Self {
            id: n.id.clone(),
            patch: SovereignAudioPatch::from_native(&n.patch),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::Track`].
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SovereignTrack {
    pub events: Vec<SovereignEvent>,
}

impl SovereignTrack {
    pub fn to_native(&self) -> bevy_symbios_audio::Track {
        bevy_symbios_audio::Track {
            events: self.events.iter().map(SovereignEvent::to_native).collect(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Track) -> Self {
        Self {
            events: n.events.iter().map(SovereignEvent::from_native).collect(),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::Event`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignEvent {
    pub time_beats: Fp,
    pub instrument_id: String,
    pub pitch_multiplier: Fp,
    pub volume: Fp,
    pub gate_beats: Fp,
    /// Extra tail baked *after* the gate closes, in beats — enough for
    /// the envelope's release to ring out. `0.0` cuts the note the
    /// instant the gate closes (a hard one-shot). `#[serde(default)]`
    /// so records authored before this field existed decode as `0.0`,
    /// matching the audio crate's own back-compat default.
    #[serde(default)]
    pub release_beats: Fp,
    /// How `pitch_multiplier` is realised — resample (`Varispeed`,
    /// default) or synthesis-time retune (`TimePreserving`).
    /// `#[serde(default)]` keeps pre-existing recipes on the historical
    /// resample path.
    #[serde(default)]
    pub pitch_mode: SovereignPitchMode,
}

impl Default for SovereignEvent {
    fn default() -> Self {
        Self {
            time_beats: Fp(0.0),
            instrument_id: String::new(),
            pitch_multiplier: Fp(1.0),
            volume: Fp(1.0),
            gate_beats: Fp(1.0),
            release_beats: Fp(0.0),
            pitch_mode: SovereignPitchMode::Varispeed,
        }
    }
}

impl SovereignEvent {
    pub fn to_native(&self) -> bevy_symbios_audio::Event {
        bevy_symbios_audio::Event {
            time_beats: self.time_beats.0,
            instrument_id: self.instrument_id.clone(),
            pitch_multiplier: self.pitch_multiplier.0,
            volume: self.volume.0,
            gate_beats: self.gate_beats.0,
            release_beats: self.release_beats.0,
            pitch_mode: self.pitch_mode.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Event) -> Self {
        Self {
            time_beats: Fp(n.time_beats),
            instrument_id: n.instrument_id.clone(),
            pitch_multiplier: Fp(n.pitch_multiplier),
            volume: Fp(n.volume),
            gate_beats: Fp(n.gate_beats),
            release_beats: Fp(n.release_beats),
            pitch_mode: SovereignPitchMode::from_native(n.pitch_mode),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::PitchMode`] — how an event's
/// `pitch_multiplier` is realised at mixdown time.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignPitchMode {
    /// Resample the native bake — pitch and duration coupled (the
    /// historical default).
    #[default]
    Varispeed,
    /// Retune oscillators at synthesis time — pitch and duration
    /// independent.
    TimePreserving,
    #[serde(other, skip_serializing)]
    Unknown,
}

impl SovereignPitchMode {
    pub fn to_native(self) -> bevy_symbios_audio::PitchMode {
        match self {
            // Unknown -> Varispeed matches the audio crate's Default.
            Self::Varispeed | Self::Unknown => bevy_symbios_audio::PitchMode::Varispeed,
            Self::TimePreserving => bevy_symbios_audio::PitchMode::TimePreserving,
        }
    }

    pub fn from_native(n: bevy_symbios_audio::PitchMode) -> Self {
        match n {
            bevy_symbios_audio::PitchMode::Varispeed => Self::Varispeed,
            bevy_symbios_audio::PitchMode::TimePreserving => Self::TimePreserving,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Parity guards for the audio mirror (#1160, finding 94 of #1109) —
    //! the audio half of what `pds::texture`'s
    //! `mirror_defaults_match_upstream` has had since the texture mirror
    //! was macro-generated.
    use super::*;
    use bevy_symbios_audio as native;

    /// A mirror's declared default must be what mirroring the upstream
    /// default yields.
    ///
    /// This is the one way a hand-written mirror can be wrong that the
    /// compiler never sees: `to_native` names every field, so a missing
    /// one is a build error, but a mistyped default constant compiles
    /// fine — and then decides what an *unset* field sounds like, on
    /// every peer, without any record changing (`amplitude`, `anti_alias`,
    /// `invert`, `release_beats` and `pitch_mode` all default on decode).
    ///
    /// Compared as the *wire form* of `Sov::default()` against the wire
    /// form of `Sov::from_native(&Native::default())`, so both sides sit
    /// on the [`Fp`] grid and the test asks the question that matters:
    /// does a node the editor creates from the mirror's default write the
    /// same bytes as one mirrored from upstream's? (It did not, for the
    /// two biquads: a hand-typed `0.707` is tick 7070, `FRAC_1_SQRT_2` is
    /// 7071.)
    #[test]
    fn mirror_defaults_match_upstream() {
        macro_rules! assert_default_matches {
            ($sov:ty, $native:ty) => {
                let declared = serde_json::to_value(<$sov>::default()).expect("serialises");
                let mirrored = serde_json::to_value(<$sov>::from_native(&<$native>::default()))
                    .expect("serialises");
                assert_eq!(
                    declared,
                    mirrored,
                    "mirror default drifted from upstream for {}",
                    stringify!($native)
                );
            };
        }
        assert_default_matches!(SovereignSineOsc, native::SineOsc);
        assert_default_matches!(SovereignSquareOsc, native::SquareOsc);
        assert_default_matches!(SovereignSawtoothOsc, native::SawtoothOsc);
        assert_default_matches!(SovereignTriangleOsc, native::TriangleOsc);
        assert_default_matches!(SovereignWhiteNoise, native::WhiteNoise);
        assert_default_matches!(SovereignPinkNoise, native::PinkNoise);
        assert_default_matches!(SovereignBrownNoise, native::BrownNoise);
        assert_default_matches!(SovereignAdsrEnvelope, native::AdsrEnvelope);
        assert_default_matches!(SovereignBiquadLowpass, native::BiquadLowpass);
        assert_default_matches!(SovereignBiquadHighpass, native::BiquadHighpass);
        assert_default_matches!(SovereignBiquadBandpass, native::BiquadBandpass);
        assert_default_matches!(SovereignLfo, native::Lfo);
        assert_default_matches!(SovereignMix, native::Mix);
        assert_default_matches!(SovereignGain, native::Gain);
        assert_default_matches!(SovereignGate, native::Gate);
        assert_default_matches!(SovereignChorus, native::Chorus);
        assert_default_matches!(SovereignReverb, native::Reverb);
        // The graph and sequence containers, and the by-value enums.
        assert_default_matches!(SovereignAudioPatch, native::AudioPatch);
        assert_default_matches!(SovereignNodeGraph, native::NodeGraph);
        assert_default_matches!(SovereignGraphNode, native::GraphNode);
        assert_default_matches!(SovereignConnection, native::Connection);
        assert_default_matches!(SovereignNodeKind, native::NodeKind);
        assert_default_matches!(SovereignSequenceRecipe, native::SequenceRecipe);
        assert_default_matches!(SovereignInstrument, native::Instrument);
        assert_default_matches!(SovereignTrack, native::Track);
        assert_default_matches!(SovereignEvent, native::Event);
        assert_eq!(
            SovereignSawPolarity::default(),
            SovereignSawPolarity::from_native(native::SawPolarity::default())
        );
        assert_eq!(
            SovereignAntiAlias::default(),
            SovereignAntiAlias::from_native(native::AntiAlias::default())
        );
        assert_eq!(
            SovereignAdsrCurve::default(),
            SovereignAdsrCurve::from_native(native::AdsrCurve::default())
        );
        assert_eq!(
            SovereignLfoShape::default(),
            SovereignLfoShape::from_native(native::LfoShape::default())
        );
        assert_eq!(
            SovereignPitchMode::default(),
            SovereignPitchMode::from_native(native::PitchMode::default())
        );
    }

    /// Every node kind the audio crate ships has a mirror arm, and the arm
    /// round-trips: `from_native` must never land a known kind on
    /// `Unknown`, because `Unknown` plays as silence (#1170 finding 105 —
    /// the `_ => Unknown` fallback in `from_native` is what a future
    /// upstream node falls into, and it does so without a sound).
    ///
    /// `NodeKind` is `#[non_exhaustive]`, so this cannot be a compile-time
    /// property and the roster below is the one hand-written list left:
    /// it is the eighteen kinds of `bevy_symbios_audio` 0.3 and belongs on
    /// the dependency-bump checklist alongside the avian canary.
    #[test]
    fn every_native_node_kind_has_a_mirror_arm() {
        use native::NodeKind as N;
        let roster = [
            N::Silence,
            N::Sine(Default::default()),
            N::Square(Default::default()),
            N::Sawtooth(Default::default()),
            N::Triangle(Default::default()),
            N::WhiteNoise(Default::default()),
            N::PinkNoise(Default::default()),
            N::BrownNoise(Default::default()),
            N::Adsr(Default::default()),
            N::BiquadLowpass(Default::default()),
            N::BiquadHighpass(Default::default()),
            N::BiquadBandpass(Default::default()),
            N::Lfo(Default::default()),
            N::Mix(Default::default()),
            N::Gain(Default::default()),
            N::Gate(Default::default()),
            N::Chorus(Default::default()),
            N::Reverb(Default::default()),
        ];
        let mut seen = std::collections::HashSet::new();
        for kind in &roster {
            let mirror = SovereignNodeKind::from_native(kind);
            assert_ne!(
                mirror,
                SovereignNodeKind::Unknown,
                "{kind:?} has no mirror arm — it would decode as silence"
            );
            assert_eq!(
                &mirror.to_native(),
                kind,
                "{kind:?} does not survive the mirror round trip"
            );
            assert!(
                seen.insert(std::mem::discriminant(&mirror)),
                "{kind:?} shares a mirror arm with another kind"
            );
        }
        // One mirror arm per native kind, plus `Unknown` itself.
        assert_eq!(seen.len(), roster.len());
    }
}
