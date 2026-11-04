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
//! `drag_started` needs that same movement - so a press that becomes a
//! drag never selects, and reading `clicked()` costs the drag nothing.
//! [`stash_row`] is a widget function precisely so that claim can be
//! driven with synthetic pointer events rather than taken on trust.
//!
//! An avatar item (#1838) is a row of its own kind ([`RowKind::Avatar`]):
//! it selects on a click and never arms a drag, because an avatar is
//! neither placed nor gifted in this slice - a drag that cannot land must
//! not start. Its expansion is [`avatar_row_controls`]: Wear, Rename and
//! delete. Its pane says what it is and how many wearables it carries, and
//! pictures a vehicle from its visuals tree; a rigged body has no picture
//! yet and says so (#1842).
//!
//! A world item (#1839) is a row of its own kind too ([`RowKind::World`]),
//! click-only for the same reason: a world is neither placed nor gifted.
//! Its expansion is [`world_row_controls`] - Load into my world, Rename and
//! delete - and its pane is a text card from the head's summary: the
//! world's name, whether it is procedural or a Berlin square, how much is in
//! it and how large it is ([`world_pane_facts`]). It has no picture, and
//! says so.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::pds::inventory::{AvatarItem, AvatarItemBody, WearMeta, WorldItem};
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
/// Items are keyed by NAME - the PDS rkey is derived from it - so the
/// selection has to FOLLOW a rename ([`Self::follow_rename`]) and DROP
/// when the name stops resolving ([`Self::is_stale`]): a delete, a Load or
/// Reset of the whole stash, a reload after a degraded fetch.
#[derive(Resource, Default, Debug)]
pub struct InventoryBrowser {
    selected: Option<String>,
    /// When [`Self::selected`] was picked, in `Time::elapsed_secs_f64`
    /// seconds - the item preview's tie-break against the Catalogue's pick
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
    /// pick - a click is a pick.
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
    /// selected for - gone, or unreadable by this build (a reload can put
    /// a newer client's item under the same name, and an unreadable row is
    /// not selectable). Asked through `Deref` every frame; [`Self::clear`]
    /// is called only when it answers yes, so the resource's tick moves on
    /// real changes alone (#879).
    pub fn is_stale(&self, stash: &InventoryRecord) -> bool {
        self.selected
            .as_deref()
            .is_some_and(|name| !stash.is_readable(name))
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
    /// Terrain or water: click to select - its Rename and delete live in
    /// the expansion - and no drag sense, because a release would be
    /// refused (#832).
    RoomScoped,
    /// A whole avatar (#1838): click to select - its Wear, Rename and
    /// delete live in the expansion - and no drag sense, because an avatar
    /// is neither placed nor gifted, and a drag that cannot land must not
    /// start.
    Avatar,
    /// A whole world (#1839): click to select - its Load, Rename and
    /// delete live in the expansion - and no drag sense, for the avatar's
    /// reason.
    World,
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
/// The tag is the kind, or the wearable's socket, or the room-scoped note -
/// the facets [`super::row_matches`] searches, which is why they stay on
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
        RowKind::RoomScoped | RowKind::Avatar | RowKind::World => {
            (name.to_string(), egui::Sense::click())
        }
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
        RowKind::RoomScoped | RowKind::Avatar | RowKind::World => response,
    }
}

/// What a click in an avatar row's expansion asks for (#1838).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AvatarRowAction {
    /// Put this avatar on, replacing the one worn.
    Wear,
    /// Open the rename dialog.
    Rename,
    /// Ask to delete the item.
    Delete,
}

/// The selected avatar row's controls (#1838): Wear - disabled with
/// `wear_blocked` on the hover when it cannot - Rename, and delete at the
/// right. A widget function so the row harness can drive it.
pub(crate) fn avatar_row_controls(
    ui: &mut egui::Ui,
    wear_blocked: Option<&str>,
) -> Option<AvatarRowAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        let wear = ui
            .add_enabled(wear_blocked.is_none(), egui::Button::new("Wear").small())
            .on_hover_text(
                "Wear this avatar instead of the one you have on - it stays unsaved \
                 until you Save in the Avatar window",
            )
            .on_disabled_hover_text(wear_blocked.unwrap_or_default());
        if wear.clicked() {
            action = Some(AvatarRowAction::Wear);
        }
        if ui
            .small_button("Rename")
            .on_hover_text("Rename this item")
            .clicked()
        {
            action = Some(AvatarRowAction::Rename);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::affordances::remove_button(ui, "Delete this item from your inventory")
                .clicked()
            {
                action = Some(AvatarRowAction::Delete);
            }
        });
    });
    action
}

/// An avatar item's row tag (#1838): that it is an avatar, and what kind -
/// a person, a beast, or the vehicle as far as the record can tell. A
/// search facet, like a generator row's kind.
pub(crate) fn avatar_tag(item: &AvatarItem) -> String {
    format!("avatar \u{b7} {}", item.kind().label())
}

/// Why an avatar item has no picture, when it has none (#1838) - in
/// [`no_picture_reason`]'s voice. A vehicle is pictured from its tree and a
/// person or a beast from its built body (#1842); a body that turns out not
/// to build says so under the tile ([`crate::ui::item_picture::BODY_NOT_BUILT`]),
/// which only the build can tell.
pub(crate) fn avatar_no_picture_reason(item: &AvatarItem) -> Option<&'static str> {
    match item.body {
        AvatarItemBody::Generator(_) | AvatarItemBody::Rigged(_) => None,
        AvatarItemBody::Unknown => {
            Some("This build cannot read this item, so there is nothing to picture.")
        }
    }
}

/// What the pane says about an avatar item (#1838): what it is, what it
/// wears, and what wearing it does - the row already shows the kind tag.
pub(crate) fn avatar_pane_facts(item: &AvatarItem) -> Vec<String> {
    let mut facts = Vec::new();
    if let Some(reason) = avatar_no_picture_reason(item) {
        facts.push(reason.to_string());
    }
    match &item.body {
        AvatarItemBody::Rigged(_) => {
            let worn = item.wearable_count();
            facts.push(match worn {
                0 => String::from("Wears nothing."),
                n => format!(
                    "Wears {n} {}.",
                    crate::text::plural(n, "wearable", "wearables")
                ),
            });
        }
        AvatarItemBody::Generator(body) => {
            let parts = crate::ui::room::caps::node_count(&body.visuals);
            facts.push(format!(
                "{parts} {}",
                crate::text::plural(parts, "part", "parts")
            ));
        }
        AvatarItemBody::Unknown => {}
    }
    facts.push(format!("Locomotion: {}.", item.locomotion.display_label()));
    facts.push(String::from(
        "Wear replaces the avatar you have on. It stays unsaved until you Save in the \
         Avatar window.",
    ));
    facts
}

/// What a click in a world row's expansion asks for (#1839).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorldRowAction {
    /// Replace the owner's world with this one (after a confirm).
    Load,
    /// Open the rename dialog.
    Rename,
    /// Ask to delete the item.
    Delete,
}

/// The selected world row's controls (#1839): Load into my world -
/// disabled with `load_blocked` on the hover when it cannot - Rename, and
/// delete at the right. A widget function so the row harness can drive it.
pub(crate) fn world_row_controls(
    ui: &mut egui::Ui,
    load_blocked: Option<&str>,
) -> Option<WorldRowAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        let load = ui
            .add_enabled(
                load_blocked.is_none(),
                egui::Button::new("Load into my world").small(),
            )
            .on_hover_text(
                "Replace everything in your world with this one, as a re-roll does - you are \
                 asked first, it can be undone, and it stays unsaved until you Save in the \
                 World Editor",
            )
            .on_disabled_hover_text(load_blocked.unwrap_or_default());
        if load.clicked() {
            action = Some(WorldRowAction::Load);
        }
        if ui
            .small_button("Rename")
            .on_hover_text("Rename this item")
            .clicked()
        {
            action = Some(WorldRowAction::Rename);
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if crate::ui::affordances::remove_button(ui, "Delete this item from your inventory")
                .clicked()
            {
                action = Some(WorldRowAction::Delete);
            }
        });
    });
    action
}

/// A world item's row tag (#1839): that it is a world, and what kind - a
/// search facet, like an avatar row's.
pub(crate) fn world_tag(item: &WorldItem) -> String {
    format!("world \u{b7} {}", item.head().summary.kind_tag())
}

/// Why a world item has no picture - in [`no_picture_reason`]'s voice.
pub(crate) const WORLD_NO_PICTURE: &str = "A world is a whole place - ground, sky and everything in it - so it is described here \
     rather than pictured. Load it to see it.";

/// What the pane says about a world item (#1839), from its head's summary
/// alone - nothing is fetched or decoded to say it: what it is called and
/// what kind of world it is, how much is in it, how large it is, and what
/// loading it does.
pub(crate) fn world_pane_facts(item: &WorldItem) -> Vec<String> {
    let head = item.head();
    let summary = &head.summary;
    let mut facts = vec![WORLD_NO_PICTURE.to_string()];
    let kind = summary.kind_label();
    let mut kind_sentence = kind[..1].to_uppercase();
    kind_sentence.push_str(&kind[1..]);
    facts.push(if summary.name.is_empty() {
        format!("{kind_sentence}.")
    } else {
        format!("\"{}\", {kind}.", summary.name)
    });
    let items = summary.generators as usize;
    let placements = summary.placements as usize;
    let mut contents = format!(
        "{items} {}, {placements} {}",
        crate::text::plural(items, "item", "items"),
        crate::text::plural(placements, "placement", "placements"),
    );
    if summary.copies > 0 {
        let copies = summary.copies as usize;
        contents.push_str(&format!(
            ", {copies} {}",
            crate::text::plural(copies, "copy", "copies")
        ));
    }
    facts.push(format!("{contents}."));
    facts.push(format!(
        "{} saved.",
        crate::pds::record_size::human_bytes(usize::try_from(head.bytes).unwrap_or(usize::MAX))
    ));
    facts.push(String::from(
        "Load into my world replaces everything in your world with it, as a re-roll does. It \
         can be undone, and stays unsaved until you Save in the World Editor.",
    ));
    facts
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
/// it stands - nothing is serialised to find a size.
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
        if meta.fit_beast_mm != 0 {
            facts.push(String::from(
                "Cut for a four-legged body, and sizes itself to it.",
            ));
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
    pub(crate) what: PaneWhat<'a>,
}

/// Which kind of item the pane describes (#1838).
pub(crate) enum PaneWhat<'a> {
    /// A generator item, with its wear metadata and whether it is worn.
    Generator {
        generator: &'a Generator,
        wear: Option<&'a WearMeta>,
        worn: bool,
    },
    /// A whole avatar.
    Avatar(&'a AvatarItem),
    /// A whole world (#1839).
    World(&'a WorldItem),
}

/// The pane beside the list: the picture, the name, the facts - or the
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
    let facts = match item.what {
        PaneWhat::Generator {
            generator,
            wear,
            worn,
        } => pane_facts(generator, wear, worn),
        PaneWhat::Avatar(avatar) => avatar_pane_facts(avatar),
        PaneWhat::World(world) => world_pane_facts(world),
    };
    for fact in facts {
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
    /// "lantern", rename it - the pane must still describe it, under the
    /// new name - then delete it, and the pane must stop describing
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

        assert!(stash.remove_item("ship lantern"));
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
    /// selectable - so the selection must not survive onto it.
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
    /// has - a fact the record answers without being serialised.
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
            fit_beast_mm: 0,
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
    /// it must NOT arm the drop bus - an armed bus with the button up is a
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
    /// always did, and it must NOT select - the click the owner added is
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
    /// delete are reachable, and a drag on it arms nothing - the release
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

    /// #1838. An avatar row selects on a click, so its Wear, Rename and
    /// delete are reachable, and a drag on it arms nothing: an avatar is
    /// neither placed nor gifted, and a drag that cannot land must not
    /// start - it would release onto the world or a person and do nothing.
    #[test]
    fn an_avatar_row_selects_but_never_arms_a_drag() {
        let mut row = RowHarness::new();
        let at = row.frame(RowKind::Avatar, vec![]).center();
        row.frame(RowKind::Avatar, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::Avatar, vec![button(at, true)]);
        for step in 1..=4 {
            let to = at + egui::vec2(12.0 * step as f32, 0.0);
            row.frame(RowKind::Avatar, vec![egui::Event::PointerMoved(to)]);
        }
        row.frame(
            RowKind::Avatar,
            vec![button(at + egui::vec2(48.0, 0.0), false)],
        );
        assert_eq!(row.pending.generator_name, None, "no drag sense, no drag");

        let mut row = RowHarness::new();
        let at = row.frame(RowKind::Avatar, vec![]).center();
        row.frame(RowKind::Avatar, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::Avatar, vec![button(at, true)]);
        row.frame(RowKind::Avatar, vec![button(at, false)]);
        assert_eq!(row.browser.selected_name(), Some("lantern"));
    }

    impl RowHarness {
        /// One frame of the selected avatar row's controls, with AccessKit
        /// on: what they asked for, every labelled node with the centre of
        /// its bounds, and every piece of text the frame painted - which is
        /// where a tooltip shows.
        fn controls_frame(
            &mut self,
            blocked: Option<&str>,
            events: Vec<egui::Event>,
        ) -> (
            Option<AvatarRowAction>,
            Vec<(String, egui::Pos2)>,
            Vec<String>,
        ) {
            self.frame_of(events, |ui| avatar_row_controls(ui, blocked))
        }

        /// [`Self::controls_frame`] over any row's controls (#1839).
        fn frame_of<A>(
            &mut self,
            events: Vec<egui::Event>,
            mut draw: impl FnMut(&mut egui::Ui) -> Option<A>,
        ) -> (Option<A>, Vec<(String, egui::Pos2)>, Vec<String>) {
            self.frame += 1;
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(300.0, 200.0),
                )),
                time: Some(f64::from(self.frame) / 60.0),
                events,
                ..Default::default()
            };
            let mut action = None;
            let output = self.ctx.run_ui(input, |ui| {
                action = draw(ui);
            });
            fn texts_of(shape: &egui::Shape, out: &mut Vec<String>) {
                match shape {
                    egui::Shape::Text(text) => out.push(text.galley.text().to_string()),
                    egui::Shape::Vec(shapes) => {
                        for shape in shapes {
                            texts_of(shape, out);
                        }
                    }
                    _ => {}
                }
            }
            let mut texts = Vec::new();
            for clipped in &output.shapes {
                texts_of(&clipped.shape, &mut texts);
            }
            let update = output
                .platform_output
                .accesskit_update
                .expect("AccessKit is on");
            let nodes = update
                .nodes
                .iter()
                .filter_map(|(_, node)| {
                    let label = node.label()?.to_string();
                    let bounds = node.bounds()?;
                    let centre = egui::pos2(
                        ((bounds.x0 + bounds.x1) / 2.0) as f32,
                        ((bounds.y0 + bounds.y1) / 2.0) as f32,
                    );
                    Some((label, centre))
                })
                .collect();
            (action, nodes, texts)
        }
    }

    /// #1838. The avatar row's Wear asks to wear when it can, and when it
    /// cannot it does nothing on a click and says why on the hover - egui
    /// shows `on_hover_text` only while enabled (#1289), so the reason must
    /// ride `on_disabled_hover_text`. The reasons are the wear surface's
    /// own (`wear_avatar_blocked_reason`).
    #[test]
    fn the_avatar_rows_wear_wears_or_says_why_not() {
        use crate::pds::inventory::avatar_item_tests::{generator_record, rigged_record};
        let item = AvatarItem::from_record(&rigged_record()).expect("copies");
        let live = generator_record();
        assert_eq!(
            crate::ui::avatar::wear_avatar_blocked_reason(Some(&live), &item),
            None
        );
        let not_loaded =
            crate::ui::avatar::wear_avatar_blocked_reason(None, &item).expect("no avatar, no wear");
        let mut future = item.clone();
        future.body = AvatarItemBody::Unknown;
        assert!(crate::ui::avatar::wear_avatar_blocked_reason(Some(&live), &future).is_some());

        for (blocked, expect) in [
            (None, Some(AvatarRowAction::Wear)),
            (Some(not_loaded), None),
        ] {
            let mut row = RowHarness::new();
            row.ctx.enable_accesskit();
            if blocked.is_some() {
                row.ctx.global_style_mut(|style| {
                    style.interaction.tooltip_delay = 0.0;
                    style.interaction.show_tooltips_only_when_still = false;
                });
            }
            let (_, nodes, _) = row.controls_frame(blocked, vec![]);
            let wear = nodes
                .iter()
                .find(|(label, _)| label == "Wear")
                .map(|(_, at)| *at)
                .expect("the row draws Wear");
            assert!(
                nodes.iter().any(|(label, _)| label == "Rename"),
                "and Rename beside it"
            );
            // The hover first: egui hides a tooltip over a widget that was
            // just pressed until the pointer leaves it.
            if let Some(reason) = blocked {
                let mut said = false;
                for _ in 0..4 {
                    let (_, _, texts) =
                        row.controls_frame(blocked, vec![egui::Event::PointerMoved(wear)]);
                    said |= texts.iter().any(|text| text == reason);
                }
                assert!(said, "a disabled Wear says {reason:?} on the hover");
            } else {
                row.controls_frame(blocked, vec![egui::Event::PointerMoved(wear)]);
            }
            row.controls_frame(blocked, vec![button(wear, true)]);
            let (action, _, _) = row.controls_frame(blocked, vec![button(wear, false)]);
            assert_eq!(action, expect, "blocked by {blocked:?}");
        }
    }

    /// #1838. An avatar's pane says what it is and what it wears. A person
    /// and a vehicle both have a picture (#1842), so neither pane opens on a
    /// sentence saying why there is none.
    #[test]
    fn an_avatar_pane_says_what_it_is() {
        use crate::pds::inventory::avatar_item_tests::{generator_record, rigged_record};
        let person = AvatarItem::from_record(&rigged_record()).expect("copies");
        assert_eq!(avatar_tag(&person), "avatar \u{b7} person");
        assert!(super::super::row_matches(
            "me",
            &avatar_tag(&person),
            None,
            "person"
        ));
        assert!(super::super::row_matches(
            "me",
            &avatar_tag(&person),
            None,
            "avatar"
        ));
        let facts = avatar_pane_facts(&person);
        assert_eq!(
            avatar_no_picture_reason(&person),
            None,
            "a person is pictured"
        );
        assert!(
            facts.first().is_some_and(|fact| fact.starts_with("Wears")),
            "{facts:?}"
        );
        assert!(
            facts.contains(&"Wears 1 wearable.".to_string()),
            "{facts:?}"
        );

        let vehicle = AvatarItem::from_record(&generator_record()).expect("copies");
        assert_eq!(
            avatar_no_picture_reason(&vehicle),
            None,
            "a vehicle is pictured"
        );
        assert!(super::super::row_matches(
            "ride",
            &avatar_tag(&vehicle),
            None,
            vehicle.kind().label()
        ));
        let facts = avatar_pane_facts(&vehicle);
        assert!(
            facts
                .iter()
                .any(|f| f.ends_with("parts") || f.ends_with("part")),
            "{facts:?}"
        );
    }

    /// #1838. A selected avatar item is not stale; one that leaves the
    /// stash, or turns unreadable under its name, is.
    #[test]
    fn an_avatar_selection_goes_stale_as_a_generators_does() {
        use crate::pds::inventory::avatar_item_tests::generator_record;
        let mut stash = InventoryRecord::default();
        stash.put_avatar(
            String::from("ride"),
            AvatarItem::from_record(&generator_record()).expect("copies"),
        );
        let mut browser = InventoryBrowser::default();
        browser.select("ride", 1.0);
        assert!(!browser.is_stale(&stash));
        stash.avatars.get_mut("ride").expect("held").body = AvatarItemBody::Unknown;
        assert!(browser.is_stale(&stash), "unreadable");
        stash.remove_item("ride");
        assert!(browser.is_stale(&stash), "gone");
    }

    /// #1839. A world row selects on a click, so its Load, Rename and
    /// delete are reachable, and a drag on it arms nothing: a world is
    /// neither placed nor gifted.
    #[test]
    fn a_world_row_selects_but_never_arms_a_drag() {
        let mut row = RowHarness::new();
        let at = row.frame(RowKind::World, vec![]).center();
        row.frame(RowKind::World, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::World, vec![button(at, true)]);
        for step in 1..=4 {
            let to = at + egui::vec2(12.0 * step as f32, 0.0);
            row.frame(RowKind::World, vec![egui::Event::PointerMoved(to)]);
        }
        row.frame(
            RowKind::World,
            vec![button(at + egui::vec2(48.0, 0.0), false)],
        );
        assert_eq!(row.pending.generator_name, None, "no drag sense, no drag");

        let mut row = RowHarness::new();
        let at = row.frame(RowKind::World, vec![]).center();
        row.frame(RowKind::World, vec![egui::Event::PointerMoved(at)]);
        row.frame(RowKind::World, vec![button(at, true)]);
        row.frame(RowKind::World, vec![button(at, false)]);
        assert_eq!(row.browser.selected_name(), Some("lantern"));
    }

    /// #1839. The world row's Load asks to load when it can; when it
    /// cannot - not your own world, or a load in flight - a click does
    /// nothing and the hover says why (#1289: `on_disabled_hover_text`).
    #[test]
    fn the_world_rows_load_loads_or_says_why_not() {
        let elsewhere = super::super::world::world_load_blocked_reason(false, false)
            .expect("not your own world");
        let loading =
            super::super::world::world_load_blocked_reason(true, true).expect("a load in flight");
        for (blocked, expect) in [
            (None, Some(WorldRowAction::Load)),
            (Some(elsewhere), None),
            (Some(loading), None),
        ] {
            let mut row = RowHarness::new();
            row.ctx.enable_accesskit();
            if blocked.is_some() {
                row.ctx.global_style_mut(|style| {
                    style.interaction.tooltip_delay = 0.0;
                    style.interaction.show_tooltips_only_when_still = false;
                });
            }
            let (_, nodes, _) = row.frame_of(vec![], |ui| world_row_controls(ui, blocked));
            let load = nodes
                .iter()
                .find(|(label, _)| label == "Load into my world")
                .map(|(_, at)| *at)
                .expect("the row draws Load");
            assert!(nodes.iter().any(|(label, _)| label == "Rename"));
            if let Some(reason) = blocked {
                let mut said = false;
                for _ in 0..4 {
                    let (_, _, texts) = row.frame_of(vec![egui::Event::PointerMoved(load)], |ui| {
                        world_row_controls(ui, blocked)
                    });
                    said |= texts.iter().any(|text| text == reason);
                }
                assert!(said, "a disabled Load says {reason:?} on the hover");
            } else {
                row.frame_of(vec![egui::Event::PointerMoved(load)], |ui| {
                    world_row_controls(ui, blocked)
                });
            }
            row.frame_of(vec![button(load, true)], |ui| {
                world_row_controls(ui, blocked)
            });
            let (action, _, _) = row.frame_of(vec![button(load, false)], |ui| {
                world_row_controls(ui, blocked)
            });
            assert_eq!(action, expect, "blocked by {blocked:?}");
        }
    }

    /// #1839. A world's tag and pane say what it is from its head alone:
    /// a world, procedural or a Berlin square, its name, how much is in it
    /// and how large it is - and why there is no picture.
    #[test]
    fn a_world_pane_says_what_it_is() {
        let mut world = crate::pds::inventory::world::tests::seeded_world(41);
        world.identity = Some(crate::pds::WorldIdentity {
            name: String::from("Harbour"),
            ..Default::default()
        });
        let item = WorldItem::snapshot(&world).expect("copies");
        assert_eq!(world_tag(&item), "world \u{b7} procedural");
        assert!(super::super::row_matches(
            "x",
            &world_tag(&item),
            None,
            "world"
        ));
        let facts = world_pane_facts(&item);
        assert_eq!(facts[0], WORLD_NO_PICTURE);
        assert_eq!(facts[1], "\"Harbour\", a procedural world.");
        assert!(
            facts[2].starts_with(&format!("{} item", world.generators.len())),
            "{facts:?}"
        );
        assert!(facts[3].ends_with(" saved."), "{facts:?}");
        assert!(facts.iter().any(|fact| fact.contains("re-roll")));

        let mut berlin = world.clone();
        berlin.identity = None;
        berlin.geo_source = Some(crate::pds::GeoSource::berlin(geodata::GeoSquare {
            min_e: 391_000,
            min_n: 5_819_500,
            size_m: 1_000,
        }));
        let item = WorldItem::snapshot(&berlin).expect("copies");
        assert_eq!(world_tag(&item), "world \u{b7} Berlin");
        assert!(world_pane_facts(&item)[1].starts_with("A Berlin square in "));
    }
}
