//! The single integration-test harness (#1179).
//!
//! Every `tests/*.rs` beside this file is a `mod` of it rather than a
//! `[[test]]` target of its own. Nothing about the tests changed; what changed
//! is how many whole-program links a test run pays for. Each integration
//! target statically links the entire Bevy engine, and there were nineteen of
//! them plus a harness for each of the two binaries — twenty-four Bevy links
//! on any `cargo test` that touched the lib, which `.cargo/config.toml` caps
//! at six concurrent precisely because they will not all fit in memory at
//! once. One target links once.
//!
//! ## What this changes about running them
//!
//! Per-file targeting moves from the target flag to the name filter, because
//! there is now one target:
//!
//! ```text
//! cargo test --test freeze_rigid_body            →  cargo test --test integration freeze_rigid_body
//! cargo nextest run -E 'binary(publish_snapshot)' →  cargo nextest run -E 'test(publish_snapshot::)'
//! ```
//!
//! The module path is part of every test's name here (`pds_sanitize::a_room_…`),
//! so a bare substring filter still selects exactly one file's tests.
//!
//! The avian island-corruption canary (#740/#1150) is `#[ignore]`d and no gate
//! runs it. Its invocation is now:
//!
//! ```text
//! cargo test --test integration -- --ignored freeze_rigid_body
//! ```
//!
//! ## Why one process is safe here, and where the line is
//!
//! Sharing a target means sharing a process under `cargo test` (nextest still
//! forks per test). That is only safe for tests that do not share mutable
//! process-global state, and the #1189 audit is what says these do: sixteen of
//! the nineteen files are pure wire-format, sanitiser and deriver tests with
//! no `App` at all, and the three that build one touch nothing global beyond
//! Bevy's task pools, which are `OnceLock`s that behave identically however
//! many tests reach them.
//!
//! The line is worth stating for whatever is added next. A test that arms the
//! panic shadow (`diagnostics::panic::arm` keeps the FIRST directory it is
//! given, for the life of the process), installs a panic hook, asserts on
//! `alloc_track`'s process-wide allocation counters, or asserts on the offload
//! census's global instance, is NOT safe as a module here — it was isolated by
//! having its own binary, and that isolation is what this file spends. Such a
//! test belongs in the lib's unit tests where the fixture can be injected, or
//! in a `[[test]]` target of its own with the reason written down.

mod ambient_recipe;
mod asset_reference;
mod attract_backdrop;
mod audio_config;
mod audio_wire;
mod avatar_record;
mod biome_filter;
mod fixed_point;
mod freeze_rigid_body;
mod inventory_record;
mod misc;
mod oauth_flow;
mod open_union_unknown;
mod particle_generator;
mod pds_records;
mod pds_sanitize;
mod prim_wire;
mod publish_snapshot;
mod sign_generator;
mod texture_referenced;
mod vegetation_wind;
