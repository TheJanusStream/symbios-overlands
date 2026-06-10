//! Avatar-scope DID-seeded derivers.
//!
//! Mirrors [`super::room`] in shape: each submodule owns one
//! parameter group and is fully independent — the avatar deriver
//! doesn't read the room deriver (a user's avatar looks the same
//! regardless of which room they visit) and the per-submodule outputs
//! don't depend on each other.
//!
//! The top-level discrete pick is [`ChassisFamily`]: a DID resolves
//! to exactly one visual family (boat / airship / humanoid / skiff),
//! and the wiring layer (`crate::pds::avatar::default_visuals`) reads
//! only that family's design deriver:
//!
//! - [`ChassisFamily::Boat`] → [`VesselDesign`] (hull form, ornament
//!   kit, prow rake, mast taper).
//! - [`ChassisFamily::Airship`] → [`AirshipDesign`] (envelope form,
//!   gondola, fins, engine pods).
//! - [`ChassisFamily::Humanoid`] → [`HumanoidStyle`] +
//!   [`AvatarGait`]; this is the family that consumes the
//!   skin / hair / eye colours and `torso_leg_ratio` directly.
//! - [`ChassisFamily::Skiff`] → [`SkiffDesign`] (running gear,
//!   canopy, exhausts).
//!
//! [`AvatarBody`] (proportions) and [`AvatarPalette`] (colours) are
//! family-agnostic and feed every builder.

pub mod airship;
pub mod body;
pub mod chassis;
pub mod gait;
pub mod humanoid_style;
pub mod palette;
pub mod skiff;
pub mod vessel;

pub use airship::{AirshipDesign, EnvelopeForm};
pub use body::{AvatarBody, BodyArchetype};
pub use chassis::ChassisFamily;
pub use gait::AvatarGait;
pub use humanoid_style::{HatStyle, HumanoidStyle};
pub use palette::AvatarPalette;
pub use skiff::{CanopyStyle, SkiffDesign, SkiffForm};
pub use vessel::{BowStyle, HullForm, VesselArchetype, VesselDesign};
