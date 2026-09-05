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
    FacePanel, FacePickUi, PrimEdit, draw_primitive_bevel, draw_primitive_blob_group,
    draw_primitive_capsule, draw_primitive_cone, draw_primitive_cuboid, draw_primitive_cylinder,
    draw_primitive_helix, draw_primitive_lathe, draw_primitive_plane, draw_primitive_sphere,
    draw_primitive_spine, draw_primitive_superellipsoid, draw_primitive_tetrahedron,
    draw_primitive_torus, draw_primitive_tube,
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
    // Click-to-pick face selection (#961): the shared arm flag plus the
    // pick channel, resolved against THIS node's id below.
    face_pick: &mut crate::editor_gizmo::FacePick,
    // The asset caches (#1246), for every field naming a fetched asset.
    assets: &mut super::super::assets::AssetPanel<'_>,
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
    // This NODE's grammar compile outcome (#1250 f84).
    //
    // The spawn path always filed per node — `record_grammar_status` is
    // handed the synthetic cache key, which is `<root>/<i>/<j>` for a child
    // — but the panel looked up the ROOT name, so a tree with two grammar
    // nodes showed the root's line-numbered error under both, and a child's
    // own failure was never shown at all. The read now asks the same
    // question the write answered.
    let grammar_status = grammar_diag.get(&crate::world_builder::compile::synthetic_cache_key(
        &id.root, &id.path,
    ));
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

    // How many placements point at this root (#1244 f424). The count was
    // already computed — by `placement_ref_count` — and shown in exactly
    // two places, BOTH inside the destructive path: the delete confirm's
    // body and the undo label after it is answered. So the only way to
    // learn whether a generator was used anywhere was to start deleting
    // it: the action that reveals the number is the one that destroys the
    // thing. Orphans are the natural sediment of a long session (catalogue
    // stamps never placed, assets superseded by a v2) and each still costs
    // a PDS child record, room bytes and one of the 256 slots.
    let placement_uses = is_root.then(|| source.placement_ref_count(&id.root));
    ui.horizontal(|ui| {
        if is_root {
            ui.heading(&id.root);
            ui.label(
                egui::RichText::new(format!("({})", kind_tag))
                    .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
            if let Some(uses) = placement_uses {
                let th = crate::ui::theme::current(ui.ctx());
                let (text, colour) = if uses == 0 {
                    (String::from("· not placed in the world"), th.status.warn)
                } else {
                    (
                        format!(
                            "· used by {uses} placement{}",
                            if uses == 1 { "" } else { "s" }
                        ),
                        th.text_weak,
                    )
                };
                ui.label(egui::RichText::new(text).small().color(colour))
                    .on_hover_text(
                        "A region asset is a blueprint; a placement is where \
                         a copy of it stands. An asset with no placements is \
                         still saved, and still counts against this world's \
                         asset limit.",
                    );
            }
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

    // A scene pick names one node (#961); resolving it here — where the
    // node's id is known — is what keeps a stale pick (world editor closed
    // between the click and this draw) from painting whichever node happens
    // to be selected now.
    let picked_face = face_pick.take_for(&id.root, &id.path);
    // #1237 f140: only trees a scene click can address back to may offer
    // "Pick from scene". Read here, before the source is borrowed for the
    // node lookup below.
    let source_resolves_face_picks = source.resolves_face_picks();

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
        // #1243 f152: a ROOT's in-world gizmo has no translate handles
        // (`prim_modes`), and the panel drew the Translation boxes anyway
        // — handles silently disappearing, contradicted three inches away
        // by a numeric field that does the forbidden thing. Say which is
        // which, the way the neighbouring restrictions already do.
        if id.path.is_empty() {
            ui.label(
                egui::RichText::new(
                    "This is a root: set its position here or on its Placement. \
                     The in-world gizmo rotates and scales it.",
                )
                .small()
                .color(crate::ui::theme::current(ui.ctx()).text_weak),
            );
        }
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
                    FacePickUi {
                        picked: picked_face,
                        resolvable: source_resolves_face_picks,
                        pick: face_pick,
                    },
                    assets,
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
                // The pop-out's title is this label (#1202): the same
                // root / kind / path vocabulary the header above uses,
                // not the egui id salt.
                let label = if is_root {
                    format!("{} ({kind_tag})", id.root)
                } else {
                    format!("{} / {kind_tag} (/{})", id.root, path_string(&id.path))
                };
                super::super::audio::draw_audio_bridge(
                    ui,
                    &mut node.audio,
                    &salt,
                    &label,
                    dirty,
                    audio_editor,
                    assets,
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
    if ui
        .add(crate::ui::num::slider(v, range).text(label))
        .changed()
    {
        undo_label.set(format!("road {undo_name}"));
        *dirty = true;
    }
}

/// Vertex count above which the stats readout tints warn — a road mesh this
/// heavy is a real slice of the wasm frame/memory budget.
const ROAD_HEAVY_VERTS: usize = 150_000;

/// The lot layer's arithmetic as readout lines, `(text, warn)` (#1211,
/// finding 384). Four clamps used to drop content with no report — density
/// thinning, the per-district building cap or the room's free placement
/// budget, the furniture cap, and the generator ceiling — leaving the owner
/// tuning density, extent and spacing against a number that moved for
/// reasons they could not attribute. Nothing for adopted (saved) content,
/// which this session did not inject.
pub(crate) fn lot_clamp_lines(
    clamps: &crate::terrain::LotClamps,
    buildings: usize,
) -> Vec<(String, bool)> {
    let mut lines = Vec::new();
    if clamps.lots_found > 0 {
        lines.push((
            format!(
                "{} lots · {} kept by density · {buildings} grown",
                clamps.lots_found, clamps.lots_kept
            ),
            false,
        ));
    }
    if clamps.buildings_dropped > 0 {
        lines.push((
            if clamps.buildings_capped_by_budget {
                format!(
                    "{} lots left empty — only {} placements were left in this world's \
                     {}-placement budget",
                    clamps.buildings_dropped,
                    buildings,
                    crate::pds::sanitize::limits::MAX_PLACEMENTS
                )
            } else {
                format!(
                    "{} lots left empty — capped at {} buildings per district",
                    clamps.buildings_dropped,
                    crate::terrain::MAX_LOT_BUILDINGS
                )
            },
            true,
        ));
    }
    if clamps.props_dropped > 0 {
        lines.push((
            if clamps.props_capped_by_budget {
                format!(
                    "{} furniture spots left empty — this world's {}-placement budget is used up",
                    clamps.props_dropped,
                    crate::pds::sanitize::limits::MAX_PLACEMENTS
                )
            } else {
                format!(
                    "{} furniture spots left empty — capped at {} props per district",
                    clamps.props_dropped,
                    crate::terrain::MAX_FURNITURE_PROPS
                )
            },
            true,
        ));
    }
    if clamps.generator_cap_skips > 0 {
        lines.push((
            format!(
                "{} lots skipped — the world is at its {}-generator limit",
                clamps.generator_cap_skips,
                crate::pds::sanitize::limits::MAX_GENERATORS
            ),
            true,
        ));
    }
    lines
}

/// The sentence over the layout controls naming what changing them
/// replaces (#1245 f378). Pure so the arithmetic and the plural are
/// testable without egui.
fn regrow_warning(buildings: usize, props: usize) -> String {
    let mut what = Vec::new();
    if buildings > 0 {
        what.push(format!(
            "{buildings} building{}",
            if buildings == 1 { "" } else { "s" }
        ));
    }
    if props > 0 {
        what.push(format!("{props} prop{}", if props == 1 { "" } else { "s" }));
    }
    format!(
        "Changing the layout below re-grows the district, replacing the {} \
         standing here — including any you have moved.",
        what.join(" and "),
    )
}

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
                text.push_str(&format!(
                    " · {} {}",
                    stats.props,
                    crate::ui::toolbar::plural(stats.props, "prop", "props")
                ));
            }
            // Marked stale rather than reported as settled fact (#1245
            // f385): these are the PREVIOUS layout's numbers for the whole
            // debounce-plus-build window after an edit, which is long
            // enough for an owner to read them, believe them, and tune
            // against the wrong number.
            if stats.pending {
                text.push_str(" · rebuilding…");
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
            // The arithmetic behind the building count (#1211): every
            // clamp that emptied a lot, named, where the knobs are.
            for (line, warn) in lot_clamp_lines(&stats.clamps, stats.buildings) {
                let color = if warn {
                    theme.status.warn
                } else {
                    theme.text_weak
                };
                ui.label(egui::RichText::new(line).small().color(color));
            }
        } else if config.enabled {
            ui.label(
                egui::RichText::new("building…")
                    .small()
                    .color(theme.text_weak),
            );
        }
    }
    // What a layout edit costs, said BEFORE the slider moves (#1245 f378).
    //
    // Any Layout or Lots control changes the fingerprint, and a third of a
    // second later `strip_lot_buildings` removes every grown generator and
    // placement — including ones the owner dragged into place with the
    // gizmo, which is the most expensive work they do. Undo covers it (the
    // derived write folds into the slider's own entry) but only if they
    // realise inside the 32-step ring, and nothing told them: the buildings
    // vanish after the drag ends, when attention has already moved on.
    //
    // A confirm is not available here — the strip happens in a system with
    // no UI, a debounce later — so the honest surface is the sentence
    // before the gesture and the toast after it.
    if let Some(stats) = road_stats
        && stats.built
        && stats.buildings + stats.props > 0
    {
        ui.label(
            egui::RichText::new(regrow_warning(stats.buildings, stats.props))
                .small()
                .color(crate::ui::theme::current(ui.ctx()).status.warn),
        )
        .on_hover_text(
            "Undo restores them together with the edit that replaced them, but \
             only within the last 32 steps.",
        );
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
                        .add(crate::ui::num::drag(axis).speed(1.0).range(-512.0..=512.0))
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
                    // Through the shared helper, not egui directly (#1249
                    // f58): this row is the twenty-fifth picker in the
                    // editor and the only one that bypassed it.
                    if let Some(c) = slot
                        && super::super::widgets::edit_srgb_rgb(ui, &mut c.0)
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
                        .add(crate::ui::num::slider(&mut r.0, 0.0..=1.0).step_by(0.01))
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
                        .add(crate::ui::num::slider(&mut s.0, 0.0..=10.0).step_by(0.1))
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

    // Its own section, not a row inside "Lots" (#1245 f380). The Lots body
    // early-returns when "Grow buildings on lots" is unticked, and the
    // furniture checkbox was drawn after that point — so unticking
    // buildings removed the ONLY control over a layer the injector treats
    // as independent (`active_configs` accepts a config on
    // `populate_lots || furniture.enabled`), while up to 160 props kept
    // being derived, written into the record and published, with no way to
    // turn them off short of deleting the whole network.
    egui::CollapsingHeader::new("Street furniture")
        .default_open(false)
        .show(ui, |ui| {
            if ui
                .checkbox(&mut config.furniture.enabled, "Plant street props")
                .on_hover_text(
                    "Plant theme props (lamps, signs, clutter) along the streets, \
                     just outside the curbs, sides alternating. Independent of the \
                     buildings — this layer grows with or without them.",
                )
                .changed()
            {
                undo_label.set("street furniture toggle".to_string());
                *dirty = true;
            }
            ui.add_enabled_ui(config.furniture.enabled, |ui| {
                if ui
                    .add(
                        crate::ui::num::slider(&mut config.furniture.spacing.0, 8.0..=200.0)
                            .text("Prop spacing (m)"),
                    )
                    .changed()
                {
                    undo_label.set("street furniture spacing".to_string());
                    *dirty = true;
                }
            });
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
                .add(crate::ui::num::slider(&mut lots.density.0, 0.0..=1.0).text("Density"))
                .on_hover_text("Fraction of lots that grow a building — the largest lots win")
                .changed()
            {
                undo_label.set("lot density".to_string());
                *dirty = true;
            }
            // Building-theme override (#892): "Room theme" or an explicit
            // archetype, stored as a lenient label string.
            // #1251 f390: the combo used to print the raw stored string as
            // its selected text, so a label this build does not know read as
            // the ACTIVE theme while `maybe_populate_lots` matched it
            // case-insensitively against the roster and fell through to the
            // room theme. The panel asserted a setting with no effect and no
            // row in its own dropdown highlighted.
            let unrecognised_theme = !lots.theme_override.trim().is_empty()
                && crate::terrain::resolve_lot_theme(&lots.theme_override).is_none();
            ui.horizontal(|ui| {
                ui.label("Theme:");
                let current = if lots.theme_override.trim().is_empty() || unrecognised_theme {
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
            if unrecognised_theme {
                super::super::widgets::unrecognised_value_line(
                    ui,
                    "building theme",
                    Some("the room's own theme is growing instead"),
                );
            }
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
            let lots = &mut config.lots;
            ui.horizontal(|ui| {
                ui.label("Building scale");
                // Bounded against each other (#1238 f90). This pair was
                // repaired downstream by SWAPPING — a third convention
                // beside the particles' clamp-to-min and the splat rules'
                // nothing at all, applied ~0.25 s after the drag with no
                // message. Ranges read before either drag, so a drag on
                // one cannot widen its own bound within the frame.
                let (min, max) = (lots.scale_min.0, lots.scale_max.0);
                let mut changed = false;
                changed |= ui
                    .add(
                        crate::ui::num::drag(&mut lots.scale_min.0)
                            .speed(0.05)
                            .range(0.1..=max.clamp(0.1, 5.0)),
                    )
                    .changed();
                ui.label("to");
                changed |= ui
                    .add(
                        crate::ui::num::drag(&mut lots.scale_max.0)
                            .speed(0.05)
                            .range(min.clamp(0.1, 5.0)..=5.0),
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
/// identifier plus the world-space exit position in that room.
///
/// The identifier field is a deferred-commit row validated against the same
/// shape check the login form uses (#1251 f92), so a typo is refused where it
/// is typed instead of at walk-in. The exit-position drags carry the ranges
/// the sanitiser clamps them to — they were unbounded in the UI and clamped
/// on the wire, which is the worst of both.
fn draw_portal_editor(
    ui: &mut egui::Ui,
    target_did: &mut String,
    target_pos: &mut crate::pds::Fp3,
    dirty: &mut bool,
    undo_label: &mut crate::ui::undo::LabelSlot,
) {
    ui.label("Where it leads");
    let out = super::super::widgets::text_draft_row(
        ui,
        "portal_target_did",
        target_did,
        260.0,
        "The identifier of the person whose world this portal opens into. \
         Press Enter, or click away, to apply it.",
        |draft| match crate::ui::login::validation::validate_destination(draft) {
            // Home is what a blank field means on the login form; here it
            // would be a portal that leads nowhere.
            Ok(crate::ui::login::validation::Destination::Home) => None,
            Ok(crate::ui::login::validation::Destination::Did(_)) => None,
            // A handle is a reasonable thing to type and the field cannot
            // take one yet, so say which of the two this is rather than
            // calling it malformed.
            Ok(crate::ui::login::validation::Destination::Handle(_)) => Some(
                "This field needs the did:… identifier, not an @handle. You can \
                 find it on the person's profile."
                    .to_string(),
            ),
            Err(reason) => Some(reason),
        },
    );
    if let Some(committed) = out.committed {
        *target_did = committed;
        undo_label.set("portal destination".to_string());
        *dirty = true;
    }
    ui.label(
        egui::RichText::new(
            "Leave it blank for a portal that goes nowhere yet — walking into \
             one always shows the destination's name first.",
        )
        .small()
        .color(crate::ui::theme::current(ui.ctx()).text_weak),
    );

    ui.add_space(4.0);
    ui.label("Exit position (world space in the target room)");
    ui.horizontal(|ui| {
        // The ranges the sanitiser already enforces (`pds::sanitize`'s
        // Portal arm): ±10 km horizontally, −1 km to 10 km vertically. The
        // drags were unbounded, so a number typed past them was silently
        // rewritten on the next flush.
        let ranges = [
            -10_000.0..=10_000.0,
            -1_000.0..=10_000.0,
            -10_000.0..=10_000.0,
        ];
        for ((label, axis), range) in ["X", "Y", "Z"]
            .iter()
            .zip(target_pos.0.iter_mut())
            .zip(ranges)
        {
            ui.label(*label);
            if ui
                .add(crate::ui::num::drag(axis).speed(0.1).range(range))
                .changed()
            {
                undo_label.set("portal exit position".to_string());
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
                .add(crate::ui::num::drag(axis).speed(0.1).range(0.25..=50.0))
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
    // Click-to-pick channel (#961), already narrowed to this node by the
    // caller: the arm flag plus any face a scene click resolved here.
    pick: FacePickUi<'_>,
    // The asset caches, for every field that names a fetched image, sound
    // or terrain layer (#1246).
    assets: &mut super::super::assets::AssetPanel<'_>,
) {
    // Snapshot taken before the match's mutable borrow: the per-face panel
    // (#960) needs the WHOLE kind — which faces the current cut state emits,
    // what an override's "inherit" projection resolves to — and the arm that
    // runs holds the same node's `faces` mutably. Only primitives (the kinds
    // that have faces at all) pay the clone.
    let snapshot = kind.faces().is_some().then(|| kind.clone());
    // One `PrimEdit` per primitive arm, from the arm's own `common` binding
    // plus the snapshot (#1188). A macro rather than a helper fn because the
    // panel borrows `undo_label` mutably, and only the arm that runs may
    // take that borrow.
    let FacePickUi {
        pick: face_pick,
        resolvable,
        picked: just_picked,
    } = pick;
    macro_rules! edit {
        ($common:expr) => {
            PrimEdit {
                common: $common,
                faces: FacePanel {
                    snapshot: snapshot.as_ref(),
                    undo_label: &mut *undo_label,
                    pick: FacePickUi {
                        pick: &mut *face_pick,
                        resolvable,
                        picked: just_picked,
                    },
                },
                salt,
                dirty: &mut *dirty,
                assets: &mut *assets,
            }
        };
    }
    match kind {
        GeneratorKind::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty, assets),
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
            assets,
        ),
        GeneratorKind::Shape {
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
            round_meshes,
        } => draw_shape_forge(
            ui,
            grammar_status,
            grammar_source,
            root_rule,
            footprint,
            seed,
            materials,
            round_meshes,
            dirty,
            assets,
        ),
        GeneratorKind::Portal {
            target_did,
            target_pos,
        } => draw_portal_editor(ui, target_did, target_pos, dirty, undo_label),
        GeneratorKind::Gateway { size } => draw_gateway_editor(ui, size, dirty),
        GeneratorKind::Cuboid { size, common, .. } => {
            draw_primitive_cuboid(ui, size, edit!(common))
        }
        GeneratorKind::Sphere {
            radius,
            resolution,
            common,
            ..
        } => draw_primitive_sphere(ui, radius, resolution, edit!(common)),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            common,
            ..
        } => draw_primitive_cylinder(ui, radius, height, resolution, edit!(common)),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            common,
            ..
        } => draw_primitive_capsule(ui, radius, length, latitudes, longitudes, edit!(common)),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            common,
            ..
        } => draw_primitive_cone(ui, radius, height, resolution, edit!(common)),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            common,
            ..
        } => draw_primitive_torus(
            ui,
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            edit!(common),
        ),
        GeneratorKind::Plane {
            size,
            subdivisions,
            common,
            ..
        } => draw_primitive_plane(ui, size, subdivisions, edit!(common)),
        GeneratorKind::Tetrahedron { size, common, .. } => {
            draw_primitive_tetrahedron(ui, size, edit!(common))
        }
        GeneratorKind::Tube {
            radius,
            inner_radius,
            height,
            resolution,
            common,
            ..
        } => draw_primitive_tube(ui, radius, inner_radius, height, resolution, edit!(common)),
        GeneratorKind::Bevel {
            size,
            bevel,
            bevel_segments,
            common,
            ..
        } => draw_primitive_bevel(ui, size, bevel, bevel_segments, edit!(common)),
        // A wedge carries the same fields as a cuboid (a bounding box); reuse
        // the cuboid editor.
        GeneratorKind::Wedge { size, common, .. } => draw_primitive_cuboid(ui, size, edit!(common)),
        GeneratorKind::Helix {
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            common,
            ..
        } => draw_primitive_helix(
            ui,
            radius,
            tube_radius,
            pitch,
            turns,
            resolution,
            edit!(common),
        ),
        GeneratorKind::Superellipsoid {
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            common,
            ..
        } => draw_primitive_superellipsoid(
            ui,
            half_extents,
            exponent_ns,
            exponent_ew,
            latitudes,
            longitudes,
            edit!(common),
        ),
        GeneratorKind::Spine {
            points,
            resolution,
            samples_per_segment,
            common,
            ..
        } => draw_primitive_spine(ui, points, resolution, samples_per_segment, edit!(common)),
        GeneratorKind::Lathe {
            points,
            resolution,
            smooth,
            common,
            ..
        } => draw_primitive_lathe(ui, points, resolution, smooth, edit!(common)),
        GeneratorKind::BlobGroup {
            elements,
            resolution,
            common,
            ..
        } => draw_primitive_blob_group(
            ui,
            elements,
            resolution,
            edit!(common),
            blob_selected_element,
        ),
        // The legacy `uv_repeat` / `uv_offset` have no editor: the sanitizer
        // folds them into the material's UV transform (#964), which the
        // material section below edits.
        GeneratorKind::Sign {
            source,
            size,
            material,
            double_sided,
            alpha_mode,
            unlit,
            texture_filter,
            ..
        } => draw_generator_sign(
            ui,
            source,
            size,
            material,
            double_sided,
            alpha_mode,
            unlit,
            texture_filter,
            salt,
            dirty,
            assets,
        ),
        GeneratorKind::ParticleSystem(params) => {
            draw_generator_particles(ui, params, salt, dirty, assets)
        }
        GeneratorKind::Unknown => {
            ui.colored_label(
                crate::ui::theme::current(ui.ctx()).status.warn,
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}

#[cfg(test)]
mod regrow_warning_tests {
    use super::regrow_warning;

    /// #1245 f378. The sentence has to name the real scope before the
    /// slider moves, because the destruction happens a debounce later in a
    /// system with no UI — there is no frame in which a confirm could ask.
    #[test]
    fn the_warning_names_what_is_standing_and_that_moves_are_included() {
        let both = regrow_warning(312, 40);
        assert!(both.contains("312 buildings"), "{both}");
        assert!(both.contains("40 props"), "{both}");
        assert!(
            both.contains("moved"),
            "the expensive work is named: {both}"
        );

        // One of each reads as one of each.
        let one = regrow_warning(1, 1);
        assert!(one.contains("1 building and 1 prop"), "{one}");
        assert!(!one.contains("buildings"), "{one}");

        // A layer that grew nothing is not mentioned at all.
        let buildings_only = regrow_warning(5, 0);
        assert!(!buildings_only.contains("prop"), "{buildings_only}");
        let props_only = regrow_warning(0, 5);
        assert!(!props_only.contains("building"), "{props_only}");
    }
}

#[cfg(test)]
mod grammar_key_tests {
    use crate::world_builder::compile::synthetic_cache_key;

    /// #1250 f84. The spawn path files a grammar outcome under the node's
    /// synthetic cache key; the panel used to look up the ROOT name. For a
    /// tree with two grammar nodes that painted one node's line-numbered
    /// error under the other, and a child's own failure was never shown at
    /// all — the author edits the wrong file looking for a line number that
    /// is not there, and the genuinely broken node reads as healthy.
    ///
    /// The read and the write have to build the same string, and they are in
    /// different crates' worth of module apart, so this is the contract.
    #[test]
    fn the_panel_looks_a_grammar_status_up_under_the_key_the_spawn_path_wrote() {
        // A root grammar node: the key is the record key, unchanged.
        assert_eq!(synthetic_cache_key("oak", &[]), "oak");
        // A child: root plus its path, which is what `spawn_generator`
        // passes to `record_grammar_status` for every nested node.
        assert_eq!(synthetic_cache_key("oak", &[0]), "oak/0");
        assert_eq!(synthetic_cache_key("oak", &[2, 1, 3]), "oak/2/1/3");
        // Two siblings are two keys — the whole point.
        assert_ne!(
            synthetic_cache_key("oak", &[0]),
            synthetic_cache_key("oak", &[1])
        );
        // And neither is the root's.
        assert_ne!(
            synthetic_cache_key("oak", &[0]),
            synthetic_cache_key("oak", &[])
        );
    }
}

#[cfg(test)]
mod lot_clamp_tests {
    use super::*;
    use crate::terrain::LotClamps;

    /// #1211, finding 384. Each clamp that emptied a lot gets a warn line
    /// naming it; the plain arithmetic line is quiet; adopted content
    /// (nothing injected this session) says nothing.
    #[test]
    fn every_clamp_that_bit_is_named() {
        assert!(lot_clamp_lines(&LotClamps::default(), 12).is_empty());
        let clamps = LotClamps {
            lots_found: 400,
            lots_kept: 200,
            buildings_dropped: 76,
            buildings_capped_by_budget: true,
            props_dropped: 3,
            props_capped_by_budget: false,
            generator_cap_skips: 2,
        };
        let lines = lot_clamp_lines(&clamps, 124);
        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines[0],
            (
                String::from("400 lots · 200 kept by density · 124 grown"),
                false
            )
        );
        assert!(lines[1].0.contains("76 lots left empty"), "{}", lines[1].0);
        assert!(lines[1].0.contains("placement budget"), "{}", lines[1].0);
        assert!(lines[1].1);
        assert!(lines[2].0.contains("3 furniture spots"), "{}", lines[2].0);
        assert!(lines[2].0.contains("per district"), "{}", lines[2].0);
        assert!(lines[3].0.contains("generator limit"), "{}", lines[3].0);
    }
}
