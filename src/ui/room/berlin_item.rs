//! The World Editor's panel for a Berlin item picked in the world (#1590):
//! one of the walkable ground's buildings, trees or items of street
//! furniture, named as Berlin records it, with the two edits the owner may
//! make of it. The edits themselves are
//! [`crate::terrain::derived::edit`]'s; this only asks for them.

use bevy_egui::egui;

use crate::terrain::derived::SourceId;
use crate::terrain::derived::edit::describe;
use crate::terrain::geo::street_level::StreetLevel;

/// What the owner asked of the picked item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ItemAction {
    /// Draw the world without it.
    Remove,
    /// Make a copy as drawn the world's own.
    Adopt,
    /// Pick nothing.
    Close,
}

const REMOVE_HOVER: &str = "Draw this world without it. It is listed under Environment, \
                            Region source, where Restore brings it back.";

const ADOPT_HOVER: &str = "Make a copy of it as drawn - the same items at the same place, turn \
                           and size - this world's own, to move, restyle or delete like anything \
                           placed here. Berlin's own is drawn no more; Restore, under \
                           Environment, Region source, brings it back and takes the copy away.";

/// Draw the panel for the item `id`, named from the walkable ground's
/// `level`, with why its last edit was `refused`: what the owner asked, if
/// anything.
pub(super) fn draw_berlin_item(
    ui: &mut egui::Ui,
    id: &SourceId,
    level: Option<&StreetLevel>,
    refused: Option<&str>,
) -> Option<ItemAction> {
    let theme = crate::ui::theme::current(ui.ctx());
    let mut action = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.strong("From Berlin");
            if ui
                .small_button("Close")
                .on_hover_text("Pick nothing.")
                .clicked()
            {
                action = Some(ItemAction::Close);
            }
        });
        ui.label(describe(level, id));
        ui.label(
            egui::RichText::new(id.to_string())
                .small()
                .color(theme.text_weak),
        );
        ui.horizontal_wrapped(|ui| {
            if ui.button("Remove").on_hover_text(REMOVE_HOVER).clicked() {
                action = Some(ItemAction::Remove);
            }
            if ui
                .button("Make it this world's own")
                .on_hover_text(ADOPT_HOVER)
                .clicked()
            {
                action = Some(ItemAction::Adopt);
            }
        });
        if let Some(why) = refused {
            ui.colored_label(theme.status.error, why);
        }
    });
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::derived::SourceLayer;

    /// Drawn with no input, the panel asks nothing, and names the item
    /// whether or not the walkable ground holds it.
    #[test]
    fn an_untouched_panel_asks_nothing() {
        let id = SourceId::new(SourceLayer::Tree, "00008100_0014f258");
        for refused in [None, Some("It is still being drawn.")] {
            let ctx = egui::Context::default();
            let mut asked = Some(ItemAction::Close);
            let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
                asked = draw_berlin_item(ui, &id, None, refused);
            });
            assert_eq!(asked, None);
        }
    }
}
