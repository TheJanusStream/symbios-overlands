//! Right-side detail panel: a header naming the selected node + its
//! kind picker + transform editor, followed by the per-kind detail
//! editor (delegated to [`primitive`](super::primitive),
//! [`sign`](super::sign), [`particles`](super::particles),
//! [`water`](super::water), or the Terrain / LSystem / Shape forges in
//! sibling modules of the room editor).

use bevy_egui::egui;

use crate::pds::GeneratorKind;

use super::super::construct::generator_kind_picker;
use super::super::lsystem::draw_lsystem_forge;
use super::super::shape::draw_shape_forge;
use super::super::terrain::draw_terrain_forge;
use super::super::widgets::draw_transform;
use super::particles::draw_generator_particles;
use super::primitive::{
    draw_primitive_bevel, draw_primitive_blob_group, draw_primitive_capsule, draw_primitive_cone,
    draw_primitive_cuboid, draw_primitive_cylinder, draw_primitive_helix, draw_primitive_lathe,
    draw_primitive_plane, draw_primitive_sphere, draw_primitive_spine,
    draw_primitive_superellipsoid, draw_primitive_tetrahedron, draw_primitive_torus,
    draw_primitive_tube,
};
use super::reparent::{current_id, find_node, find_node_mut};
use super::sign::draw_generator_sign;
use super::tree::{node_salt, path_string};
use super::water::draw_water_editor;
use super::{GenNodeId, GeneratorTreeSource};

/// Renders only the *content* of the selected node — kind picker,
/// transform, per-kind detail editor — plus a header that names the node
/// and shows its path. Every structural operation (Add child / Add child
/// from Inventory / Rename / Save to Inventory / Delete) lives in the
/// per-row context menu on the tree panel; this function never mutates
/// the tree shape.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_detail_panel(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    audio_editor: &mut super::super::audio::AudioEditorState,
    grammar_diag: &crate::world_builder::grammar_diag::GrammarDiagnostics,
    dirty: &mut bool,
    // In-scene blob element selection (#705); see `draw_primitive_blob_group`.
    blob_selected_element: &mut Option<usize>,
    // Pending kind-change confirmation (#838), answered by the caller.
    kind_confirm: &mut crate::ui::confirm::ConfirmState<(GenNodeId, &'static str)>,
    // Undo-toast label slot — per-kind editors with named actions (the
    // road seed row) set it; plain slider edits keep the generic label.
    undo_label: &mut crate::ui::undo::LabelSlot,
    // Live road stats for the RoadNetwork readout (#888); `None` when the
    // source can't grow roads.
    road_stats: Option<&crate::terrain::RoadPanelStats>,
) {
    let Some(id) = current_id(selected_generator, selected_prim_path) else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("Select a generator from the tree to edit.")
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Right-click any tree row for: + Add child / Rename / Save to Inventory / − Delete.")
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        });
        return;
    };

    let is_root = id.path.is_empty();
    // Grammar compile status is keyed by the ROOT name — the spawn path
    // records one outcome per generator tree (#829).
    let grammar_status = grammar_diag.get(&id.root);
    // Snapshot the kind tag and choose the kind-picker vocabulary up
    // front so the immutable borrow used for the header is released
    // before we re-enter the source mutably for the editor body.
    let kind_tag = match find_node(&*source, &id) {
        Some(snapshot) => snapshot.kind_tag(),
        None => {
            // The selection points at a node that just disappeared (e.g.
            // its parent was kind-changed to a no-children variant). The
            // tree panel will sync the selection to None on the next
            // frame; show a brief placeholder for this frame.
            ui.label("(selected node no longer exists)");
            return;
        }
    };
    let allowed_kinds: &'static [&'static str] = if is_root {
        source.allowed_kinds_for_root()
    } else {
        source.allowed_kinds_for_child()
    };

    ui.horizontal(|ui| {
        if is_root {
            ui.heading(&id.root);
            ui.label(
                egui::RichText::new(format!("({})", kind_tag))
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        } else {
            ui.heading(kind_tag);
            ui.label(
                egui::RichText::new(format!("path: /{}", path_string(&id.path)))
                    .small()
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        }
    });

    ui.separator();

    let salt = node_salt(&id);

    // Resolved BEFORE the mutable node borrow below: which nodes the
    // terrain plugin actually reads roads from (#886/#895).
    let active_road_nodes = active_road_node_ids(source);

    if let Some(node) = find_node_mut(source, &id) {
        let child_count = node.children.len();
        ui.horizontal(|ui| {
            ui.label("Kind:");
            generator_kind_picker(
                ui,
                &mut node.kind,
                allowed_kinds,
                &salt,
                dirty,
                &id,
                child_count,
                kind_confirm,
            );
        });

        // Placement warning (#886): only the first RoadNetwork directly
        // under the deterministically-chosen Terrain root is ever read
        // (`find_road_config`); anywhere else the node is silently inert
        // — say so instead of letting a dead panel look live.
        if matches!(node.kind, GeneratorKind::RoadNetwork(_)) && !active_road_nodes.contains(&id) {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                format!(
                    "This node grows no roads: only the first {} RoadNetwork nodes \
                     placed directly under the Terrain generator are read. Move it \
                     there — or remove it if those slots are taken.",
                    crate::pds::room::MAX_ROAD_NETWORKS
                ),
            );
        }

        ui.add_space(4.0);
        draw_transform(ui, &mut node.transform, dirty);
        ui.add_space(4.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt(("gen_detail_scroll", &salt))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                draw_generator_detail(
                    ui,
                    &salt,
                    &mut node.kind,
                    grammar_status,
                    dirty,
                    blob_selected_element,
                    undo_label,
                    road_stats,
                );

                // Per-construct audio slot (#314). The bridge writes back
                // any committed pop-out edit and offers the variant picker
                // + "Edit audio…" button, salted by node so each
                // construct keeps its own slot in egui's id stack.
                ui.add_space(6.0);
                ui.separator();
                ui.label(
                    egui::RichText::new("Audio")
                        .strong()
                        .color(crate::ui::theme::current(ui.ctx()).text_strong),
                );
                super::super::audio::draw_audio_bridge(
                    ui,
                    &mut node.audio,
                    &salt,
                    dirty,
                    audio_editor,
                );
            });
    }
}

/// The node ids the terrain plugin reads road networks from, mirroring
/// [`crate::pds::room::find_road_configs`]'s selection rule exactly: the
/// first [`crate::pds::room::MAX_ROAD_NETWORKS`] `RoadNetwork` children of
/// the sorted-first Terrain root — and only that root, even when it carries
/// none. Empty when the tree has no active network. Drives the #886
/// misplaced-node warning.
fn active_road_node_ids(source: &dyn GeneratorTreeSource) -> Vec<GenNodeId> {
    for name in source.root_names() {
        let Some(root) = source.get_root(&name) else {
            continue;
        };
        if matches!(root.kind, GeneratorKind::Terrain(_)) {
            return root
                .children
                .iter()
                .enumerate()
                .filter(|(_, c)| matches!(c.kind, GeneratorKind::RoadNetwork(_)))
                .take(crate::pds::room::MAX_ROAD_NETWORKS)
                .map(|(i, _)| GenNodeId::child(name.clone(), vec![i]))
                .collect();
        }
    }
    Vec::new()
}

/// Inline editor for a [`crate::pds::generator::RoadConfig`] (the RoadNetwork
/// generator). Exposes the authorable street knobs; the terrain plugin
/// recomputes the road mesh from the heightmap on any change. Geometry-only
/// rendering constants (UV tile, ribbon step) stay in code.
/// One labelled road slider with a named undo entry (#887).
fn road_slider(
    ui: &mut egui::Ui,
    v: &mut f32,
    range: std::ops::RangeInclusive<f32>,
    label: &str,
    undo_name: &str,
    dirty: &mut bool,
    undo_label: &mut crate::ui::undo::LabelSlot,
) {
    if ui.add(egui::Slider::new(v, range).text(label)).changed() {
        undo_label.set(format!("road {undo_name}"));
        *dirty = true;
    }
}

/// Vertex count above which the stats readout tints warn — a road mesh this
/// heavy is a real slice of the wasm frame/memory budget.
const ROAD_HEAVY_VERTS: usize = 150_000;

fn draw_road_editor(
    ui: &mut egui::Ui,
    config: &mut crate::pds::generator::RoadConfig,
    dirty: &mut bool,
    undo_label: &mut crate::ui::undo::LabelSlot,
    road_stats: Option<&crate::terrain::RoadPanelStats>,
) {
    if ui.checkbox(&mut config.enabled, "Roads enabled").changed() {
        undo_label.set(format!(
            "roads {}",
            if config.enabled {
                "enabled"
            } else {
                "disabled"
            }
        ));
        *dirty = true;
    }

    // Live network readout (#888) — what the last completed re-mesh
    // actually built, so every slider edit becomes legible ~a debounce
    // later. Warn-tinted when the mesh weight starts to matter on wasm.
    if let Some(stats) = road_stats {
        let theme = crate::ui::theme::current(ui.ctx());
        if stats.built {
            let mut text = format!(
                "{} streets · {} junctions · {} buildings · {} verts",
                stats.streets, stats.junctions, stats.buildings, stats.vertices
            );
            if stats.props > 0 {
                text.push_str(&format!(" · {} props", stats.props));
            }
            let heavy = stats.vertices > ROAD_HEAVY_VERTS;
            let color = if heavy {
                theme.status.warn
            } else {
                theme.text_weak
            };
            let label = ui.label(egui::RichText::new(text).small().color(color));
            if heavy {
                label.on_hover_text(
                    "This road mesh is heavy — consider a smaller district or \
                     wider spacing, especially for wasm visitors.",
                );
            }
        } else if config.enabled {
            ui.label(
                egui::RichText::new("building…")
                    .small()
                    .color(theme.text_weak),
            );
        }
    }
    ui.add_space(4.0);
    // Editable seed row (#885): type a layout number to reproduce/share a
    // street plan, or 🎲 for a fresh one. The buffer lives in egui temp
    // memory keyed to this node, re-synced whenever the record's seed
    // changes underneath it (dice, undo, remote edit).
    #[derive(Clone)]
    struct SeedBuf {
        text: String,
        synced_to: u64,
    }
    let seed_row = |ui: &mut egui::Ui,
                    config: &mut crate::pds::generator::RoadConfig,
                    dirty: &mut bool,
                    undo_label: &mut crate::ui::undo::LabelSlot| {
        ui.horizontal(|ui| {
            ui.label("Layout seed:");
            let id = ui.id().with("road_seed");
            let mut st = ui
                .data_mut(|d| d.get_temp::<SeedBuf>(id))
                .unwrap_or(SeedBuf {
                    text: config.seed.to_string(),
                    synced_to: config.seed,
                });
            if st.synced_to != config.seed {
                st.text = config.seed.to_string();
                st.synced_to = config.seed;
            }
            let parse_ok = st.text.trim().parse::<u64>().is_ok();
            let mut field = egui::TextEdit::singleline(&mut st.text).desired_width(150.0);
            if !parse_ok {
                field = field.text_color(crate::ui::theme::current(ui.ctx()).status.error);
            }
            let resp = ui.add(field).on_hover_text(
                "Street-layout seed. Type a number and press Enter to apply — \
             the same seed reproduces the same streets. Terrain is untouched.",
            );
            if resp.lost_focus()
                && let Ok(v) = st.text.trim().parse::<u64>()
                && v != config.seed
            {
                config.seed = v;
                st.synced_to = v;
                undo_label.set(format!("road seed set ({v})"));
                *dirty = true;
            }
            if ui
                .button("🎲")
                .on_hover_text("Re-roll the street layout — terrain untouched")
                .clicked()
            {
                // Deterministic LCG step → a fresh street layout.
                config.seed = config
                    .seed
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                st.text = config.seed.to_string();
                st.synced_to = config.seed;
                undo_label.set(format!("road seed re-roll ({})", config.seed));
                *dirty = true;
            }
            ui.data_mut(|d| d.insert_temp(id, st));
        });
    };

    // Slider ranges follow the `sanitize_road` clamps (#883) except
    // where a deliberately tighter max keeps the slider usable: half-widths
    // stop well short of the sanitizer's 20 m ceiling (a 20 m-wide lane is
    // a plaza, and the full range would make the useful 1–4 m band a
    // couple of pixels), and curb/chamfer stop at 1 m for the same reason.
    // The sanitizer still accepts hand-edited records up to its bounds.

    // #887: three sections — the street PLAN (what moves lots too), the
    // ribbon cross-SECTION (mesh-only), and the building layer.
    egui::CollapsingHeader::new("Layout")
        .default_open(true)
        .show(ui, |ui| {
            seed_row(ui, config, dirty, undo_label);
            ui.add_space(4.0);
            // Street-plan style (#890).
            ui.horizontal(|ui| {
                ui.label("Style:");
                for (value, label, tip) in crate::pds::generator::RoadStyle::pickers() {
                    if ui
                        .selectable_label(config.style == value, label)
                        .on_hover_text(tip)
                        .clicked()
                        && config.style != value
                    {
                        config.style = value;
                        undo_label.set(format!("road style {label}"));
                        *dirty = true;
                    }
                }
            });
            road_slider(
                ui,
                &mut config.district_half_extent.0,
                10.0..=512.0,
                "District ½-extent (m)",
                "district extent",
                dirty,
                undo_label,
            );
            // District centre offset (#889).
            ui.horizontal(|ui| {
                ui.label("District centre (m)");
                for (axis_label, axis) in ["X", "Z"].iter().zip(config.center.0.iter_mut()) {
                    ui.label(*axis_label);
                    if ui
                        .add(egui::DragValue::new(axis).speed(1.0).range(-512.0..=512.0))
                        .changed()
                    {
                        undo_label.set("road district centre".to_string());
                        *dirty = true;
                    }
                }
            });
            ui.label(
                egui::RichText::new(
                    "Offset from the room origin; the district slides back inside \
                     the map when pushed past an edge.",
                )
                .small()
                .weak(),
            );
            road_slider(
                ui,
                &mut config.major_spacing.0,
                10.0..=500.0,
                "Major spacing (m)",
                "major spacing",
                dirty,
                undo_label,
            );
            road_slider(
                ui,
                &mut config.minor_spacing.0,
                8.0..=400.0,
                "Minor spacing (m)",
                "minor spacing",
                dirty,
                undo_label,
            );
        });

    egui::CollapsingHeader::new("Ribbon profile")
        .default_open(false)
        .show(ui, |ui| {
            road_slider(
                ui,
                &mut config.major_half_width.0,
                0.5..=8.0,
                "Major ½-width (m)",
                "major width",
                dirty,
                undo_label,
            );
            road_slider(
                ui,
                &mut config.minor_half_width.0,
                0.5..=6.0,
                "Minor ½-width (m)",
                "minor width",
                dirty,
                undo_label,
            );
            road_slider(
                ui,
                &mut config.curb_height.0,
                0.0..=1.0,
                "Curb height (m)",
                "curb height",
                dirty,
                undo_label,
            );
            road_slider(
                ui,
                &mut config.curb_top_width.0,
                0.0..=1.0,
                "Curb top width (m)",
                "curb top width",
                dirty,
                undo_label,
            );
            road_slider(
                ui,
                &mut config.chamfer_width.0,
                0.0..=1.0,
                "Curb chamfer (m)",
                "curb chamfer",
                dirty,
                undo_label,
            );
            road_slider(
                ui,
                &mut config.skirt_depth.0,
                0.5..=50.0,
                "Skirt depth (m)",
                "skirt depth",
                dirty,
                undo_label,
            );
        });

    egui::CollapsingHeader::new("Appearance")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                egui::RichText::new(
                    "Overrides the room theme's road look. Unchecked = theme \
                     default. Colour edits apply instantly (no rebuild).",
                )
                .small()
                .weak(),
            );
            let ap = &mut config.appearance;
            // One override row: checkbox arms the Option, colour button edits it.
            let mut color_row = |ui: &mut egui::Ui,
                                 slot: &mut Option<crate::pds::Fp3>,
                                 label: &str,
                                 armed_default: [f32; 3]| {
                ui.horizontal(|ui| {
                    let mut on = slot.is_some();
                    if ui.checkbox(&mut on, label).changed() {
                        *slot = on.then_some(crate::pds::Fp3(armed_default));
                        undo_label.set(format!("road {label} override"));
                        *dirty = true;
                    }
                    if let Some(c) = slot
                        && ui.color_edit_button_rgb(&mut c.0).changed()
                    {
                        undo_label.set(format!("road {label}"));
                        *dirty = true;
                    }
                });
            };
            color_row(ui, &mut ap.deck_color, "Deck colour", [0.03, 0.03, 0.035]);
            color_row(
                ui,
                &mut ap.structure_color,
                "Curb/skirt colour",
                [0.09, 0.09, 0.10],
            );
            color_row(ui, &mut ap.neon_color, "Edge-line colour", [0.6, 0.8, 1.0]);
            ui.horizontal(|ui| {
                let mut on = ap.deck_roughness.is_some();
                if ui.checkbox(&mut on, "Deck roughness").changed() {
                    ap.deck_roughness = on.then_some(crate::pds::Fp(0.22));
                    undo_label.set("road deck roughness override".to_string());
                    *dirty = true;
                }
                if let Some(r) = &mut ap.deck_roughness
                    && ui
                        .add(egui::Slider::new(&mut r.0, 0.0..=1.0).step_by(0.01))
                        .changed()
                {
                    undo_label.set("road deck roughness".to_string());
                    *dirty = true;
                }
            });
            ui.horizontal(|ui| {
                let mut on = ap.neon_strength.is_some();
                if ui.checkbox(&mut on, "Edge-line strength").changed() {
                    ap.neon_strength = on.then_some(crate::pds::Fp(2.5));
                    undo_label.set("road edge strength override".to_string());
                    *dirty = true;
                }
                if let Some(s) = &mut ap.neon_strength
                    && ui
                        .add(egui::Slider::new(&mut s.0, 0.0..=10.0).step_by(0.1))
                        .changed()
                {
                    undo_label.set("road edge strength".to_string());
                    *dirty = true;
                }
            });
            if !ap.is_all_theme() && ui.button("⟲ Theme defaults").clicked() {
                *ap = Default::default();
                undo_label.set("road appearance reset".to_string());
                *dirty = true;
            }
        });

    egui::CollapsingHeader::new("Lots")
        .default_open(true)
        .show(ui, |ui| {
            if ui
                .checkbox(&mut config.populate_lots, "Grow buildings on lots")
                .on_hover_text(
                    "Fill the network's enclosed blocks with themed buildings at load. \
                     Re-roll the layout to re-seed them.",
                )
                .changed()
            {
                undo_label.set("road lots toggle".to_string());
                *dirty = true;
            }
            if !config.populate_lots {
                return;
            }
            let lots = &mut config.lots;
            if ui
                .add(egui::Slider::new(&mut lots.density.0, 0.0..=1.0).text("Density"))
                .on_hover_text("Fraction of lots that grow a building — the largest lots win")
                .changed()
            {
                undo_label.set("lot density".to_string());
                *dirty = true;
            }
            // Building-theme override (#892): "Room theme" or an explicit
            // archetype, stored as a lenient label string.
            ui.horizontal(|ui| {
                ui.label("Theme:");
                let current = if lots.theme_override.trim().is_empty() {
                    "Room theme".to_string()
                } else {
                    lots.theme_override.clone()
                };
                egui::ComboBox::from_id_salt("road_lot_theme")
                    .selected_text(current)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(lots.theme_override.trim().is_empty(), "Room theme")
                            .clicked()
                            && !lots.theme_override.is_empty()
                        {
                            lots.theme_override.clear();
                            undo_label.set("lot theme override cleared".to_string());
                            *dirty = true;
                        }
                        for t in crate::seeded_defaults::ThemeArchetype::ALL {
                            let label = t.label();
                            if ui
                                .selectable_label(
                                    lots.theme_override.eq_ignore_ascii_case(label),
                                    label,
                                )
                                .clicked()
                                && !lots.theme_override.eq_ignore_ascii_case(label)
                            {
                                lots.theme_override = label.to_string();
                                undo_label.set(format!("lot theme {label}"));
                                *dirty = true;
                            }
                        }
                    });
            });
            ui.horizontal(|ui| {
                ui.label("Mix:");
                for (value, label, tip) in crate::pds::generator::LotTierBias::pickers() {
                    if ui
                        .selectable_label(lots.tier_bias == value, label)
                        .on_hover_text(tip)
                        .clicked()
                        && lots.tier_bias != value
                    {
                        lots.tier_bias = value;
                        undo_label.set(format!("lot mix {label}"));
                        *dirty = true;
                    }
                }
            });
            ui.separator();
            if ui
                .checkbox(&mut config.furniture.enabled, "Street furniture")
                .on_hover_text(
                    "Plant theme props (lamps, signs, clutter) along the streets, \
                     just outside the curbs, sides alternating.",
                )
                .changed()
            {
                undo_label.set("street furniture toggle".to_string());
                *dirty = true;
            }
            if config.furniture.enabled
                && ui
                    .add(
                        egui::Slider::new(&mut config.furniture.spacing.0, 8.0..=200.0)
                            .text("Prop spacing (m)"),
                    )
                    .changed()
            {
                undo_label.set("street furniture spacing".to_string());
                *dirty = true;
            }
            ui.separator();
            let lots = &mut config.lots;
            ui.horizontal(|ui| {
                ui.label("Building scale");
                let mut changed = false;
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut lots.scale_min.0)
                            .speed(0.05)
                            .range(0.1..=5.0),
                    )
                    .changed();
                ui.label("to");
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut lots.scale_max.0)
                            .speed(0.05)
                            .range(0.1..=5.0),
                    )
                    .changed();
                if changed {
                    undo_label.set("lot building scale".to_string());
                    *dirty = true;
                }
            });
        });
}

/// Inline editor for a [`GeneratorKind::Portal`]: the destination room's
/// DID plus the world-space exit position in that room.
fn draw_portal_editor(
    ui: &mut egui::Ui,
    target_did: &mut String,
    target_pos: &mut crate::pds::Fp3,
    dirty: &mut bool,
) {
    ui.label("Target DID (destination room)");
    if ui
        .add(egui::TextEdit::singleline(target_did).hint_text("did:plc:…"))
        .changed()
    {
        *dirty = true;
    }
    ui.add_space(4.0);
    ui.label("Exit position (world space in the target room)");
    ui.horizontal(|ui| {
        for (label, axis) in ["X", "Y", "Z"].iter().zip(target_pos.0.iter_mut()) {
            ui.label(*label);
            if ui.add(egui::DragValue::new(axis).speed(0.1)).changed() {
                *dirty = true;
            }
        }
    });
}

/// Inline editor for a [`GeneratorKind::Gateway`]: only the interaction
/// zone's extents — the destination list is social-graph data resolved at
/// interaction time, so there is nothing else to author.
fn draw_gateway_editor(ui: &mut egui::Ui, size: &mut crate::pds::Fp3, dirty: &mut bool) {
    ui.label("Zone size (m) — the walk-in volume that opens the destination picker");
    ui.horizontal(|ui| {
        for (label, axis) in ["X", "Y", "Z"].iter().zip(size.0.iter_mut()) {
            ui.label(*label);
            if ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.25..=50.0))
                .changed()
            {
                *dirty = true;
            }
        }
    });
}

/// Per-kind variant detail editor — a thin dispatch: every arm is a
/// single delegation into a per-kind editor fn (the Terrain / LSystem /
/// Shape forges, the shared primitive editors, or the inline-widget
/// helpers above).
/// Does NOT render the local transform — that's drawn separately in the detail
/// panel header.
///
/// `salt` uniquely identifies this node in egui's ID stack — it's passed
/// through to nested material widgets so collapsing one node never
/// affects another when the same widget type repeats across the tree.
#[allow(clippy::too_many_arguments)] // one shared dispatch; each arg is a distinct channel.
fn draw_generator_detail(
    ui: &mut egui::Ui,
    salt: &str,
    kind: &mut GeneratorKind,
    // Latest compile outcome for the ROOT this node lives under (#829) —
    // consumed by the L-system / Shape forges; a broken grammar anywhere
    // under the root shows the same (line-numbered) message.
    grammar_status: Option<&crate::world_builder::grammar_diag::GrammarStatus>,
    dirty: &mut bool,
    blob_selected_element: &mut Option<usize>,
    undo_label: &mut crate::ui::undo::LabelSlot,
    road_stats: Option<&crate::terrain::RoadPanelStats>,
) {
    match kind {
        GeneratorKind::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty),
        GeneratorKind::Water { surface } => {
            draw_water_editor(ui, surface, dirty);
        }
        GeneratorKind::RoadNetwork(config) => {
            draw_road_editor(ui, config, dirty, undo_label, road_stats)
        }
        GeneratorKind::LSystem {
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            materials,
            prop_mappings,
            prop_scale,
            mesh_resolution,
            ..
        } => draw_lsystem_forge(
            ui,
            grammar_status,
            source_code,
            finalization_code,
            iterations,
            seed,
            angle,
            step,
            width,
            elasticity,
            tropism,
            materials,
            prop_mappings,
            prop_scale,
            mesh_resolution,
            dirty,
        ),
        GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
        } => draw_shape_forge(
            ui,
            grammar_status,
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
            dirty,
        ),
        GeneratorKind::Portal {
            target_did,
            target_pos,
        } => draw_portal_editor(ui, target_did, target_pos, dirty),
        GeneratorKind::Gateway { size } => draw_gateway_editor(ui, size, dirty),
        GeneratorKind::Cuboid {
            size,
            solid,
            material,
            torture,
        } => draw_primitive_cuboid(ui, size, solid, material, torture, salt, dirty),
        GeneratorKind::Sphere {
            radius,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_sphere(
            ui, radius, resolution, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_cylinder(
            ui, radius, height, resolution, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            solid,
            material,
            torture,
        } => draw_primitive_capsule(
            ui, radius, length, latitudes, longitudes, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_cone(
            ui, radius, height, resolution, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            torture,
        } => draw_primitive_torus(
            ui,
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Plane {
            size,
            subdivisions,
            solid,
            material,
            torture,
        } => draw_primitive_plane(
            ui,
            size,
            subdivisions,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Tetrahedron {
            size,
            solid,
            material,
            torture,
        } => draw_primitive_tetrahedron(ui, size, solid, material, torture, salt, dirty),
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_tube(
            ui,
            radius,
            inner_radius,
            height,
            resolution,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            solid,
            material,
            torture,
        } => draw_primitive_bevel(
            ui,
            size,
            bevel,
            bevel_segments,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        // A wedge carries the same fields as a cuboid (a bounding box); reuse
        // the cuboid editor.
        GeneratorKind::Wedge {
            size,
            solid,
            material,
            torture,
        } => draw_primitive_cuboid(ui, size, solid, material, torture, salt, dirty),
        GeneratorKind::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            solid,
            material,
            torture,
        } => draw_primitive_helix(
            ui,
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Superellipsoid {
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            solid,
            material,
            torture,
        } => draw_primitive_superellipsoid(
            ui,
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Spine {
            points,
            resolution,
            samples_per_segment,
            solid,
            material,
            torture,
        } => draw_primitive_spine(
            ui,
            points,
            resolution,
            samples_per_segment,
            solid,
            material,
            torture,
            salt,
            dirty,
        ),
        GeneratorKind::Lathe {
            points,
            resolution,
            smooth,
            solid,
            material,
            torture,
        } => draw_primitive_lathe(
            ui, points, resolution, smooth, solid, material, torture, salt, dirty,
        ),
        GeneratorKind::BlobGroup {
            elements,
            resolution,
            solid,
            uv_mapping,
            material,
            torture,
        } => draw_primitive_blob_group(
            ui,
            elements,
            resolution,
            solid,
            uv_mapping,
            material,
            torture,
            salt,
            dirty,
            blob_selected_element,
        ),
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
            texture_filter,
        } => draw_generator_sign(
            ui,
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
            texture_filter,
            salt,
            dirty,
        ),
        GeneratorKind::ParticleSystem(params) => draw_generator_particles(ui, params, salt, dirty),
        GeneratorKind::Unknown => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}
