//! Computed non-overlapping default window layout + persisted rects (#833).
//!
//! Before this existed every window's default position was an absolute
//! pixel constant tuned for a ~1340px desktop window: Chat `[960,10]`
//! overlapped People `[770,10]` exactly over its Mute column, the
//! 820x620 World Editor buried everything to its right, and every
//! `y=10` title bar spawned UNDER the toolbar (egui windows constrain
//! to the full `content_rect`, not the panel-free `available_rect`).
//! The constants were also split between `config.rs` and inline
//! literals at four call sites, so nobody could see the whole layout in
//! one place.
//!
//! This module is now the single home of window geometry:
//!
//! * Every toolbar-managed window has a [`Slot`] — a default size plus
//!   a horizontal [`SlotAnchor`] — and its default position is computed
//!   from the panel-free rect the toolbar publishes as [`PanelFreeRect`]
//!   (the toolbar system is chained first, so it is current by the time
//!   any window asks) the first time it opens. Social panels anchor right, diagnostics left, the big
//!   editors center-left.
//! * A window opening while others are up staggers around them:
//!   [`resolve_overlaps`] tries stacking below the open windows first
//!   (keeping columns), then beside them, and only accepts an overlap
//!   as a bounded cascade when the screen genuinely has no free room.
//! * The rect a window actually ends up with (drag, resize) is captured
//!   every frame by [`WindowChrome::remember`] and persisted through
//!   the #820 prefs layer, so the machine's arranged layout survives a
//!   restart and beats the computed default thereafter.
//!
//! Consumers add a [`WindowChrome`] system param, ask it to
//! [`place`](WindowChrome::place) the window before building it, and
//! [`remember`](WindowChrome::remember) the shown rect afterwards.

use bevy::diagnostic::FrameCount;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use bevy_egui::egui;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Lay a window's fixed footer against its bottom edge, so the
/// scrollable body above it can be given **exactly** what is left
/// (#1280). Pair with [`fill_above`], which the closure calls last.
///
/// # The bug this exists to make unwriteable
///
/// The idiom it replaces guessed the footer's height:
///
/// ```ignore
/// const INPUT_RESERVE_HEIGHT: f32 = 44.0;
/// let scroll_height = (ui.available_height() - INPUT_RESERVE_HEIGHT).max(60.0);
/// egui::ScrollArea::vertical()
///     .auto_shrink([true, false])
///     .max_height(scroll_height)
/// ```
///
/// `auto_shrink[1] == false` means the scroll area always *claims*
/// `max_height`, so the window's measured content comes to
/// `(available_height - guess) + the footer's real height`. The moment
/// the footer outgrows the guess by a pixel the content is taller than
/// the window — and egui's `Resize` responds like this on every frame it
/// is not being actively dragged (`egui-0.35.0/src/containers/resize.rs`,
/// `Resize::begin`):
///
/// ```ignore
/// // We are not being actively resized, so auto-expand to include size of last frame.
/// state.desired_size = state.desired_size.max(state.last_content_size);
/// ```
///
/// `desired_size` never decreases on its own, so the overshoot is added
/// to the window *every frame*: `available_height` grows, the scroll area
/// grows with it, and the window climbs until `constrain_to` clamps it at
/// the screen edge. That is #1280 — Chat filled the viewport in under a
/// second once #1141 and #1213 had each added a line below its input row.
///
/// # Why the replacement is a fixed point
///
/// The footer is laid out FIRST, in a bottom-up `Ui`, so its height is
/// *measured*; [`fill_above`] then hands the body a `Ui` whose available
/// height is the true remainder. A `ScrollArea` with
/// `auto_shrink([true, false])` and **no `max_height`** fills exactly
/// that, so content height == available height on every frame and
/// `desired_size.max(..)` is a no-op.
///
/// The closure adds the footer BOTTOM-MOST FIRST — a bottom-up layout
/// stacks upward — and ends with [`fill_above`]:
///
/// ```ignore
/// layout::bottom_anchored(ui, |ui| {
///     ui.small(hint_line());     // sits at the very bottom
///     ui.horizontal(|ui| { .. }); // the input row, above it
///     layout::fill_above(ui, |ui| {
///         egui::ScrollArea::vertical()
///             .auto_shrink([true, false])
///             .show(ui, |ui| { .. });
///     });
/// });
/// ```
///
/// Splitting this into two calls rather than taking `body` and `footer`
/// closures together is deliberate: the two halves of a chat window
/// touch the same state (the footer sends a message, the body renders
/// the history), and two closures alive at once cannot both borrow it.
pub fn bottom_anchored<R>(
    ui: &mut egui::Ui,
    footer_then_body: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), footer_then_body)
        .inner
}

/// The scrollable remainder above a [`bottom_anchored`] footer: draws
/// the separator that divides them, then runs `body` in a normal
/// top-down `Ui` sized to whatever the footer left.
///
/// **Do not set `max_height` on a scroll area inside `body`** — that is
/// the guess this pair exists to delete (see [`bottom_anchored`]).
pub fn fill_above<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    ui.separator();
    // Back to top-down: a bottom-up `Ui` would emit a scrollback in
    // reverse, and everything inside the scroll area wants the ordinary
    // reading direction.
    ui.with_layout(egui::Layout::top_down(egui::Align::Min), body)
        .inner
}

/// Gap between a computed window rect and its neighbours / the screen
/// edges. Matches the ~10px the old absolute constants used.
const MARGIN: f32 = 10.0;

/// Horizontal placement of a [`SlotAnchor::CenterLeft`] window: this
/// fraction of the leftover width goes to its left. 0.25 reads as
/// "left of center" — enough room that the right-anchored social
/// column stays clear on a 1280px window.
const CENTER_LEFT_FRACTION: f32 = 0.25;

/// Cascade fallback when no free spot exists: diagonal step and how
/// many steps to try before giving up at the preferred position.
const CASCADE_STEP: f32 = 24.0;
const CASCADE_TRIES: usize = 8;

/// A live rect older than this many frames no longer counts as "open"
/// for collision avoidance. Window systems re-stamp every frame they
/// show, so anything beyond a couple of frames is a closed window.
const LIVE_STALE_FRAMES: u32 = 3;

/// Every window whose geometry this module manages. The variant is the
/// in-code identity; [`UiWindow::key`] is the stable string the
/// persisted rect map is keyed by (strings, not the enum, so a prefs
/// file written by a newer binary with more windows still loads here).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum UiWindow {
    Chat,
    People,
    Avatar,
    Inventory,
    Catalogue,
    WorldEditor,
    Diagnostics,
    AudioEditor,
    Controls,
    Settings,
}

/// Where a slot's default position hugs horizontally. Vertically every
/// slot starts at the top of the available rect (below the toolbar).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SlotAnchor {
    /// Right edge — the glanceable social column (Chat, People) plus
    /// Inventory, which participates in drag-to-gift onto People rows.
    Right,
    /// Left edge — Diagnostics.
    Left,
    /// Left of center — the big editors, so they neither bury the
    /// right-anchored column nor pin themselves into the corner.
    CenterLeft,
}

/// A window's default geometry: size plus horizontal anchor.
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub anchor: SlotAnchor,
    pub size: [f32; 2],
}

impl UiWindow {
    /// Stable key into the persisted rect map ([`WindowLayout::rects`]).
    pub fn key(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::People => "people",
            Self::Avatar => "avatar",
            Self::Inventory => "inventory",
            Self::Catalogue => "catalogue",
            Self::WorldEditor => "world_editor",
            Self::Diagnostics => "diagnostics",
            Self::AudioEditor => "audio_editor",
            Self::Controls => "controls",
            Self::Settings => "settings",
        }
    }

    /// Default geometry. Sizes carried over from the old constants,
    /// with two trims so the right column stacks inside a 720px-tall
    /// window (Bevy's default): People 300→280, Inventory 400→340 —
    /// both windows scroll, and top+Inventory+People must fit
    /// 40+340+10+280 ≤ 690 for the #833 acceptance layout.
    pub fn slot(self) -> Slot {
        use SlotAnchor::*;
        let (anchor, size) = match self {
            Self::Chat => (Right, [380.0, 400.0]),
            Self::People => (Right, [280.0, 280.0]),
            Self::Inventory => (Right, [300.0, 340.0]),
            Self::Diagnostics => (Left, [280.0, 480.0]),
            // 760 wide so the embedded generator tree's 260px side
            // panel leaves a usable detail panel (#830). Height is an
            // estimate for collision math only — the window itself
            // auto-heights (its call site applies width only).
            Self::Avatar => (CenterLeft, [760.0, 620.0]),
            Self::WorldEditor => (CenterLeft, [820.0, 620.0]),
            Self::Catalogue => (CenterLeft, [560.0, 440.0]),
            Self::AudioEditor => (CenterLeft, [900.0, 640.0]),
            // The de-anchored Controls sheet (#834): a compact card
            // near the right edge once it stops being center-pinned.
            // The height is the OWNER variant's real content (#1235 f245)
            // — heading, ~11 grid rows, the emote hint, the portal
            // paragraph, the avatar block, then a separator, a heading,
            // 4 editor rows, two notes and "Got it". At 280 the collision
            // math was computed against less than half the window, so the
            // sheet was placed as if it could not overlap anything.
            Self::Controls => (Right, [340.0, 580.0]),
            // Compact preference card (#857) — same right-edge family
            // as the Controls sheet it usually appears near.
            Self::Settings => (Right, [300.0, 200.0]),
        };
        Slot { anchor, size }
    }
}

/// Persisted window rects, keyed by [`UiWindow::key`] as `[x, y, w, h]`.
/// Written whenever a shown window's rect actually changes (drag,
/// resize — not every frame, so the #820 save debounce can settle) and
/// saved/restored through [`crate::prefs::PersistedPrefs`]. A persisted
/// rect beats the computed default; `constrain_to` at the call sites
/// keeps a rect from a bigger screen on-screen and below the toolbar.
#[derive(Resource, Serialize, Deserialize, Clone, Default, Debug, PartialEq)]
pub struct WindowLayout {
    #[serde(default)]
    pub rects: HashMap<String, [f32; 4]>,
}

/// Runtime-only: the rect each window was shown at, stamped with the
/// frame it was last seen. This is the "already open" set that new
/// windows stagger around. Deliberately separate from [`WindowLayout`]
/// so the every-frame stamp writes don't re-arm the prefs save
/// debounce forever.
/// The viewport minus the top-level panels, published every frame by the
/// toolbar — the one top-level panel — once it has laid itself out.
///
/// egui 0.35 panels carve the `Ui` they are shown into rather than the
/// `Context`, so `ctx.available_rect()` is gone and the context no longer
/// knows what the toolbar took. Window systems read this through
/// [`WindowChrome::available_rect`] instead. `None` until the toolbar has
/// run once (the login screen has no toolbar), when the whole content rect
/// is the honest answer.
#[derive(Resource, Default)]
pub struct PanelFreeRect(pub Option<egui::Rect>);

#[derive(Resource, Default)]
pub struct LiveWindowRects {
    entries: HashMap<UiWindow, (u32, egui::Rect)>,
}

/// The one param a window system needs to opt into managed geometry:
/// [`place`](Self::place) before building the window,
/// [`remember`](Self::remember) with the shown rect afterwards.
#[derive(SystemParam)]
pub struct WindowChrome<'w> {
    layout: ResMut<'w, WindowLayout>,
    live: ResMut<'w, LiveWindowRects>,
    frame: Res<'w, FrameCount>,
    free: Res<'w, PanelFreeRect>,
}

impl WindowChrome<'_> {
    /// The rect windows may occupy: the viewport minus the toolbar, as the
    /// toolbar published it this frame ([`PanelFreeRect`]), or the whole
    /// content rect before the toolbar has run. The replacement for egui's
    /// pre-0.35 `ctx.available_rect()`.
    pub fn available_rect(&self, ctx: &egui::Context) -> egui::Rect {
        self.free.0.unwrap_or_else(|| ctx.content_rect())
    }

    /// Default position + size for `id`: the persisted rect when this
    /// machine has one, otherwise the slot default staggered around the
    /// currently-open windows. Cheap to call every frame — egui only
    /// consumes `default_pos`/`default_size` on a window's first show.
    pub fn place(&self, id: UiWindow, ctx: &egui::Context) -> (egui::Pos2, egui::Vec2) {
        let avail = self.available_rect(ctx);
        if let Some(&[x, y, w, h]) = self.layout.rects.get(id.key()) {
            return (egui::pos2(x, y), egui::vec2(w, h));
        }
        let taken: Vec<egui::Rect> = self
            .live
            .entries
            .iter()
            .filter(|(other, (stamp, _))| {
                **other != id && self.frame.0.wrapping_sub(*stamp) <= LIVE_STALE_FRAMES
            })
            .map(|(_, (_, rect))| *rect)
            .collect();
        place_in(id.slot(), &taken, avail)
    }

    /// Record the rect a window was actually shown at this frame: into
    /// the live open-set always, into the persisted layout only when it
    /// changed (so parked windows don't hold the save debounce open).
    pub fn remember(&mut self, id: UiWindow, rect: egui::Rect) {
        self.live.entries.insert(id, (self.frame.0, rect));
        let stored = [rect.min.x, rect.min.y, rect.width(), rect.height()];
        if self.layout.rects.get(id.key()) != Some(&stored) {
            self.layout.rects.insert(id.key().to_owned(), stored);
        }
    }
}

/// Pure placement: slot default staggered around `taken` within
/// `avail`. Factored out of [`WindowChrome::place`] so the layout is
/// unit-testable without an egui context.
fn place_in(slot: Slot, taken: &[egui::Rect], avail: egui::Rect) -> (egui::Pos2, egui::Vec2) {
    let size = egui::vec2(slot.size[0], slot.size[1]);
    let preferred = preferred_pos(slot.anchor, size, avail);
    (resolve_overlaps(size, preferred, taken, avail), size)
}

/// The slot's ideal position: top of the available rect (i.e. just
/// below the toolbar), hugging the anchor's horizontal edge.
fn preferred_pos(anchor: SlotAnchor, size: egui::Vec2, avail: egui::Rect) -> egui::Pos2 {
    let x = match anchor {
        SlotAnchor::Right => avail.right() - size.x - MARGIN,
        SlotAnchor::Left => avail.left() + MARGIN,
        SlotAnchor::CenterLeft => avail.left() + (avail.width() - size.x) * CENTER_LEFT_FRACTION,
    };
    egui::pos2(x.max(avail.left() + MARGIN), avail.top() + MARGIN)
}

/// Find a spot for a `size` window near `preferred` that overlaps none
/// of `taken` and stays inside `avail`. Candidate order is what makes
/// the common layouts read well: below the open windows first (a second
/// right-anchored panel stacks into a column), then beside them by
/// horizontal proximity. When the screen is genuinely full, cascade
/// diagonally so the newcomer at least doesn't superimpose exactly.
fn resolve_overlaps(
    size: egui::Vec2,
    preferred: egui::Pos2,
    taken: &[egui::Rect],
    avail: egui::Rect,
) -> egui::Pos2 {
    let rect_at = |p: egui::Pos2| egui::Rect::from_min_size(p, size);
    // Shrink so rects that merely share a margin-wide edge don't count
    // as overlapping.
    let free = |r: egui::Rect| taken.iter().all(|t| !t.intersects(r.shrink(0.5)));
    let fits = |r: egui::Rect| avail.contains_rect(r);

    if free(rect_at(preferred)) {
        return preferred;
    }

    let mut below: Vec<egui::Pos2> = taken
        .iter()
        .map(|t| egui::pos2(preferred.x, t.bottom() + MARGIN))
        .collect();
    below.sort_by(|a, b| a.y.total_cmp(&b.y));
    let mut beside: Vec<egui::Pos2> = taken
        .iter()
        .flat_map(|t| {
            [
                egui::pos2(t.right() + MARGIN, preferred.y),
                egui::pos2(t.left() - size.x - MARGIN, preferred.y),
            ]
        })
        .collect();
    beside.sort_by(|a, b| {
        (a.x - preferred.x)
            .abs()
            .total_cmp(&(b.x - preferred.x).abs())
    });

    for candidate in below.into_iter().chain(beside) {
        let rect = rect_at(candidate);
        if fits(rect) && free(rect) {
            return candidate;
        }
    }

    // No free spot: cascade to the last in-bounds diagonal offset so
    // the overlap is at least a readable stack, not a superimposition.
    // Step horizontally TOWARD the screen center — a right-anchored
    // window cascading further right would leave the screen on step one.
    let dx = if preferred.x > avail.center().x {
        -CASCADE_STEP
    } else {
        CASCADE_STEP
    };
    let mut last_in_bounds = preferred;
    let mut p = preferred;
    for _ in 0..CASCADE_TRIES {
        p += egui::vec2(dx, CASCADE_STEP);
        let rect = rect_at(p);
        if !fits(rect) {
            break;
        }
        last_in_bounds = p;
        if free(rect) {
            return p;
        }
    }
    last_in_bounds
}

/// #1280, as a law rather than two fixes: no window may size a scroll
/// area by subtracting a guessed footer height.
///
/// `available_height() - <anything>` is the signature of the defect —
/// the number on the right is a prediction of how tall the widgets
/// BELOW the scroll area will turn out to be, and every one of those
/// predictions is wrong the day somebody adds a line. Chat's was 44 pt
/// and two lines were added under it; the Inventory's was 80 pt with a
/// wrapping error line one failure away from breaking it.
///
/// [`bottom_anchored`] + [`fill_above`] measure the footer instead, so
/// there is a supported way to do this and the scan can be strict.
#[cfg(test)]
mod reserve_scan {
    /// Every `.rs` under `src/ui`.
    fn ui_sources() -> Vec<std::path::PathBuf> {
        let mut found = Vec::new();
        let mut stack = vec![std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui")];
        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("src/ui is readable") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    found.push(path);
                }
            }
        }
        found
    }

    #[test]
    fn no_window_sizes_a_scroll_area_by_subtracting_a_guessed_reserve() {
        let sources = ui_sources();
        assert!(sources.len() > 20, "the walk found no UI sources");
        let mut offenders = Vec::new();
        for path in &sources {
            // This file is where the defect is defined, deliberately
            // REPRODUCED (`growth::the_guessed_reserve_is_what_grows` is
            // the negative control) and scanned for.
            if path.ends_with("layout.rs") {
                continue;
            }
            let source = std::fs::read_to_string(path).expect("UI source is readable");
            for (i, line) in source.lines().enumerate() {
                // Comments are where the defect gets EXPLAINED — both
                // fixed sites quote the line they used to run.
                if line.trim_start().starts_with("//") {
                    continue;
                }
                if line.contains("available_height() -") {
                    offenders.push(format!(
                        "{}:{}: {}",
                        path.strip_prefix(env!("CARGO_MANIFEST_DIR"))
                            .unwrap_or(path)
                            .display(),
                        i + 1,
                        line.trim()
                    ));
                }
            }
        }
        offenders.sort();
        assert!(
            offenders.is_empty(),
            "a guessed footer reserve grows its window without bound (#1280) — \
             measure the footer with `layout::bottom_anchored` + \
             `layout::fill_above` instead:\n  {}",
            offenders.join("\n  ")
        );
    }
}

/// #1280: a window whose scroll area is sized from a GUESSED footer
/// reserve grows without bound. These drive a real (headless) egui
/// context for a run of frames and read the height the `Resize` state
/// settles on.
///
/// The negative control is the point: `the_guessed_reserve_is_what_grows`
/// reproduces the defect with the exact idiom `chat_ui` and
/// `inventory_ui` used, so the passing case and the failing case do not
/// look alike. Without it this file could assert stability against a
/// layout that was never capable of growing.
#[cfg(test)]
mod growth {
    use bevy_egui::egui;

    const SCREEN: f32 = 600.0;
    /// The guess, deliberately smaller than the footer below.
    const RESERVE: f32 = 10.0;
    /// How tall the window asks to be to begin with.
    const START_HEIGHT: f32 = 200.0;

    /// A footer that is unambiguously taller than `RESERVE` — three
    /// labels, the shape of Chat's note + input row + emote hint.
    fn footer(ui: &mut egui::Ui) {
        ui.label("one");
        ui.label("two");
        ui.label("three");
    }

    fn body(ui: &mut egui::Ui) {
        ui.label("a short scrollback");
    }

    /// Run one resizable window for `frames` frames and return the
    /// height it ended up at.
    fn settle(frames: usize, contents: impl Fn(&mut egui::Ui)) -> f32 {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, SCREEN));
        let mut height = 0.0_f32;
        for _ in 0..frames {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            // egui 0.35 renamed `Context::run` to `run_ui` and hands the
            // closure a root `Ui`; the window still goes on the context.
            let _ = ctx.run_ui(input, |ui| {
                let response = egui::Window::new("growth")
                    .default_pos(egui::Pos2::ZERO)
                    .default_size(egui::vec2(300.0, START_HEIGHT))
                    .constrain_to(screen)
                    .resizable(true)
                    .show(ui.ctx(), |ui| contents(ui));
                if let Some(r) = response {
                    height = r.response.rect.height();
                }
            });
        }
        height
    }

    /// THE DEFECT, reproduced: reserve a constant for a footer that is
    /// taller than it, claim the rest with `auto_shrink([true, false])`,
    /// and egui's `Resize` — which only ever takes the MAX of its
    /// desired size and last frame's content — walks the window to the
    /// bottom of the screen.
    #[test]
    fn the_guessed_reserve_is_what_grows() {
        let guessed = |ui: &mut egui::Ui| {
            let scroll_height = (ui.available_height() - RESERVE).max(60.0);
            egui::ScrollArea::vertical()
                .id_salt("guessed")
                .auto_shrink([true, false])
                .max_height(scroll_height)
                .show(ui, body);
            ui.separator();
            footer(ui);
        };
        let early = settle(2, guessed);
        let late = settle(40, guessed);
        assert!(
            late > early + 50.0,
            "the reserve idiom was supposed to grow: {early:.0} -> {late:.0}"
        );
        assert!(
            late > SCREEN * 0.75,
            "it should run to the screen edge, not stop somewhere: {late:.0}"
        );
    }

    /// The fix: measure the footer instead of predicting it, and the
    /// content height equals the available height on every frame, so
    /// `desired_size.max(last_content_size)` has nothing to add.
    #[test]
    fn a_measured_footer_does_not_grow_the_window() {
        let measured = |ui: &mut egui::Ui| {
            super::bottom_anchored(ui, |ui| {
                footer(ui);
                super::fill_above(ui, |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("measured")
                        .auto_shrink([true, false])
                        .show(ui, body);
                });
            });
        };
        let early = settle(4, measured);
        let late = settle(40, measured);
        assert!(
            (late - early).abs() < 1.0,
            "the window drifted between frame 4 and frame 40: {early:.1} -> {late:.1}"
        );
        assert!(
            late < SCREEN * 0.6,
            "it should still be near its default size, not filling the screen: {late:.0}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 1280x720 window with a ~30px toolbar carved off the top — the
    /// Bevy default the #833 acceptance criterion is stated against.
    fn default_avail() -> egui::Rect {
        egui::Rect::from_min_max(egui::pos2(0.0, 30.0), egui::pos2(1280.0, 720.0))
    }

    /// Simulate windows opening one at a time, each staggering around
    /// those already open — exactly what `WindowChrome::place` does
    /// with an empty persisted layout.
    fn open_in_sequence(ids: &[UiWindow], avail: egui::Rect) -> Vec<egui::Rect> {
        let mut open: Vec<egui::Rect> = Vec::new();
        for id in ids {
            let (pos, size) = place_in(id.slot(), &open, avail);
            open.push(egui::Rect::from_min_size(pos, size));
        }
        open
    }

    fn assert_layout_clean(ids: &[UiWindow], rects: &[egui::Rect], avail: egui::Rect) {
        for (i, a) in rects.iter().enumerate() {
            assert!(
                avail.contains_rect(*a),
                "{:?} at {a:?} escapes the available rect {avail:?} (under the toolbar or off-screen)",
                ids[i]
            );
            for (j, b) in rects.iter().enumerate().skip(i + 1) {
                assert!(
                    !a.intersects(b.shrink(0.5)),
                    "{:?} at {a:?} overlaps {:?} at {b:?}",
                    ids[i],
                    ids[j]
                );
            }
        }
    }

    #[test]
    fn acceptance_trio_never_overlaps_in_any_open_order() {
        // #833 acceptance: on a 1280x720 window, opening World Editor +
        // Inventory + People yields zero overlap and nothing under the
        // toolbar — in whatever order the user clicks the toggles.
        use UiWindow::{Inventory, People, WorldEditor};
        let orders: [[UiWindow; 3]; 6] = [
            [WorldEditor, Inventory, People],
            [WorldEditor, People, Inventory],
            [Inventory, WorldEditor, People],
            [Inventory, People, WorldEditor],
            [People, WorldEditor, Inventory],
            [People, Inventory, WorldEditor],
        ];
        for order in orders {
            let rects = open_in_sequence(&order, default_avail());
            assert_layout_clean(&order, &rects, default_avail());
        }
    }

    #[test]
    fn social_column_stays_clear_of_a_center_left_editor() {
        // Chat + People + Catalogue: the old absolute constants put
        // Chat over People's Mute column and the Catalogue under both.
        use UiWindow::{Catalogue, Chat, People};
        let order = [Chat, People, Catalogue];
        let rects = open_in_sequence(&order, default_avail());
        assert_layout_clean(&order, &rects, default_avail());
    }

    #[test]
    fn every_slot_spawns_below_the_toolbar_and_on_screen() {
        let avail = default_avail();
        for id in [
            UiWindow::Chat,
            UiWindow::People,
            UiWindow::Avatar,
            UiWindow::Inventory,
            UiWindow::Catalogue,
            UiWindow::WorldEditor,
            UiWindow::Diagnostics,
            UiWindow::AudioEditor,
            UiWindow::Controls,
            UiWindow::Settings,
        ] {
            let (pos, size) = place_in(id.slot(), &[], avail);
            let rect = egui::Rect::from_min_size(pos, size);
            assert!(
                rect.top() >= avail.top(),
                "{id:?} spawns under the toolbar: {rect:?}"
            );
            assert!(
                avail.contains_rect(rect),
                "{id:?} default rect {rect:?} escapes {avail:?}"
            );
        }
    }

    #[test]
    fn right_anchored_windows_hug_the_right_edge() {
        let avail = default_avail();
        let (pos, size) = place_in(UiWindow::Chat.slot(), &[], avail);
        assert_eq!(pos.x, avail.right() - size.x - MARGIN);
        assert_eq!(pos.y, avail.top() + MARGIN);
    }

    #[test]
    fn full_screen_falls_back_to_a_cascade_not_a_superimposition() {
        // One giant open window covering everything: the newcomer can't
        // find a free spot, but it must still offset off the preferred
        // position so the two title bars don't superimpose.
        let avail = default_avail();
        let wall = avail.shrink(1.0);
        let slot = UiWindow::People.slot();
        let (pos, size) = place_in(slot, &[wall], avail);
        let preferred = preferred_pos(slot.anchor, egui::vec2(slot.size[0], slot.size[1]), avail);
        assert_ne!(pos, preferred, "cascade fallback did not offset");
        assert!(avail.contains_rect(egui::Rect::from_min_size(pos, size)));
    }

    #[test]
    fn stale_live_rects_are_ignored_for_collision() {
        // Not a WindowChrome test (that needs a world) — assert the
        // constant relationship the filter depends on: a rect stamped
        // LIVE_STALE_FRAMES+1 ago must not count.
        let now: u32 = 100;
        let fresh = now - LIVE_STALE_FRAMES;
        let stale = now - LIVE_STALE_FRAMES - 1;
        assert!(now.wrapping_sub(fresh) <= LIVE_STALE_FRAMES);
        assert!(now.wrapping_sub(stale) > LIVE_STALE_FRAMES);
    }

    #[test]
    fn persisted_layout_round_trips_through_json() {
        let mut layout = WindowLayout::default();
        layout
            .rects
            .insert(UiWindow::Chat.key().to_owned(), [890.0, 40.0, 380.0, 400.0]);
        let json = serde_json::to_string(&layout).unwrap();
        let back: WindowLayout = serde_json::from_str(&json).unwrap();
        assert_eq!(back, layout);
        // A rect keyed by a window this binary doesn't know (written by
        // a newer build) survives the round trip instead of erroring.
        let newer: WindowLayout =
            serde_json::from_str(r#"{"rects":{"holo_deck":[1.0,2.0,3.0,4.0]}}"#).unwrap();
        assert_eq!(newer.rects["holo_deck"], [1.0, 2.0, 3.0, 4.0]);
    }
}
