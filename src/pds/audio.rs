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
    #[serde(other)]
    Unknown,
}

impl SovereignAudioConfig {
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
    #[serde(other)]
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
    #[serde(other)]
    Unknown,
}

impl SovereignNodeKind {
    /// Human-readable variant name for editor pickers.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Silence => "Silence",
            Self::Sine(_) => "Sine",
            Self::Square(_) => "Square",
            Self::Sawtooth(_) => "Sawtooth",
            Self::Triangle(_) => "Triangle",
            Self::WhiteNoise(_) => "White noise",
            Self::PinkNoise(_) => "Pink noise",
            Self::BrownNoise(_) => "Brown noise",
            Self::Adsr(_) => "ADSR",
            Self::BiquadLowpass(_) => "Lowpass",
            Self::BiquadHighpass(_) => "Highpass",
            Self::BiquadBandpass(_) => "Bandpass",
            Self::Lfo(_) => "LFO",
            Self::Mix(_) => "Mix",
            Self::Gain(_) => "Gain (VCA)",
            Self::Gate(_) => "Gate",
            Self::Chorus(_) => "Chorus",
            Self::Reverb(_) => "Reverb",
            Self::Unknown => "Unknown",
        }
    }

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

/// Mirror of [`bevy_symbios_audio::SineOsc`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignSineOsc {
    pub freq_hz: Fp,
    pub phase_offset: Fp,
    #[serde(default = "default_amplitude")]
    pub amplitude: Fp,
}

impl Default for SovereignSineOsc {
    fn default() -> Self {
        Self {
            freq_hz: Fp(440.0),
            phase_offset: Fp(0.0),
            amplitude: Fp(1.0),
        }
    }
}

impl SovereignSineOsc {
    pub fn to_native(&self) -> bevy_symbios_audio::SineOsc {
        bevy_symbios_audio::SineOsc {
            freq_hz: self.freq_hz.0,
            phase_offset: self.phase_offset.0,
            amplitude: self.amplitude.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::SineOsc) -> Self {
        Self {
            freq_hz: Fp(n.freq_hz),
            phase_offset: Fp(n.phase_offset),
            amplitude: Fp(n.amplitude),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::SquareOsc`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignSquareOsc {
    pub freq_hz: Fp,
    pub duty: Fp,
    #[serde(default = "default_amplitude")]
    pub amplitude: Fp,
    /// Band-limiting mode. `#[serde(default)]` so records authored
    /// before this field existed decode to `Naive` — matching the audio
    /// crate's own back-compat default.
    #[serde(default)]
    pub anti_alias: SovereignAntiAlias,
}

impl Default for SovereignSquareOsc {
    fn default() -> Self {
        Self {
            freq_hz: Fp(440.0),
            duty: Fp(0.5),
            amplitude: Fp(1.0),
            anti_alias: SovereignAntiAlias::Naive,
        }
    }
}

impl SovereignSquareOsc {
    pub fn to_native(&self) -> bevy_symbios_audio::SquareOsc {
        bevy_symbios_audio::SquareOsc {
            freq_hz: self.freq_hz.0,
            duty: self.duty.0,
            amplitude: self.amplitude.0,
            anti_alias: self.anti_alias.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::SquareOsc) -> Self {
        Self {
            freq_hz: Fp(n.freq_hz),
            duty: Fp(n.duty),
            amplitude: Fp(n.amplitude),
            anti_alias: SovereignAntiAlias::from_native(n.anti_alias),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::SawtoothOsc`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignSawtoothOsc {
    pub freq_hz: Fp,
    pub polarity: SovereignSawPolarity,
    #[serde(default = "default_amplitude")]
    pub amplitude: Fp,
    /// Band-limiting mode. `#[serde(default)]` so pre-existing records
    /// decode to `Naive`.
    #[serde(default)]
    pub anti_alias: SovereignAntiAlias,
}

impl Default for SovereignSawtoothOsc {
    fn default() -> Self {
        Self {
            freq_hz: Fp(440.0),
            polarity: SovereignSawPolarity::Up,
            amplitude: Fp(1.0),
            anti_alias: SovereignAntiAlias::Naive,
        }
    }
}

impl SovereignSawtoothOsc {
    pub fn to_native(&self) -> bevy_symbios_audio::SawtoothOsc {
        bevy_symbios_audio::SawtoothOsc {
            freq_hz: self.freq_hz.0,
            polarity: self.polarity.to_native(),
            amplitude: self.amplitude.0,
            anti_alias: self.anti_alias.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::SawtoothOsc) -> Self {
        Self {
            freq_hz: Fp(n.freq_hz),
            polarity: SovereignSawPolarity::from_native(n.polarity),
            amplitude: Fp(n.amplitude),
            anti_alias: SovereignAntiAlias::from_native(n.anti_alias),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::SawPolarity`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignSawPolarity {
    #[default]
    Up,
    Down,
    #[serde(other)]
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
    #[serde(other)]
    Unknown,
}

impl SovereignAntiAlias {
    /// Human-readable label for editor pickers.
    pub fn label(self) -> &'static str {
        match self {
            Self::Naive => "Naive",
            Self::PolyBlep => "PolyBLEP",
            Self::Unknown => "Unknown",
        }
    }

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

/// Mirror of [`bevy_symbios_audio::TriangleOsc`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignTriangleOsc {
    pub freq_hz: Fp,
    #[serde(default = "default_amplitude")]
    pub amplitude: Fp,
    /// Band-limiting mode. `#[serde(default)]` so pre-existing records
    /// decode to `Naive`.
    #[serde(default)]
    pub anti_alias: SovereignAntiAlias,
}

impl Default for SovereignTriangleOsc {
    fn default() -> Self {
        Self {
            freq_hz: Fp(440.0),
            amplitude: Fp(1.0),
            anti_alias: SovereignAntiAlias::Naive,
        }
    }
}

impl SovereignTriangleOsc {
    pub fn to_native(&self) -> bevy_symbios_audio::TriangleOsc {
        bevy_symbios_audio::TriangleOsc {
            freq_hz: self.freq_hz.0,
            amplitude: self.amplitude.0,
            anti_alias: self.anti_alias.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::TriangleOsc) -> Self {
        Self {
            freq_hz: Fp(n.freq_hz),
            amplitude: Fp(n.amplitude),
            anti_alias: SovereignAntiAlias::from_native(n.anti_alias),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::WhiteNoise`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignWhiteNoise {
    pub amplitude: Fp,
}

impl Default for SovereignWhiteNoise {
    fn default() -> Self {
        Self { amplitude: Fp(0.5) }
    }
}

impl SovereignWhiteNoise {
    pub fn to_native(&self) -> bevy_symbios_audio::WhiteNoise {
        bevy_symbios_audio::WhiteNoise {
            amplitude: self.amplitude.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::WhiteNoise) -> Self {
        Self {
            amplitude: Fp(n.amplitude),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::PinkNoise`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignPinkNoise {
    pub amplitude: Fp,
}

impl Default for SovereignPinkNoise {
    fn default() -> Self {
        Self { amplitude: Fp(0.5) }
    }
}

impl SovereignPinkNoise {
    pub fn to_native(&self) -> bevy_symbios_audio::PinkNoise {
        bevy_symbios_audio::PinkNoise {
            amplitude: self.amplitude.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::PinkNoise) -> Self {
        Self {
            amplitude: Fp(n.amplitude),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::BrownNoise`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignBrownNoise {
    pub amplitude: Fp,
}

impl Default for SovereignBrownNoise {
    fn default() -> Self {
        Self { amplitude: Fp(0.5) }
    }
}

impl SovereignBrownNoise {
    pub fn to_native(&self) -> bevy_symbios_audio::BrownNoise {
        bevy_symbios_audio::BrownNoise {
            amplitude: self.amplitude.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::BrownNoise) -> Self {
        Self {
            amplitude: Fp(n.amplitude),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::AdsrEnvelope`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignAdsrEnvelope {
    pub attack_s: Fp,
    pub decay_s: Fp,
    pub sustain_level: Fp,
    pub release_s: Fp,
    pub curve: SovereignAdsrCurve,
}

impl Default for SovereignAdsrEnvelope {
    fn default() -> Self {
        Self {
            attack_s: Fp(0.01),
            decay_s: Fp(0.1),
            sustain_level: Fp(0.7),
            release_s: Fp(0.2),
            curve: SovereignAdsrCurve::Linear,
        }
    }
}

impl SovereignAdsrEnvelope {
    pub fn to_native(&self) -> bevy_symbios_audio::AdsrEnvelope {
        bevy_symbios_audio::AdsrEnvelope {
            attack_s: self.attack_s.0,
            decay_s: self.decay_s.0,
            sustain_level: self.sustain_level.0,
            release_s: self.release_s.0,
            curve: self.curve.to_native(),
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::AdsrEnvelope) -> Self {
        Self {
            attack_s: Fp(n.attack_s),
            decay_s: Fp(n.decay_s),
            sustain_level: Fp(n.sustain_level),
            release_s: Fp(n.release_s),
            curve: SovereignAdsrCurve::from_native(n.curve),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::AdsrCurve`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignAdsrCurve {
    #[default]
    Linear,
    Exponential,
    #[serde(other)]
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

/// Mirror of [`bevy_symbios_audio::BiquadLowpass`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignBiquadLowpass {
    pub cutoff_hz: Fp,
    pub q: Fp,
}

impl Default for SovereignBiquadLowpass {
    fn default() -> Self {
        Self {
            cutoff_hz: Fp(1_000.0),
            q: Fp(0.707),
        }
    }
}

impl SovereignBiquadLowpass {
    pub fn to_native(&self) -> bevy_symbios_audio::BiquadLowpass {
        bevy_symbios_audio::BiquadLowpass {
            cutoff_hz: self.cutoff_hz.0,
            q: self.q.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::BiquadLowpass) -> Self {
        Self {
            cutoff_hz: Fp(n.cutoff_hz),
            q: Fp(n.q),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::BiquadHighpass`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignBiquadHighpass {
    pub cutoff_hz: Fp,
    pub q: Fp,
}

impl Default for SovereignBiquadHighpass {
    fn default() -> Self {
        Self {
            cutoff_hz: Fp(1_000.0),
            q: Fp(0.707),
        }
    }
}

impl SovereignBiquadHighpass {
    pub fn to_native(&self) -> bevy_symbios_audio::BiquadHighpass {
        bevy_symbios_audio::BiquadHighpass {
            cutoff_hz: self.cutoff_hz.0,
            q: self.q.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::BiquadHighpass) -> Self {
        Self {
            cutoff_hz: Fp(n.cutoff_hz),
            q: Fp(n.q),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::BiquadBandpass`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignBiquadBandpass {
    pub center_hz: Fp,
    pub q: Fp,
}

impl Default for SovereignBiquadBandpass {
    fn default() -> Self {
        Self {
            center_hz: Fp(1_000.0),
            q: Fp(1.0),
        }
    }
}

impl SovereignBiquadBandpass {
    pub fn to_native(&self) -> bevy_symbios_audio::BiquadBandpass {
        bevy_symbios_audio::BiquadBandpass {
            center_hz: self.center_hz.0,
            q: self.q.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::BiquadBandpass) -> Self {
        Self {
            center_hz: Fp(n.center_hz),
            q: Fp(n.q),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::Lfo`].
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignLfo {
    pub rate_hz: Fp,
    pub shape: SovereignLfoShape,
    pub depth: Fp,
    pub offset: Fp,
}

impl Default for SovereignLfo {
    fn default() -> Self {
        Self {
            rate_hz: Fp(1.0),
            shape: SovereignLfoShape::Sine,
            depth: Fp(1.0),
            offset: Fp(0.0),
        }
    }
}

impl SovereignLfo {
    pub fn to_native(&self) -> bevy_symbios_audio::Lfo {
        bevy_symbios_audio::Lfo {
            rate_hz: self.rate_hz.0,
            shape: self.shape.to_native(),
            depth: self.depth.0,
            offset: self.offset.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Lfo) -> Self {
        Self {
            rate_hz: Fp(n.rate_hz),
            shape: SovereignLfoShape::from_native(n.shape),
            depth: Fp(n.depth),
            offset: Fp(n.offset),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::LfoShape`].
#[derive(Serialize, Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SovereignLfoShape {
    #[default]
    Sine,
    Triangle,
    Square,
    Saw,
    Random,
    #[serde(other)]
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

/// Mirror of [`bevy_symbios_audio::Mix`] — additive bus, sums all wired
/// input ports scaled by `gain`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignMix {
    #[serde(default = "default_gain")]
    pub gain: Fp,
}

impl Default for SovereignMix {
    fn default() -> Self {
        Self { gain: Fp(1.0) }
    }
}

impl SovereignMix {
    pub fn to_native(&self) -> bevy_symbios_audio::Mix {
        bevy_symbios_audio::Mix { gain: self.gain.0 }
    }

    pub fn from_native(n: &bevy_symbios_audio::Mix) -> Self {
        Self { gain: Fp(n.gain) }
    }
}

/// Mirror of [`bevy_symbios_audio::Gain`] — voltage-controlled
/// amplifier, `in * (gain + input("gain"))`.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignGain {
    #[serde(default = "default_gain")]
    pub gain: Fp,
}

impl Default for SovereignGain {
    fn default() -> Self {
        Self { gain: Fp(1.0) }
    }
}

impl SovereignGain {
    pub fn to_native(&self) -> bevy_symbios_audio::Gain {
        bevy_symbios_audio::Gain { gain: self.gain.0 }
    }

    pub fn from_native(n: &bevy_symbios_audio::Gain) -> Self {
        Self { gain: Fp(n.gain) }
    }
}

/// Mirror of [`bevy_symbios_audio::Gate`] — note-gate signal driven by
/// the sequencer's gate window. `invert` is a plain `bool` (no `Fp`).
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct SovereignGate {
    #[serde(default)]
    pub invert: bool,
}

impl SovereignGate {
    pub fn to_native(&self) -> bevy_symbios_audio::Gate {
        bevy_symbios_audio::Gate {
            invert: self.invert,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Gate) -> Self {
        Self { invert: n.invert }
    }
}

/// Mirror of [`bevy_symbios_audio::Chorus`] — internally-modulated
/// fractional-delay chorus effect.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignChorus {
    pub rate_hz: Fp,
    pub depth_ms: Fp,
    pub base_delay_ms: Fp,
    pub feedback: Fp,
    pub mix: Fp,
}

impl Default for SovereignChorus {
    fn default() -> Self {
        Self {
            rate_hz: Fp(0.8),
            depth_ms: Fp(2.0),
            base_delay_ms: Fp(8.0),
            feedback: Fp(0.0),
            mix: Fp(0.5),
        }
    }
}

impl SovereignChorus {
    pub fn to_native(&self) -> bevy_symbios_audio::Chorus {
        bevy_symbios_audio::Chorus {
            rate_hz: self.rate_hz.0,
            depth_ms: self.depth_ms.0,
            base_delay_ms: self.base_delay_ms.0,
            feedback: self.feedback.0,
            mix: self.mix.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Chorus) -> Self {
        Self {
            rate_hz: Fp(n.rate_hz),
            depth_ms: Fp(n.depth_ms),
            base_delay_ms: Fp(n.base_delay_ms),
            feedback: Fp(n.feedback),
            mix: Fp(n.mix),
        }
    }
}

/// Mirror of [`bevy_symbios_audio::Reverb`] — mono Freeverb
/// reverberator.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SovereignReverb {
    pub room_size: Fp,
    pub damping: Fp,
    pub mix: Fp,
}

impl Default for SovereignReverb {
    fn default() -> Self {
        Self {
            room_size: Fp(0.5),
            damping: Fp(0.5),
            mix: Fp(0.3),
        }
    }
}

impl SovereignReverb {
    pub fn to_native(&self) -> bevy_symbios_audio::Reverb {
        bevy_symbios_audio::Reverb {
            room_size: self.room_size.0,
            damping: self.damping.0,
            mix: self.mix.0,
        }
    }

    pub fn from_native(n: &bevy_symbios_audio::Reverb) -> Self {
        Self {
            room_size: Fp(n.room_size),
            damping: Fp(n.damping),
            mix: Fp(n.mix),
        }
    }
}

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
    #[serde(other)]
    Unknown,
}

impl SovereignPitchMode {
    /// Human-readable label for editor pickers.
    pub fn label(self) -> &'static str {
        match self {
            Self::Varispeed => "Varispeed",
            Self::TimePreserving => "Time-preserving",
            Self::Unknown => "Unknown",
        }
    }

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
