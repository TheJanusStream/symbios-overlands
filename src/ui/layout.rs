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
/// (#1280). Call this FIRST, then [`fill_above`] for the body.
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
/// # Why it is a bottom panel, after two attempts that were not
///
/// The first two versions of this ran the footer inside a **bottom-up
/// `Ui`** so its height would be measured rather than guessed. That
/// measures correctly and anchors wrongly, and #1285 is what wrongly
/// looks like: `Ui::with_layout` hands its child the parent's whole
/// available rect, so a top-down child of a bottom-up parent draws from
/// the TOP of the window while the parent merely *accounts* for its
/// height at the bottom. Chat and the Inventory drew their footer over
/// their own scrollback at the top of the window with a dead half below,
/// and the growth guard could not see it because the window's total size
/// was right the whole time. Writing the footer straight into the
/// bottom-up `Ui` anchors correctly but emits it bottom-first, which is
/// how the Inventory's status line ended up above the Save row it
/// reports on (#1282).
///
/// `egui::Panel::bottom` is the tool that does both: it measures its
/// content, reserves that height against the parent's bottom edge, lays
/// the content out top-down in ordinary reading order, and shrinks
/// `available_rect` for everything after it. It also draws the dividing
/// line, so [`fill_above`] no longer needs a separator of its own.
///
/// `id_salt` is hashed with the calling `Ui`'s id, so two windows using
/// this cannot collide; `Frame::NONE` because a panel's default frame
/// paints `panel_fill`, which inside a window is a differently-coloured
/// strip along the bottom.
///
/// ```ignore
/// layout::footer(ui, "chat_footer", |ui| {
///     // in ordinary reading order, top to bottom
///     if let Some(note) = .. { ui.label(note); }
///     ui.horizontal(|ui| { .. });  // the input row
///     ui.small(hint_line());
/// });
/// layout::fill_above(ui, |ui| {
///     egui::ScrollArea::vertical()
///         .auto_shrink([true, false])
///         .show(ui, |ui| { .. });
/// });
/// ```
///
/// Two calls rather than one taking both closures: the two halves of a
/// chat window touch the same state (the footer sends a message, the
/// body renders the history), and two closures alive at once cannot both
/// borrow it.
pub fn footer<R>(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash + std::fmt::Debug,
    contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    egui::Panel::bottom(ui.id().with(id_salt))
        .frame(egui::Frame::NONE)
        .show(ui, contents)
        .inner
}

/// The scrollable remainder above a [`footer`], as a normal top-down
/// `Ui` sized to exactly what the footer left.
///
/// **Do not set `max_height` on a scroll area inside `body`** — that is
/// the guess this pair exists to delete (see [`footer`]).
pub fn fill_above<R>(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // No separator here: the bottom panel draws the dividing line
    // itself, and a second one left a doubled rule (#1285).
    //
    // The remainder, claimed as a FIXED rect (#1282). `with_layout`
    // advances the parent's cursor by the CHILD's `min_rect`, so a body
    // that reports more than the space it was handed — a `ScrollArea`
    // hitting its `min_scrolled_size` floor, a row wider than the
    // window, anything carrying a minimum of its own — passes that
    // excess up to `Resize`, which takes the max and never gives it
    // back. Advancing by the rect we MEANT to give makes the fixed
    // point a guarantee instead of something that happens to hold for
    // the bodies we tried.
    //
    // Back to top-down inside it: a bottom-up `Ui` would emit a
    // scrollback in reverse, and everything in a scroll area wants the
    // ordinary reading direction.
    let rect = ui.available_rect_before_wrap();
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect)
            .layout(egui::Layout::top_down(egui::Align::Min)),
    );
    let out = body(&mut child);
    ui.advance_cursor_after_rect(rect);
    out
}

/// A persisted rect, nudged onto the current screen — or `None` when it
/// cannot be made to fit and the computed layout should run again
/// (#1261 f45).
///
/// Sliding is enough for the common case (a window parked near the right
/// edge of a wider display), and it preserves the arrangement the user
/// made, which is the whole point of persisting it. A rect that is
/// simply too BIG for this screen cannot be preserved — shrinking it
/// would invent a size the user never chose — so that one falls through
/// to `place_in` and re-tidies.
fn fit_to_screen(
    pos: egui::Pos2,
    size: egui::Vec2,
    avail: egui::Rect,
) -> Option<(egui::Pos2, egui::Vec2)> {
    if size.x > avail.width() || size.y > avail.height() {
        return None;
    }
    let x = pos.x.clamp(avail.left(), avail.right() - size.x);
    let y = pos.y.clamp(avail.top(), avail.bottom() - size.y);
    Some((egui::pos2(x, y), size))
}

/// Gap between a computed window rect and its neighbours / the screen
/// edges. Matches the ~10px the old absolute constants used.
const MARGIN: f32 = 10.0;

/// The narrowest a browsing list may be drawn beside a detail pane: the
/// Catalogue's tree floor, and the floor #1301's Inventory list is
/// measured against (`tests::the_inventory_list_keeps_the_catalogue_floor_beside_its_picture`).
pub(crate) const LIST_MIN_WIDTH: f32 = 180.0;

/// The spacing egui 0.35 gives a `Separator`. Not a style field — it is
/// hard-coded in `Style::separator_style` (`widget_style.rs`) — so it is
/// restated here, and the measurement test above is what catches an egui
/// that changes it.
const SEPARATOR_SPACING: f32 = 6.0;

/// A list with a fixed-width pane beside it, separated by a rule (#1301):
/// the Inventory's master-detail split. The pane gets `pane_width`; the list
/// gets everything else, never less than nothing.
///
/// Both regions are top-down and take the full remaining height, the
/// Catalogue's idiom (a region inheriting the horizontal flow lays its
/// children out side by side). Returns what `list` returned.
pub(crate) fn beside_pane<R>(
    ui: &mut egui::Ui,
    pane_width: f32,
    list: impl FnOnce(&mut egui::Ui) -> R,
    pane: impl FnOnce(&mut egui::Ui),
) -> R {
    let gap = SEPARATOR_SPACING + 2.0 * ui.spacing().item_spacing.x;
    let list_width = (ui.available_width() - pane_width - gap).max(0.0);
    ui.horizontal_top(|ui| {
        let height = ui.available_height();
        let out = ui
            .allocate_ui_with_layout(
                egui::vec2(list_width, height),
                egui::Layout::top_down(egui::Align::Min),
                list,
            )
            .inner;
        ui.separator();
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), height),
            egui::Layout::top_down(egui::Align::Min),
            pane,
        );
        out
    })
    .inner
}

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
    ///
    /// The Inventory's WIDTH is at its ceiling (#1301). The World Editor is
    /// `CenterLeft` at x = (1280 - 820) × 0.25 = 115, so its right edge is
    /// 935; opened first, it leaves the Inventory one free spot, beside it
    /// at 945, and 1280 - 945 = 335. Swept through the trio test's six
    /// orders: clean to 335, red at 336. The width buys the list-and-pane
    /// split its list — see `ui::item_picture::INVENTORY_SIDE`.
    pub fn slot(self) -> Slot {
        use SlotAnchor::*;
        let (anchor, size) = match self {
            Self::Chat => (Right, [380.0, 400.0]),
            Self::People => (Right, [280.0, 280.0]),
            Self::Inventory => (Right, [335.0, 340.0]),
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
    /// machine has one **that still fits this screen**, otherwise the slot
    /// default staggered around the currently-open windows. Cheap to call
    /// every frame — egui only consumes `default_pos`/`default_size` on a
    /// window's first show.
    ///
    /// The fit check is #1261 f45. A persisted rect used to short-circuit
    /// unconditionally, which meant the whole #833 staggering machinery —
    /// and everything its acceptance tests exercise — was dead for every
    /// window after its first appearance on a machine. Undock a laptop
    /// from a 4K monitor and the rects that were tidy at 3840x2160 arrive
    /// off the side of a 1280x720 screen, with "delete prefs.json" as the
    /// only recovery. `constrain_to` at each call site clamps position
    /// but never size, so a window sized on the big display keeps that
    /// size on the small one.
    pub fn place(&self, id: UiWindow, ctx: &egui::Context) -> (egui::Pos2, egui::Vec2) {
        let avail = self.available_rect(ctx);
        if let Some(&[x, y, w, h]) = self.layout.rects.get(id.key())
            && let Some(fitted) = fit_to_screen(egui::pos2(x, y), egui::vec2(w, h), avail)
        {
            return fitted;
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

    /// Forget every persisted rect, so the next open of each window runs
    /// the computed staggering again (#1261 f45).
    ///
    /// The #833 guarantee — open World Editor, Inventory and People in
    /// any order and get zero overlap — held only until each window had
    /// been shown once, because [`remember`](Self::remember) writes a
    /// rect on the very first frame and [`place`](Self::place) returns it
    /// thereafter. From then on a machine inherits whatever geometry its
    /// first session happened to produce, and the recovery path was
    /// "delete prefs.json". This is the recovery path.
    ///
    /// Returns whether anything was actually forgotten, so the caller can
    /// say so — and so a click on an already-tidy layout does not dirty
    /// the prefs resource. The live open-set is deliberately untouched:
    /// it is this frame's fact about which windows are up, and clearing
    /// it would make the re-tidy stagger around nothing.
    pub fn reset_layout(&mut self) -> bool {
        if self.layout.rects.is_empty() {
            return false;
        }
        self.layout.rects.clear();
        true
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
/// [`footer`] + [`fill_above`] measure the footer instead, so
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
             measure the footer with `layout::footer` + \
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

    /// #1282: the cap holds even when the BODY asks for more than it was
    /// given.
    ///
    /// The other half of #1280's fix, and the half that took a second
    /// pass. Measuring the footer makes content height equal available
    /// height *for a well-behaved body* — but a `ScrollArea` at its
    /// `min_scrolled_size` floor, a row wider than the window, or any
    /// widget with a minimum of its own reports more than it was handed,
    /// and `with_layout` passed that straight up to `Resize`, which
    /// takes the max and never gives it back. `fill_above` now advances
    /// the parent by the rect it MEANT to give.
    ///
    /// The body here is deliberately absurd — a rect four times the
    /// window's height — because the guarantee has to be structural, not
    /// a property of the bodies that happen to be in the app today.
    #[test]
    fn an_oversized_body_cannot_grow_the_window() {
        let greedy = |ui: &mut egui::Ui| {
            super::footer(ui, "greedy_footer", footer);
            super::fill_above(ui, |ui| {
                ui.allocate_space(egui::vec2(50.0, SCREEN * 4.0));
            });
        };
        let early = settle(4, greedy);
        let late = settle(40, greedy);
        assert!(
            (late - early).abs() < 1.0,
            "a greedy body walked the window: {early:.1} -> {late:.1}"
        );
        assert!(
            late < SCREEN * 0.6,
            "the window followed its content off the screen: {late:.0}"
        );
    }

    /// The fix: measure the footer instead of predicting it, and the
    /// content height equals the available height on every frame, so
    /// `desired_size.max(last_content_size)` has nothing to add.
    #[test]
    fn a_measured_footer_does_not_grow_the_window() {
        let measured = |ui: &mut egui::Ui| {
            super::footer(ui, "measured_footer", footer);
            super::fill_above(ui, |ui| {
                egui::ScrollArea::vertical()
                    .id_salt("measured")
                    .auto_shrink([true, false])
                    .show(ui, body);
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

    /// #1261 f45: a persisted rect is honoured while it still fits, and
    /// falls through to the computed layout when it does not.
    ///
    /// THE SEQUENCE: arrange the windows on a docked 3840x2160 monitor,
    /// undock, open them on the laptop. Before this, `place` returned the
    /// stored rect unconditionally, so they arrived off the side of the
    /// screen — and `constrain_to` at each call site clamps position but
    /// never size, so a window sized on the big display kept that size.
    #[test]
    fn a_persisted_rect_that_no_longer_fits_gives_way_to_the_computed_layout() {
        let avail = default_avail();

        // Inside the screen already: returned untouched, because the
        // arrangement the user made is the whole point of persisting it.
        let (pos, size) = fit_to_screen(egui::pos2(300.0, 100.0), egui::vec2(400.0, 300.0), avail)
            .expect("a rect that fits is kept");
        assert_eq!(pos, egui::pos2(300.0, 100.0));
        assert_eq!(size, egui::vec2(400.0, 300.0));

        // Off the right edge of a narrower screen: SLID back on, size
        // intact. Sliding preserves the arrangement; re-tidying would
        // throw it away for a window that only needed nudging.
        let (pos, size) = fit_to_screen(egui::pos2(3000.0, 100.0), egui::vec2(400.0, 300.0), avail)
            .expect("a rect that can be slid on is kept");
        assert_eq!(size, egui::vec2(400.0, 300.0));
        assert!(avail.contains_rect(egui::Rect::from_min_size(pos, size)));

        // Above the toolbar — the #833 defect, from the other direction.
        let (pos, _) =
            fit_to_screen(egui::pos2(300.0, 0.0), egui::vec2(400.0, 300.0), avail).expect("kept");
        assert!(pos.y >= avail.top(), "{pos:?} is under the toolbar");

        // Simply too big for this screen: `None`, so `place` re-tidies.
        // Shrinking instead would invent a size the user never chose.
        assert!(
            fit_to_screen(egui::pos2(0.0, 30.0), egui::vec2(4000.0, 300.0), avail).is_none(),
            "a rect wider than the screen cannot be preserved"
        );
        assert!(fit_to_screen(egui::pos2(0.0, 30.0), egui::vec2(400.0, 3000.0), avail).is_none());
    }

    /// #1261 f45: and there is a way to ask for the tidy-up by hand.
    ///
    /// `reset_layout` reports whether it forgot anything, which is what
    /// keeps a click on an already-tidy layout from dirtying the prefs
    /// resource and re-arming the save debounce for nothing.
    #[test]
    fn resetting_an_empty_layout_changes_nothing() {
        let mut layout = WindowLayout::default();
        assert!(layout.rects.is_empty());
        // The same shape `WindowChrome::reset_layout` runs, on the field
        // it owns — the `SystemParam` itself needs a World to build.
        let forgot = if layout.rects.is_empty() {
            false
        } else {
            layout.rects.clear();
            true
        };
        assert!(!forgot);

        layout
            .rects
            .insert(UiWindow::People.key().to_owned(), [1.0, 2.0, 3.0, 4.0]);
        assert!(!layout.rects.is_empty());
        layout.rects.clear();
        assert!(layout.rects.is_empty(), "a reset forgets every window");
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

    /// #1301. The Inventory's list keeps the Catalogue's floor beside its
    /// picture, in a real window at the slot's size, under every palette.
    ///
    /// Measured rather than added up, because the sum has three terms egui
    /// owns: `Window::default_size` is the OUTER size, so the frame's
    /// margin and stroke come off first — and the stroke is a point wider
    /// in high contrast (#1283) — and the separator's spacing is not a
    /// style field at all. The arithmetic says 335 - 14 - 22 - 112 = 187;
    /// this says what egui actually lays out.
    #[test]
    fn the_inventory_list_keeps_the_catalogue_floor_beside_its_picture() {
        use crate::ui::item_picture::INVENTORY_SIDE;
        use crate::ui::theme::Theme;

        let slot = UiWindow::Inventory.slot();
        for (palette, theme) in [
            ("dark", Theme::dark()),
            ("light", Theme::light()),
            ("high contrast", Theme::high_contrast()),
        ] {
            let ctx = egui::Context::default();
            let (mut list, mut pane) = (0.0_f32, 0.0_f32);
            for _ in 0..3 {
                let input = egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_max(
                        egui::Pos2::ZERO,
                        egui::pos2(1280.0, 720.0),
                    )),
                    ..Default::default()
                };
                let _ = ctx.run_ui(input, |ui| {
                    crate::ui::theme::apply_theme(ui.ctx(), &theme);
                    egui::Window::new("Inventory")
                        .default_pos(egui::pos2(945.0, 40.0))
                        .default_size(egui::vec2(slot.size[0], slot.size[1]))
                        .resizable(true)
                        .show(ui.ctx(), |ui| {
                            beside_pane(
                                ui,
                                INVENTORY_SIDE,
                                |ui| list = ui.available_width(),
                                |ui| pane = ui.available_width(),
                            );
                        });
                });
            }
            assert!(
                pane >= INVENTORY_SIDE - 0.5,
                "{palette}: the pane got {pane:.1}, less than its {INVENTORY_SIDE} picture"
            );
            assert!(
                list >= LIST_MIN_WIDTH,
                "{palette}: the list got {list:.1} beside the picture, under the \
                 {LIST_MIN_WIDTH} floor the Catalogue keeps"
            );
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

    /// #1261 f43: the toast stack must not open in the corner the
    /// right-anchored window column owns.
    ///
    /// The toast area is a real pointer area at `Order::Foreground` —
    /// deliberately, so a click on a toast cannot fall through to the 3D
    /// scene — which also means it eats clicks on whatever is under it.
    /// It was anchored `RIGHT_TOP` once, which is where all five of these
    /// windows open, so for the toast's full life it covered their title
    /// bars and swallowed clicks on them (#1261 f43).
    ///
    /// The stack is centred at the top now (#1286), which is a different
    /// question with the same answer: it shares the windows' vertical
    /// band, so what has to hold is HORIZONTAL separation. Checked at the
    /// smallest supported width, because that is where a centred band of
    /// [`MAX_WIDTH`](crate::config::ui::toast::MAX_WIDTH) and a
    /// right-hand column come closest.
    #[test]
    fn the_toast_stack_does_not_open_in_the_window_column() {
        use crate::config::ui::toast as toast_cfg;
        let avail = default_avail();

        let mut leftmost_window_edge = f32::INFINITY;
        for id in [
            UiWindow::Chat,
            UiWindow::People,
            UiWindow::Inventory,
            UiWindow::Controls,
            UiWindow::Settings,
        ] {
            let (pos, size) = place_in(id.slot(), &[], avail);
            assert_eq!(
                pos.y,
                avail.top() + MARGIN,
                "{id:?} does not open against the top edge any more — recheck the toast band"
            );
            assert_eq!(pos.x, avail.right() - size.x - MARGIN, "{id:?}");
            leftmost_window_edge = leftmost_window_edge.min(pos.x);
        }

        // The stack is centred and at most `MAX_WIDTH` wide, so its right
        // edge is the half-width past centre. It must stop short of the
        // nearest window in that column.
        let stack_right = avail.center().x + toast_cfg::MAX_WIDTH / 2.0;
        assert!(
            stack_right < leftmost_window_edge,
            "a centred toast stack reaches {stack_right:.0} and the window column \
             starts at {leftmost_window_edge:.0} — it would eat their title-bar clicks"
        );

        // And it starts below the toolbar, not under it: the offset is
        // measured from the PANEL-FREE top, which is what `default_avail`
        // models by starting at y=30.
        const {
            assert!(
                toast_cfg::TOP_OFFSET > 0.0,
                "the stack must sit below the panel-free top edge, not on it"
            )
        };
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

#[cfg(test)]
mod placement {
    use bevy_egui::egui;

    const SCREEN: egui::Vec2 = egui::vec2(400.0, 600.0);
    const WINDOW: egui::Vec2 = egui::vec2(300.0, 350.0);

    /// Where the footer and the body actually landed, in screen space.
    struct Landed {
        window: egui::Rect,
        content: egui::Rect,
        footer: egui::Rect,
        body: egui::Rect,
    }

    /// Run the real pair in a real window for `passes` frames and report
    /// the last frame's geometry. Several passes because a panel's size
    /// is not known until it has been laid out once.
    fn run(passes: usize) -> Landed {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN);
        let mut landed = None;
        for _ in 0..passes {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                let shown = egui::Window::new("placement")
                    .default_pos(egui::Pos2::ZERO)
                    .default_size(WINDOW)
                    .constrain_to(screen)
                    .resizable(true)
                    .show(ui.ctx(), |ui| {
                        let content = ui.max_rect();
                        let footer = super::footer(ui, "probe_footer", |ui| {
                            ui.label("a composer note");
                            ui.label("the input row");
                            ui.label("a hint line");
                            ui.min_rect()
                        });
                        let body = super::fill_above(ui, |ui| {
                            ui.label("scrollback");
                            ui.max_rect()
                        });
                        (content, footer, body)
                    });
                if let Some(shown) = shown
                    && let Some((content, footer, body)) = shown.inner
                {
                    landed = Some(Landed {
                        window: shown.response.rect,
                        content,
                        footer,
                        body,
                    });
                }
            });
        }
        landed.expect("the window was shown")
    }

    /// #1285: the footer sits against the window's BOTTOM edge, and the
    /// body gets everything above it.
    ///
    /// This is the guard the first two attempts did not have. Both of
    /// them measured the footer correctly and the growth tests passed —
    /// the window's total size was right the whole time — while the
    /// footer was drawn at the TOP of the window over the scrollback,
    /// with the bottom half of the window dead. Total size cannot see
    /// where anything is, so something has to ask.
    #[test]
    fn the_footer_sits_at_the_bottom_and_the_body_fills_above_it() {
        let l = run(4);

        assert!(
            (l.footer.bottom() - l.content.bottom()).abs() < 1.0,
            "the footer is not against the window's bottom edge: footer {:?} in content {:?}",
            l.footer,
            l.content
        );
        assert!(
            (l.body.top() - l.content.top()).abs() < 1.0,
            "the body does not start at the top of the window: body {:?} in content {:?}",
            l.body,
            l.content
        );
        assert!(
            l.body.bottom() <= l.footer.top() + 1.0,
            "the body overlaps the footer: body {:?}, footer {:?}",
            l.body,
            l.footer
        );

        // And between them they account for the whole window, so there is
        // no dead band — the visible half of #1285 was a window whose
        // lower two thirds were empty.
        let used = l.body.height() + l.footer.height();
        assert!(
            (used - l.content.height()).abs() < 6.0,
            "{:.0} pt of the window's {:.0} is unaccounted for",
            l.content.height() - used,
            l.content.height()
        );

        // The footer is a real, measured height, not a sliver: three
        // labels cannot come to nothing.
        assert!(
            l.footer.height() > 20.0,
            "footer height {}",
            l.footer.height()
        );
        assert!(
            l.window.height() <= WINDOW.y + 1.0,
            "the window grew: {}",
            l.window.height()
        );
    }

    /// Which container an anchored, auto-sized control sits in (#1290).
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Anchored {
        /// A bare `egui::Area` — the login screen's "New world" chip.
        Area,
        /// A non-resizable, title-bar-less `egui::Window` — both approach
        /// prompts.
        Window,
    }

    /// Draw an anchored, auto-sized control repeatedly and report the
    /// width its content settles at, before and after a palette switch
    /// (#1290).
    ///
    /// **The hazard this measures.** An `Area` with no explicit size hands
    /// its `Ui` a `max_rect` built from the size it MEASURED last pass —
    /// once settled, `ui.available_width()` is exactly the content's own
    /// width. That is an equilibrium with no slack in it: any pass where
    /// the content wants even one point more, a wrappable widget wraps
    /// instead of growing, the area measures NARROWER, and the next pass
    /// offers that narrower width. It only ever ratchets down, and it
    /// never recovers.
    ///
    /// The frame matters and is not decoration: its stroke and inner
    /// margin are what consume the slack, and `Theme::border_stroke_width`
    /// is the term that differs between palettes. A probe without one
    /// measures a control that has room to grow and reports no defect.
    ///
    /// Two settle passes before the switch and six after — the latch needs
    /// one pass to bite and a recovery, if there were one, would take
    /// another.
    fn anchored_width_across_a_palette_switch(
        container: Anchored,
        content: impl Fn(&mut egui::Ui) -> egui::Rect,
    ) -> (f32, f32) {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1280.0, 720.0));
        let pass = |theme: &crate::ui::theme::Theme| {
            let mut width = 0.0_f32;
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                // `apply_theme`, not `set_visuals`: the palette is read
                // back through `theme::current`, which only `apply_theme`
                // stashes in the context data map (#1284's lesson).
                crate::ui::theme::apply_theme(ui.ctx(), theme);
                // The login card's chrome, spelled out rather than shared:
                // `border_stroke_width` is the term under test and a
                // helper would hide it.
                let frame = egui::Frame::new()
                    .fill(theme.window_fill)
                    .stroke(egui::Stroke::new(theme.border_stroke_width, theme.border))
                    .inner_margin(8.0);
                match container {
                    Anchored::Area => {
                        egui::Area::new(egui::Id::new("latch-probe"))
                            .anchor(egui::Align2::RIGHT_BOTTOM, [-16.0, -16.0])
                            .show(ui.ctx(), |ui| {
                                frame.show(ui, |ui| width = content(ui).width());
                            });
                    }
                    Anchored::Window => {
                        egui::Window::new("latch-probe")
                            .title_bar(false)
                            .resizable(false)
                            .anchor(egui::Align2::CENTER_BOTTOM, [0.0, -24.0])
                            .show(ui.ctx(), |ui| width = content(ui).width());
                    }
                }
            });
            width
        };
        let mut settled = 0.0;
        for _ in 0..2 {
            settled = pass(&crate::ui::theme::Theme::dark());
        }
        let mut after = 0.0;
        for _ in 0..6 {
            after = pass(&crate::ui::theme::Theme::high_contrast());
        }
        (settled, after)
    }

    /// No anchored control collapses when the palette changes (#1290).
    ///
    /// The sequence the owner hit: switch the theme with the login
    /// screen's own picker (#1276 f39, which is what first made this
    /// reachable at all) and the "New world" chip becomes one character
    /// wide, reading "New / worl / d", and stays that way.
    ///
    /// **High contrast is the trigger and one point is the whole margin.**
    /// Every palette settles this chip at the same 70.8 pt on its own —
    /// no palette is wrong. But high contrast's `border_stroke_width` is a
    /// point wider (#1283 gave controls a real frame), so the pass that
    /// switches into it offers 69.8 pt of content width for a label that
    /// needs 70.8. One point, and the ratchet does the rest: 70.8 -> 41.6,
    /// latched, with no recovery.
    ///
    /// A palette switch stands in for the general trigger. A font swap
    /// (the lazy CJK load, #858), an interface-scale change (#1259 f239)
    /// and a longer resolved handle all perturb the width the same way;
    /// the palette is simply the one a user can now reach from the login
    /// screen in one click.
    #[test]
    fn an_anchored_control_does_not_latch_narrow_when_the_palette_changes() {
        // The control: the shape as it shipped. A wrapping button in an
        // anchored `Area` really does collapse, so the assertions below
        // are not describing a palette that happens not to perturb it.
        let (before, after) = anchored_width_across_a_palette_switch(Anchored::Area, |ui| {
            ui.add(egui::Button::new("New world")).rect
        });
        assert!(
            after < before - 1.0,
            "the control must latch: {before:.1} -> {after:.1}"
        );

        // The fix, on the site that was reported.
        let (before, after) = anchored_width_across_a_palette_switch(Anchored::Area, |ui| {
            ui.add(egui::Button::new("New world").wrap_mode(egui::TextWrapMode::Extend))
                .rect
        });
        assert!(
            after >= before - 0.01,
            "the \"New world\" chip shrank from {before:.1} to {after:.1}"
        );
    }

    /// An anchored non-resizable `egui::Window` does NOT carry the `Area`
    /// latch (#1290) — recorded so the two approach prompts are not
    /// "fixed" for a defect they never had.
    ///
    /// A `Window` wraps its `Area` in a `Resize`, which keeps its own
    /// remembered size and — unlike a bare area — lets its content ASK for
    /// more room and grows to it. That is the whole difference, and it is
    /// the same one #898 recorded from the other direction: a `ScrollArea`
    /// collapses to a slit inside an auto-sized `Area` and behaves inside
    /// a `Window`, because only one of the two has a real max rect.
    #[test]
    fn an_anchored_window_recovers_where_a_bare_area_latches() {
        let wrapping = |ui: &mut egui::Ui| ui.add(egui::Button::new("New world")).rect;
        let (area_before, area_after) =
            anchored_width_across_a_palette_switch(Anchored::Area, wrapping);
        let (win_before, win_after) =
            anchored_width_across_a_palette_switch(Anchored::Window, wrapping);
        assert!(
            area_after < area_before - 1.0,
            "the area control must latch: {area_before:.1} -> {area_after:.1}"
        );
        assert!(
            win_after >= win_before - 0.01,
            "a window is supposed to recover: {win_before:.1} -> {win_after:.1}"
        );
    }

    /// The control: the pre-#1285 idiom really did put the footer at the
    /// top, so the assertions above are not describing a coincidence.
    ///
    /// Reproduces the old shape — a top-down child of a bottom-up parent
    /// — and asserts the failure. `Ui::with_layout` hands that child the
    /// parent's whole available rect, so it draws from the rect's top
    /// while the parent accounts for its height at the bottom.
    #[test]
    fn the_old_bottom_up_idiom_drew_the_footer_at_the_top() {
        let ctx = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, SCREEN);
        let mut seen = None;
        for _ in 0..4 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let _ = ctx.run_ui(input, |ui| {
                egui::Window::new("old")
                    .default_pos(egui::Pos2::ZERO)
                    .default_size(WINDOW)
                    .constrain_to(screen)
                    .resizable(true)
                    .show(ui.ctx(), |ui| {
                        let content = ui.max_rect();
                        let footer = ui
                            .with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                                    ui.label("a composer note");
                                    ui.label("the input row");
                                    ui.label("a hint line");
                                    ui.min_rect()
                                })
                                .inner
                            })
                            .inner;
                        seen = Some((content, footer));
                    });
            });
        }
        let (content, footer) = seen.expect("shown");
        assert!(
            (footer.top() - content.top()).abs() < 1.0,
            "the old idiom is supposed to fail by drawing at the TOP; it put the \
             footer at {:?} in {:?}",
            footer,
            content
        );
    }
}
