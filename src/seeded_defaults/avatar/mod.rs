//! Avatar-scope DID-seeded derivers.
//!
//! Mirrors [`super::room`] in shape: a shared seed-derived anchor feeds a set
//! of independent per-domain derivers. The anchor is [`AvatarCharacter`] (the
//! avatar analogue of [`super::scene::SceneCharacter`]): chassis +
//! [`ThemeArchetype`](super::scene::ThemeArchetype) style + continuous
//! ornateness / wear axes, all derived from the avatar owner's DID. An avatar
//! is independent of every room — a user's avatar reads the same regardless
//! of which room they visit.
//!
//! The data flow per avatar:
//!
//! ```text
//!   DID → AvatarCharacter (anchor: chassis + style + ornateness/wear)
//!     → palette    (skin/hair/eye + style/temperature/wear-aware accents)
//!     → materials  (MaterialKit: style + wear finish per surface role)
//!     → fx         (style-gated particle aura + spatial-audio voice)
//!     → outfit     (slot → part choice, querying the part catalogue)
//!     → body/gait  (proportions + locomotion tuning)
//! ```
//!
//! The top-level discrete pick is [`ChassisFamily`] (boat / airship /
//! humanoid / skiff). The actual silhouette is no longer a per-family design
//! deriver — it is *composed* from the tagged part catalogue
//! ([`crate::pds::avatar::parts`]): [`AvatarOutfit`] fills each chassis slot
//! by querying parts for the avatar's style + tiers, and the assembler
//! ([`crate::pds::avatar::default_visuals`]) builds + positions them.
//!
//! [`AvatarBody`] (proportions) and [`AvatarPalette`] (colours) are
//! family-agnostic and feed every part build; [`MaterialKit`] supplies the
//! style/wear finish.

pub mod body;
pub mod character;
pub mod chassis;
pub mod fx;
pub mod gait;
pub mod materials;
pub mod outfit;
pub mod palette;

pub use body::{AvatarBody, BodyArchetype};
pub use character::{AvatarCharacter, OrnatenessBand, OrnatenessTier, WearBand, WearTier};
pub use chassis::ChassisFamily;
pub use fx::{AvatarFx, AvatarVoice, ParticleAura};
pub use gait::AvatarGait;
pub use materials::MaterialKit;
pub use outfit::{AvatarOutfit, OutfitPart};
pub use palette::AvatarPalette;
