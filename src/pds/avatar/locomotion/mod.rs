//! Locomotion presets — the open-union physics-body half of the avatar
//! record.
//!
//! Each preset (HoverBoat / Humanoid / Airplane / Helicopter / Car) lives
//! in its own submodule with its parameter struct + `Default` + an impl of
//! [`LocomotionPreset`]. The central [`LocomotionConfig`] enum is the
//! tagged union the wire format speaks; the trait lets shared dispatch
//! (kind-tag lookup, sanitisation, picker construction) read each preset's
//! identity off its parameter struct without a hand-maintained `match`
//! ladder.
//!
//! Adding a new preset is therefore a localised change: drop a new file
//! under this module, define the parameter struct, impl
//! [`LocomotionPreset`], and add one variant to [`LocomotionConfig`] plus
//! one row in [`LocomotionConfig::pickers`].

pub mod airplane;
pub mod car;
pub mod helicopter;
pub mod hover_boat;
pub mod humanoid;

pub use airplane::AirplaneParams;
pub use car::CarParams;
pub use helicopter::HelicopterParams;
pub use hover_boat::HoverBoatParams;
pub use humanoid::HumanoidParams;

use super::super::types::{Fp, Fp3};
use serde::{Deserialize, Serialize};

/// One row in the locomotion-picker table — `(kind_tag, display_label,
/// default_constructor)`. The avatar editor uses this to render the
/// preset selector and to materialise a fresh default-tuned variant
/// when the user picks a new preset.
pub type LocomotionPickerEntry = (&'static str, &'static str, fn() -> LocomotionConfig);

/// Behaviour shared by every locomotion preset's parameter struct. The
/// constants carry the wire / UI identity that the central enum used to
/// hard-code in `match` arms; `sanitize` clamps every numeric field; the
/// `into_config`/`default_config` helpers wrap a struct value back into the
/// open-union [`LocomotionConfig`] without forcing every caller to spell
/// out the variant constructor.
pub trait LocomotionPreset: Default + Clone + Send + Sync + 'static {
    /// Stable string used by hot-swap detection in `player::mod` and as
    /// the picker's `kind` key. Mirrors [`LocomotionConfig::kind_tag`].
    const KIND_TAG: &'static str;
    /// Human-readable label for the locomotion picker UI.
    const DISPLAY_LABEL: &'static str;

    /// In-place numeric clamp. Mirrors the pre-refactor private
    /// `*Params::sanitize` methods; called from
    /// [`LocomotionConfig::sanitize`].
    fn sanitize(&mut self);

    /// Wrap an existing parameter struct into [`LocomotionConfig`].
    fn into_config(self) -> LocomotionConfig;

    /// Wrap a fresh default-tuned instance into [`LocomotionConfig`]. Used
    /// by [`LocomotionConfig::pickers`] so the picker table can list
    /// `Self::default_config` instead of an ad-hoc closure.
    fn default_config() -> LocomotionConfig {
        Self::default().into_config()
    }
}

/// Open-union locomotion preset. Each variant carries its own collider
/// dimensions + physics tuning so the chassis is fully self-describing —
/// the visuals tree is independent of the physics body.
///
/// Future presets add new `#[serde(rename)]` arms; older clients fall
/// through to `Unknown`, which the player module treats as "no
/// locomotion" and gives the entity a minimal placeholder collider so
/// the simulation does not explode.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "$type")]
pub enum LocomotionConfig {
    #[serde(rename = "network.symbios.locomotion.hover_boat")]
    HoverBoat(Box<HoverBoatParams>),

    #[serde(rename = "network.symbios.locomotion.humanoid")]
    Humanoid(Box<HumanoidParams>),

    #[serde(rename = "network.symbios.locomotion.airplane")]
    Airplane(Box<AirplaneParams>),

    #[serde(rename = "network.symbios.locomotion.helicopter")]
    Helicopter(Box<HelicopterParams>),

    #[serde(rename = "network.symbios.locomotion.car")]
    Car(Box<CarParams>),

    #[serde(other)]
    Unknown,
}

impl LocomotionConfig {
    /// Stable string tag used by hot-swap detection so a variant change
    /// (HoverBoat → Humanoid) can be seen without a full `==` compare.
    pub fn kind_tag(&self) -> &'static str {
        match self {
            LocomotionConfig::HoverBoat(_) => HoverBoatParams::KIND_TAG,
            LocomotionConfig::Humanoid(_) => HumanoidParams::KIND_TAG,
            LocomotionConfig::Airplane(_) => AirplaneParams::KIND_TAG,
            LocomotionConfig::Helicopter(_) => HelicopterParams::KIND_TAG,
            LocomotionConfig::Car(_) => CarParams::KIND_TAG,
            LocomotionConfig::Unknown => "unknown",
        }
    }

    /// Human-readable label for the locomotion picker UI.
    pub fn display_label(&self) -> &'static str {
        match self {
            LocomotionConfig::HoverBoat(_) => HoverBoatParams::DISPLAY_LABEL,
            LocomotionConfig::Humanoid(_) => HumanoidParams::DISPLAY_LABEL,
            LocomotionConfig::Airplane(_) => AirplaneParams::DISPLAY_LABEL,
            LocomotionConfig::Helicopter(_) => HelicopterParams::DISPLAY_LABEL,
            LocomotionConfig::Car(_) => CarParams::DISPLAY_LABEL,
            LocomotionConfig::Unknown => "Unknown",
        }
    }

    /// Ordered list of preset constructors used by the locomotion picker
    /// to enumerate every selectable preset. Each entry returns a fresh
    /// default-tuned variant.
    pub fn pickers() -> &'static [LocomotionPickerEntry] {
        &[
            (
                HoverBoatParams::KIND_TAG,
                HoverBoatParams::DISPLAY_LABEL,
                HoverBoatParams::default_config,
            ),
            (
                HumanoidParams::KIND_TAG,
                HumanoidParams::DISPLAY_LABEL,
                HumanoidParams::default_config,
            ),
            (
                AirplaneParams::KIND_TAG,
                AirplaneParams::DISPLAY_LABEL,
                AirplaneParams::default_config,
            ),
            (
                HelicopterParams::KIND_TAG,
                HelicopterParams::DISPLAY_LABEL,
                HelicopterParams::default_config,
            ),
            (
                CarParams::KIND_TAG,
                CarParams::DISPLAY_LABEL,
                CarParams::default_config,
            ),
        ]
    }

    /// In-place sanitisation. Delegates to the per-variant
    /// [`LocomotionPreset::sanitize`] impl; `Unknown` is left as-is.
    pub fn sanitize(&mut self) {
        match self {
            LocomotionConfig::HoverBoat(p) => p.sanitize(),
            LocomotionConfig::Humanoid(p) => p.sanitize(),
            LocomotionConfig::Airplane(p) => p.sanitize(),
            LocomotionConfig::Helicopter(p) => p.sanitize(),
            LocomotionConfig::Car(p) => p.sanitize(),
            LocomotionConfig::Unknown => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Sanitiser primitives — shared by every preset's `LocomotionPreset` impl.
// ---------------------------------------------------------------------------

/// Clamp a finite scalar into `[lo, hi]`; non-finite inputs collapse to
/// `lo` (matches the pre-refactor private helper).
pub(super) fn clamp_pos(v: Fp, lo: f32, hi: f32) -> Fp {
    let x = v.0;
    Fp(if x.is_finite() { x.clamp(lo, hi) } else { lo })
}

/// `clamp_pos` shorthand for unit-range fields (`[0.0, 1.0]`).
pub(super) fn clamp_unit(v: Fp) -> Fp {
    clamp_pos(v, 0.0, 1.0)
}

/// Clamp every component of an `Fp3` half-extents triple; non-finite axes
/// collapse to `0.5`.
pub(super) fn clamp_half_extents(e: &mut Fp3) {
    let mut a = e.0;
    for c in a.iter_mut() {
        *c = if c.is_finite() {
            c.clamp(0.05, 50.0)
        } else {
            0.5
        };
    }
    *e = Fp3(a);
}
