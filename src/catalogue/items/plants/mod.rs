//! Plant-role catalogue entries — L-system trees used both as
//! standalone catalogue items and as the species behind seeded
//! tree-scatters (see [`crate::seeded_defaults::room::scatters`]).

pub mod lsys_monopodial_tree;
pub mod lsys_sympodial_tree;
pub mod lsys_ternary_gravity;
pub mod lsys_ternary_props;
// Biome-specific species (epic #458 biome overhaul) — distinctive
// silhouettes the four generic ABOP trees can't supply.
pub mod lsys_acacia;
pub mod lsys_cactus;
pub mod lsys_dead_shrub;
pub mod lsys_mangrove;
pub mod lsys_palm;
