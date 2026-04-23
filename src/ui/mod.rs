//! Egui overlay panels. Each submodule exposes at least one system function
//! that `main.rs` registers under the appropriate [`crate::state::AppState`]
//! and schedule.
//!
//! * [`login`]        — OAuth 2.0 + DPoP login form, runs in `AppState::Login`.
//! * [`diagnostics`]  — peer roster, mute toggles, event log, logout button.
//! * [`chat`]         — in-room chat window (Reliable channel).
//! * [`people`]       — room roster with per-peer mute toggles.
//! * [`avatar`]       — Avatar editor (HoverRover / Humanoid).
//! * [`inventory`]    — personal stash of `Generator` blueprints.
//! * [`room`]         — owner-only tabbed World Editor (Environment /
//!   Generators / Placements / Raw JSON), gated on `session.did == room.did`.

pub mod avatar;
pub mod chat;
pub mod diagnostics;
pub mod inventory;
pub mod login;
pub mod people;
pub mod room;
