//! Room-editor tab for authored avatar-world contact effects (#246).
//!
//! Persistent master-detail (#825 / W4): recipe list on the left with
//! the Add action above it, the selected recipe's editor on the right —
//! the same split-panel layout as the Region Assets and Placements
//! tabs. Edits [`crate::pds::ContactEffects`] in place; any change
//! flips the shared `dirty` flag, and the world compiler's
//! `apply_contact_recipes` rebuilds the runtime registry on the live
//! record's next debounce flush — edits apply LIVE, no publish needed.

use bevy_egui::egui;

use crate::pds::contact_effects::{
    AudioClipSource, AudioParams, ContactEffectKind, ContactEffectRecord, ContactEffects,
    ContactPhaseKind, ContactSurfaceKind, DecalParams,
};
use crate::pds::generator::EmitterShape;
use crate::pds::types::{Fp, Fp3};

use crate::pds::sanitize::limits;

use super::widgets::{
    color_picker, color_picker_rgba, drag_u32, fp_range_sliders, fp_slider, fp_slider_log,
};

/// A reasonable starting point for a freshly-added recipe (a copy of
/// the canonical splash, renamed) so "Add" yields something that
/// already works.
fn new_recipe(existing: &[ContactEffectRecord]) -> ContactEffectRecord {
    let mut r = crate::pds::default_contact_effects().recipes.swap_remove(0);
    r.name = next_effect_name(existing);
    r
}

/// The lowest unused `effect_N` (#1253 f321).
///
/// The suffix used to be `recipes.len()` — a COUNT, not a counter — so
/// adding three, deleting the middle one and adding another produced two
/// rows both called `effect_2`. In a master-detail list the row label is the
/// whole navigational affordance, and nothing else distinguishes them:
/// cooldown state is keyed by position, and the sanitiser's over-64
/// truncation sorts BY NAME, so duplicates make which survivor is dropped
/// arbitrary.
fn next_effect_name(existing: &[ContactEffectRecord]) -> String {
    (0..)
        .map(|n| format!("effect_{n}"))
        .find(|candidate| !existing.iter().any(|r| &r.name == candidate))
        .unwrap_or_else(|| String::from("effect"))
}

pub(super) fn draw_contact_effects_tab(
    ui: &mut egui::Ui,
    effects: &mut ContactEffects,
    selected: &mut Option<usize>,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
    muted: &mut crate::audio_mute::AudioMuted,
) {
    // The worst path in #1252 f303 is the FIRST one: `AudioMuted` defaults
    // to true, so a brand-new owner's very first correct cue is silent and
    // indistinguishable from an empty URL, a dead host and a wrong
    // container. The banner names the one cause the owner cannot deduce
    // from anything on this tab, and offers the same toggle the toolbar
    // has rather than sending them to look for it.
    if muted.0 {
        ui.horizontal(|ui| {
            ui.label(
                egui::RichText::new(
                    "Sound is muted for the whole app, so nothing here will be heard.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).status.warn),
            );
            if ui.small_button("Unmute").clicked() {
                muted.0 = false;
            }
        });
        ui.add_space(2.0);
    }
    // Drop a selection whose row vanished (delete, Load-from-PDS shrink).
    if selected.is_some_and(|i| i >= effects.recipes.len()) {
        *selected = None;
    }

    egui::Panel::left("effects_list_panel")
        .resizable(true)
        .default_size(260.0)
        .min_size(180.0)
        .show(ui, |ui| {
            // Add action ABOVE the list (#825), refused at the cap with the
            // reason (#1210): a 65th recipe used to be pushed, and the next
            // flush re-sorted the whole list alphabetically and dropped one
            // — an unannounced reorder of the authored order plus a
            // deletion. With the add refused here the sort never runs.
            let cap = crate::ui::room::caps::Cap::Recipes;
            let full = cap.is_full(effects.recipes.len());
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(!full, egui::Button::new("+ Add recipe"))
                    .on_disabled_hover_text(cap.full_reason())
                    .clicked()
                {
                    effects.recipes.push(new_recipe(&effects.recipes));
                    *selected = Some(effects.recipes.len() - 1);
                    *dirty = true;
                }
                let (count_text, tone) = cap.readout(effects.recipes.len());
                ui.label(
                    egui::RichText::new(count_text)
                        .small()
                        .color(crate::ui::room::caps::tone_color(ui, tone)),
                );
            });
            // A ROOM-WIDE ceiling, not a property of the selected recipe
            // (#1253 f306) — and its floor is 1, not 0. At 0 the particle
            // dispatcher breaks out before spawning anything, for every
            // sample and every recipe, while each row still reads Enabled
            // and this tab still tells the owner to test by touching the
            // surface: a single unlabelled number in the list column acting
            // as an undiscoverable kill switch for the whole channel.
            ui.separator();
            ui.label(
                egui::RichText::new("Room limits")
                    .small()
                    .strong()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            let mut per_frame = effects.max_particles_per_frame;
            drag_u32(ui, "Max particles / frame", &mut per_frame, 1, 4096, dirty).on_hover_text(
                "A ceiling across every recipe and every avatar in this room, not \
                 a setting on one recipe. Lower it if effects are costing frames.",
            );
            effects.max_particles_per_frame = per_frame;
            ui.separator();

            let mut remove: Option<usize> = None;
            egui::ScrollArea::vertical()
                .id_salt("effects_list")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    if effects.recipes.is_empty() {
                        ui.label(
                            egui::RichText::new("(no recipes — click + Add recipe above)")
                                .small()
                                .color(crate::ui::theme::current(ui.ctx()).text_weak),
                        );
                    }
                    for (i, r) in effects.recipes.iter().enumerate() {
                        // The two fields an owner sorts by (#1253 f313):
                        // whether it runs at all — the runtime's own
                        // designer kill-switch, and therefore the field they
                        // toggle most while debugging "why is nothing
                        // happening" — and what kind of effect it is. Both
                        // were visible only in the detail pane, one click at
                        // a time. The disabled marker is a GLYPH as well as
                        // a tint, per the theming rules: never colour alone.
                        let label = format!(
                            "{}{}  ({}, {}, {})",
                            if r.enabled {
                                String::new()
                            } else {
                                format!("{} ", crate::ui::affordances::CROSS)
                            },
                            r.name,
                            effect_kind_label(&r.effect),
                            surface_label(r.surface),
                            phase_label(r.phase),
                        );
                        let label = if r.enabled {
                            egui::RichText::new(label)
                        } else {
                            egui::RichText::new(label)
                                .color(crate::ui::theme::current(ui.ctx()).text_weak)
                        };
                        ui.horizontal(|ui| {
                            if ui.selectable_label(*selected == Some(i), label).clicked() {
                                *selected = Some(i);
                            }
                            if crate::ui::affordances::remove_button(ui, "Remove this recipe")
                                .clicked()
                            {
                                remove = Some(i);
                            }
                        });
                    }
                });
            if let Some(i) = remove {
                effects.recipes.remove(i);
                *selected = match *selected {
                    Some(s) if s == i => None,
                    Some(s) if s > i => Some(s - 1),
                    other => other,
                };
                *dirty = true;
            }
        });

    egui::CentralPanel::default().show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("effect_detail")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.label(
                    egui::RichText::new(
                        "Particle bursts, decals and audio cues triggered when an \
                         avatar contacts a surface (e.g. a boat hitting water). \
                         Edits apply live — trigger the effect by touching the \
                         surface.",
                    )
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
                );
                ui.add_space(4.0);
                let Some(i) = *selected else {
                    ui.label(
                        egui::RichText::new("Select a recipe on the left.")
                            .small()
                            .color(crate::ui::theme::current(ui.ctx()).text_weak),
                    );
                    return;
                };
                let Some(r) = effects.recipes.get_mut(i) else {
                    return;
                };
                draw_recipe_detail(ui, i, r, dirty, assets);
            });
    });
}

/// The selected recipe's full editor — everything that used to live in
/// the per-recipe `CollapsingHeader` body before the split (#825).
fn draw_recipe_detail(
    ui: &mut egui::Ui,
    i: usize,
    r: &mut ContactEffectRecord,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    ui.horizontal(|ui| {
        ui.label("Name");
        if ui.text_edit_singleline(&mut r.name).changed() {
            *dirty = true;
        }
    });
    if ui.checkbox(&mut r.enabled, "Enabled").changed() {
        *dirty = true;
    }

    ui.collapsing("Trigger", |ui| {
        surface_combo(ui, i, &mut r.surface, dirty);
        phase_combo(ui, i, &mut r.phase, dirty);
        // Widened to the bound the record actually enforces (#1254 f317),
        // on a log track so the useful low end keeps its resolution. The
        // stated convention for these editors is "ranges mirror
        // `pds::sanitize::limits`", and a value the format permits but the
        // GUI cannot reach sends the owner to the Raw JSON tab — where the
        // number they set is then DISPLAYED pinned at the old maximum,
        // indistinguishable from one legitimately there.
        fp_slider_log(
            ui,
            "Min speed (m/s)",
            &mut r.min_speed,
            0.0,
            limits::MAX_CONTACT_MIN_SPEED,
            dirty,
        );
        fp_slider(ui, "Min intensity", &mut r.min_intensity, 0.0, 1.0, dirty);
    });

    fp_slider_log(
        ui,
        "Cooldown (s)",
        &mut r.cooldown,
        0.0,
        limits::MAX_CONTACT_COOLDOWN,
        dirty,
    )
    .on_hover_text(
        "The shortest gap between two firings of this recipe for one avatar. \
         0 means every matching frame, which is what an Enter or Exit recipe \
         wants and never what a Dwell one does.",
    );
    // The coupling that reaches visitors live, one click away (#1253 f310).
    // A new recipe is born water/Enter/cooldown 0 — safe only for the phase
    // it was born with — and Dwell is emitted every frame by construction,
    // so switching phase turns the default into 24 overlapping voices or a
    // decal blizzard. All three channels consult the cooldown only when it
    // is `> 0.0`; the knowledge lived in one doc comment.
    if let Some(warning) = dwell_cooldown_warning(r.phase, r.cooldown.0) {
        ui.label(
            egui::RichText::new(warning)
                .small()
                .color(crate::ui::theme::current(ui.ctx()).status.warn),
        );
    }

    effect_kind_combo(ui, i, &mut r.effect, dirty);
    match &mut r.effect {
        ContactEffectKind::ParticleBurst {
            count,
            radius_scale,
            velocity_inherit,
            particle,
        } => {
            ui.collapsing("Count = clamp(speed·gain + base, min, max)", |ui| {
                fp_slider(ui, "Gain", &mut count.gain, 0.0, 40.0, dirty);
                fp_slider(ui, "Base", &mut count.base, 0.0, 40.0, dirty);
                // Bounded against each other (#1254 f318). The sanitiser
                // resolved an inversion by LOWERING min to max here and by
                // RAISING max to min for the two `Fp` pairs below — two
                // opposite conventions in one form, applied a quarter
                // second after the drag, so a slider the owner never
                // touched moved on its own and no mental model could be
                // formed from watching it.
                let count_max = count.max;
                drag_u32(ui, "Min", &mut count.min, 0, count_max.min(512), dirty);
                let count_min = count.min;
                drag_u32(ui, "Max", &mut count.max, count_min.min(512), 512, dirty);
            });
            fp_slider(ui, "Radius scale", radius_scale, 0.0, 8.0, dirty);
            fp_slider(ui, "Velocity inherit", velocity_inherit, 0.0, 2.0, dirty);
            ui.collapsing("Particle", |ui| {
                shape_combo(ui, i, &mut particle.shape, dirty);
                fp_range_sliders(
                    ui,
                    "Lifetime min (s)",
                    "Lifetime max (s)",
                    &mut particle.lifetime_min,
                    &mut particle.lifetime_max,
                    0.0,
                    5.0,
                    dirty,
                );
                fp_range_sliders(
                    ui,
                    "Speed min",
                    "Speed max",
                    &mut particle.speed_min,
                    &mut particle.speed_max,
                    0.0,
                    20.0,
                    dirty,
                );
                fp_slider(
                    ui,
                    "Gravity ×",
                    &mut particle.gravity_multiplier,
                    -2.0,
                    2.0,
                    dirty,
                );
                fp_slider(
                    ui,
                    "Linear drag",
                    &mut particle.linear_drag,
                    0.0,
                    5.0,
                    dirty,
                );
                fp_slider(ui, "Start size", &mut particle.start_size, 0.0, 1.0, dirty);
                fp_slider(ui, "End size", &mut particle.end_size, 0.0, 1.0, dirty);
                color_picker_rgba(ui, "Start colour", &mut particle.start_color, dirty);
                color_picker_rgba(ui, "End colour", &mut particle.end_color, dirty);
                if ui.checkbox(&mut particle.billboard, "Billboard").changed() {
                    *dirty = true;
                }
                drag_u32(
                    ui,
                    "Max particles",
                    &mut particle.max_particles,
                    0,
                    512,
                    dirty,
                );
                // Procedural sprite billboard (#367). Reuses the
                // material tab's picker; `allow_referenced =
                // false` because the contact-burst bake path
                // ignores fetched-asset references (same as the
                // ParticleSystem generator's procedural slot).
                // `None` falls back to a flat coloured quad.
                ui.horizontal(|ui| {
                    ui.label("Sprite");
                    super::material::draw_texture_bridge_opts(
                        ui,
                        &mut particle.procedural_texture,
                        &format!("contact_particle_sprite_{i}"),
                        dirty,
                        false,
                        assets,
                    );
                });
            });
        }
        ContactEffectKind::DecalStamp { decal } => {
            decal_form(ui, decal, dirty);
        }
        ContactEffectKind::AudioCue { audio } => {
            audio_form(ui, i, audio, dirty, assets);
        }
        ContactEffectKind::Unknown => {
            super::widgets::unrecognised_value_line(
                ui,
                "effect",
                Some("it is shown read-only and does nothing here"),
            );
        }
    }
}

/// The warning under a Dwell recipe's cooldown (#1253 f310), or `None`
/// when the pair is safe. Pure: the coupling is arithmetic, and this is the
/// only place it is stated to the person who can change it.
fn dwell_cooldown_warning(phase: ContactPhaseKind, cooldown: f32) -> Option<String> {
    (phase == ContactPhaseKind::Dwell && cooldown <= 0.0).then(|| {
        String::from(
            "Dwell fires on every frame an avatar stays in contact, and a cooldown \
             of 0 means nothing throttles it — give it a cooldown, or visitors get \
             one firing per frame for as long as they stand there.",
        )
    })
}

fn surface_label(s: ContactSurfaceKind) -> &'static str {
    match s {
        ContactSurfaceKind::Water => "water",
        ContactSurfaceKind::Terrain => "terrain",
        ContactSurfaceKind::Unknown => "unknown",
    }
}

fn phase_label(p: ContactPhaseKind) -> &'static str {
    match p {
        ContactPhaseKind::Enter => "enter",
        ContactPhaseKind::Dwell => "dwell",
        ContactPhaseKind::Exit => "exit",
        ContactPhaseKind::Unknown => "unknown",
    }
}

fn surface_combo(ui: &mut egui::Ui, salt: usize, s: &mut ContactSurfaceKind, dirty: &mut bool) {
    // Water and terrain are the modelled surfaces (terrain landed in
    // Phase 3, #245). `Unknown` is intentionally not offered — it's a
    // forward-compat deserialize fallback, not an authorable choice.
    ui.horizontal(|ui| {
        ui.label("Surface");
        egui::ComboBox::from_id_salt(("surface", salt))
            .selected_text(surface_label(*s))
            .show_ui(ui, |ui| {
                for opt in [ContactSurfaceKind::Water, ContactSurfaceKind::Terrain] {
                    if ui.selectable_value(s, opt, surface_label(opt)).clicked() {
                        *dirty = true;
                    }
                }
            });
    });
}

fn phase_combo(ui: &mut egui::Ui, salt: usize, p: &mut ContactPhaseKind, dirty: &mut bool) {
    ui.horizontal(|ui| {
        ui.label("Phase");
        egui::ComboBox::from_id_salt(("phase", salt))
            .selected_text(phase_label(*p))
            .show_ui(ui, |ui| {
                for opt in [
                    ContactPhaseKind::Enter,
                    ContactPhaseKind::Dwell,
                    ContactPhaseKind::Exit,
                ] {
                    if ui.selectable_value(p, opt, phase_label(opt)).clicked() {
                        *dirty = true;
                    }
                }
            });
    });
}

/// The canonical ParticleBurst payload (the seeded splash effect), used
/// as the sane default when an author switches a recipe *to* Particle.
fn default_particle_effect() -> ContactEffectKind {
    crate::pds::default_contact_effects()
        .recipes
        .swap_remove(0)
        .effect
}

fn effect_kind_label(e: &ContactEffectKind) -> &'static str {
    match e {
        ContactEffectKind::ParticleBurst { .. } => "particle burst",
        ContactEffectKind::DecalStamp { .. } => "decal",
        ContactEffectKind::AudioCue { .. } => "audio cue",
        ContactEffectKind::Unknown => "unknown",
    }
}

/// Effect-kind picker. Switching kind swaps in that kind's canonical
/// default (so the sub-form below is immediately valid); re-picking the
/// current kind is a no-op. `Unknown` is never offered — it's a
/// forward-compat decode fallback, not an authorable choice.
fn effect_kind_combo(
    ui: &mut egui::Ui,
    salt: usize,
    effect: &mut ContactEffectKind,
    dirty: &mut bool,
) {
    // Caption BESIDE the control and the current entry marked (#1253 f311).
    // A vertical stack of combos each printing its caption on the line below
    // itself is genuinely ambiguous — with four in a row the reader has to
    // guess which label belongs to which dropdown — and the house idiom in
    // the generators tab one file away is the opposite.
    ui.horizontal(|ui| {
        ui.label("Effect kind");
        egui::ComboBox::from_id_salt(("effect_kind", salt))
            .selected_text(effect_kind_label(effect))
            .show_ui(ui, |ui| {
                let is_particle = matches!(effect, ContactEffectKind::ParticleBurst { .. });
                if ui.selectable_label(is_particle, "particle burst").clicked() && !is_particle {
                    *effect = default_particle_effect();
                    *dirty = true;
                }
                let is_decal = matches!(effect, ContactEffectKind::DecalStamp { .. });
                if ui.selectable_label(is_decal, "decal").clicked() && !is_decal {
                    *effect = ContactEffectKind::DecalStamp {
                        decal: DecalParams::default(),
                    };
                    *dirty = true;
                }
                let is_audio = matches!(effect, ContactEffectKind::AudioCue { .. });
                if ui.selectable_label(is_audio, "audio cue").clicked() && !is_audio {
                    *effect = ContactEffectKind::AudioCue {
                        audio: AudioParams::default(),
                    };
                    *dirty = true;
                }
            });
    });
}

/// Editor for an [`AudioParams`] payload. v1 clips are Ogg/Vorbis
/// (Bevy's default audio feature); the source is fetched + cached the
/// same way Sign textures are.
fn audio_form(
    ui: &mut egui::Ui,
    salt: usize,
    audio: &mut AudioParams,
    dirty: &mut bool,
    assets: &mut super::assets::AssetPanel<'_>,
) {
    ui.collapsing("Audio cue", |ui| {
        // Source kind (Url | AtprotoBlob). `Unknown` is a forward-compat
        // decode fallback, not offered for authoring.
        let src_label = match &audio.source {
            AudioClipSource::Url { .. } => "url",
            AudioClipSource::AtprotoBlob { .. } => "atproto blob",
            AudioClipSource::Unknown => "unknown",
        };
        ui.horizontal(|ui| {
            ui.label("Clip source");
            egui::ComboBox::from_id_salt(("audio_src", salt))
                .selected_text(src_label)
                .show_ui(ui, |ui| {
                    let is_url = matches!(audio.source, AudioClipSource::Url { .. });
                    if ui.selectable_label(is_url, "url").clicked() && !is_url {
                        audio.source = AudioClipSource::Url { url: String::new() };
                        *dirty = true;
                    }
                    let is_blob = matches!(audio.source, AudioClipSource::AtprotoBlob { .. });
                    if ui.selectable_label(is_blob, "atproto blob").clicked() && !is_blob {
                        audio.source = AudioClipSource::AtprotoBlob {
                            did: String::new(),
                            cid: String::new(),
                        };
                        *dirty = true;
                    }
                });
        });

        match &mut audio.source {
            AudioClipSource::Url { url } => {
                // Deferred-commit with the refusal rule (#1248 f345 gave
                // this field the rule; f79/f340 gave it the row). Until
                // #1248 the contact cue was the ONE URL-carrying reference
                // the sanitiser never gated, and it fires when a visitor's
                // own avatar touches geometry — the most reliable presence
                // beacon of the three.
                ui.horizontal(|ui| {
                    ui.label("URL (.ogg)");
                    let out = super::widgets::text_draft_row(
                        ui,
                        ("contact_audio_url", salt),
                        url,
                        240.0,
                        "The address of the sound this cue plays. Press Enter, \
                         or click away, to apply it.",
                        crate::pds::sanitize::refusal_reason,
                    );
                    if let Some(committed) = out.committed {
                        *url = committed;
                        *dirty = true;
                    }
                });
                super::widgets::caps_line(
                    ui,
                    &crate::world_builder::asset_failure::audio_clip_caps(),
                );
            }
            AudioClipSource::AtprotoBlob { did, cid } => {
                ui.horizontal(|ui| {
                    ui.label("DID");
                    if ui.text_edit_singleline(did).changed() {
                        *dirty = true;
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("CID");
                    if ui.text_edit_singleline(cid).changed() {
                        *dirty = true;
                    }
                });
            }
            AudioClipSource::Unknown => {
                ui.label(
                    egui::RichText::new(
                        "Unknown clip source (newer client) — read-only; \
                         re-pick a source kind above.",
                    )
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
                );
            }
        }

        // Whether the clip arrived (#1246, #1247 f309). A contact cue is
        // triggered by a visitor walking into something, so the owner never
        // sees the failure and the visitor's client used to re-request the
        // dead URL once per contact sample forever. The line reports the
        // cache entry that now survives the failure.
        match assets.contact_clip(&audio.source) {
            Some(status) => {
                if super::assets::asset_status_row(ui, Some(status), assets.now)
                    && let Some(retry) = super::assets::AssetPanel::clip_retry(&audio.source)
                {
                    assets.retry(retry);
                }
            }
            // Nothing has been asked for, which for a cue means one of two
            // things and the difference is the whole finding: a source that
            // resolves to no key is a PERMANENT no-op — picking "audio cue"
            // installs an empty URL, `AudioClipKey::from_source` answers
            // `None`, and `play_contact_audio` skips the recipe forever with
            // no marker anywhere.
            None => {
                let theme = crate::ui::theme::current(ui.ctx());
                let (text, color) = if crate::interaction::audio::AudioClipKey::from_source(
                    &audio.source,
                )
                .is_none()
                {
                    (
                        "No sound set — this cue will never play.".to_string(),
                        theme.status.warn,
                    )
                } else {
                    (
                        "Not loaded yet — it is fetched the first time somebody \
                             touches this surface."
                            .to_string(),
                        theme.text_weak,
                    )
                };
                ui.label(egui::RichText::new(text).small().color(color));
            }
        }

        fp_slider(ui, "Volume", &mut audio.volume, 0.0, 4.0, dirty);
        fp_slider(
            ui,
            "Volume / (m/s)",
            &mut audio.volume_per_speed,
            0.0,
            2.0,
            dirty,
        );
        fp_slider(ui, "Pitch ×", &mut audio.pitch, 0.1, 4.0, dirty);
        fp_slider(
            ui,
            "Pitch jitter ±",
            &mut audio.pitch_jitter,
            0.0,
            1.0,
            dirty,
        );
        if ui
            .checkbox(&mut audio.spatial, "Spatial (positional)")
            .changed()
        {
            *dirty = true;
        }
    });
}

/// Editor for a [`DecalParams`] payload.
fn decal_form(ui: &mut egui::Ui, decal: &mut DecalParams, dirty: &mut bool) {
    ui.collapsing("Decal", |ui| {
        fp_slider_log(
            ui,
            "TTL (s)",
            &mut decal.ttl,
            0.05,
            limits::MAX_CONTACT_DECAL_TTL,
            dirty,
        );
        fp_slider_log(
            ui,
            "Start size (m)",
            &mut decal.start_size,
            0.0,
            limits::MAX_CONTACT_DECAL_SIZE,
            dirty,
        );
        fp_slider_log(
            ui,
            "End size (m)",
            &mut decal.end_size,
            0.0,
            limits::MAX_CONTACT_DECAL_SIZE,
            dirty,
        );
        fp_slider(ui, "Start alpha", &mut decal.start_alpha, 0.0, 1.0, dirty);
        fp_slider(ui, "End alpha", &mut decal.end_alpha, 0.0, 1.0, dirty);
        color_picker(ui, "Colour", &mut decal.color, dirty);
        fp_slider(
            ui,
            "Normal offset (m)",
            &mut decal.normal_offset,
            0.0,
            1.0,
            dirty,
        );
    });
}

fn shape_combo(ui: &mut egui::Ui, salt: usize, shape: &mut EmitterShape, dirty: &mut bool) {
    let label = match shape {
        EmitterShape::Point => "Point",
        EmitterShape::Sphere { .. } => "Sphere",
        EmitterShape::Box { .. } => "Box",
        EmitterShape::Cone { .. } => "Cone",
        EmitterShape::Unknown => "Unknown",
    };
    // Guarded and marked (#1253 f311/f312). Every arm wrote a fresh default
    // on every click with no identity guard, and the list marked nothing as
    // selected — so opening the dropdown to SEE which shape a burst uses and
    // clicking the one it already said, which is the natural way to dismiss
    // a list, snapped a tuned 4 m radius back to 0.2. Its two sibling combos
    // in this file and the generators tab's equivalent all guard; this was
    // the one that did not. Capitalised to match the generators tab, which
    // names the same enum.
    ui.horizontal(|ui| {
        ui.label("Emitter shape");
        egui::ComboBox::from_id_salt(("shape", salt))
            .selected_text(label)
            .show_ui(ui, |ui| {
                // Switching variant resets to that variant's sane default;
                // the per-variant sliders below then tune it.
                let is_point = matches!(shape, EmitterShape::Point);
                if ui.selectable_label(is_point, "Point").clicked() && !is_point {
                    *shape = EmitterShape::Point;
                    *dirty = true;
                }
                let is_sphere = matches!(shape, EmitterShape::Sphere { .. });
                if ui.selectable_label(is_sphere, "Sphere").clicked() && !is_sphere {
                    *shape = EmitterShape::Sphere { radius: Fp(0.2) };
                    *dirty = true;
                }
                let is_box = matches!(shape, EmitterShape::Box { .. });
                if ui.selectable_label(is_box, "Box").clicked() && !is_box {
                    *shape = EmitterShape::Box {
                        half_extents: Fp3([0.2, 0.2, 0.2]),
                    };
                    *dirty = true;
                }
                let is_cone = matches!(shape, EmitterShape::Cone { .. });
                if ui.selectable_label(is_cone, "Cone").clicked() && !is_cone {
                    *shape = EmitterShape::Cone {
                        half_angle: Fp(0.7),
                        height: Fp(0.4),
                    };
                    *dirty = true;
                }
            });
    });

    match shape {
        EmitterShape::Sphere { radius } => {
            fp_slider(ui, "Radius", radius, 0.0, 8.0, dirty);
        }
        EmitterShape::Box { half_extents } => {
            let mut e = half_extents.0;
            let mut changed = false;
            for (axis, v) in ["X", "Y", "Z"].iter().zip(e.iter_mut()) {
                let mut f = Fp(*v);
                fp_slider(ui, axis, &mut f, 0.0, 8.0, &mut changed);
                *v = f.0;
            }
            if changed {
                half_extents.0 = e;
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
            fp_slider(ui, "Height", height, 0.0, 8.0, dirty);
        }
        EmitterShape::Point | EmitterShape::Unknown => {}
    }
}

#[cfg(test)]
mod authoring_tests {
    use super::*;

    /// #1253 f321. Sequence: add three, delete the middle one, add another
    /// — and two rows are both `effect_2`. The suffix was the LIST LENGTH,
    /// not a counter, and in a master-detail list the row label is the
    /// whole navigational affordance: cooldown state is keyed by position,
    /// and the sanitiser's over-64 truncation sorts BY NAME, so duplicates
    /// make which survivor is dropped arbitrary.
    #[test]
    fn a_new_recipe_never_reuses_a_name_that_is_already_in_the_list() {
        let mut recipes: Vec<ContactEffectRecord> = Vec::new();
        for _ in 0..3 {
            recipes.push(new_recipe(&recipes));
        }
        assert_eq!(
            recipes.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            vec!["effect_0", "effect_1", "effect_2"]
        );

        // Delete the middle one and add another: the old arithmetic
        // produced a second `effect_2`.
        recipes.remove(1);
        recipes.push(new_recipe(&recipes));
        let names: Vec<&str> = recipes.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["effect_0", "effect_2", "effect_1"]);
        let unique: std::collections::HashSet<&&str> = names.iter().collect();
        assert_eq!(unique.len(), names.len(), "names must stay unique");

        // A hand-typed collision is stepped over, not collided with.
        recipes[0].name = "effect_3".to_string();
        assert_eq!(new_recipe(&recipes).name, "effect_0");
    }

    /// #1253 f310. The default a recipe is born with — Enter, cooldown 0 —
    /// is safe only for the phase it was born with, and changing phase is
    /// one click away in a combo that gave no hint of the coupling. The
    /// result reaches visitors live, before anything is saved.
    #[test]
    fn the_dwell_warning_fires_on_exactly_the_unsafe_pair() {
        assert!(dwell_cooldown_warning(ContactPhaseKind::Dwell, 0.0).is_some());
        // A cooldown makes it safe.
        assert!(dwell_cooldown_warning(ContactPhaseKind::Dwell, 0.25).is_none());
        // And 0 is exactly what a one-shot Enter or Exit recipe wants.
        assert!(dwell_cooldown_warning(ContactPhaseKind::Enter, 0.0).is_none());
        assert!(dwell_cooldown_warning(ContactPhaseKind::Exit, 0.0).is_none());
        // The sentence names the consequence, not the mechanism.
        let text = dwell_cooldown_warning(ContactPhaseKind::Dwell, 0.0).expect("warned");
        assert!(text.contains("every frame"), "{text}");
    }
}
