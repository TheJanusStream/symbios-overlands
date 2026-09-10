//! The peer profile-picture icon (#1225 f351), and the initial it falls
//! back to.
//!
//! It lived in `crate::avatar` beside the cache it reads and moved here
//! under #1297: it is an `egui` widget — it allocates, paints and reads
//! [`super::super::theme::current`] — and every one of its five callers
//! is a `ui` surface (People, chat, the gateway picker, the account chip).
//! A drawing function in the domain layer had to import the theme to
//! draw, which is the dependency this issue exists to turn around; the
//! CACHE it reads stays where it is, because filling it is a PDS fetch.
//!
//! Not [`crate::item_preview`]'s neighbour despite drawing a picture of a
//! thing: that path owns a camera and renders to an off-screen target,
//! this one uploads bytes fetched from a PDS.

use crate::avatar::BskyProfileCache;

/// The first character of `name`, upper-cased, for a picture-less icon
/// (#1225 f351).
///
/// `None` when there is nothing worth drawing — an empty name, or one whose
/// first character is not alphanumeric, where a lone `@` or an emoji
/// fragment says less than the plain tile does. Written as a pure function
/// because it is the only decision in the placeholder worth testing.
pub fn icon_initial(name: Option<&str>) -> Option<char> {
    let first = name?.trim().chars().next()?;
    // Skip a leading sigil so "@alice" reads as A, not @.
    let first = if first == '@' {
        name?.trim().chars().nth(1)?
    } else {
        first
    };
    first
        .is_alphanumeric()
        .then(|| first.to_uppercase().next().unwrap_or(first))
}

/// Render a small profile-picture icon for `did`, falling back to a tile
/// carrying `name`'s initial when there is no picture (#1225 f351).
///
/// The miss arm used to allocate a transparent square, and this function's
/// own doc conceded that in-flight, no-picture and failed-fetch were
/// indistinguishable — three different facts rendered as the same nothing.
/// On wasm the failure rate is structurally higher, because `cdn.bsky.app`
/// serves no CORS headers and the original PDS blob has to be fetched
/// instead.
///
/// In the People panel that empty square is the row's only visual anchor
/// besides the handle, so a room of pending or picture-less peers read as
/// broken rather than as loading. A drawn tile is not a picture, but it is
/// an anchor and it carries an identity.
pub fn draw_avatar_icon(
    ui: &mut bevy_egui::egui::Ui,
    did: Option<&str>,
    name: Option<&str>,
    cache: &BskyProfileCache,
    size: f32,
) {
    use bevy_egui::egui;

    let texture_id = did.and_then(|d| cache.get(d)).map(|p| p.egui_texture);
    match texture_id {
        Some(texture_id) => {
            ui.add(egui::Image::from_texture((
                texture_id,
                egui::vec2(size, size),
            )));
        }
        None => {
            // Same square either way, so a row's layout does not shift
            // between a cache miss and a cache hit — the reason the miss
            // arm allocated space in the first place.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
            if !ui.is_rect_visible(rect) {
                return;
            }
            let theme = crate::ui::theme::current(ui.ctx());
            ui.painter()
                .rect_filled(rect, size * 0.25, theme.chart_fill);
            if let Some(initial) = icon_initial(name) {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    initial,
                    egui::FontId::proportional(size * 0.62),
                    theme.text_weak,
                );
            }
        }
    }
}

#[cfg(test)]
mod icon_placeholder_tests {
    use super::*;

    /// #1225 f351. The sequence: you open People in a room of people who
    /// have just arrived, and every row has a gap where a picture should be
    /// — and the gap means "still loading", "has no picture" and "the fetch
    /// failed" indistinguishably, because all three ended at the same
    /// transparent square. On wasm the third is structurally more common,
    /// because `cdn.bsky.app` serves no CORS headers and the original PDS
    /// blob has to be fetched instead. In the People panel that square is
    /// the row's only visual anchor besides the handle.
    #[test]
    fn a_picture_less_row_still_carries_an_identity() {
        assert_eq!(icon_initial(Some("alice.bsky.social")), Some('A'));
        assert_eq!(
            icon_initial(Some("@alice.bsky.social")),
            Some('A'),
            "the sigil is decoration, not a name"
        );
        assert_eq!(icon_initial(Some("  sam ")), Some('S'));
        assert_eq!(icon_initial(Some("7ravellers")), Some('7'));
    }

    /// Nothing worth drawing draws nothing: a lone sigil, an emoji
    /// fragment or an empty name says less than the plain tile does.
    #[test]
    fn a_nameless_row_gets_the_tile_without_a_letter() {
        assert_eq!(icon_initial(None), None);
        assert_eq!(icon_initial(Some("")), None);
        assert_eq!(icon_initial(Some("   ")), None);
        assert_eq!(icon_initial(Some("@")), None);
        assert_eq!(icon_initial(Some("!!!")), None);
    }

    /// Non-Latin names are names: the initial is whatever the script's
    /// first character is, and `to_uppercase` is a no-op where the script
    /// has no case.
    #[test]
    fn a_non_latin_name_keeps_its_own_first_character() {
        assert_eq!(icon_initial(Some("さくら")), Some('さ'));
        assert_eq!(icon_initial(Some("Ács")), Some('Á'));
    }
}
