//! Avatar-scope DID-seeded derivers.
//!
//! Mirrors [`super::room`] in shape: a shared seed-derived anchor feeds a set
//! of independent per-domain derivers. The anchor is [`AvatarCharacter`] (the
//! avatar analogue of [`super::scene::SceneCharacter`]): chassis +
//! [`ThemeArchetype`](super::scene::ThemeArchetype) style + continuous
//! ornateness / wear axes, all derived from the avatar owner's DID. An avatar
//! is independent of every room - a user's avatar reads the same regardless
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
//!     → vehicle_blueprint (the vehicle counterpart: hull / cabin / envelope
//!                  proportions from a seeded stance register)
//! ```
//!
//! The top-level discrete pick is [`ChassisFamily`] (boat / airship /
//! humanoid / skiff), and inside the two part-assembled vehicle families a
//! second discrete pick follows it: the [`CraftType`] (sloop / longship /
//! tug / junk / runabout / scow, roadster / buggy / armoured car / cyclecar /
//! wagon / rover), weighted by the style through the shared mood taxonomy in
//! [`mood`] so a theme arrives on the craft it would actually build (#1362).
//! Since #1369 every one of those twelve types is BUILT, on both sides: the
//! rover closed the skiff half at #1378 and the longship closes the boat
//! half. The actual silhouette is no
//! longer a per-family design
//! deriver - it is *composed* from the tagged part catalogue
//! ([`crate::pds::avatar::parts`]): [`AvatarOutfit`] fills each chassis slot
//! by querying parts for the avatar's style + tiers, and the assembler
//! ([`crate::pds::avatar::default_visuals`]) builds + positions them.
//!
//! [`AvatarBody`] (proportions) and [`AvatarPalette`] (colours) are
//! family-agnostic and feed every part build; [`MaterialKit`] supplies the
//! style/wear finish.

pub mod armoured;
pub mod body;
pub mod buggy;
pub mod character;
pub mod chassis;
pub mod craft;
pub mod fx;
pub mod gait;
pub mod longship;
pub mod materials;
pub mod mood;
pub mod outfit;
pub mod palette;
pub mod roadster;
pub mod rover;
pub mod runabout;
pub mod scow;
pub mod sloop;
pub mod tug;
pub mod vehicle_blueprint;
pub mod wagon;

pub use armoured::ArmouredVariant;
pub use body::{AvatarBody, BodyArchetype, StylizationTier};
pub use buggy::BuggyVariant;
pub use character::{
    AvatarCharacter, AvatarPins, FinishRegister, OrnatenessBand, OrnatenessTier, WearBand, WearTier,
};
pub use chassis::ChassisFamily;
pub use craft::{BoatType, CraftType, SkiffType};
pub use fx::{AvatarFx, AvatarVoice, ParticleAura};
pub use gait::AvatarGait;
pub use longship::LongshipVariant;
pub use materials::MaterialKit;
pub use outfit::{AvatarOutfit, OutfitPart};
pub use palette::AvatarPalette;
pub use roadster::{RoadsterBody, RoadsterTop, RoadsterWheels};
pub use rover::RoverVariant;
pub use runabout::RunaboutVariant;
pub use scow::ScowLoad;
pub use sloop::{SloopHull, SloopRig};
pub use tug::TugVariant;
pub use vehicle_blueprint::{
    AirshipBlueprint, BoatBlueprint, NOMINAL_BODY_LEN, NOMINAL_HULL_LEN, SkiffBlueprint,
    VehicleBlueprint, VehicleStance,
};
pub use wagon::WagonBody;
