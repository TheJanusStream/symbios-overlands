//! Egui overlay panels. Each submodule exposes at least one system function
//! that the library entry point in [`crate::run`] registers under the
//! appropriate [`crate::state::AppState`] and schedule.
//!
//! * [`login`]        — OAuth 2.0 + DPoP login form, runs in `AppState::Login`.
//! * [`diagnostics`]  — tabbed diagnostics HUD: Overview / Runtime /
//!   Network / Offload metric sparklines, per-subsystem health cards and
//!   anomaly badges, plus the Session tab (peer roster, mute toggles,
//!   event log, session-log export).
//! * [`chat`]         — in-room chat window (Reliable channel).
//! * [`nametag`]      — in-world identity (#1226): a name over every
//!   remote body, and the two-way hover link between a People row and the
//!   body it names.
//! * [`people`]       — room roster with per-peer mute toggles; peer rows
//!   double as drop targets for inventory gifts, and `incoming_offer_ui`
//!   renders the Accept / Decline / Mute & Decline modal for inbound
//!   [`crate::protocol::OverlandsMessage::ItemOffer`]s.
//! * [`avatar`]       — Avatar editor, four tabs: Body (the rigged
//!   `symbios-avatar` parameter panel), Attachments (what is worn, and
//!   where), Visuals (the generator-tree editor, for generator bodies) and
//!   Locomotion (HoverBoat / Humanoid / Airplane / Helicopter / Car preset
//!   picker with per-preset physics tuning).
//! * [`inventory`]    — personal stash of `Generator` blueprints, with
//!   drag-to-place onto terrain and drag-to-gift onto peer rows.
//! * [`catalogue`]    — read-only browser for client-shipped catalogue
//!   entries (see [`crate::catalogue`]), with the same drag-to-place
//!   semantics as `inventory`.
//! * [`room`]         — owner-only tabbed World Editor (Environment /
//!   Region Assets / Placements / Effects / Raw JSON), gated on
//!   `session.did == room.did`.
//! * [`editable`]     — shared Save / Load / Reset commit row, publish
//!   status line, and seed-row widgets used by the Room / Avatar /
//!   Inventory editors.
//! * [`unsaved_guard`] — confirm dialog that gates portal travel and
//!   logout while any editable record has unpublished edits.
//! * [`loading`]      — per-task progress panel for the
//!   `AppState::Loading` gate (fetch / retry / bake status rows).
//! * [`toolbar`]      — top toolbar with per-panel toggle buttons
//!   ([`toolbar::UiPanels`]) and the first-run controls hint.
//! * [`layout`]       — computed non-overlapping default window
//!   geometry + persisted rects ([`layout::WindowChrome`], #833).
//! * [`shortcuts`]    — global keyboard shortcuts: the Esc back-out
//!   ladder, Enter-to-chat, Ctrl+S publish (#836) and Ctrl+Z /
//!   Ctrl+Shift+Z undo (#864).
//! * [`confirm`]      — shared destructive-action confirm modal +
//!   rename dialog ([`confirm::ConfirmState`], #838).
//! * [`travel`]       — in-flight travel overlay + portal approach
//!   prompt (#842).
//! * [`toast`]        — RENDERING for the notification stack; the queue
//!   itself is [`crate::notify::Toasts`], outside `ui` since #1158
//!   because `network`, `player`, `loading` and `terrain` all raise
//!   toasts. The one channel for "something just happened"
//!   feedback (#819). Bottom-right since #1261 f43 — the top-right corner
//!   is where all five right-anchored windows open, and the toast area is
//!   a real pointer area, so it ate their clicks.
//! * [`gateway`]      — gateway destination picker (#748): walking into a
//!   gateway zone lists the **room owner's** mutual follows, so a visitor
//!   browses the owner's social neighbourhood rather than their own.
//! * [`settings`]     — the Settings window (#857): this-machine-only
//!   preferences (theme pick, remote-peer smoothing), persisted by
//!   [`crate::prefs`].
//! * [`theme`]        — semantic theme foundation (#855): three palettes
//!   behind `theme::current(ctx)`, applied on startup and re-applied
//!   whenever the picker swaps the resource.
//! * [`fonts`]        — the bundled base font plus the at-most-once lazy
//!   CJK fallback fetch (#858), so a Chinese / Japanese / Korean string
//!   never renders as tofu; also the home of the source scans that hold
//!   the UI's glyph, spelling and numeric-widget laws.
//! * [`num`]          — the only place a `DragValue` or `Slider` is
//!   built (#1264 f364), so every numeric field in the app accepts the
//!   decimal comma most of Europe and Latin America types.
//! * [`affordances`]  — shared affordance idioms (#859): one add wording,
//!   one danger idiom, one checkmark, one status dot.
//! * [`undo`]         — bounded whole-record undo/redo rings for the room
//!   and avatar editors (#862), captured off the editors' existing commit
//!   ticks in `PostUpdate`.
//! * [`perf`]         — the per-frame costs that scaled with authored
//!   content (#1270) and the rule the guards on them follow: count the
//!   work, do not time it. Holds `LiveValueCache`, the tick-and-flag
//!   record cache the room and avatar editors share.

pub mod affordances;
pub mod avatar;
pub mod catalogue;
pub mod chat;
pub mod confirm;
pub mod diagnostics;
pub mod editable;
pub mod fonts;
pub mod gateway;
pub mod inventory;
pub mod layout;
pub mod loading;
pub mod login;
pub mod modes;
pub mod nametag;
pub mod num;
pub mod other_session;
pub mod people;
pub mod perf;
pub mod reauth;
pub mod room;
pub mod settings;
pub mod shortcuts;
pub mod theme;
pub mod toast;
pub mod toolbar;
pub mod travel;
pub mod undo;
pub mod unsaved_guard;

#[cfg(test)]
mod tests {
    /// #1297 steps (1) and (4). The placement visualiser and the attract
    /// backdrop each read the egui layer's state directly; each now reads
    /// a resource it owns, written once per frame by a `ui` mirror
    /// (`room::mirror_placement_focus`, `login::mirror_login_activity`) —
    /// the `player::RigHold` shape of #1158. A source read, like
    /// `avatar::tests::the_parts_panel_closes_through_one_body`: the
    /// fact being pinned is that neither consumer reaches back in.
    /// Comment lines are exempt, because a rustdoc link is not a
    /// dependency and #1297 group 6 says so.
    ///
    /// The second half pins that each mirror is REGISTERED in `lib.rs`:
    /// a mirror nobody schedules leaves its resource at `Default` for
    /// the app's whole life, every unit test of it still passes, and the
    /// consumer silently reads "nothing is happening" — which for the
    /// login activity means a demo world seeded behind a redirect.
    #[test]
    fn the_mirrored_consumers_do_not_import_the_ui_layer() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let code = |line: &str| line.split("//").next().unwrap_or("").to_owned();
        let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("lib.rs is readable");
        for mirror in [
            "ui::avatar::mirror_rig_hold",
            "ui::room::mirror_placement_focus",
            "ui::login::mirror_login_activity",
        ] {
            assert!(
                lib.lines().any(|line| code(line).contains(mirror)),
                "{mirror} is not registered in src/lib.rs; its resource would stay Default"
            );
        }
        for rel in ["src/world_builder/mod.rs", "src/attract.rs"] {
            let source = std::fs::read_to_string(root.join(rel)).expect("source is readable");
            let hits: Vec<usize> = source
                .lines()
                .enumerate()
                .filter(|(_, line)| code(line).contains("crate::ui::"))
                .map(|(n, _)| n + 1)
                .collect();
            assert!(
                hits.is_empty(),
                "{rel} imports crate::ui:: outside comments at lines {hits:?}; \
                 the fact it needs belongs in a resource it owns, mirrored from ui"
            );
        }
    }
}
