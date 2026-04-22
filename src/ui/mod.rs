//! Egui overlay panels: login screen, in-game HUD, and the room owner's
//! environment editor.  Each submodule exposes at least one system function
//! that `main.rs` registers under the appropriate state and schedule.

pub mod avatar;
pub mod chat;
pub mod diagnostics;
pub mod inventory;
pub mod login;
pub mod room;
