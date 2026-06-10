//! Catalogue browser window — the client-shipped sibling of the
//! Inventory window. Lists every entry in [`crate::catalogue::ENTRIES`]
//! grouped by [`crate::catalogue::CatalogueCategory`], with each row
//! armed as a drag-source that stamps a fresh copy of the entry into
//! the active room on viewport release.
//!
//! Drag mechanics are identical to [`crate::ui::inventory::inventory_ui`]
//! — the only difference is the drop source (`DropSource::Catalogue`),
//! which makes [`handle_generator_drop`](crate::ui::inventory::handle_generator_drop)
//! resolve the dragged `generator_name` against the catalogue
//! registry instead of the user's stash. Authentication / ownership
//! gating is identical (the user must own the active room to actually
//! place a drop).

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::catalogue::{CatalogueCategory, ENTRIES};
use crate::state::CurrentRoomDid;
use crate::ui::inventory::{DropSource, PendingGeneratorDrop, is_drop_placeable};

#[allow(clippy::too_many_arguments)]
pub fn catalogue_ui(
    mut contexts: EguiContexts,
    mut panels: ResMut<crate::ui::toolbar::UiPanels>,
    session: Option<Res<AtprotoSession>>,
    room_did: Option<Res<CurrentRoomDid>>,
    mut pending_drop: ResMut<PendingGeneratorDrop>,
) {
    // Drag-to-place gating mirrors inventory_ui: rows still render
    // in any room (the user can browse the catalogue regardless),
    // but the drag affordance only arms when the active room belongs
    // to the signed-in user — otherwise a release over the viewport
    // would mutate a `RoomRecord` we don't own.
    let can_drag_place = match (session.as_ref(), room_did.as_ref()) {
        (Some(s), Some(r)) => s.did == r.0,
        _ => false,
    };

    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };

    egui::Window::new("Catalogue")
        .open(&mut panels.catalogue)
        .default_pos([390.0, 420.0])
        .default_size([300.0, 380.0])
        .resizable(true)
        .collapsible(true)
        .show(ctx, |ui| {
            ui.label(format!("Entries: {}", ENTRIES.len()));
            ui.separator();

            egui::ScrollArea::vertical()
                .auto_shrink([true, false])
                .show(ui, |ui| {
                    // Section-by-category in the canonical order from
                    // `CatalogueCategory::ALL` so the layout is stable
                    // and self-explanatory. Categories with no entries
                    // are skipped silently.
                    for category in CatalogueCategory::ALL {
                        let entries_here: Vec<_> = ENTRIES
                            .iter()
                            .copied()
                            .filter(|e| e.category() == category)
                            .collect();
                        if entries_here.is_empty() {
                            continue;
                        }
                        ui.label(
                            egui::RichText::new(category.label())
                                .strong()
                                .color(egui::Color32::from_rgb(180, 180, 220)),
                        );
                        for entry in entries_here {
                            draw_row(ui, entry, &mut pending_drop, can_drag_place);
                        }
                        ui.add_space(6.0);
                    }
                });
        });
}

fn draw_row(
    ui: &mut egui::Ui,
    entry: &'static dyn crate::catalogue::CatalogueEntry,
    pending_drop: &mut PendingGeneratorDrop,
    can_drag_place: bool,
) {
    let slug = entry.slug();
    let name = entry.name();
    let description = entry.description();

    // Entries that produce non-placeable kinds (Terrain / Water — both
    // are room-scoped, not point-placed) render as plain labels so the
    // drag sense doesn't arm a release we'd ignore. In practice none
    // of the current catalogue entries fall into this bucket, but the
    // check keeps the catalogue future-proof against authoring a room
    // generator as an entry.
    //
    // We build a throwaway sample here purely to call `is_drop_placeable`.
    // The build call is cheap (a few HashMap inserts + a string clone)
    // and only runs once per row per frame; `Resp::on_hover_text` below
    // gives the user the description tooltip without an extra build.
    // Placeability only inspects the kind discriminant, so the
    // local-DID stamp is irrelevant — pass an empty placeholder. The
    // real drag-release path in [`crate::ui::inventory::drop`] threads
    // the session DID when it materialises the actual generator.
    let placeable = is_drop_placeable(&entry.build(""));

    ui.horizontal(|ui| {
        if can_drag_place && placeable {
            let label = egui::Label::new(name).sense(egui::Sense::click_and_drag());
            let resp = ui.add(label).on_hover_text(description);
            if resp.drag_started() {
                pending_drop.generator_name = Some(slug.to_string());
                pending_drop.source = DropSource::Catalogue;
            }
            if resp.dragged() && pending_drop.generator_name.as_deref() == Some(slug) {
                egui::Tooltip::always_open(
                    ui.ctx().clone(),
                    ui.layer_id(),
                    egui::Id::new(("catalogue_drag_tip", slug)),
                    egui::PopupAnchor::Pointer,
                )
                .show(|ui| {
                    ui.label(format!("Place “{name}”"));
                });
            }
        } else {
            ui.label(name).on_hover_text(description);
        }
    });
}
