//! Room-record → ECS compile engine: incremental (per-placement diff)
//! and time-sliced (per-frame budget).
//!
//! The engine itself is [`executor::compile_room_record`] — see the
//! [`executor`] module docs for the plan/execute split and why both
//! halves exist for the wasm build. This module is wiring + re-exports
//! only.
//!
//! ## Sub-module map
//!
//! * [`executor`] — the `compile_room_record` system: the planning
//!   diff ([`job::unit_fingerprint`] vs [`job::CompiledWorld`]) and the
//!   sliced unit builder (`start_unit` / `step_unit`).
//! * [`job`] — [`CompiledWorld`] / [`CompileJob`] state, the unit
//!   fingerprint, the slice budget, and the resume cursors.
//! * [`spawn_ctx`] — [`SpawnCtx`] (the write-context shared with every
//!   sibling spawner module), [`GeneratorCaches`] system param,
//!   [`MAX_ROOM_ENTITIES`](spawn_ctx::MAX_ROOM_ENTITIES) cap +
//!   [`budget_exceeded`] gate, and [`spawn_ctx::transform_from_data`].
//! * [`water`] — [`water::room_water_level`] sea-level lookup and the
//!   dry-land relocation walk for water-avoiding placements.
//! * [`environment`] — [`apply_environment_state`] (its own system).
//! * [`scatter`] — sampling helpers and the biome-rule evaluator.
//! * [`census`] — offline replay of the sampling loop, for measuring what
//!   a seeded room actually places (`render --scatter-census`).
//! * [`dispatch`] — recursive [`spawn_generator`] +
//!   [`dispatch::dispatch_top_level`] walker into the per-generator spawners.
//! * [`contact_recipes`] — [`apply_contact_recipes`] system.

// Native-only: the census replays sampling against a heightmap rebuilt via
// `terrain::rebuild_heightmap_for_record`, which (like the render tool that
// is its only caller) does not exist on wasm.
#[cfg(not(target_arch = "wasm32"))]
mod census;
mod contact_recipes;
mod dispatch;
mod environment;
mod executor;
pub(super) mod job;
mod scatter;
mod slope;
mod spawn_ctx;
mod water;

// External callers (`super::compile::SpawnCtx` etc.) reach these names
// through this re-export. Behavioural surface is identical to the
// pre-refactor flat `compile.rs`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use census::scatter_census;
pub(super) use contact_recipes::apply_contact_recipes;
pub use dispatch::spawn_generator;
pub(super) use environment::apply_environment_state;
pub(super) use executor::compile_room_record;
pub use job::{CompileJob, CompiledWorld};
#[cfg(not(target_arch = "wasm32"))]
pub(crate) use scatter::ScatterPreview;
pub use spawn_ctx::{GeneratorCaches, SpawnCtx, budget_exceeded};
/// Re-exported so the terrain splat pass reads the room's water line from
/// the same single source the scatter sampler does — if the two ever
/// disagreed, the damp margin drawn on the ground and the riparian band the
/// reeds are placed in would sit at different heights (#913).
pub(crate) use water::room_water_level;
