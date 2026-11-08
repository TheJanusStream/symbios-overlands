//! The world's way into the Inventory (#1839): the World Editor's Save to
//! Inventory copies the LIVE world - unsaved edits included, what the owner
//! sees - into a world item, and the Inventory window's Save writes it, as
//! the editor's other Save to Inventory and the Avatar editor's do.
//!
//! The way back is the Inventory row's Load into my world
//! (`ui::inventory::world`), which replaces the world through
//! [`super::replace_world`] - the path a re-roll takes.
//!
//! The Catalogue's worlds (#1841) come in through the same two doors: its
//! Copy to inventory stashes a built entry through
//! [`save_world_to_inventory_as`], and its Load into my world lands one
//! through the Inventory's own landing, so through [`super::replace_world`].

use bevy_egui::egui;

use crate::pds::inventory::WorldItem;
use crate::pds::inventory::world::{SIGN_IN_AGAIN, snapshot_blocked};
use crate::pds::{InventoryRecord, RoomRecord};

/// The name a world is saved under when it has none of its own.
pub(crate) const DEFAULT_WORLD_ITEM_NAME: &str = "world";

/// Whether the live world can be copied whole into a world item, asked no
/// more often than the size readout measures (#1839): the answer
/// serialises the whole world ([`snapshot_blocked`]), and the footer that
/// shows it draws every frame. Keyed by the World Editor's live-value
/// generation, so an edit is asked about again once the throttle allows;
/// between, the last answer stands - the click asks again for real.
#[derive(Default)]
pub(crate) struct WorldStashCheck {
    asked: Option<(u64, f64)>,
    answer: Option<String>,
}

impl WorldStashCheck {
    /// Why the world `generation` names cannot be copied, as of the last
    /// time it was asked.
    pub(crate) fn reason(
        &mut self,
        generation: u64,
        now: f64,
        record: &RoomRecord,
    ) -> Option<&str> {
        let due = match self.asked {
            None => true,
            Some((asked, at)) => {
                asked != generation
                    && now - at >= crate::config::ui::editor::SIZE_READOUT_REFRESH_SECS
            }
        };
        if due {
            self.answer = snapshot_blocked(record);
            self.asked = Some((generation, now));
        }
        self.answer.as_deref()
    }
}

/// Why the live world cannot be saved to the Inventory right now, or `None`
/// when it can: the inventory has not loaded or is full, the sign-in
/// predates the grant a world item's parts need, or the world cannot be
/// copied whole or is past what an item holds (`copy_blocked`, from
/// [`WorldStashCheck`] or the copy itself).
pub(crate) fn save_world_blocked_reason(
    inventory: Option<&InventoryRecord>,
    parts_granted: bool,
    copy_blocked: Option<&str>,
) -> Option<String> {
    let Some(inventory) = inventory else {
        return Some(String::from("Your inventory has not loaded yet."));
    };
    if inventory.is_full() {
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        return Some(format!(
            "Inventory full ({cap}/{cap}) - remove an item first."
        ));
    }
    if !parts_granted {
        return Some(SIGN_IN_AGAIN.to_owned());
    }
    copy_blocked.map(|reason| format!("This world cannot be copied: {reason}"))
}

/// The name `record` is saved under, before it is made unique: the world's
/// own name, or [`DEFAULT_WORLD_ITEM_NAME`].
fn default_name(record: &RoomRecord) -> &str {
    record
        .identity
        .as_ref()
        .map(|identity| identity.name.trim())
        .filter(|name| !name.is_empty())
        .unwrap_or(DEFAULT_WORLD_ITEM_NAME)
}

/// Copy `record` - the live world, unsaved edits included - into
/// `inventory` under a name no item of any kind holds. Returns the name it
/// landed under, or the sentence saying why it did not.
pub(crate) fn save_world_to_inventory(
    record: &RoomRecord,
    inventory: &mut InventoryRecord,
    parts_granted: bool,
) -> Result<String, String> {
    save_world_to_inventory_as(record, None, inventory, parts_granted)
}

/// [`save_world_to_inventory`] under a name built on `name` rather than the
/// world's own - the Catalogue's Copy to inventory (#1841), which names
/// the copy for its entry.
pub(crate) fn save_world_to_inventory_as(
    record: &RoomRecord,
    name: Option<&str>,
    inventory: &mut InventoryRecord,
    parts_granted: bool,
) -> Result<String, String> {
    if let Some(reason) = save_world_blocked_reason(Some(inventory), parts_granted, None) {
        return Err(reason);
    }
    let item = WorldItem::snapshot(record)?;
    let name = inventory.free_name(name.unwrap_or_else(|| default_name(record)));
    if !inventory.put_world(name.clone(), item) {
        // Unreachable: `free_name` asked every map.
        return Err(format!("\"{name}\" is already taken."));
    }
    Ok(name)
}

/// The footer's Save to Inventory row: the button, disabled with `blocked`
/// on the hover when it cannot, and the reason painted beside it as well
/// (#1289: a hover never fires on a disabled widget for everyone who
/// needs it), on one truncated line so the row's height holds (#1433).
/// Whether it was clicked.
pub(crate) fn save_world_row(ui: &mut egui::Ui, blocked: Option<&str>) -> bool {
    ui.horizontal(|ui| {
        let response = ui
            .add_enabled(blocked.is_none(), egui::Button::new("Save to Inventory"))
            .on_hover_text(
                "Copy this world, as you see it now, into your inventory - load it again \
                 from there",
            )
            .on_disabled_hover_text(blocked.unwrap_or_default());
        if let Some(reason) = blocked {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(reason)
                        .small()
                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                )
                .truncate(),
            );
        }
        response.clicked()
    })
    .inner
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::inventory::world::tests::seeded_world;
    use crate::ui::headless::Headless as _;

    /// Save to Inventory copies the world the owner SEES - the live record
    /// with its unsaved edit - names it for the world's own name, free
    /// across every kind of item, and holds its contents for the save.
    #[test]
    fn save_to_inventory_copies_the_live_world_with_its_unsaved_edit() {
        let stored = seeded_world(21);
        let mut live = stored.clone();
        live.environment.sun_color = crate::pds::types::Fp3([0.9, 0.1, 0.1]);
        live.identity = Some(crate::pds::WorldIdentity {
            name: String::from("Harbour"),
            ..Default::default()
        });
        let mut inventory = InventoryRecord::default();
        inventory.put_item(
            String::from("Harbour"),
            crate::pds::Generator::default_cuboid(),
            None,
        );

        let name = save_world_to_inventory(&live, &mut inventory, true).expect("saves");
        assert_eq!(name, "Harbour_1", "free across kinds");
        let item = &inventory.worlds[&name];
        assert_eq!(item.head().summary.name, "Harbour");
        let held = crate::pds::inventory::world::join_parts(
            item.head(),
            item.slices().expect("held for the save"),
        )
        .expect("loads");
        assert_eq!(held.environment.sun_color, live.environment.sun_color);
        assert_ne!(
            held.environment.sun_color, stored.environment.sun_color,
            "the live world, not the saved one"
        );

        // A world without a name of its own is a plain "world" (a seeded
        // one is named for its place).
        let mut unnamed = stored.clone();
        unnamed.identity = None;
        let name = save_world_to_inventory(&unnamed, &mut inventory, true).expect("saves");
        assert_eq!(name, DEFAULT_WORLD_ITEM_NAME);
    }

    /// Every reason Save to Inventory is disabled is one this answers, in
    /// the order the owner can act on them.
    #[test]
    fn save_to_inventory_says_why_it_cannot() {
        let world = seeded_world(22);
        let empty = InventoryRecord::default();
        assert!(
            save_world_blocked_reason(None, true, None)
                .is_some_and(|why| why.contains("not loaded"))
        );
        assert_eq!(save_world_blocked_reason(Some(&empty), true, None), None);
        let mut full = InventoryRecord::default();
        for i in 0..crate::config::state::MAX_INVENTORY_ITEMS {
            full.put_item(
                format!("g{i}"),
                crate::pds::Generator::default_cuboid(),
                None,
            );
        }
        assert!(
            save_world_blocked_reason(Some(&full), true, None)
                .is_some_and(|why| why.contains("full"))
        );
        assert_eq!(
            save_world_blocked_reason(Some(&empty), false, None).as_deref(),
            Some(SIGN_IN_AGAIN)
        );
        let mut future = world.clone();
        future.placements.push(crate::pds::Placement::Unknown);
        let mut check = WorldStashCheck::default();
        let refusal = check.reason(1, 0.0, &future).map(str::to_owned);
        let why = save_world_blocked_reason(Some(&empty), true, refusal.as_deref())
            .expect("a world this build cannot copy whole");
        assert!(why.contains("could not be read in full"), "{why}");
        assert!(save_world_to_inventory(&future, &mut InventoryRecord::default(), true).is_err());
        assert!(
            save_world_to_inventory(&world, &mut InventoryRecord::default(), false)
                .is_err_and(|why| why == SIGN_IN_AGAIN),
            "the grant is refused before anything is copied"
        );

        // The check is asked again for a new generation once the throttle
        // allows, and not before.
        assert!(
            check.reason(1, 0.1, &world).is_some(),
            "the same generation"
        );
        assert!(
            check.reason(2, 0.1, &world).is_some(),
            "inside the throttle"
        );
        assert_eq!(check.reason(2, 10.0, &world), None, "asked again");
    }

    /// The row's button saves when it can, and when it cannot it does
    /// nothing on a click and paints the reason beside itself.
    #[test]
    fn the_save_row_saves_or_says_why_not() {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let run = |blocked: Option<&str>, events: Vec<egui::Event>, frame: u32| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(400.0, 100.0),
                )),
                time: Some(f64::from(frame) / 60.0),
                events,
                ..Default::default()
            };
            let mut clicked = false;
            let output = ctx.run_headless(input, |ui| clicked = save_world_row(ui, blocked));
            let at = output
                .platform_output
                .accesskit_update
                .expect("AccessKit is on")
                .nodes
                .iter()
                .find_map(|(_, node)| {
                    (node.label()? == "Save to Inventory").then(|| {
                        let b = node.bounds().expect("laid out");
                        egui::pos2(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
                    })
                })
                .expect("the row draws the button");
            fn paints(shape: &egui::Shape, why: &str) -> bool {
                match shape {
                    egui::Shape::Text(text) => text.galley.text() == why,
                    egui::Shape::Vec(shapes) => shapes.iter().any(|shape| paints(shape, why)),
                    _ => false,
                }
            }
            let painted = blocked.is_some_and(|why| {
                output
                    .shapes
                    .iter()
                    .any(|clipped| paints(&clipped.shape, why))
            });
            (clicked, at, painted)
        };
        let press = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        for (frame0, blocked) in [(0, None), (10, Some("Your inventory has not loaded yet."))] {
            let (_, at, painted) = run(blocked, vec![], frame0 + 1);
            assert_eq!(painted, blocked.is_some(), "the reason is painted");
            run(blocked, vec![egui::Event::PointerMoved(at)], frame0 + 2);
            run(blocked, vec![press(at, true)], frame0 + 3);
            let (clicked, _, _) = run(blocked, vec![press(at, false)], frame0 + 4);
            assert_eq!(clicked, blocked.is_none(), "blocked by {blocked:?}");
        }
    }
}
