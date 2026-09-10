//! The Inventory's selected-item surface (#1301): which stash item is
//! selected, the row that selects it, and the pane beside the list that
//! pictures and describes it.
//!
//! The owner's decision, not to be re-litigated: a click selects a row;
//! the selected row EXPANDS in place to carry that item's controls; a pane
//! BESIDE the list, like the Catalogue's, carries the picture and the
//! facts. Hover-only and a pane below the list were both considered and
//! closed.
//!
//! The row stays the drag source it always was. egui decides between the
//! two gestures itself: `could_any_button_be_click` is false once the
//! pointer has moved past `max_click_dist`, and on a click-and-drag sense
//! `drag_started` needs that same movement — so a press that becomes a
//! drag never selects, and reading `clicked()` costs the drag nothing.
//! [`stash_row`] is a widget function precisely so that claim can be
//! driven with synthetic pointer events rather than taken on trust.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::pds::inventory::WearMeta;
use crate::pds::{Generator, GeneratorKind, InventoryRecord};

use super::{DropSource, PendingGeneratorDrop};

/// Which stash item the Inventory window has selected, and when (#1301).
///
/// A RESOURCE rather than a field of the window's `Local` editor state,
/// for the two things a `Local` cannot do. `ui::catalogue::
/// mirror_preview_request` reads it in `PreUpdate` to decide what the item
/// preview stages, and a `Local` is invisible outside its system. And
/// logout must be able to reach it: a name is a claim about ONE user's
/// stash, and #1140 found the chat draft living in a `Local` nothing could
/// scrub. [`crate::ui::catalogue::CatalogueBrowser`] is the shape; this one
/// has a real [`Self::select`] because the row click is a real caller.
///
/// Items are keyed by NAME — the PDS rkey is derived from it — so the
/// selection has to FOLLOW a rename ([`Self::follow_rename`]) and DROP
/// when the name stops resolving ([`Self::is_stale`]): a delete, a Load or
/// Reset of the whole stash, a reload after a degraded fetch.
#[derive(Resource, Default, Debug)]
pub struct InventoryBrowser {
    selected: Option<String>,
    /// When [`Self::selected`] was picked, in `Time::elapsed_secs_f64`
    /// seconds — the item preview's tie-break against the Catalogue's pick
    /// ([`crate::item_preview::wanted_subject`]).
    picked_at: f64,
}

impl InventoryBrowser {
    /// The selected item's name, if any.
    pub fn selected_name(&self) -> Option<&str> {
        self.selected.as_deref()
    }

    /// When the selection was made.
    pub fn picked_at(&self) -> f64 {
        self.picked_at
    }

    /// Whether `name` is the selected item.
    pub fn is_selected(&self, name: &str) -> bool {
        self.selected.as_deref() == Some(name)
    }

    /// Select `name`, stamped `now`. Re-selecting the selected row
    /// re-stamps it, so it takes the stage back from a later Catalogue
    /// pick — a click is a pick.
    pub fn select(&mut self, name: &str, now: f64) {
        self.selected = Some(name.to_string());
        self.picked_at = now;
    }

    /// Carry the selection across a rename of `from` to `to`. The stamp is
    /// kept: renaming is not picking.
    pub fn follow_rename(&mut self, from: &str, to: &str) {
        if self.is_selected(from) {
            self.selected = Some(to.to_string());
        }
    }

    /// Whether the selection names something the stash can no longer be
    /// selected for — gone, or unreadable by this build (a reload can put
    /// a newer client's item under the same name, and an unreadable row is
    /// not selectable). Asked through `Deref` every frame; [`Self::clear`]
    /// is called only when it answers yes, so the resource's tick moves on
    /// real changes alone (#879).
    pub fn is_stale(&self, stash: &InventoryRecord) -> bool {
        self.selected.as_deref().is_some_and(|name| {
            stash
                .generators
                .get(name)
                .is_none_or(|g| matches!(g.kind, GeneratorKind::Unknown))
        })
    }

    /// Drop the selection.
    pub fn clear(&mut self) {
        self.selected = None;
    }
}

/// How a stash row answers the pointer (#1301).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    /// Point-placeable: click to select, drag to place or to gift.
    Placeable,
    /// Terrain or water: click to select — its Rename and delete live in
    /// the expansion — and no drag sense, because a release would be
    /// refused (#832).
    RoomScoped,
}

/// What the pointer did to a row this frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RowGesture {
    pub(crate) clicked: bool,
    pub(crate) drag_started: bool,
    pub(crate) dragged: bool,
}

impl RowGesture {
    pub(crate) fn of(response: &egui::Response) -> Self {
        Self {
            clicked: response.clicked(),
            drag_started: response.drag_started(),
            dragged: response.dragged(),
        }
    }

    /// Whether [`apply_row_gesture`] has anything to write. The caller
    /// asks this first so the selection and drag resources are borrowed
    /// mutably only on a real gesture, never once per row per frame.
    pub(crate) fn acts(self) -> bool {
        self.clicked || self.drag_started
    }
}

/// Draw one stash row: the item's name as a full-width selectable row,
/// with its tag at the right (#1301).
///
/// The tag is the kind, or the wearable's socket, or the room-scoped note
/// — the facets [`super::row_matches`] searches, which is why they stay on
/// the row and the pane does not repeat them. Returns the response; see
/// [`RowGesture::of`] and [`apply_row_gesture`] for what it means.
pub(crate) fn stash_row(
    ui: &mut egui::Ui,
    name: &str,
    tag: &str,
    kind: RowKind,
    selected: bool,
) -> egui::Response {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    let (label, sense) = match kind {
        // The handle glyph and the grab cursor make the row read as
        // draggable (#832).
        RowKind::Placeable => (format!("☰ {name}"), egui::Sense::click_and_drag()),
        RowKind::RoomScoped => (name.to_string(), egui::Sense::click()),
    };
    let response = ui.add(
        egui::Button::selectable(selected, label)
            .right_text(egui::RichText::new(tag).small().color(weak))
            .truncate()
            .min_size(egui::vec2(ui.available_width(), 0.0))
            .sense(sense),
    );
    match kind {
        RowKind::Placeable => response.on_hover_cursor(egui::CursorIcon::Grab),
        RowKind::RoomScoped => response,
    }
}

/// Apply a row's gesture: a click selects, a drag start arms the shared
/// drop bus. egui never reports both for one press (see the module
/// header), and the drag tooltip is the caller's, because it needs to
/// know whose room this is.
pub(crate) fn apply_row_gesture(
    gesture: RowGesture,
    name: &str,
    browser: &mut InventoryBrowser,
    pending: &mut PendingGeneratorDrop,
    now: f64,
) {
    if gesture.clicked {
        browser.select(name, now);
    }
    if gesture.drag_started {
        pending.generator_name = Some(name.to_string());
        pending.source = DropSource::Inventory;
    }
}

/// Why an item has no picture, when it has none: the sentence the pane
/// shows under the placeholder tile instead of leaving it blank.
pub(crate) fn no_picture_reason(generator: &Generator) -> Option<&'static str> {
    match generator.kind {
        GeneratorKind::Terrain(_) | GeneratorKind::Water { .. } => {
            Some("Terrain and water shape a whole region, so there is no single object to picture.")
        }
        GeneratorKind::Unknown => {
            Some("This build cannot read this item, so there is nothing to picture.")
        }
        _ => None,
    }
}

/// What the pane says about an item below its picture and name (#1301).
///
/// Only what the row does NOT already show: the row carries the kind and
/// the wearable's socket because those are the search facets, and no fact
/// is printed in two places. Everything here is answered by the record as
/// it stands — nothing is serialised to find a size.
pub(crate) fn pane_facts(
    generator: &Generator,
    wear: Option<&WearMeta>,
    worn: bool,
) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(reason) = no_picture_reason(generator) {
        facts.push(reason.to_string());
    }
    if let Some(meta) = wear {
        facts.push(String::from(if worn { "Worn now." } else { "Not worn." }));
        if meta.fit_band_mm != 0 {
            facts.push(String::from("Sizes itself to the wearer's head."));
        }
    }
    let parts = crate::ui::room::caps::node_count(generator);
    facts.push(format!(
        "{parts} {}",
        crate::text::plural(parts, "part", "parts")
    ));
    facts
}

/// The item the pane describes.
pub(crate) struct PaneItem<'a> {
    pub(crate) name: &'a str,
    pub(crate) generator: &'a Generator,
    pub(crate) wear: Option<&'a WearMeta>,
    pub(crate) worn: bool,
}

/// The pane beside the list: the picture, the name, the facts — or the
/// sentence that says to select something. Never blank.
pub(crate) fn draw_pane(
    ui: &mut egui::Ui,
    item: Option<PaneItem<'_>>,
    preview: Option<&crate::item_preview::ItemPreview>,
) {
    let weak = crate::ui::theme::current(ui.ctx()).text_weak;
    let Some(item) = item else {
        ui.add_space(8.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new("Select an item to see its details.")
                    .italics()
                    .color(weak),
            )
            .wrap(),
        );
        return;
    };
    // The tile is drawn for a room-scoped item too, which is never staged:
    // the pane keeps one shape whatever is selected, so nothing below it
    // jumps between items.
    crate::ui::item_picture::draw_preview(
        ui,
        preview,
        crate::ui::item_picture::PictureOf::Inventory(item.name),
        crate::ui::item_picture::INVENTORY_SIDE,
    );
    ui.add(egui::Label::new(egui::RichText::new(item.name).strong()).wrap());
    for fact in pane_facts(item.generator, item.wear, item.worn) {
        ui.add(egui::Label::new(egui::RichText::new(fact).small().color(weak)).wrap());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stash_of(names: &[&str]) -> InventoryRecord {
        let mut stash = InventoryRecord::default();
        for name in names {
            stash.put_item((*name).to_string(), Generator::default_cuboid(), None);
        }
        stash
    }

    /// #1301. Items are keyed by NAME, so the selection is a claim about a
    /// key that a rename moves and a delete removes. The sequence: select
    /// "lantern", rename it — the pane must still describe it, under the
    /// new name — then delete it, and the pane must stop describing
    /// anything rather than a key the stash no longer holds.
    #[test]
    fn a_rename_carries_the_selection_and_a_delete_drops_it() {
        let mut stash = stash_of(&["lantern", "bench"]);
        let mut browser = InventoryBrowser::default();
        browser.select("lantern", 4.0);

        assert!(stash.rename_item("lantern", "ship lantern".to_string()));
        browser.follow_rename("lantern", "ship lantern");
        assert_eq!(browser.selected_name(), Some("ship lantern"));
        assert!(!browser.is_stale(&stash), "the renamed item still resolves");
        assert_eq!(browser.picked_at(), 4.0, "a rename is not a pick");

        // A rename of some OTHER item leaves the selection alone.
        assert!(stash.rename_item("bench", "stool".to_string()));
        browser.follow_rename("bench", "stool");
        assert_eq!(browser.selected_name(), Some("ship lantern"));

        assert!(stash.remove_item("ship lantern").is_some());
        assert!(browser.is_stale(&stash), "a deleted name is stale");
        browser.clear();
        assert_eq!(browser.selected_name(), None);
        assert!(!browser.is_stale(&stash), "nothing selected is never stale");

        // A Reset empties the stash under a live selection.
        browser.select("stool", 5.0);
        assert!(browser.is_stale(&InventoryRecord::default()));
    }

    /// #1301. A reload can replace an item with one this build cannot
    /// decode under the same name, and an unreadable row is not
    /// selectable — so the selection must not survive onto it.
    #[test]
    fn a_selection_that_becomes_unreadable_is_stale() {
        let mut stash = stash_of(&["lantern"]);
        let mut browser = InventoryBrowser::default();
        browser.select("lantern", 1.0);
        assert!(!browser.is_stale(&stash));
        stash.generators.get_mut("lantern").expect("stocked").kind = GeneratorKind::Unknown;
        assert!(browser.is_stale(&stash));
    }

    /// #1301. The pane is never blank: a room-scoped or unreadable item
    /// says why it has no picture, and every item says how many parts it
    /// has — a fact the record answers without being serialised.
    #[test]
    fn the_pane_says_why_there_is_no_picture() {
        let cuboid = Generator::default_cuboid();
        assert_eq!(no_picture_reason(&cuboid), None, "a prop has a picture");
        let facts = pane_facts(&cuboid, None, false);
        assert_eq!(facts, vec!["1 part".to_string()]);

        let unknown = Generator {
            kind: GeneratorKind::Unknown,
            ..Default::default()
        };
        assert!(no_picture_reason(&unknown).is_some());
        assert_eq!(
            pane_facts(&unknown, None, false)[0],
            no_picture_reason(&unknown).expect("reason").to_string()
        );
        for (kind, what) in [("Terrain", "terrain"), ("Water", "water")] {
            let generator = Generator {
                kind: crate::ui::room::construct::make_default_for_kind(kind),
                ..Default::default()
            };
            assert!(
                no_picture_reason(&generator).is_some(),
                "{what} is room-scoped and must say so"
            );
        }

        let worn_hat = WearMeta {
            socket: String::from("head"),
            fit_band_mm: 178,
            offset: crate::pds::TransformData::default(),
        };
        let facts = pane_facts(&cuboid, Some(&worn_hat), true);
        assert!(facts.contains(&"Worn now.".to_string()));
        assert!(facts.contains(&"Sizes itself to the wearer's head.".to_string()));
        let facts = pane_facts(&cuboid, Some(&worn_hat), false);
        assert!(facts.contains(&"Not worn.".to_string()));
    }

    /// Drive one row across frames with synthetic pointer events, applying
    /// each frame's gesture exactly as `inventory_ui` does. Returns the
    /// row's rect, measured on a first, event-free frame.
    struct RowHarness {
        ctx: egui::Context,
        frame: u32,
        browser: InventoryBrowser,
        pending: PendingGeneratorDrop,
    }

    impl RowHarness {
        fn new() -> Self {
            Self {
                ctx: egui::Context::default(),
                frame: 0,
                browser: InventoryBrowser::default(),
                pending: PendingGeneratorDrop::default(),
            }
        }

        fn frame(&mut self, kind: RowKind, events: Vec<egui::Event>) -> egui::Rect {
            self.frame += 1;
            let now = f64::from(self.frame) / 60.0;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 200.0),
                )),
                time: Some(now),
                events,
                ..Default::default()
            };
            let mut rect = egui::Rect::NOTHING;
            let (browser, pending) = (&mut self.browser, &mut self.pending);
            let _ = self.ctx.run_ui(input, |ui| {
                let selected = browser.is_selected("lantern");
                let response = stash_row(ui, "lantern", "prim", kind, selected);
                rect = response.rect;
                let gesture = RowGesture::of(&response);
                if gesture.acts() {
                    apply_row_gesture(gesture, "lantern", browser, pending, now);
                }
            });
            rect
        }
    }

    fn button(pos: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// #1301. A press and a release in place is a click: it selects, and
    /// it must NOT arm the drop bus — an armed bus with the button up is a
    /// release `handle_generator_drop` would act on.
    #[test]
    fn a_click_selects_the_row_and_arms_no_drag() {
        let mut row = RowHarness::new();
        let at = row.frame(RowKind::Placeable, vec![]).center();
        row.frame(RowKind::Placeable, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::Placeable, vec![button(at, true)]);
        row.frame(RowKind::Placeable, vec![button(at, false)]);
        assert_eq!(
            row.browser.selected_name(),
            Some("lantern"),
            "a click selects"
        );
        assert_eq!(
            row.pending.generator_name, None,
            "a click must not arm the drop bus"
        );
    }

    /// #1301. A press that moves is a drag: it arms the drop bus as it
    /// always did, and it must NOT select — the click the owner added is
    /// not allowed to cost the drag anything, including a selection the
    /// user did not ask for on the way to placing an item.
    #[test]
    fn a_drag_arms_the_drop_and_selects_nothing() {
        let mut row = RowHarness::new();
        let at = row.frame(RowKind::Placeable, vec![]).center();
        row.frame(RowKind::Placeable, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::Placeable, vec![button(at, true)]);
        for step in 1..=4 {
            let to = at + egui::vec2(12.0 * step as f32, 0.0);
            row.frame(RowKind::Placeable, vec![egui::Event::PointerMoved(to)]);
        }
        assert_eq!(
            row.pending.generator_name.as_deref(),
            Some("lantern"),
            "the drag arms the drop bus"
        );
        assert_eq!(row.pending.source, DropSource::Inventory);
        let released = at + egui::vec2(48.0, 0.0);
        row.frame(RowKind::Placeable, vec![button(released, false)]);
        assert_eq!(
            row.browser.selected_name(),
            None,
            "a drag, released anywhere, is not a click"
        );
    }

    /// #1301. A room-scoped row selects on a click, so its Rename and
    /// delete are reachable, and a drag on it arms nothing — the release
    /// would be refused, which is why it never had a drag sense (#832).
    #[test]
    fn a_room_scoped_row_selects_but_never_arms_a_drag() {
        let mut row = RowHarness::new();
        let at = row.frame(RowKind::RoomScoped, vec![]).center();
        row.frame(RowKind::RoomScoped, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::RoomScoped, vec![button(at, true)]);
        for step in 1..=4 {
            let to = at + egui::vec2(12.0 * step as f32, 0.0);
            row.frame(RowKind::RoomScoped, vec![egui::Event::PointerMoved(to)]);
        }
        assert_eq!(row.pending.generator_name, None, "no drag sense, no drag");

        let mut row = RowHarness::new();
        let at = row.frame(RowKind::RoomScoped, vec![]).center();
        row.frame(RowKind::RoomScoped, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::RoomScoped, vec![button(at, true)]);
        row.frame(RowKind::RoomScoped, vec![button(at, false)]);
        assert_eq!(row.browser.selected_name(), Some("lantern"));
    }
}
