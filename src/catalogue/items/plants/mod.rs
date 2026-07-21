//! Plant-role catalogue entries — L-system trees used both as
//! standalone catalogue items and as the species behind seeded
//! tree-scatters (see [`crate::seeded_defaults::room::scatters`]).

pub mod variant;

/// Ground-cover props (#911) — the cheap card/decal scatter tier below the
/// trees, consuming the WS1 vegetation textures.
pub mod groundcover;

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
// WS2 expansion species (epic #907, #910) — understory, grove and
// ornamental silhouettes rounding the pool out to the teens.
pub mod lsys_bamboo;
pub mod lsys_birch;
pub mod lsys_bush;
pub mod lsys_fern;
pub mod lsys_flowering_tree;
