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
//!   from `ctx.available_rect()` (which the toolbar has already carved,
//!   because the toolbar system is chained first) the first time it
//!   opens. Social panels anchor right, diagnostics left, the big
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
            Self::Controls => (Right, [300.0, 280.0]),
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
}

impl WindowChrome<'_> {
    /// Default position + size for `id`: the persisted rect when this
    /// machine has one, otherwise the slot default staggered around the
    /// currently-open windows. Cheap to call every frame — egui only
    /// consumes `default_pos`/`default_size` on a window's first show.
    pub fn place(&self, id: UiWindow, ctx: &egui::Context) -> (egui::Pos2, egui::Vec2) {
        let avail = ctx.available_rect();
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
