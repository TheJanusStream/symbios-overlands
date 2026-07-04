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
//! * [`dispatch`] — recursive [`spawn_generator`] +
//!   [`dispatch::dispatch_top_level`] walker into the per-generator spawners.
//! * [`contact_recipes`] — [`apply_contact_recipes`] system.

mod contact_recipes;
mod dispatch;
mod environment;
mod executor;
pub(super) mod job;
mod scatter;
mod spawn_ctx;
mod water;

// External callers (`super::compile::SpawnCtx` etc.) reach these names
// through this re-export. Behavioural surface is identical to the
// pre-refactor flat `compile.rs`.
pub(super) use contact_recipes::apply_contact_recipes;
pub use dispatch::spawn_generator;
pub(super) use environment::apply_environment_state;
pub(super) use executor::compile_room_record;
pub use job::{CompileJob, CompiledWorld};
pub use spawn_ctx::{GeneratorCaches, SpawnCtx, budget_exceeded};
