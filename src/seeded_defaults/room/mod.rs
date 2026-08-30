//! Room-scope DID-seeded derivers.
//!
//! Each submodule owns one parameter group of the room — palette, the theme
//! identity accent, terrain shape and the flat-region siting probe derived
//! from it, biome textures, atmosphere, tree / rock / ground-cover / particle
//! scatters, the spawn-side settlement with its social gateway and owner
//! monument, and the ambient-audio bed. They share the
//! [`super::SceneCharacter`] anchor so the derived values stay
//! internally coherent — see the module-level docstring on
//! [`super`] for the full data flow.
//!
//! [`build`] is the odd one out: it derives nothing itself, it *assembles* —
//! it is where every other submodule's derived shape is wired into a
//! `RoomRecord`, and it is the entry point (`build_room`) that
//! `RoomRecord::default_for_seed` forwards to.

pub mod accent;
pub mod atmosphere;
pub mod audio;
pub mod build;
mod exotic;
pub mod gateway;
pub mod groundcover;
pub mod monument;
pub mod palette;
pub mod particles;
pub mod rocks;
pub mod scatters;
pub mod settlement;
pub mod siting;
pub mod terrain;
pub mod textures;

pub use accent::{ThemeAccent, theme_luminosity};
pub use atmosphere::{Atmosphere, WaterDynamics};
pub use audio::AmbientRecipe;
pub use gateway::GatewaySpot;
pub use groundcover::{GroundCoverScatter, GroundCoverScatters, GroundCoverSpecies};
pub use monument::MonumentSpot;
pub use palette::RoomPalette;
pub use particles::{AmbientParticles, ParticleMood};
pub use rocks::{RockScatter, RockScatters};
pub use scatters::{TreeScatter, TreeScatters, TreeSpecies};
pub use settlement::{
    BUILD_SLOPE_LIMIT, Settlement, SettlementCluster, SettlementMember, SettlementPlan,
};
pub use siting::{BuildableRegion, TerrainProbe};
pub use terrain::{SplatRule, TerrainShape};
pub use textures::{BiomeTextures, GroundTextureParams, RockTextureParams};
