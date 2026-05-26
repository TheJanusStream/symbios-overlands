//! Room-scope DID-seeded derivers.
//!
//! Each submodule owns one parameter group of the room (palette,
//! terrain shape, biome textures, atmosphere). They share the
//! [`super::SceneCharacter`] anchor so the derived values stay
//! internally coherent — see the module-level docstring on
//! [`super`] for the full data flow.

pub mod atmosphere;
pub mod audio;
pub mod palette;
pub mod scatters;
pub mod terrain;
pub mod textures;

pub use atmosphere::{Atmosphere, WaterDynamics};
pub use audio::AmbientRecipe;
pub use palette::RoomPalette;
pub use scatters::{TreeScatter, TreeScatters};
pub use terrain::{GeneratorKind, SplatRule, TerrainShape};
pub use textures::{BiomeTextures, GroundTextureParams, RockTextureParams};
