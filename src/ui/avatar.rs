//! Avatar editor panel — the sovereign analogue of the old Airship
//! Design window.
//!
//! The paradigm is **Live UX**: every slider mutates
//! [`LiveAvatarRecord`] in place. The hot-swap system in `player.rs`
//! rebuilds the local visuals the same frame the resource changes, and
//! `network::broadcast_avatar_state` pushes a preview update to peers so
//! they see the edit before the author commits. Two explicit buttons drive
//! PDS persistence:
//!
//! - **Publish** writes the current `LiveAvatarRecord` to the owner's PDS
//!   via `com.atproto.repo.putRecord` and then syncs the value into
//!   [`StoredAvatarRecord`] on success.
//! - **Revert** drops all in-flight edits by copying `StoredAvatarRecord`
//!   back into `LiveAvatarRecord`.

use bevy::prelude::*;
use bevy_egui::{EguiContexts, egui};
use bevy_symbios_multiuser::auth::AtprotoSession;

use crate::pds::{
    self, AvatarBody, AvatarRecord, Fp, Fp3, HumanoidKinematics, HumanoidPhenotype,
    RoverKinematics, RoverPhenotype,
};
use crate::protocol::PontoonShape;
use crate::state::{LiveAvatarRecord, LocalSettings, PublishFeedback, StoredAvatarRecord};

/// Async task for publishing the avatar record to the owner's PDS.
#[derive(Component)]
pub struct PublishAvatarTask(pub bevy::tasks::Task<Result<(), String>>);

#[allow(clippy::too_many_arguments)]
pub fn avatar_ui(
    mut contexts: EguiContexts,
    mut commands: Commands,
    mut live: ResMut<LiveAvatarRecord>,
    stored: Option<Res<StoredAvatarRecord>>,
    mut settings: ResMut<LocalSettings>,
    session: Option<Res<AtprotoSession>>,
    mut feedback: ResMut<PublishFeedback>,
) {
    use crate::config::ui::airship as cfg;

    egui::Window::new("Avatar")
        .default_open(false)
        .default_pos(cfg::WINDOW_DEFAULT_POS)
        .default_width(cfg::WINDOW_DEFAULT_WIDTH)
        .resizable(true)
        .collapsible(true)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            // --- Body variant selector ---------------------------------
            let current_kind = live.0.body.kind_tag();
            ui.horizontal(|ui| {
                ui.label("Body:");
                if ui
                    .selectable_label(current_kind == "hover_rover", "Hover-Rover")
                    .clicked()
                    && current_kind != "hover_rover"
                {
                    live.0.body = AvatarBody::HoverRover {
                        phenotype: RoverPhenotype::default(),
                        kinematics: RoverKinematics::default(),
                    };
                }
                if ui
                    .selectable_label(current_kind == "humanoid", "Humanoid")
                    .clicked()
                    && current_kind != "humanoid"
                {
                    live.0.body = AvatarBody::Humanoid {
                        phenotype: HumanoidPhenotype::default(),
                        kinematics: HumanoidKinematics::default(),
                    };
                }
            });

            ui.separator();

            match &mut live.0.body {
                AvatarBody::HoverRover {
                    phenotype,
                    kinematics,
                } => {
                    rover_panel(ui, phenotype, kinematics);
                }
                AvatarBody::Humanoid {
                    phenotype,
                    kinematics,
                } => {
                    humanoid_panel(ui, phenotype, kinematics);
                }
                AvatarBody::Unknown => {
                    ui.colored_label(
                        egui::Color32::ORANGE,
                        "This avatar was authored against a newer schema — pick a body type above to replace it.",
                    );
                }
            }

            ui.separator();

            // --- Networking (local-only, not broadcast) ----------------
            egui::CollapsingHeader::new("Networking")
                .default_open(false)
                .show(ui, |ui| {
                    ui.checkbox(
                        &mut settings.smooth_kinematics,
                        "Smooth remote peers (Hermite spline + 100 ms buffer)",
                    );
                    ui.label(
                        egui::RichText::new(
                            "Uncheck to snap to the latest packet and expose raw jitter.",
                        )
                        .small()
                        .weak(),
                    );
                });

            ui.separator();

            // --- Publish / Revert --------------------------------------
            let (is_dirty, can_revert) = if let Some(stored) = &stored {
                let dirty = stored.0 != live.0;
                (dirty, dirty)
            } else {
                (false, false)
            };

            ui.horizontal(|ui| {
                let publish_button = egui::Button::new(
                    egui::RichText::new("Publish to PDS").color(if is_dirty {
                        egui::Color32::LIGHT_GREEN
                    } else {
                        egui::Color32::GRAY
                    }),
                );
                let publish_enabled = is_dirty && session.is_some();
                if ui.add_enabled(publish_enabled, publish_button).clicked()
                    && let Some(session) = session.as_ref()
                {
                    // Flip to `Publishing` the same frame the click fires so
                    // the user gets immediate visual confirmation; without
                    // this, the panel stays on `Idle` for the full PDS
                    // round-trip and the click looks like it was dropped.
                    *feedback = PublishFeedback::Publishing;
                    spawn_publish_avatar_task(&mut commands, session, live.0.clone());
                }

                if ui
                    .add_enabled(can_revert, egui::Button::new("Revert"))
                    .clicked()
                    && let Some(stored) = &stored
                {
                    live.0 = stored.0.clone();
                }
            });

            // --- Status line ------------------------------------------
            match &*feedback {
                PublishFeedback::Idle => {}
                PublishFeedback::Publishing => {
                    ui.label(egui::RichText::new("Publishing…").italics().weak());
                }
                PublishFeedback::Success { .. } => {
                    ui.colored_label(egui::Color32::LIGHT_GREEN, "Published ✓");
                }
                PublishFeedback::Failed { message, .. } => {
                    ui.colored_label(
                        egui::Color32::LIGHT_RED,
                        format!("Publish failed: {message}"),
                    );
                }
            }
        });
}

fn rover_panel(ui: &mut egui::Ui, phen: &mut RoverPhenotype, kin: &mut RoverKinematics) {
    egui::CollapsingHeader::new("Hull")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Length (m)");
            fp_slider(ui, &mut phen.hull_length, 1.0..=5.0, 0.05);
            ui.label("Width (m)");
            fp_slider(ui, &mut phen.hull_width, 0.6..=3.0, 0.05);
            ui.label("Keel depth (m)");
            fp_slider(ui, &mut phen.hull_depth, 0.1..=1.5, 0.05);
        });

    egui::CollapsingHeader::new("Pontoons")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Shape");
            let shape_label = match phen.pontoon_shape {
                PontoonShape::Capsule => "Capsule",
                PontoonShape::VHull => "V-Hull",
            };
            egui::ComboBox::from_id_salt("pontoon_shape")
                .selected_text(shape_label)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut phen.pontoon_shape, PontoonShape::Capsule, "Capsule");
                    ui.selectable_value(&mut phen.pontoon_shape, PontoonShape::VHull, "V-Hull");
                });

            ui.label("Spread (m)");
            fp_slider(ui, &mut phen.pontoon_spread, 0.5..=2.5, 0.05);
            ui.label("Length (m)");
            fp_slider(ui, &mut phen.pontoon_length, 0.5..=3.5, 0.05);
            ui.label("Width (m)");
            fp_slider(ui, &mut phen.pontoon_width, 0.05..=1.0, 0.01);
            ui.label("Height (m)");
            fp_slider(ui, &mut phen.pontoon_height, 0.05..=1.0, 0.01);
            ui.label("Strut drop (% keel)");
            fp_slider(ui, &mut phen.strut_drop, 0.0..=1.0, 0.01);
        });

    egui::CollapsingHeader::new("Mast & Sail")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Mast height (m)");
            fp_slider(ui, &mut phen.mast_height, 0.3..=2.5, 0.05);
            ui.label("Mast radius (m)");
            fp_slider(ui, &mut phen.mast_radius, 0.01..=0.2, 0.005);
            ui.label("Mast offset X (m)");
            fp_slider_array(ui, &mut phen.mast_offset.0[0], -1.5..=1.5, 0.05);
            ui.label("Mast offset Z (m)");
            fp_slider_array(ui, &mut phen.mast_offset.0[1], -2.0..=2.0, 0.05);
            ui.label("Sail size (m)");
            fp_slider(ui, &mut phen.sail_size, 0.3..=2.5, 0.05);
        });

    egui::CollapsingHeader::new("Materials")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Metallic");
            fp_slider(ui, &mut phen.metallic, 0.0..=1.0, 0.01);
            ui.label("Roughness");
            fp_slider(ui, &mut phen.roughness, 0.0..=1.0, 0.01);
            ui.collapsing("Hull colour", |ui| color_sliders(ui, &mut phen.hull_color));
            ui.collapsing("Pontoon colour", |ui| {
                color_sliders(ui, &mut phen.pontoon_color)
            });
            ui.collapsing("Mast colour", |ui| color_sliders(ui, &mut phen.mast_color));
            ui.collapsing("Strut colour", |ui| {
                color_sliders(ui, &mut phen.strut_color)
            });
        });

    egui::CollapsingHeader::new("Suspension & Drive")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Suspension rest length");
            fp_slider(ui, &mut kin.suspension_rest_length, 0.2..=2.0, 0.05);
            ui.label("Suspension stiffness");
            fp_slider(ui, &mut kin.suspension_stiffness, 500.0..=15_000.0, 50.0);
            ui.label("Suspension damping");
            fp_slider(ui, &mut kin.suspension_damping, 10.0..=500.0, 5.0);
            ui.label("Drive force");
            fp_slider(ui, &mut kin.drive_force, 500.0..=10_000.0, 50.0);
            ui.label("Turn torque");
            fp_slider(ui, &mut kin.turn_torque, 200.0..=6_000.0, 50.0);
            ui.label("Lateral grip");
            fp_slider(ui, &mut kin.lateral_grip, 500.0..=15_000.0, 100.0);
            ui.label("Jump force");
            fp_slider(ui, &mut kin.jump_force, 500.0..=8_000.0, 50.0);
            ui.label("Uprighting torque");
            fp_slider(ui, &mut kin.uprighting_torque, 100.0..=3_000.0, 50.0);
        });

    egui::CollapsingHeader::new("Chassis")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Mass (kg)");
            fp_slider(ui, &mut kin.mass, 5.0..=200.0, 1.0);
            ui.label("Linear damping");
            fp_slider(ui, &mut kin.linear_damping, 0.0..=10.0, 0.1);
            ui.label("Angular damping");
            fp_slider(ui, &mut kin.angular_damping, 0.0..=20.0, 0.1);
        });

    egui::CollapsingHeader::new("Buoyancy")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Water rest length (m)");
            fp_slider(ui, &mut kin.water_rest_length, 0.0..=3.0, 0.05);
            ui.label("Strength (N/m)");
            fp_slider(ui, &mut kin.buoyancy_strength, 0.0..=10_000.0, 50.0);
            ui.label("Damping (N·s/m)");
            fp_slider(ui, &mut kin.buoyancy_damping, 0.0..=2_000.0, 10.0);
            ui.label("Max depth (m)");
            fp_slider(ui, &mut kin.buoyancy_max_depth, 0.1..=5.0, 0.05);
        });
}

fn humanoid_panel(ui: &mut egui::Ui, phen: &mut HumanoidPhenotype, kin: &mut HumanoidKinematics) {
    egui::CollapsingHeader::new("Body")
        .default_open(true)
        .show(ui, |ui| {
            ui.label("Height (m)");
            fp_slider(ui, &mut phen.height, 0.4..=3.0, 0.05);
            ui.label("Torso half-width (m)");
            fp_slider(ui, &mut phen.torso_half_width, 0.1..=0.5, 0.01);
            ui.label("Torso half-depth (m)");
            fp_slider(ui, &mut phen.torso_half_depth, 0.05..=0.4, 0.01);
            ui.label("Head size (m)");
            fp_slider(ui, &mut phen.head_size, 0.1..=0.5, 0.01);
            ui.label("Limb thickness (m)");
            fp_slider(ui, &mut phen.limb_thickness, 0.04..=0.25, 0.005);
            ui.label("Arm length (× torso)");
            fp_slider(ui, &mut phen.arm_length_ratio, 0.5..=1.5, 0.01);
            ui.label("Leg length (× height)");
            fp_slider(ui, &mut phen.leg_length_ratio, 0.3..=0.6, 0.01);
            ui.checkbox(&mut phen.show_badge, "Show profile badge");
        });

    egui::CollapsingHeader::new("Materials")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Metallic");
            fp_slider(ui, &mut phen.metallic, 0.0..=1.0, 0.01);
            ui.label("Roughness");
            fp_slider(ui, &mut phen.roughness, 0.0..=1.0, 0.01);
            ui.collapsing("Body colour", |ui| color_sliders(ui, &mut phen.body_color));
            ui.collapsing("Head colour", |ui| color_sliders(ui, &mut phen.head_color));
            ui.collapsing("Limb colour", |ui| color_sliders(ui, &mut phen.limb_color));
        });

    egui::CollapsingHeader::new("Locomotion")
        .default_open(false)
        .show(ui, |ui| {
            ui.label("Walk speed (m/s)");
            fp_slider(ui, &mut kin.walk_speed, 1.0..=10.0, 0.1);
            ui.label("Acceleration (1/s)");
            fp_slider(ui, &mut kin.acceleration, 2.0..=30.0, 0.5);
            ui.label("Jump impulse (N·s)");
            fp_slider(ui, &mut kin.jump_impulse, 100.0..=1500.0, 10.0);
            ui.label("Mass (kg)");
            fp_slider(ui, &mut kin.mass, 30.0..=150.0, 1.0);
            ui.label("Linear damping");
            fp_slider(ui, &mut kin.linear_damping, 0.0..=3.0, 0.05);
        });
}

fn fp_slider(ui: &mut egui::Ui, value: &mut Fp, range: std::ops::RangeInclusive<f32>, step: f64) {
    ui.add(egui::Slider::new(&mut value.0, range).step_by(step));
}

fn fp_slider_array(
    ui: &mut egui::Ui,
    value: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    step: f64,
) {
    ui.add(egui::Slider::new(value, range).step_by(step));
}

fn color_sliders(ui: &mut egui::Ui, rgb: &mut Fp3) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("R").color(egui::Color32::from_rgb(220, 80, 80)));
        ui.add(egui::Slider::new(&mut rgb.0[0], 0.0..=1.0).step_by(0.01));
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("G").color(egui::Color32::from_rgb(80, 200, 80)));
        ui.add(egui::Slider::new(&mut rgb.0[1], 0.0..=1.0).step_by(0.01));
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("B").color(egui::Color32::from_rgb(80, 130, 220)));
        ui.add(egui::Slider::new(&mut rgb.0[2], 0.0..=1.0).step_by(0.01));
    });

    let swatch = egui::Color32::from_rgb(
        (rgb.0[0] * 255.0) as u8,
        (rgb.0[1] * 255.0) as u8,
        (rgb.0[2] * 255.0) as u8,
    );
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 14.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, 3.0, swatch);
}

fn spawn_publish_avatar_task(
    commands: &mut Commands,
    session: &AtprotoSession,
    record: AvatarRecord,
) {
    let session_clone = session.clone();
    let pool = bevy::tasks::IoTaskPool::get();
    let task = pool.spawn(async move {
        let fut = async {
            let client = crate::config::http::default_client();
            pds::publish_avatar_record(&client, &session_clone, &record).await
        };
        #[cfg(target_arch = "wasm32")]
        {
            fut.await
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap()
                .block_on(fut)
        }
    });
    commands.spawn(PublishAvatarTask(task));
}

/// Poll outstanding avatar publish tasks. On success, sync `LiveAvatarRecord`
/// into `StoredAvatarRecord` so the "Revert" button is disabled until the
/// next edit.
pub fn poll_publish_avatar_tasks(
    mut commands: Commands,
    mut tasks: Query<(Entity, &mut PublishAvatarTask)>,
    live: Res<LiveAvatarRecord>,
    mut stored: Option<ResMut<StoredAvatarRecord>>,
    mut feedback: ResMut<PublishFeedback>,
    time: Res<Time>,
) {
    for (entity, mut task) in tasks.iter_mut() {
        let Some(result) =
            futures_lite::future::block_on(futures_lite::future::poll_once(&mut task.0))
        else {
            continue;
        };
        commands.entity(entity).despawn();
        let now = time.elapsed_secs_f64();
        match result {
            Ok(()) => {
                info!("Avatar record saved to PDS");
                if let Some(stored) = stored.as_mut() {
                    stored.0 = live.0.clone();
                }
                *feedback = PublishFeedback::Success { at_secs: now };
            }
            Err(e) => {
                warn!("Failed to save avatar record: {}", e);
                *feedback = PublishFeedback::Failed {
                    at_secs: now,
                    message: e,
                };
            }
        }
    }
}
