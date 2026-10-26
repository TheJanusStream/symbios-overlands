//! A world's name and banner as the app shows them (#1783): the title a
//! visitor arrives under, and the picture of the banner that title and the
//! World Editor draw.
//!
//! The name is its owner's claim about their world, so it is shown as one:
//! under it, always, whose world it is, as the app's one naming ladder
//! names its owner ([`travel_label`], with the verified handle
//! [`WorldNames`] holds) - and never in its place. The portal prompt and
//! the travel card, which answer whether to go into somebody's world, name
//! the owner alone.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::avatar::BskyProfileCache;
use crate::network::presence::travel_label;
use crate::pds::WorldBanner;
use crate::state::{CurrentRoomDid, LiveRoomRecord, LocalPlayer, TravelingTo};
use crate::ui::travel::WorldNames;

/// How long the arrival title shows, from the arrival (s), and how long it
/// takes to come up and to fade.
const TITLE_SECS: f64 = 6.0;
const FADE_IN_SECS: f64 = 0.8;
const FADE_OUT_SECS: f64 = 1.5;

/// The title's distance under the screen's top edge (points): below the
/// toolbar.
const TITLE_DROP: f32 = 64.0;

/// The banner's picture in the title, wide (points); three high for two.
const TITLE_BANNER_W: f32 = 34.0;

/// The banner's picture, as an egui texture: drawn once for the banner it
/// shows (undyed linen for `None`), and kept until it is asked for another.
pub fn banner_texture(ctx: &egui::Context, banner: Option<&WorldBanner>) -> egui::TextureHandle {
    let id = egui::Id::new("world_banner_texture");
    let want = banner.cloned();
    if let Some((drawn, texture)) =
        ctx.data(|d| d.get_temp::<(Option<WorldBanner>, egui::TextureHandle)>(id))
        && drawn == want
    {
        return texture;
    }
    let (w, h) = (crate::banner::WIDTH, crate::banner::HEIGHT);
    let image = egui::ColorImage::from_rgba_unmultiplied(
        [w as usize, h as usize],
        &crate::banner::shared_raster(want.as_ref()),
    );
    let texture = ctx.load_texture("world_banner", image, egui::TextureOptions::LINEAR);
    ctx.data_mut(|d| d.insert_temp(id, (want, texture.clone())));
    texture
}

/// The banner's picture `width` points wide.
pub fn banner_picture(ui: &mut egui::Ui, banner: Option<&WorldBanner>, width: f32) {
    let texture = banner_texture(ui.ctx(), banner);
    let height = width * crate::banner::HEIGHT as f32 / crate::banner::WIDTH as f32;
    ui.add(egui::Image::new(&texture).fit_to_exact_size(egui::vec2(width, height)));
}

/// How opaque the title is `elapsed` seconds after the arrival: up over
/// [`FADE_IN_SECS`], held, and down over the last [`FADE_OUT_SECS`] of
/// [`TITLE_SECS`] - `None` once it is gone.
fn title_opacity(elapsed: f64) -> Option<f32> {
    if !(0.0..TITLE_SECS).contains(&elapsed) {
        return None;
    }
    let up = (elapsed / FADE_IN_SECS).min(1.0);
    let down = ((TITLE_SECS - elapsed) / FADE_OUT_SECS).min(1.0);
    Some(up.min(down) as f32)
}

/// The world's name and banner, over whose world it is, for a few seconds
/// after the player arrives in a world that has a name: on entering it, and
/// on every travel into another. Not during the travel's veil, and not over
/// a world without a name.
#[allow(clippy::too_many_arguments)]
pub fn world_title_ui(
    mut contexts: EguiContexts,
    record: Option<Res<LiveRoomRecord>>,
    current_room: Option<Res<CurrentRoomDid>>,
    session: Option<Res<AtprotoSession>>,
    traveling: Option<Res<TravelingTo>>,
    players: Query<(), With<LocalPlayer>>,
    names: Res<WorldNames>,
    profile_cache: Res<BskyProfileCache>,
    time: Res<Time>,
    mut arrived: Local<Option<(String, f64)>>,
) {
    // A player who is not in a world - logged out - arrives afresh.
    if players.is_empty() {
        *arrived = None;
        return;
    }
    if traveling.is_some() {
        return;
    }
    let Some(room) = current_room.as_deref() else {
        return;
    };
    let now = time.elapsed_secs_f64();
    if arrived.as_ref().is_none_or(|(did, _)| *did != room.0) {
        *arrived = Some((room.0.clone(), now));
    }
    let Some((_, at)) = arrived.as_ref() else {
        return;
    };
    let Some(opacity) = title_opacity(now - at) else {
        return;
    };
    let Some(identity) = record
        .as_deref()
        .and_then(|record| record.0.identity.as_ref())
        .filter(|identity| !identity.name.is_empty())
    else {
        return;
    };
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    let whose = if crate::ui::toolbar::owns_current_room(session.as_deref(), Some(room)) {
        "Your world".to_string()
    } else {
        format!(
            "{}'s world",
            travel_label(&profile_cache, &room.0, names.get(&room.0))
        )
    };
    let theme = crate::ui::theme::current(ctx);
    egui::Area::new(egui::Id::new("world_title"))
        .anchor(egui::Align2::CENTER_TOP, egui::vec2(0.0, TITLE_DROP))
        .order(egui::Order::Middle)
        .interactable(false)
        .show(ctx, |ui| {
            ui.set_opacity(opacity);
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    banner_picture(ui, Some(&identity.banner), TITLE_BANNER_W);
                    ui.add_space(6.0);
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new(&identity.name).size(24.0).strong());
                        ui.label(egui::RichText::new(whose).color(theme.text_weak));
                    });
                });
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_title_comes_up_holds_and_fades() {
        assert_eq!(title_opacity(-0.1), None);
        assert_eq!(title_opacity(0.0), Some(0.0));
        assert!(title_opacity(FADE_IN_SECS * 0.5).is_some_and(|o| (o - 0.5).abs() < 1e-3));
        assert_eq!(title_opacity(3.0), Some(1.0));
        let late = title_opacity(TITLE_SECS - FADE_OUT_SECS * 0.5).unwrap();
        assert!((late - 0.5).abs() < 1e-3, "{late}");
        assert_eq!(title_opacity(TITLE_SECS), None);
    }
}
