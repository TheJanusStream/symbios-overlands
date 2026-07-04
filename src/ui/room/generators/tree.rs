//! Tree-panel widget: the egui tree-view sidebar and its per-row context
//! menu (`+ Add child` / `Rename` / `Save to Inventory` / `− Delete`).
//! Structural mutations are buffered as [`PendingAction`]s and applied by
//! the pure-model engine in [`super::reparent`] after the tree-build
//! closure releases its shared borrow.

use std::cell::RefCell;

use bevy_egui::egui;
use egui_ltreeview::{Action, NodeBuilder, TreeView};

use crate::pds::Generator;
use crate::state::LiveInventoryRecord;
use crate::ui::inventory::is_drop_placeable;

use super::super::construct::{allows_children, make_default_for_kind};
use super::reparent::{PendingAction, apply_pending, find_node};
use super::{GenNodeId, GeneratorTreeSource, TreeViewState};

/// Shared "+ From Catalogue" submenu body: entry buttons grouped under
/// category headers (Buildings / Plants / Patterns, from
/// [`crate::catalogue::CatalogueCategory::ALL`]; empty categories are
/// skipped so the menu stays compact as new ones are added). A click hands
/// `on_pick` the entry's slug plus a DID-less stamp — the submenu seeds the
/// editor with a fresh blueprint, and personalisable entries (e.g.
/// Teleporter) get their DID filled in at gift/drop time (editors can also
/// type it into the `target_did` field by hand). Used by both the toolbar's
/// add-root menu and the per-node context menu's add-child path.
fn catalogue_menu_entries(ui: &mut egui::Ui, mut on_pick: impl FnMut(String, Generator)) {
    for category in crate::catalogue::CatalogueCategory::ALL {
        let entries_here: Vec<_> = crate::catalogue::ENTRIES
            .iter()
            .copied()
            .filter(|e| e.category() == category)
            .collect();
        if entries_here.is_empty() {
            continue;
        }
        ui.label(
            egui::RichText::new(category.label())
                .strong()
                .color(egui::Color32::from_rgb(180, 180, 220)),
        );
        for entry in entries_here {
            if ui
                .button(entry.name())
                .on_hover_text(entry.description())
                .clicked()
            {
                on_pick(entry.slug().to_string(), entry.build(""));
                ui.close();
            }
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_tree_panel(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
    renaming_generator: &mut Option<(String, String)>,
    inventory: Option<&mut LiveInventoryRecord>,
    dirty: &mut bool,
) {
    ui.heading("Generators");
    ui.add_space(2.0);

    let allowed_root_kinds = source.allowed_kinds_for_root();
    let allowed_child_kinds = source.allowed_kinds_for_child();
    let allow_rename = source.allow_multiple_roots();

    ui.horizontal_wrapped(|ui| {
        ui.menu_button("+ New", |ui| {
            for kind_tag in allowed_root_kinds {
                if ui.button(*kind_tag).clicked() {
                    let kind = make_default_for_kind(kind_tag);
                    if let Some(name) =
                        source.add_root(&kind_tag.to_lowercase(), Generator::from_kind(kind))
                    {
                        *selected_generator = Some(name.clone());
                        *selected_prim_path = Some(Vec::new());
                        tree_view_state.set_one_selected(GenNodeId::root(name));
                        *dirty = true;
                    }
                    ui.close();
                }
            }
        });

        if let Some(inv) = inventory.as_deref()
            && !inv.0.generators.is_empty()
        {
            ui.menu_button("+ From Inventory", |ui| {
                let mut names: Vec<&String> = inv.0.generators.keys().collect();
                names.sort();
                let mut picked: Option<(String, Generator)> = None;
                for inv_name in names {
                    if ui.button(inv_name).clicked()
                        && let Some(g) = inv.0.generators.get(inv_name)
                    {
                        picked = Some((inv_name.clone(), g.clone()));
                        ui.close();
                    }
                }
                if let Some((inv_name, g)) = picked
                    && let Some(new_name) = source.add_root(&inv_name, g)
                {
                    *selected_generator = Some(new_name.clone());
                    *selected_prim_path = Some(Vec::new());
                    tree_view_state.set_one_selected(GenNodeId::root(new_name));
                    *dirty = true;
                }
            });
        }

        // Catalogue submenu — the client-shipped sibling of Inventory.
        // Same shape as "+ From Inventory": click an entry to stamp a
        // fresh copy into the tree as a new root.
        if !crate::catalogue::ENTRIES.is_empty() {
            ui.menu_button("+ From Catalogue", |ui| {
                let mut picked: Option<(String, Generator)> = None;
                catalogue_menu_entries(ui, |slug, g| picked = Some((slug, g)));
                if let Some((slug, g)) = picked
                    && let Some(new_name) = source.add_root(&slug, g)
                {
                    *selected_generator = Some(new_name.clone());
                    *selected_prim_path = Some(Vec::new());
                    tree_view_state.set_one_selected(GenNodeId::root(new_name));
                    *dirty = true;
                }
            });
        }
    });

    ui.separator();

    // The tree itself. Roots are sorted by the source for stable
    // presentation — HashMap iteration order would otherwise reshuffle
    // every frame as the layout cache rebuilds.
    let root_names: Vec<String> = source.root_names();

    // Pending-action channel shared into every per-row `context_menu`
    // closure. Closures all hold `&pending`; clicks call `borrow_mut()` to
    // stash an action. We drain it after `show_state` returns and apply
    // with mutable source access — that ordering keeps the tree's
    // immutable read of the source's roots (during the build closure)
    // clean of structural mutations.
    let pending: RefCell<Option<PendingAction>> = RefCell::new(None);

    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if root_names.is_empty() {
                ui.label(
                    egui::RichText::new("(no generators — click \"+ New\" above)")
                        .small()
                        .color(egui::Color32::GRAY),
                );
                return;
            }
            let inv_for_build: Option<&LiveInventoryRecord> = inventory.as_deref();
            // Reborrow as a shared trait-object reference for the
            // tree-build closure: it only needs read access via
            // `get_root`, and pending-action mutations are buffered into
            // the `RefCell` for application after the closure returns.
            let source_ref: &dyn GeneratorTreeSource = &*source;
            let (_resp, actions) = TreeView::new(ui.make_persistent_id("generators_tree_view"))
                .allow_drag_and_drop(true)
                .allow_multi_selection(false)
                .show_state(ui, tree_view_state, |builder| {
                    for name in &root_names {
                        if let Some(node) = source_ref.get_root(name) {
                            build_tree_node(
                                builder,
                                name,
                                node,
                                Vec::new(),
                                true,
                                allowed_child_kinds,
                                allow_rename,
                                &pending,
                                inv_for_build,
                            );
                        }
                    }
                });

            // Drain a Move (drag-commit) into the pending channel. We
            // only honour the first move event per frame and skip if a
            // context-menu click already staged something — collisions
            // are improbable but it keeps single-action semantics.
            for action in actions {
                if let Action::Move(dnd) = action {
                    if pending.borrow().is_some() {
                        break;
                    }
                    if let Some(src) = dnd.source.into_iter().next() {
                        *pending.borrow_mut() = Some(PendingAction::Reparent {
                            source: src,
                            target: dnd.target,
                            position: dnd.position,
                        });
                    }
                    break;
                }
            }
        });

    if let Some(action) = pending.into_inner() {
        apply_pending(
            action,
            source,
            selected_generator,
            selected_prim_path,
            tree_view_state,
            renaming_generator,
            inventory,
            dirty,
        );
    }

    // Sync the tree's selection back into the gizmo's source-of-truth so
    // `editor_gizmo` can read `(selected_generator, selected_prim_path)` to
    // attach the gizmo. Treat any selected id that no longer resolves to a
    // live node as "no selection" — happens after a delete / kind-change /
    // rename leaves the tree state holding a stale path.
    let valid: Option<GenNodeId> = tree_view_state
        .selected()
        .first()
        .filter(|id| find_node(&*source, id).is_some())
        .cloned();
    match valid {
        Some(id) => {
            if selected_generator.as_deref() != Some(id.root.as_str()) {
                *selected_generator = Some(id.root.clone());
                *dirty = true;
            }
            if selected_prim_path.as_deref() != Some(id.path.as_slice()) {
                *selected_prim_path = Some(id.path.clone());
                *dirty = true;
            }
        }
        None => {
            if selected_generator.is_some() {
                *selected_generator = None;
                *dirty = true;
            }
            if selected_prim_path.is_some() {
                *selected_prim_path = None;
                *dirty = true;
            }
            if !tree_view_state.selected().is_empty() {
                tree_view_state.set_selected(Vec::new());
            }
        }
    }
}

/// Recursively add `node` and its children to the tree-view builder. The
/// label format matches the user's expectation: roots show the source's key
/// (the user-given name) plus a kind hint; inner nodes show only the kind
/// since they're positional and unnamed. Each row gets a right-click
/// context menu wired through `pending` so structural ops (Add child /
/// Rename / Save to Inventory / Delete) buffer cleanly until after the
/// tree finishes drawing.
///
/// The lifetimes on `root_name` and `node` are independent of the builder's
/// own working lifetime — the label is materialised as an owned `String`
/// before being handed to [`NodeBuilder::label`], so the builder never
/// retains a reference into the source.
#[allow(clippy::too_many_arguments)]
fn build_tree_node(
    builder: &mut egui_ltreeview::TreeViewBuilder<'_, GenNodeId>,
    root_name: &str,
    node: &Generator,
    path: Vec<usize>,
    is_root: bool,
    allowed_child_kinds: &'static [&'static str],
    allow_rename: bool,
    pending: &RefCell<Option<PendingAction>>,
    inventory: Option<&LiveInventoryRecord>,
) {
    let id = GenNodeId::child(root_name, path.clone());
    let label = if is_root {
        format!("{}  ({})", root_name, node.kind_tag())
    } else {
        node.kind_tag().to_string()
    };

    let menu_id = id.clone();
    let menu_root = id.root.clone();
    let menu_allows_children = allows_children(&node.kind);
    let menu_is_root = is_root;
    let menu_allow_rename = allow_rename;
    // `Option<&T>` is `Copy`, so the move closure below copies the option
    // into its captures rather than borrowing — no extra lifetime
    // bookkeeping needed for the "+ From Inventory" submenu inside.
    let menu_inventory = inventory;
    let context_menu = move |ui: &mut egui::Ui| {
        if menu_allows_children {
            // Mirror the toolbar's "+ New" kind picker: a submenu listing
            // every kind valid as a child (the source's
            // `allowed_kinds_for_child()` set). Picking a kind stages an
            // `AddChild` action carrying that kind's static tag —
            // `apply_pending` calls `make_default_for_kind` to build the
            // actual node.
            ui.menu_button("+ Add child", |ui| {
                for kind_tag in allowed_child_kinds {
                    if ui.button(*kind_tag).clicked() {
                        *pending.borrow_mut() = Some(PendingAction::AddChild {
                            parent: menu_id.clone(),
                            kind_tag,
                        });
                        ui.close();
                    }
                }
            });
        }
        if menu_allows_children
            && let Some(inv) = menu_inventory
            && !inv.0.generators.is_empty()
        {
            ui.menu_button("+ From Inventory", |ui| {
                let mut names: Vec<&String> = inv
                    .0
                    .generators
                    .iter()
                    .filter(|(_, g)| is_drop_placeable(g))
                    .map(|(k, _)| k)
                    .collect();
                names.sort();
                if names.is_empty() {
                    ui.label(
                        egui::RichText::new("(no placeable inventory items)")
                            .small()
                            .color(egui::Color32::GRAY),
                    );
                    return;
                }
                for inv_name in names {
                    if ui.button(inv_name).clicked()
                        && let Some(g) = inv.0.generators.get(inv_name)
                    {
                        *pending.borrow_mut() = Some(PendingAction::AddChildPrebuilt {
                            parent: menu_id.clone(),
                            generator: Box::new(g.clone()),
                        });
                        ui.close();
                    }
                }
            });
        }
        // Catalogue stamps parent to the clicked node exactly like an
        // inventory clone — same buffered insert path, fresh blueprint.
        if menu_allows_children && !crate::catalogue::ENTRIES.is_empty() {
            ui.menu_button("+ From Catalogue", |ui| {
                catalogue_menu_entries(ui, |_slug, g| {
                    *pending.borrow_mut() = Some(PendingAction::AddChildPrebuilt {
                        parent: menu_id.clone(),
                        generator: Box::new(g),
                    });
                });
            });
        }
        // Rename rewrites the source's root key plus every Placement /
        // traits reference held alongside (room source). Inner nodes are
        // positional + unnamed and have nothing to rename. Single-root
        // sources (avatar visuals) suppress the option entirely via
        // `allow_rename = false`.
        if menu_is_root && menu_allow_rename && ui.button("Rename").clicked() {
            *pending.borrow_mut() = Some(PendingAction::Rename(menu_root.clone()));
            ui.close();
        }
        if ui.button("Save to Inventory").clicked() {
            *pending.borrow_mut() = Some(PendingAction::SaveToInventory(menu_id.clone()));
            ui.close();
        }
        if ui.button("− Delete").clicked() {
            *pending.borrow_mut() = Some(PendingAction::Delete(menu_id.clone()));
            ui.close();
        }
    };

    if allows_children(&node.kind) {
        builder.node(
            NodeBuilder::dir(id)
                .label(label)
                .default_open(is_root)
                .context_menu(context_menu),
        );
        for (i, child) in node.children.iter().enumerate() {
            let mut child_path = path.clone();
            child_path.push(i);
            build_tree_node(
                builder,
                root_name,
                child,
                child_path,
                false,
                allowed_child_kinds,
                allow_rename,
                pending,
                inventory,
            );
        }
        builder.close_dir();
    } else {
        // No-children kinds (Water, Unknown) reject every drop INTO
        // them — `apply_reparent` enforces the same invariant defensively
        // but `drop_allowed(false)` keeps the drop marker from rendering
        // in the first place, so the user sees a hard "no" at hover.
        builder.node(
            NodeBuilder::leaf(id)
                .label(label)
                .drop_allowed(false)
                .context_menu(context_menu),
        );
    }
}

/// Slash-separated string form of a child path, for display in the detail
/// header. An empty path renders as the empty string and the caller chooses
/// whether to show "/" or omit the suffix entirely.
pub(super) fn path_string(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Stable per-node salt for egui IDs. Includes the root key + child path
/// so collapsing one Cuboid never affects a sibling Cuboid drawn with the
/// same widget set.
pub(super) fn node_salt(id: &GenNodeId) -> String {
    let mut s = format!("gen_{}", id.root);
    for i in &id.path {
        s.push('_');
        s.push_str(&i.to_string());
    }
    s
}
