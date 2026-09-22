//! Egui overlay panels. Each submodule exposes at least one system function
//! that the library entry point in [`crate::run`] registers under the
//! appropriate [`crate::state::AppState`] and schedule.
//!
//! * [`login`]        - OAuth 2.0 + DPoP login form, runs in `AppState::Login`.
//! * [`diagnostics`]  - tabbed diagnostics HUD: Overview / Runtime /
//!   Network / Offload metric sparklines, per-subsystem health cards and
//!   anomaly badges, plus the Session tab (peer roster, mute toggles,
//!   event log, session-log export).
//! * [`chat`]         - in-room chat window (Reliable channel).
//! * [`nametag`]      - in-world identity (#1226): a name over every
//!   remote body, and the two-way hover link between a People row and the
//!   body it names.
//! * [`people`]       - room roster with per-peer mute toggles; peer rows
//!   double as drop targets for inventory gifts, and `incoming_offer_ui`
//!   renders the Accept / Decline / Mute & Decline modal for inbound
//!   [`crate::protocol::OverlandsMessage::ItemOffer`]s.
//! * [`avatar`]       - Avatar editor, four tabs: Body (the rigged
//!   `symbios-avatar` parameter panel), Attachments (what is worn, and
//!   where), Visuals (the generator-tree editor, for generator bodies) and
//!   Locomotion (HoverBoat / Humanoid / Airplane / Helicopter / Car preset
//!   picker with per-preset physics tuning).
//! * [`inventory`]    - personal stash of `Generator` blueprints, with
//!   drag-to-place onto terrain and drag-to-gift onto peer rows.
//! * [`catalogue`]    - read-only browser for client-shipped catalogue
//!   entries (see [`crate::catalogue`]), with the same drag-to-place
//!   semantics as `inventory`.
//! * [`room`]         - owner-only tabbed World Editor (Environment /
//!   Region Assets / Placements / Effects / Raw JSON), gated on
//!   `session.did == room.did`.
//! * [`editable`]     - shared Save / Load / Reset commit row, publish
//!   status line, and seed-row widgets used by the Room / Avatar /
//!   Inventory editors.
//! * [`unsaved_guard`] - confirm dialog that gates portal travel and
//!   logout while any editable record has unpublished edits.
//! * [`logout`]       - teardown of one signed-in session (#1297
//!   group 2): it closes every window, clears the pickers, drops the
//!   session-scoped resources and cancels the publish tasks. It sat
//!   at the crate root and reached for seventeen `ui` types to do it;
//!   purifying that would have meant seventeen mirrors with no
//!   reader, so the file moved to where the state it tears down
//!   lives. `loading` and `lib` call it by path; nothing else does.
//! * [`loading`]      - per-task progress panel for the
//!   `AppState::Loading` gate (fetch / retry / bake status rows).
//! * [`toolbar`]      - top toolbar with per-panel toggle buttons
//!   ([`toolbar::UiPanels`]) and the first-run controls hint.
//! * [`layout`]       - computed non-overlapping default window
//!   geometry + persisted rects ([`layout::WindowChrome`], #833).
//! * [`shortcuts`]    - global keyboard shortcuts: the Esc back-out
//!   ladder, Enter-to-chat, Ctrl+S publish (#836) and Ctrl+Z /
//!   Ctrl+Shift+Z undo (#864).
//! * [`confirm`]      - shared destructive-action confirm modal +
//!   rename dialog ([`confirm::ConfirmState`], #838).
//! * [`travel`]       - in-flight travel overlay + portal approach
//!   prompt (#842).
//! * [`toast`]        - RENDERING for the notification stack; the queue
//!   itself is [`crate::notify::Toasts`], outside `ui` since #1158
//!   because `network`, `player`, `loading` and `terrain` all raise
//!   toasts. The one channel for "something just happened"
//!   feedback (#819). Bottom-right since #1261 f43 - the top-right corner
//!   is where all five right-anchored windows open, and the toast area is
//!   a real pointer area, so it ate their clicks.
//! * [`gateway`]      - gateway destination picker (#748): walking into a
//!   gateway zone lists the **room owner's** mutual follows, so a visitor
//!   browses the owner's social neighbourhood rather than their own.
//! * [`settings`]     - the Settings window (#857): client-side
//!   preferences (theme pick, remote-peer smoothing, …), kept per account
//!   on this machine by [`crate::prefs`] (#1407).
//! * [`theme`]        - semantic theme foundation (#855): three palettes
//!   behind `theme::current(ctx)`, applied on startup and re-applied
//!   whenever the picker swaps the resource.
//! * [`fonts`]        - the bundled base font plus the at-most-once lazy
//!   CJK fallback fetch (#858), so a Chinese / Japanese / Korean string
//!   never renders as tofu; also the home of the source scans that hold
//!   the UI's glyph, spelling and numeric-widget laws.
//! * [`num`]          - the only place a `DragValue` or `Slider` is
//!   built (#1264 f364), so every numeric field in the app accepts the
//!   decimal comma most of Europe and Latin America types.
//! * [`affordances`]  - shared affordance idioms (#859): one add wording,
//!   one danger idiom, one checkmark, one status dot.
//! * [`undo`]         - bounded whole-record undo/redo rings for the room
//!   and avatar editors (#862), captured off the editors' existing commit
//!   ticks in `PostUpdate`.
//! * [`perf`]         - the per-frame costs that scaled with authored
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
pub mod item_picture;
pub mod layout;
pub mod loading;
pub mod login;
pub mod logout;
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
    /// Paths outside `src/ui` that may import the ui layer in code - in
    /// any of the spellings [`ui_layer_hits`] knows - each with the reason
    /// it may. Everything else under `src/` is expected to reach the layer
    /// through a resource IT owns, written once a frame by a `ui` mirror
    /// in `PreUpdate` (#1158, #1297).
    ///
    /// Every entry is asserted LIVE by
    /// [`the_mirrored_consumers_do_not_import_the_ui_layer`]: an
    /// exemption whose file has stopped importing the layer is deleted
    /// rather than left standing, or the list quietly stops describing
    /// the tree it guards.
    const MAY_IMPORT_UI: &[(&str, &str)] = &[
        (
            "src/editor_gizmo/",
            "#1158's own stated destination: the gizmo IS an editor surface, \
             so it sits inside the ui layer's concerns even though its files \
             live outside src/ui.",
        ),
        (
            "src/state.rs",
            "#1297 group 5, owner decision 2026-09-10: LocalSettings::theme \
             (and LoginScreenSettings::theme since #1407) records WHICH shipped \
             palette an account or the login screen chose, and a palette \
             is ui vocabulary. Moving UserTheme to state would drag the theme \
             module's meaning out of the layer that renders it, for one \
             serialised field.",
        ),
        (
            "src/prefs.rs",
            "#1297 group 5, owner decision 2026-09-10: prefs persistence IS a \
             ui concern. This module exists to serialise WindowLayout and \
             UiPanels to disk; inverting it would mean mirroring every panel \
             flag out of ui for a writer whose only purpose is to write them \
             back.",
        ),
        (
            "src/render_tool/editor.rs",
            "#1353: the render tool's `--editor` host is a composition root, \
             like lib.rs. It schedules the game's own editor systems (the \
             toolbar, the World Editor, the Catalogue, the toasts, undo \
             capture) and seeds the ui resources they read, because a README \
             picture of the editor has to come from the real editor code path. \
             The ui layer is the subject being photographed, so there is no \
             fact to mirror out of it.",
        ),
    ];

    /// The `ui` dependency inversion, as one law (#1158 -> #1297, closed
    /// 2026-09-10). Nothing outside `src/ui` may import the ui layer in
    /// code unless [`MAY_IMPORT_UI`] says why - and "import" means every
    /// spelling [`ui_layer_hits`] knows, not the literal `crate::ui::`
    /// this sweep grepped for until #1355. What a domain module needs
    /// from a panel is a FACT, and a fact is a resource it owns, written
    /// once a frame by a `ui` mirror in `PreUpdate` - the
    /// `player::RigHold` shape, built six times now.
    ///
    /// Four things are pinned, and each of them caught something real:
    ///
    /// 1. **The tree**, swept file by file rather than from a list of the
    ///    ones this work cleared. A list cannot see a NEW domain file
    ///    reaching into the layer, which is the only regression left.
    ///    Comment lines do not count, because a rustdoc link is not a
    ///    dependency and #1297 group 6 says so; nor does a file whose
    ///    every hit sits inside `#[cfg(test)]`. The spelling does not
    ///    count either: a bare `use crate::ui;` followed by
    ///    `ui::toolbar::…` is the same dependency as the long form, and
    ///    a matcher that only knew the long form let one such file pass
    ///    on the strength of the two long-form lines beside it (#1355).
    /// 2. **[`MAY_IMPORT_UI`] against the tree**, so a stale exemption is
    ///    a failure rather than a comment nobody reads.
    /// 3. **Each mirror's REGISTRATION in `lib.rs`.** A mirror nobody
    ///    schedules leaves its resource at `Default` for the app's whole
    ///    life, every unit test of it still passes, and the consumer
    ///    silently reads "nothing is happening" - which for the login
    ///    activity means a demo world seeded behind a redirect. This is
    ///    not hypothetical: a `cargo fmt` reflow ate one registration on
    ///    2026-09-10 and the unit tests stayed green.
    /// 4. **Where the session teardown lives**, which no scan of files
    ///    still in `src` can check, because the regression is the file
    ///    moving back out.
    #[test]
    fn the_mirrored_consumers_do_not_import_the_ui_layer() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let code = |line: &str| line.split("//").next().unwrap_or("").to_owned();
        let lib = std::fs::read_to_string(root.join("src/lib.rs")).expect("lib.rs is readable");
        for mirror in [
            "ui::avatar::mirror_rig_hold",
            "ui::room::mirror_placement_focus",
            "ui::login::mirror_login_activity",
            "ui::confirm::mirror_attention_held",
            "ui::catalogue::mirror_preview_request",
        ] {
            assert!(
                lib.lines().any(|line| code(line).contains(mirror)),
                "{mirror} is not registered in src/lib.rs; its resource would stay Default"
            );
        }
        // The whole tree, not a list of the files this work happened to
        // clear (#1297's close-out). A list cannot catch a NEW domain file
        // that reaches into the layer, which is the regression that
        // matters now that the existing ones are gone - so the walk asks
        // the canonical question of every file instead:
        //
        //   grep -rl 'crate::ui::' src --exclude-dir=ui
        //
        // minus the two exclusions that command cannot make and #1297
        // group 6 insisted on, because counting them is how the number
        // gets argued about instead of acted on: a rustdoc link is not a
        // dependency, and neither is a `#[cfg(test)]` source-scan helper
        // reaching for another module's scan list - and plus the
        // spellings that command cannot see (`ui_layer_hits`, #1355).
        let mut offenders: Vec<String> = Vec::new();
        for path in walk_rs(&root.join("src")) {
            let rel = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            if rel.starts_with("src/ui/") || MAY_IMPORT_UI.iter().any(|(p, _)| rel.starts_with(p)) {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source is readable");
            let hits = ui_layer_hits(&source);
            if hits.is_empty() {
                continue;
            }
            // A file whose every hit sits after its first test gate is a
            // test helper, not a dependency: `camera.rs`'s `WorldCamera`
            // marker scan (#1300) and `oauth/capped_fetch.rs`'s reach for
            // the glyph-coverage list.
            //
            // `#[cfg(all(test, …))]` counts, and getting that wrong is not
            // theoretical - `capped_fetch.rs`'s scan is gated
            // `#[cfg(all(test, not(target_arch = "wasm32")))]` and this
            // sweep called it a real importer until the cut matched what
            // the issue's own baseline command matches:
            // `^#\[cfg\((all\()?test`. The file says so in its own
            // comments, because `fonts::glyph_coverage_tests::non_test_source`
            // has the narrower cut and it bit there first.
            let first_test = source
                .lines()
                .position(|line| {
                    let line = line.trim_start();
                    line.starts_with("#[cfg(test)") || line.starts_with("#[cfg(all(test")
                })
                .map(|n| n + 1);
            if first_test.is_some_and(|t| hits[0] > t) {
                continue;
            }
            offenders.push(format!("{rel}:{hits:?}"));
        }
        assert!(
            offenders.is_empty(),
            "these files outside src/ui import the egui layer in code: {offenders:?}. \
             The fact each needs belongs in a resource IT owns, written once a frame \
             by a `ui` mirror in PreUpdate (#1158, #1297) - or, if it is genuinely \
             the ui layer's own, in MAY_IMPORT_UI above with the reason why"
        );
        for (path, reason) in MAY_IMPORT_UI {
            let live = imports_ui_in_code(root, path);
            assert!(
                !live.is_empty(),
                "the exemption for {path} is stale - nothing under it imports \
                 the ui layer in code any more, in any spelling, so DELETE the \
                 entry rather than leave it describing a tree that has moved on. \
                 Its reason was: {reason}"
            );
        }
    }

    /// The sweep's matcher sees every spelling of the import, not only the
    /// `crate::ui::` it grepped for until #1355, and none of the
    /// look-alikes. The first row is the line that motivated this: it was
    /// in the tree, exempted, and invisible to both the sweep and the
    /// exemption's liveness check, so the entry would have read as stale
    /// the day the long-form lines beside it went.
    #[test]
    fn the_ui_sweep_sees_every_spelling_of_an_import() {
        let imports: &[(&str, &[usize])] = &[
            ("use crate::ui;", &[1]),
            ("use crate::ui::layout::WindowLayout;", &[1]),
            ("use crate::ui as u;", &[1]),
            ("use crate::{state::AppState, ui};", &[1]),
            ("use crate::{ui, state::AppState};", &[1]),
            ("use crate::{state::AppState, ui::room::EditorTab};", &[1]),
            ("use crate::{state::{AppState, LiveRoomRecord}, ui};", &[1]),
            ("use crate::{ui as u, state::AppState};", &[1]),
            ("use crate::{\n    state::AppState,\n    ui,\n};", &[3]),
            (
                "use crate::{\n    state::AppState,\n    ui::room::EditorTab,\n};",
                &[3],
            ),
            ("fn draw() {\n    crate::ui::toolbar::draw();\n}", &[2]),
            (
                "use crate::state::AppState;\nuse crate::ui;\nuse crate::ui::layout::UiWindow;",
                &[2, 3],
            ),
        ];
        for (source, lines) in imports {
            assert_eq!(&ui_layer_hits(source), lines, "{source:?}");
        }
        let look_alikes = [
            "use crate::ui_helpers::x;",
            "use crate::uix::y;",
            "use crate::state::ui;",
            "use crate::{state::{AppState, ui}};",
            "use crate::{state::ui, prefs::Prefs};",
            "use crate::{uid, state::AppState};",
            "// use crate::ui;",
            "/// [`crate::ui::layout::WindowLayout`] is where the window lives.",
            "use crate::state::AppState; // was crate::ui::layout",
        ];
        for source in look_alikes {
            assert!(ui_layer_hits(source).is_empty(), "{source:?}");
        }
    }

    /// The lines (1-based) on which `source` imports the ui layer in code,
    /// comments stripped. Every spelling counts, not only the literal
    /// `crate::ui::` the sweep grepped for until #1355: `crate::ui::x`; a
    /// bare `crate::ui` ended by `;`, `,`, `}`, ` as ` or the line; and
    /// `ui` as a member of a `crate::{...}` group - bare, `ui::x` or
    /// `ui as u` - however many lines rustfmt spread the group over.
    #[cfg(test)]
    fn ui_layer_hits(source: &str) -> Vec<usize> {
        // One string, comments gone, newlines kept: a byte offset maps
        // back to its line, and a `crate::{` group reads across lines.
        let code = source
            .lines()
            .map(|line| line.split("//").next().unwrap_or(""))
            .collect::<Vec<_>>()
            .join("\n");
        let line_of = |offset: usize| code[..offset].matches('\n').count() + 1;
        let mut hits = Vec::new();
        for (pos, _) in code.match_indices("crate::ui") {
            let after = &code[pos + "crate::ui".len()..];
            // `crate::ui_helpers`, `crate::uix`: a different name.
            if after.starts_with(|c: char| c.is_alphanumeric() || c == '_') {
                continue;
            }
            let follows = after.trim_start();
            if after.starts_with("::")
                || follows.is_empty()
                || follows.starts_with([';', ',', '}'])
                || follows.starts_with("as ")
            {
                hits.push(line_of(pos));
            }
        }
        for (pos, _) in code.match_indices("crate::{") {
            let open = pos + "crate::{".len();
            let members = &code[open..open + group_len(&code[open..])];
            // Split at this group's own commas, not a nested group's.
            let mut depth = 0usize;
            let mut from = 0usize;
            let mut bounds = Vec::new();
            for (i, c) in members.char_indices() {
                match c {
                    '{' => depth += 1,
                    '}' => depth = depth.saturating_sub(1),
                    ',' if depth == 0 => {
                        bounds.push((from, i));
                        from = i + 1;
                    }
                    _ => {}
                }
            }
            bounds.push((from, members.len()));
            for (from, to) in bounds {
                let member = &members[from..to];
                let name = member.trim();
                if name == "ui" || name.starts_with("ui::") || name.starts_with("ui ") {
                    let lead = member.len() - member.trim_start().len();
                    hits.push(line_of(open + from + lead));
                }
            }
        }
        hits.sort_unstable();
        hits.dedup();
        hits
    }

    /// Bytes from the start of `rest` to the brace that closes the group
    /// `rest` begins inside; all of `rest` when nothing closes it.
    #[cfg(test)]
    fn group_len(rest: &str) -> usize {
        let mut depth = 0usize;
        for (i, c) in rest.char_indices() {
            match c {
                '{' => depth += 1,
                '}' if depth == 0 => return i,
                '}' => depth -= 1,
                _ => {}
            }
        }
        rest.len()
    }

    /// Every `.rs` file at or under `dir`.
    #[cfg(test)]
    fn walk_rs(dir: &std::path::Path) -> Vec<std::path::PathBuf> {
        let mut out = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                let entries = std::fs::read_dir(&path).expect("directory is readable");
                stack.extend(entries.map(|e| e.expect("entry is readable").path()));
            } else if path.extension().is_some_and(|e| e == "rs") {
                out.push(path);
            }
        }
        out.sort();
        out
    }

    /// Files at or under `rel` (a file path or a directory prefix) that
    /// import the ui layer on a line with code on it, by the same
    /// [`ui_layer_hits`] the sweep uses - one matcher, so an exemption
    /// cannot read as stale for a spelling the sweep would still catch
    /// (#1355).
    #[cfg(test)]
    fn imports_ui_in_code(root: &std::path::Path, rel: &str) -> Vec<String> {
        let mut out = Vec::new();
        let mut stack = vec![root.join(rel.trim_end_matches('/'))];
        while let Some(path) = stack.pop() {
            if path.is_dir() {
                let entries = std::fs::read_dir(&path).expect("directory is readable");
                stack.extend(entries.map(|e| e.expect("entry is readable").path()));
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(&path).expect("source is readable");
            if !ui_layer_hits(&source).is_empty() {
                out.push(path.display().to_string());
            }
        }
        out.sort();
        out
    }
}
