//! Egui overlay panels. Each submodule exposes at least one system function
//! that the library entry point in [`crate::run`] registers under the
//! appropriate [`crate::state::AppState`] and schedule.
//!
//! * [`login`]        — OAuth 2.0 + DPoP login form, runs in `AppState::Login`.
//! * [`diagnostics`]  — peer roster, mute toggles, event log, logout button.
//! * [`chat`]         — in-room chat window (Reliable channel).
//! * [`people`]       — room roster with per-peer mute toggles; peer rows
//!   double as drop targets for inventory gifts, and `incoming_offer_ui`
//!   renders the Accept / Decline / Mute & Decline modal for inbound
//!   [`crate::protocol::OverlandsMessage::ItemOffer`]s.
//! * [`avatar`]       — Avatar editor: tabbed Visuals (generator-tree
//!   editor) + Locomotion (HoverBoat / Humanoid / Airplane / Helicopter /
//!   Car preset picker with per-preset physics tuning).
//! * [`inventory`]    — personal stash of `Generator` blueprints, with
//!   drag-to-place onto terrain and drag-to-gift onto peer rows.
//! * [`catalogue`]    — read-only browser for client-shipped catalogue
//!   entries (see [`crate::catalogue`]), with the same drag-to-place
//!   semantics as `inventory`.
//! * [`room`]         — owner-only tabbed World Editor (Environment /
//!   Region Assets / Placements / Effects / Raw JSON), gated on
//!   `session.did == room.did`.

pub mod avatar;
pub mod catalogue;
pub mod chat;
pub mod diagnostics;
pub mod editable;
pub mod inventory;
pub mod login;
pub mod people;
pub mod room;
