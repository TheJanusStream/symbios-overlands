//! The item picture a detail pane draws: the live off-screen render of
//! whatever [`crate::item_preview`] has on its stage (#1288), shared by the
//! Catalogue's detail panel and the Inventory's pane (#1301).
//!
//! One body for both, because the rule it carries is the whole point and
//! must not drift between two copies: draw the picture only when the stage
//! is SHOWING this pane's item, and hold the same space with a plain tile
//! otherwise — on the frame an item is picked (the camera has not been
//! reframed yet), while the other window holds the stage, or for an item
//! that is never staged at all.

use bevy_egui::egui;

use crate::item_preview::{ItemPreview, PreviewSubject};

/// Edge of the Catalogue's picture, in points (#1288).
pub(crate) const CATALOGUE_SIDE: f32 = 180.0;

/// Edge of the Inventory's picture, in points (#1301).
///
/// Smaller than the Catalogue's by owner decision (option A on #1301): the
/// Inventory is a right-anchored window that must sit beside a 820-wide
/// World Editor and above People at 1280x720 (the #833 trio), which caps
/// its slot at 335 points — the World Editor sits at x = 115..935, and the
/// Inventory's only free spot is beside it. At 335 a 180-point picture
/// left a 119-point list; at 112 the list keeps the Catalogue's own
/// [`crate::ui::layout::LIST_MIN_WIDTH`], which
/// `ui::layout::tests::the_inventory_list_keeps_the_catalogue_floor_beside_its_picture`
/// measures in a real window under every palette. It samples the 256-pixel
/// target, so it stays crisp at 2x.
pub(crate) const INVENTORY_SIDE: f32 = 112.0;

/// Which pane is asking, and for what.
#[derive(Clone, Copy, Debug)]
pub(crate) enum PictureOf<'a> {
    /// The Catalogue's detail panel, for an entry slug.
    Catalogue(&'a str),
    /// The Inventory's pane, for a stash item name.
    Inventory(&'a str),
}

impl PictureOf<'_> {
    /// Whether the stage's `subject` is a picture of this pane's item.
    ///
    /// An Inventory subject matches by NAME alone. Its `edit` tick says
    /// when the stage last followed the stash, and a just-restaged edit is
    /// withheld by [`ItemPreview::showing`] until it is framed, so the pane
    /// cannot be shown the pre-edit picture under the post-edit name.
    fn is(&self, subject: &PreviewSubject) -> bool {
        match (self, subject) {
            (Self::Catalogue(slug), PreviewSubject::Catalogue(shown)) => shown == slug,
            (Self::Inventory(name), PreviewSubject::Inventory { name: shown, .. }) => shown == name,
            _ => false,
        }
    }
}

/// The item's picture (#1288): the live off-screen render of whatever
/// [`crate::item_preview`] currently has on its stage.
///
/// Drawn only when the preview says it is SHOWING this item — which it
/// does not say on the frame the item is picked, because the camera has
/// not been reframed onto the new geometry yet. Drawing it a frame early
/// would put the previous item's picture, or the new one seen from the
/// previous one's distance, under the right item's name. The pane holds
/// the space with a plain tile instead, so nothing below it jumps.
pub(crate) fn draw_preview(
    ui: &mut egui::Ui,
    preview: Option<&ItemPreview>,
    of: PictureOf<'_>,
    side: f32,
) {
    let showing = preview.filter(|p| p.showing().is_some_and(|shown| of.is(shown)));
    ui.add_space(4.0);
    match showing {
        Some(preview) => {
            ui.add(egui::Image::from_texture((
                preview.egui_texture,
                egui::vec2(side, side),
            )));
        }
        None => {
            // Same square either way so the pane does not reflow between
            // the frame a selection lands and the frame its picture does —
            // the reason `draw_avatar_icon`'s miss arm allocates too.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(side, side), egui::Sense::hover());
            if ui.is_rect_visible(rect) {
                let theme = crate::ui::theme::current(ui.ctx());
                ui.painter().rect_filled(rect, 4.0, theme.chart_fill);
            }
        }
    }
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1301. The two panes share one stage, and a slug and a stash name
    /// are both plain strings — so a catalogue entry named like a stash
    /// item must not be shown in the Inventory's pane, nor the reverse.
    #[test]
    fn a_pane_shows_only_a_picture_of_its_own_kind_of_item() {
        let name = "lantern";
        let stash = PreviewSubject::Inventory {
            name: name.to_string(),
            edit: bevy::ecs::change_detection::Tick::new(7),
        };
        let entry = PreviewSubject::Catalogue(name.to_string());
        assert!(PictureOf::Inventory(name).is(&stash));
        assert!(!PictureOf::Inventory(name).is(&entry));
        assert!(PictureOf::Catalogue(name).is(&entry));
        assert!(!PictureOf::Catalogue(name).is(&stash));
        assert!(!PictureOf::Inventory("bench").is(&stash));
        // Any edit of the same item is still that item.
        assert!(PictureOf::Inventory(name).is(&PreviewSubject::Inventory {
            name: name.to_string(),
            edit: bevy::ecs::change_detection::Tick::new(9),
        }));
    }
}
