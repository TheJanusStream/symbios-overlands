//! Particle-emitter detail panel and its supporting variant pickers
//! (emitter shape, blend mode, simulation space, frame mode, texture
//! filter, atlas + texture sub-panel).

use bevy_egui::egui;

use crate::pds::{
    AnimationFrameMode, EmitterShape, Fp, Fp3, Fp4, ParticleBlendMode, SignSource, SimulationSpace,
    TextureAtlas, TextureFilter,
};

use super::super::widgets::{color_picker_rgba, drag_u32, fp_slider};
use super::sign::draw_sign_source;

/// Editor for [`crate::pds::GeneratorKind::ParticleSystem`]. Groups the
/// (large) parameter set into collapsible sections so the panel stays
/// browseable without scrolling: Emitter shape, Spawn, Lifetime / Speed,
/// Dynamics, Visuals, Texture, Inheritance, Collisions. Every parameter is
/// surfaced; the sanitiser owns the bounds.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_generator_particles(
    ui: &mut egui::Ui,
    emitter_shape: &mut EmitterShape,
    rate_per_second: &mut Fp,
    burst_count: &mut u32,
    max_particles: &mut u32,
    looping: &mut bool,
    duration: &mut Fp,
    lifetime_min: &mut Fp,
    lifetime_max: &mut Fp,
    speed_min: &mut Fp,
    speed_max: &mut Fp,
    gravity_multiplier: &mut Fp,
    acceleration: &mut Fp3,
    linear_drag: &mut Fp,
    start_size: &mut Fp,
    end_size: &mut Fp,
    start_color: &mut Fp4,
    end_color: &mut Fp4,
    blend_mode: &mut ParticleBlendMode,
    billboard: &mut bool,
    simulation_space: &mut SimulationSpace,
    inherit_velocity: &mut Fp,
    collide_terrain: &mut bool,
    collide_water: &mut bool,
    collide_colliders: &mut bool,
    bounce: &mut Fp,
    friction: &mut Fp,
    seed: &mut u64,
    texture: &mut Option<SignSource>,
    texture_atlas: &mut Option<TextureAtlas>,
    frame_mode: &mut AnimationFrameMode,
    texture_filter: &mut TextureFilter,
    salt: &str,
    dirty: &mut bool,
) {
    egui::CollapsingHeader::new("Emitter shape")
        .id_salt(format!("{}_pe_shape", salt))
        .default_open(true)
        .show(ui, |ui| draw_emitter_shape(ui, emitter_shape, salt, dirty));

    egui::CollapsingHeader::new("Spawn")
        .id_salt(format!("{}_pe_spawn", salt))
        .default_open(true)
        .show(ui, |ui| {
            fp_slider(ui, "Rate (per s)", rate_per_second, 0.0, 256.0, dirty);
            drag_u32(ui, "Burst count", burst_count, 0, 512, dirty);
            drag_u32(ui, "Max particles", max_particles, 0, 512, dirty);
            if ui.checkbox(looping, "Looping").changed() {
                *dirty = true;
            }
            fp_slider(ui, "Duration (s)", duration, 0.01, 600.0, dirty);
        });

    egui::CollapsingHeader::new("Lifetime & speed")
        .id_salt(format!("{}_pe_life", salt))
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(ui, "Lifetime min", lifetime_min, 0.01, 30.0, dirty);
            fp_slider(ui, "Lifetime max", lifetime_max, 0.01, 30.0, dirty);
            fp_slider(ui, "Speed min", speed_min, 0.0, 100.0, dirty);
            fp_slider(ui, "Speed max", speed_max, 0.0, 100.0, dirty);
        });

    egui::CollapsingHeader::new("Dynamics")
        .id_salt(format!("{}_pe_dyn", salt))
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Gravity multiplier",
                gravity_multiplier,
                -10.0,
                10.0,
                dirty,
            );
            ui.label("Acceleration X/Y/Z (m/s²)");
            let mut v = acceleration.0;
            let mut changed = false;
            ui.horizontal(|ui| {
                for axis in v.iter_mut() {
                    changed |= ui
                        .add(egui::DragValue::new(axis).speed(0.1).range(-100.0..=100.0))
                        .changed();
                }
            });
            if changed {
                *acceleration = Fp3(v);
                *dirty = true;
            }
            fp_slider(ui, "Linear drag", linear_drag, 0.0, 100.0, dirty);
        });

    egui::CollapsingHeader::new("Visuals")
        .id_salt(format!("{}_pe_vis", salt))
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(ui, "Start size", start_size, 0.001, 100.0, dirty);
            fp_slider(ui, "End size", end_size, 0.001, 100.0, dirty);
            color_picker_rgba(ui, "Start colour", start_color, dirty);
            color_picker_rgba(ui, "End colour", end_color, dirty);
            draw_blend_mode(ui, blend_mode, salt, dirty);
            if ui.checkbox(billboard, "Billboard (face camera)").changed() {
                *dirty = true;
            }
        });

    egui::CollapsingHeader::new("Inheritance & space")
        .id_salt(format!("{}_pe_inh", salt))
        .default_open(false)
        .show(ui, |ui| {
            draw_simulation_space(ui, simulation_space, salt, dirty);
            fp_slider(ui, "Inherit velocity", inherit_velocity, 0.0, 2.0, dirty);
        });

    egui::CollapsingHeader::new("Collisions")
        .id_salt(format!("{}_pe_col", salt))
        .default_open(false)
        .show(ui, |ui| {
            if ui.checkbox(collide_terrain, "Collide terrain").changed() {
                *dirty = true;
            }
            if ui.checkbox(collide_water, "Collide water").changed() {
                *dirty = true;
            }
            if ui
                .checkbox(collide_colliders, "Collide colliders")
                .changed()
            {
                *dirty = true;
            }
            fp_slider(ui, "Bounce", bounce, 0.0, 1.0, dirty);
            fp_slider(ui, "Friction", friction, 0.0, 1.0, dirty);
        });

    egui::CollapsingHeader::new("Texture")
        .id_salt(format!("{}_pe_tex", salt))
        .default_open(false)
        .show(ui, |ui| {
            draw_particle_texture(
                ui,
                texture,
                texture_atlas,
                frame_mode,
                texture_filter,
                salt,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Determinism")
        .id_salt(format!("{}_pe_det", salt))
        .default_open(false)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Seed:");
                let mut s = seed.to_string();
                if ui.add(egui::TextEdit::singleline(&mut s)).changed()
                    && let Ok(parsed) = s.parse::<u64>()
                {
                    *seed = parsed;
                    *dirty = true;
                }
            });
        });
}

/// Texture controls for a particle emitter: optional source picker
/// (reusing the Sign variant widget), atlas rows/cols, frame-cycling
/// mode, and sampler-filter combo. `None` for `texture` is the v1
/// "coloured quads only" baseline; setting a source switches to the
/// textured-quad path.
fn draw_particle_texture(
    ui: &mut egui::Ui,
    texture: &mut Option<SignSource>,
    texture_atlas: &mut Option<TextureAtlas>,
    frame_mode: &mut AnimationFrameMode,
    texture_filter: &mut TextureFilter,
    salt: &str,
    dirty: &mut bool,
) {
    let mut has_texture = texture.is_some();
    if ui
        .checkbox(&mut has_texture, "Use texture (else coloured quads only)")
        .changed()
    {
        *texture = if has_texture {
            Some(SignSource::default())
        } else {
            None
        };
        *dirty = true;
    }

    let Some(source) = texture else {
        // No texture configured. Atlas / frame mode / filter still
        // serialise — but they have no effect, so we hide the editors
        // to avoid confusing the author.
        return;
    };

    draw_sign_source(ui, source, &format!("{}_pe_texsrc", salt), dirty);
    ui.add_space(4.0);

    let mut has_atlas = texture_atlas.is_some();
    if ui
        .checkbox(&mut has_atlas, "Use sprite-sheet atlas")
        .changed()
    {
        *texture_atlas = if has_atlas {
            Some(TextureAtlas::default())
        } else {
            None
        };
        *dirty = true;
    }
    if let Some(atlas) = texture_atlas {
        ui.horizontal(|ui| {
            drag_u32(ui, "Rows", &mut atlas.rows, 1, 16, dirty);
            drag_u32(ui, "Cols", &mut atlas.cols, 1, 16, dirty);
        });
    }

    draw_frame_mode(ui, frame_mode, salt, dirty);
    draw_texture_filter(ui, texture_filter, salt, dirty);
}

/// Frame-mode combo: switching variants reseeds the OverLifetime fps
/// to a sensible default (8 fps) so the user lands somewhere visible.
fn draw_frame_mode(ui: &mut egui::Ui, mode: &mut AnimationFrameMode, salt: &str, dirty: &mut bool) {
    let current = match mode {
        AnimationFrameMode::Still => "Still",
        AnimationFrameMode::RandomFrame => "Random per particle",
        AnimationFrameMode::OverLifetime { .. } => "Cycle over lifetime",
        AnimationFrameMode::Unknown => "Unknown",
    };
    ui.horizontal(|ui| {
        ui.label("Frame mode:");
        egui::ComboBox::from_id_salt(format!("{}_pe_frame", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "Still", "Still").clicked()
                    && !matches!(mode, AnimationFrameMode::Still)
                {
                    *mode = AnimationFrameMode::Still;
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "Random per particle", "Random per particle")
                    .clicked()
                    && !matches!(mode, AnimationFrameMode::RandomFrame)
                {
                    *mode = AnimationFrameMode::RandomFrame;
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "Cycle over lifetime", "Cycle over lifetime")
                    .clicked()
                    && !matches!(mode, AnimationFrameMode::OverLifetime { .. })
                {
                    *mode = AnimationFrameMode::OverLifetime { fps: Fp(8.0) };
                    *dirty = true;
                }
            });
    });
    if let AnimationFrameMode::OverLifetime { fps } = mode {
        fp_slider(ui, "FPS", fps, 0.0, 60.0, dirty);
    }
}

/// Texture-filter combo for the loaded atlas image.
fn draw_texture_filter(
    ui: &mut egui::Ui,
    filter: &mut TextureFilter,
    salt: &str,
    dirty: &mut bool,
) {
    let current = match filter {
        TextureFilter::Linear => "Linear (smooth)",
        TextureFilter::Nearest => "Nearest (pixel-art)",
        TextureFilter::Unknown => "Unknown",
    };
    ui.horizontal(|ui| {
        ui.label("Sampler:");
        egui::ComboBox::from_id_salt(format!("{}_pe_filter", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(current == "Linear (smooth)", "Linear (smooth)")
                    .clicked()
                    && !matches!(filter, TextureFilter::Linear)
                {
                    *filter = TextureFilter::Linear;
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "Nearest (pixel-art)", "Nearest (pixel-art)")
                    .clicked()
                    && !matches!(filter, TextureFilter::Nearest)
                {
                    *filter = TextureFilter::Nearest;
                    *dirty = true;
                }
            });
    });
}

/// Combo + per-variant payload editor for [`EmitterShape`]. Switching
/// variants reseeds the payload from `default_particles`-style defaults
/// so the user always lands on a sensible starting point.
fn draw_emitter_shape(ui: &mut egui::Ui, shape: &mut EmitterShape, salt: &str, dirty: &mut bool) {
    let current = match shape {
        EmitterShape::Point => "Point",
        EmitterShape::Sphere { .. } => "Sphere",
        EmitterShape::Box { .. } => "Box",
        EmitterShape::Cone { .. } => "Cone",
        EmitterShape::Unknown => "Unknown",
    };
    ui.horizontal(|ui| {
        ui.label("Shape:");
        egui::ComboBox::from_id_salt(format!("{}_pe_shape_combo", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "Point", "Point").clicked()
                    && !matches!(shape, EmitterShape::Point)
                {
                    *shape = EmitterShape::Point;
                    *dirty = true;
                }
                if ui.selectable_label(current == "Sphere", "Sphere").clicked()
                    && !matches!(shape, EmitterShape::Sphere { .. })
                {
                    *shape = EmitterShape::Sphere { radius: Fp(0.5) };
                    *dirty = true;
                }
                if ui.selectable_label(current == "Box", "Box").clicked()
                    && !matches!(shape, EmitterShape::Box { .. })
                {
                    *shape = EmitterShape::Box {
                        half_extents: Fp3([0.5, 0.5, 0.5]),
                    };
                    *dirty = true;
                }
                if ui.selectable_label(current == "Cone", "Cone").clicked()
                    && !matches!(shape, EmitterShape::Cone { .. })
                {
                    *shape = EmitterShape::Cone {
                        half_angle: Fp(0.4),
                        height: Fp(0.5),
                    };
                    *dirty = true;
                }
            });
    });

    match shape {
        EmitterShape::Sphere { radius } => {
            fp_slider(ui, "Radius", radius, 0.0, 100.0, dirty);
        }
        EmitterShape::Box { half_extents } => {
            ui.label("Half extents X/Y/Z");
            let mut v = half_extents.0;
            let mut changed = false;
            ui.horizontal(|ui| {
                for axis in v.iter_mut() {
                    changed |= ui
                        .add(egui::DragValue::new(axis).speed(0.05).range(0.0..=100.0))
                        .changed();
                }
            });
            if changed {
                *half_extents = Fp3(v);
                *dirty = true;
            }
        }
        EmitterShape::Cone { half_angle, height } => {
            fp_slider(
                ui,
                "Half angle (rad)",
                half_angle,
                0.0,
                std::f32::consts::PI,
                dirty,
            );
            fp_slider(ui, "Height", height, 0.0, 100.0, dirty);
        }
        EmitterShape::Point | EmitterShape::Unknown => {}
    }
}

/// Combo for [`ParticleBlendMode`].
fn draw_blend_mode(ui: &mut egui::Ui, mode: &mut ParticleBlendMode, salt: &str, dirty: &mut bool) {
    let current = match mode {
        ParticleBlendMode::Alpha => "Alpha",
        ParticleBlendMode::Additive => "Additive",
        ParticleBlendMode::Unknown => "Unknown",
    };
    ui.horizontal(|ui| {
        ui.label("Blend:");
        egui::ComboBox::from_id_salt(format!("{}_pe_blend", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "Alpha", "Alpha").clicked()
                    && !matches!(mode, ParticleBlendMode::Alpha)
                {
                    *mode = ParticleBlendMode::Alpha;
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "Additive", "Additive")
                    .clicked()
                    && !matches!(mode, ParticleBlendMode::Additive)
                {
                    *mode = ParticleBlendMode::Additive;
                    *dirty = true;
                }
            });
    });
}

/// Combo for [`SimulationSpace`].
fn draw_simulation_space(
    ui: &mut egui::Ui,
    space: &mut SimulationSpace,
    salt: &str,
    dirty: &mut bool,
) {
    let current = match space {
        SimulationSpace::World => "World",
        SimulationSpace::Local => "Local",
        SimulationSpace::Unknown => "Unknown",
    };
    ui.horizontal(|ui| {
        ui.label("Simulation space:");
        egui::ComboBox::from_id_salt(format!("{}_pe_space", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "World", "World").clicked()
                    && !matches!(space, SimulationSpace::World)
                {
                    *space = SimulationSpace::World;
                    *dirty = true;
                }
                if ui.selectable_label(current == "Local", "Local").clicked()
                    && !matches!(space, SimulationSpace::Local)
                {
                    *space = SimulationSpace::Local;
                    *dirty = true;
                }
            });
    });
}
