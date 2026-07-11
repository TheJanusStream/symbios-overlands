//! Environment tab — directional sun, ambient, sky, fog, and room-wide water
//! widgets. Per-volume water appearance (colour, choppiness) lives on the
//! Water generator detail editor instead; the fields here are the ones that
//! should match the room's mood rather than varying between water bodies.
//!
//! It also hosts the owner-configurable **arrival point** (#773): the
//! [`crate::pds::DefaultLanding`] pose visitors come to rest at when they
//! enter without an explicit destination link (including through another
//! room's social gateway). Unset means the legacy random scatter near the
//! world origin.

use bevy::prelude::*;
use bevy_egui::egui;

use crate::pds::{DefaultLanding, Environment, Fp, Fp2};

use super::widgets::{color_picker, color_picker_rgba, fp_slider};

/// The local player's ground pose, captured once per frame from the
/// `LocalPlayer` transform so the "Set to my position & facing" button can
/// stamp it into the arrival point without the tab body touching the ECS.
#[derive(Clone, Copy)]
pub(super) struct PlayerPose {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    /// Facing in the spawn convention (degrees, `Quat::from_rotation_y`),
    /// normalised into `[0, 360)`.
    pub yaw_deg: f32,
}

impl PlayerPose {
    /// Extract the pose from a world-space transform. The yaw is recovered
    /// from the transform's forward vector projected onto XZ so it inverts
    /// the spawn path's `Quat::from_rotation_y(yaw_deg)` exactly — and stays
    /// correct even when the chassis is tilted to a slope (the projection
    /// discards the pitch/roll the surface-normal alignment adds).
    pub(super) fn from_transform(tf: &Transform) -> Self {
        let fwd = tf.forward();
        let yaw_deg = (-fwd.x).atan2(-fwd.z).to_degrees().rem_euclid(360.0);
        Self {
            x: tf.translation.x,
            y: tf.translation.y,
            z: tf.translation.z,
            yaw_deg,
        }
    }
}

pub(super) fn draw_environment_tab(
    ui: &mut egui::Ui,
    env: &mut Environment,
    landing: &mut Option<DefaultLanding>,
    player_pose: Option<PlayerPose>,
    dirty: &mut bool,
    audio_editor: &mut super::audio::AudioEditorState,
) {
    ui.heading("Environment");
    ui.add_space(4.0);

    draw_arrival_point(ui, landing, player_pose, dirty);

    egui::CollapsingHeader::new("Lighting & Sky")
        .default_open(true)
        .show(ui, |ui| {
            color_picker(ui, "Sun colour", &mut env.sun_color, dirty);
            color_picker(ui, "Sky colour", &mut env.sky_color, dirty);
            fp_slider(
                ui,
                "Sun illuminance",
                &mut env.sun_illuminance,
                0.0,
                50_000.0,
                dirty,
            );
            fp_slider(
                ui,
                "Ambient brightness",
                &mut env.ambient_brightness,
                0.0,
                2_000.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Clouds")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(ui, "Cover", &mut env.cloud_cover, 0.0, 1.0, dirty);
            fp_slider(ui, "Density", &mut env.cloud_density, 0.0, 1.0, dirty);
            fp_slider(
                ui,
                "Edge softness",
                &mut env.cloud_softness,
                0.001,
                1.0,
                dirty,
            );
            fp_slider(
                ui,
                "Drift speed (m/s)",
                &mut env.cloud_speed,
                0.0,
                50.0,
                dirty,
            );
            fp_slider(
                ui,
                "Feature scale (m)",
                &mut env.cloud_scale,
                10.0,
                2_000.0,
                dirty,
            );
            fp_slider(
                ui,
                "Altitude (m)",
                &mut env.cloud_height,
                10.0,
                2_000.0,
                dirty,
            );
            color_picker(ui, "Sunlit colour", &mut env.cloud_color, dirty);
            color_picker(ui, "Shadow colour", &mut env.cloud_shadow_color, dirty);

            ui.label(
                egui::RichText::new("Wind direction (XZ)")
                    .small()
                    .color(egui::Color32::GRAY),
            );
            let mut wind = env.cloud_wind_dir.0;
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::DragValue::new(&mut wind[0])
                            .speed(0.05)
                            .range(-10.0..=10.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
                if ui
                    .add(
                        egui::DragValue::new(&mut wind[1])
                            .speed(0.05)
                            .range(-10.0..=10.0),
                    )
                    .changed()
                {
                    *dirty = true;
                }
            });
            env.cloud_wind_dir = Fp2(wind);

            ui.label(
                egui::RichText::new(
                    "Cloud-deck dissolves into the distance-fog colour at the horizon, \
                     so adjust Distance Fog → Visibility for a tighter or wider band.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });

    egui::CollapsingHeader::new("Distance Fog")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Visibility (m)",
                &mut env.fog_visibility,
                50.0,
                2_000.0,
                dirty,
            );
            color_picker_rgba(ui, "Fog colour", &mut env.fog_color, dirty);
            color_picker(ui, "Extinction", &mut env.fog_extinction, dirty);
            color_picker(ui, "Inscattering", &mut env.fog_inscattering, dirty);
            color_picker_rgba(ui, "Sun glow", &mut env.fog_sun_color, dirty);
            fp_slider(
                ui,
                "Sun glow exponent",
                &mut env.fog_sun_exponent,
                0.0,
                200.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Water (room-wide)")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Detail normal — near tile",
                &mut env.water_normal_scale_near,
                0.0,
                4.0,
                dirty,
            );
            fp_slider(
                ui,
                "Detail normal — far tile",
                &mut env.water_normal_scale_far,
                0.0,
                1.0,
                dirty,
            );
            ui.label(
                egui::RichText::new(
                    "Near + far tiles blend by distance so the repeating-grid look \
                     disappears on long sightlines.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
            ui.add_space(4.0);
            fp_slider(
                ui,
                "Sun glitter",
                &mut env.water_sun_glitter,
                0.0,
                8.0,
                dirty,
            );
            color_picker(
                ui,
                "Crest scatter tint",
                &mut env.water_scatter_color,
                dirty,
            );
            ui.add_space(4.0);
            fp_slider(
                ui,
                "Shoreline foam width (m)",
                &mut env.water_shore_foam_width,
                0.0,
                8.0,
                dirty,
            );
            ui.label(
                egui::RichText::new(
                    "Shoreline foam fades in where water meets terrain, over \
                     this many metres of water depth. 0 disables it.",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });

    egui::CollapsingHeader::new("Ambient audio")
        .default_open(false)
        .show(ui, |ui| {
            super::audio::draw_audio_bridge(
                ui,
                &mut env.ambient_audio,
                "environment",
                dirty,
                audio_editor,
            );
        });
}

/// The arrival-point editor (#773). Toggling it off clears the pose back
/// to `None` (legacy origin scatter); toggling on seeds it from the
/// player's current pose when available so the common "land people where
/// I'm standing" flow is one click. Height defaults to drop-pin (follows
/// the terrain at X/Z) so the pose survives later terrain edits.
fn draw_arrival_point(
    ui: &mut egui::Ui,
    landing: &mut Option<DefaultLanding>,
    player_pose: Option<PlayerPose>,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Arrival point")
        .default_open(false)
        .show(ui, |ui| {
            let mut enabled = landing.is_some();
            if ui
                .checkbox(&mut enabled, "Set a custom arrival point")
                .on_hover_text(
                    "Where visitors come to rest when they enter without a specific \
                     destination link — including through another room's gateway. \
                     Off: they scatter near the world origin.",
                )
                .changed()
            {
                *landing = enabled.then(|| match player_pose {
                    Some(p) => DefaultLanding {
                        pos: Fp2([p.x, p.z]),
                        y: None,
                        yaw_deg: Fp(p.yaw_deg),
                    },
                    None => DefaultLanding::default(),
                });
                *dirty = true;
            }

            let Some(l) = landing.as_mut() else {
                return;
            };

            if let Some(p) = player_pose
                && ui
                    .button("⟲ Set to my position & facing")
                    .on_hover_text("Stamp your avatar's current spot and heading.")
                    .clicked()
            {
                l.pos = Fp2([p.x, p.z]);
                l.yaw_deg = Fp(p.yaw_deg);
                // Only overwrite the height when the owner is pinning one;
                // a drop-pin pose keeps following the terrain.
                if l.y.is_some() {
                    l.y = Some(Fp(p.y));
                }
                *dirty = true;
            }

            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label("X");
                if ui
                    .add(egui::DragValue::new(&mut l.pos.0[0]).speed(0.25))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Z");
                if ui
                    .add(egui::DragValue::new(&mut l.pos.0[1]).speed(0.25))
                    .changed()
                {
                    *dirty = true;
                }
            });

            ui.horizontal(|ui| {
                ui.label("Facing (°)");
                if ui
                    .add(
                        egui::DragValue::new(&mut l.yaw_deg.0)
                            .speed(1.0)
                            .range(0.0..=360.0),
                    )
                    .on_hover_text("0° faces −Z, 90° faces +X.")
                    .changed()
                {
                    *dirty = true;
                }
            });

            let mut pin = l.y.is_some();
            if ui
                .checkbox(&mut pin, "Pin exact height")
                .on_hover_text(
                    "Off: height follows the terrain at (X, Z) — best for ground-level \
                     spots. On: use a fixed Y, for a platform or rooftop.",
                )
                .changed()
            {
                l.y = pin.then(|| Fp(player_pose.map(|p| p.y).unwrap_or(0.0)));
                *dirty = true;
            }
            if let Some(y) = l.y.as_mut() {
                ui.horizontal(|ui| {
                    ui.label("Y");
                    if ui.add(egui::DragValue::new(&mut y.0).speed(0.25)).changed() {
                        *dirty = true;
                    }
                });
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_1_SQRT_2;

    /// The pose yaw must invert the spawn path's
    /// `Quat::from_rotation_y(yaw_deg.to_radians())` for every heading, so a
    /// captured landing faces exactly where the owner was looking.
    #[test]
    fn yaw_extraction_inverts_spawn_rotation() {
        for deg in [0.0_f32, 30.0, 45.0, 90.0, 135.0, 180.0, 225.0, 359.0] {
            let tf = Transform::from_rotation(Quat::from_rotation_y(deg.to_radians()));
            let pose = PlayerPose::from_transform(&tf);
            let mut diff = (pose.yaw_deg - deg).rem_euclid(360.0);
            diff = diff.min(360.0 - diff);
            assert!(diff < 0.01, "deg {deg} round-tripped to {}", pose.yaw_deg);
        }
    }

    /// A surface tilt (chassis rested on a slope) must not corrupt the yaw:
    /// the XZ projection discards the pitch the normal-alignment adds.
    #[test]
    fn yaw_survives_surface_tilt() {
        let tilt = Quat::from_rotation_arc(Vec3::Y, Vec3::new(0.3, 1.0, 0.0).normalize());
        let yaw = Quat::from_rotation_y(90.0_f32.to_radians());
        let tf = Transform::from_rotation(tilt * yaw);
        let pose = PlayerPose::from_transform(&tf);
        let mut diff = (pose.yaw_deg - 90.0).rem_euclid(360.0);
        diff = diff.min(360.0 - diff);
        assert!(diff < 1.0, "tilted yaw drifted to {}", pose.yaw_deg);
    }

    #[test]
    fn position_is_copied_verbatim() {
        let tf =
            Transform::from_xyz(3.0, 5.0, -7.0).with_rotation(Quat::from_rotation_y(FRAC_1_SQRT_2));
        let pose = PlayerPose::from_transform(&tf);
        assert_eq!((pose.x, pose.y, pose.z), (3.0, 5.0, -7.0));
    }
}
