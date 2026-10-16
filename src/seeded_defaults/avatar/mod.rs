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
//!     → craft      (the craft TYPE a vehicle is drawn as, weighted by
//!                  style)
//!     → body/gait  (proportions + locomotion tuning)
//!     → vehicle_blueprint (the vehicle counterpart: hull / cabin / fuselage
//!                  proportions from a seeded stance register)
//! ```
//!
//! The top-level discrete pick is [`ChassisFamily`] (boat / airship /
//! humanoid / skiff / helicopter - two stages since #1614, see [`chassis`]),
//! and inside three of the vehicle families a second discrete pick follows
//! it: the [`CraftType`] (sloop / longship / tug / junk / runabout / scow,
//! roadster / buggy / armoured car / cyclecar / wagon / rover, and the
//! utility helicopter), weighted by the style through the shared mood
//! taxonomy in [`mood`] so a theme arrives on the craft it would actually
//! build (#1362). Every one of those thirteen types is BUILT: the rover
//! closed the skiff half at #1378, the longship the boat half at #1369, and
//! the helicopter family opened with its hero built (#1614).
//!
//! How that pick becomes a silhouette depends on the family, and the two
//! routes are not the same one:
//!
//! * Every VEHICLE is drawn by ITS OWN TYPE'S BUILDER, one per
//!   [`CraftType`], each reading every line and every mount off one profile
//!   derived from [`VehicleBlueprint`] - a `HullProfile` for the boat
//!   (#1363), a `BodyPlan` for the skiff (#1364), a `PodPlan` for the
//!   helicopter (#1614), a `FusePlan` for the airplane (#1617) and a
//!   `HullPlan` for the airship (#1657), the last family to leave the tagged
//!   part catalogue and the outfit that filled its slots. Variety inside a
//!   type comes from the type's own seeded variants (a sloop's rig and hull
//!   form, a roadster's body, top and wheels) and from the livery.
//! * The HUMANOID short-circuits both: it rolls a `symbios-avatar` engine
//!   record and is skinned at spawn (#1060).
//!
//! [`AvatarBody`] (proportions) and [`AvatarPalette`] (colours) are
//! family-agnostic and feed every build; [`MaterialKit`] supplies the
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
pub use character::{AvatarCharacter, AvatarPins, FinishRegister, OrnatenessTier, WearTier};
pub use chassis::ChassisFamily;
pub use craft::{
    AirplaneType, AirshipType, BoatType, CraftType, HelicopterType, SkiffType, craft_axis,
};
pub use fx::{AvatarFx, AvatarVoice, ParticleAura};
pub use gait::AvatarGait;
pub use longship::LongshipVariant;
pub use materials::MaterialKit;
pub use palette::AvatarPalette;
pub use roadster::{RoadsterBody, RoadsterTop, RoadsterWheels};
pub use rover::RoverVariant;
pub use runabout::RunaboutVariant;
pub use scow::ScowLoad;
pub use sloop::{SloopHull, SloopRig};
pub use tug::TugVariant;
pub use vehicle_blueprint::{
    AirplaneBlueprint, AirshipBlueprint, BoatBlueprint, HelicopterBlueprint, NOMINAL_AIRFRAME_LEN,
    NOMINAL_AIRSHIP_LEN, NOMINAL_BODY_LEN, NOMINAL_FUSELAGE_LEN, NOMINAL_HULL_LEN, SkiffBlueprint,
    VehicleBlueprint, VehicleStance,
};
pub use wagon::WagonBody;
