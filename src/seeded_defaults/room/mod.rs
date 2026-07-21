//! Room-scope DID-seeded derivers.
//!
//! Each submodule owns one parameter group of the room (palette, the
//! theme identity accent, terrain shape, biome textures, atmosphere,
//! tree / rock / particle scatters, the spawn-side settlement, the
//! ambient-audio bed). They share the
//! [`super::SceneCharacter`] anchor so the derived values stay
//! internally coherent — see the module-level docstring on
//! [`super`] for the full data flow.

pub mod accent;
pub mod atmosphere;
pub mod audio;
mod exotic;
pub mod gateway;
pub mod groundcover;
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
pub use palette::RoomPalette;
pub use particles::{AmbientParticles, ParticleMood};
pub use rocks::{RockScatter, RockScatters};
pub use scatters::{TreeScatter, TreeScatters, TreeSpecies};
pub use settlement::{
    BUILD_SLOPE_LIMIT, Settlement, SettlementCluster, SettlementMember, SettlementPlan,
};
pub use siting::{BuildableRegion, TerrainProbe};
pub use terrain::{GeneratorKind, SplatRule, TerrainShape};
pub use textures::{BiomeTextures, GroundTextureParams, RockTextureParams};
