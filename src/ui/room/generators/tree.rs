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
use crate::ui::catalogue::catalogue_menu;
use crate::ui::inventory::is_drop_placeable;

use super::super::construct::{allows_children, make_default_for_kind};
use super::reparent::{PendingAction, apply_pending, find_node};
use super::{GenNodeId, GeneratorTreeSource, TreeViewState};

#[allow(clippy::too_many_arguments)]
pub(super) fn draw_tree_panel(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
    renaming_generator: &mut Option<(String, String)>,
    inventory: Option<&mut LiveInventoryRecord>,
    // Set for one frame after an in-world pick selected a node (#719): the
    // tree grabs keyboard focus so the row renders with the same bright
    // highlight a direct click gives it. A world-pick bypasses the tree's
    // own click-to-focus path, so without this the picked row shows the dim
    // *unfocused* highlight instead.
    request_focus: bool,
    dirty: &mut bool,
    confirms: &mut super::TreeConfirms,
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
    // Undo-entry label channel (#865): structural ops name themselves so
    // the toast can say "Undid: add of oak_3" instead of "Undid: edit".
    label: &mut crate::ui::undo::LabelSlot,
) {
    ui.heading("Generators");
    ui.add_space(2.0);

    let allowed_root_kinds = source.allowed_kinds_for_root();
    let allowed_child_kinds = source.allowed_kinds_for_child();
    // Multi-root capability drives three affordances at once: root rename,
    // root delete, and the add-root toolbar below. A single-root source
    // (avatar visuals) used to RENDER the add menus anyway — the user
    // opened a 20-entry kind list (or the whole catalogue), clicked, and
    // nothing happened because `add_root` refused (#830). Hidden now;
    // children are added via the row context menu's "+ Add child".
    let allow_rename = source.allow_multiple_roots();

    if !allow_rename {
        ui.label(
            egui::RichText::new("Right-click a row to add child parts.")
                .small()
                .weak(),
        );
    }

    ui.horizontal_wrapped(|ui| {
        if !source.allow_multiple_roots() {
            return;
        }
        ui.menu_button("+ New", |ui| {
            for kind_tag in allowed_root_kinds {
                if ui.button(*kind_tag).clicked() {
                    let kind = make_default_for_kind(kind_tag);
                    if let Some(name) =
                        source.add_root(&kind_tag.to_lowercase(), Generator::from_kind(kind))
                    {
                        *selected_generator = Some(name.clone());
                        *selected_prim_path = Some(Vec::new());
                        label.set(format!("add of {name}"));
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
                    label.set(format!("add of {new_name}"));
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
                catalogue_menu(ui, "", |slug, g| picked = Some((slug, g)));
                if let Some((slug, g)) = picked
                    && let Some(new_name) = source.add_root(&slug, g)
                {
                    *selected_generator = Some(new_name.clone());
                    *selected_prim_path = Some(Vec::new());
                    label.set(format!("add of {new_name}"));
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
                        .color(crate::ui::theme::current(ui.ctx()).text_weak),
                );
                return;
            }
            let inv_for_build: Option<&LiveInventoryRecord> = inventory.as_deref();
            // Reborrow as a shared trait-object reference for the
            // tree-build closure: it only needs read access via
            // `get_root`, and pending-action mutations are buffered into
            // the `RefCell` for application after the closure returns.
            let source_ref: &dyn GeneratorTreeSource = &*source;
            let tree_id = ui.make_persistent_id("generators_tree_view");
            let (_resp, actions) = TreeView::new(tree_id)
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

            // Grant the tree keyboard focus after an in-world pick (#719).
            // The widget only paints the bright `selection.bg_fill` while it
            // holds focus; an unfocused tree paints a dim `weak_bg_fill`, so
            // a programmatic selection would otherwise look different from a
            // direct click. Requesting focus here — inside the same egui
            // frame the tree is built — overrides the focus-clear that the
            // world click (on empty, non-egui space) would otherwise apply.
            if request_focus {
                ui.memory_mut(|m| m.request_focus(tree_id));
            }

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
            confirms,
            toasts,
            now,
            label,
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
    sync_selection_fields(
        valid,
        selected_generator,
        selected_prim_path,
        tree_view_state,
    );
}

/// Mirror the tree widget's (validated) selection into the editor-state
/// fields the gizmo layer reads. Deliberately has NO access to the shared
/// dirty flag (#828): selecting a row edits nothing, but the flag arms
/// the debounce, whose flush calls `set_changed()` on the live record —
/// a FULL recompile (room) / visuals despawn-respawn (avatar) plus a
/// whole-record peer broadcast per click. Browsing a large tree was a
/// hitch-and-network storm. Every real mutation (widgets, structural
/// ops, gizmo commits) sets dirty through its own path; keeping `dirty`
/// out of this signature makes the regression structurally impossible.
fn sync_selection_fields(
    valid: Option<GenNodeId>,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
) {
    match valid {
        Some(id) => {
            if selected_generator.as_deref() != Some(id.root.as_str()) {
                *selected_generator = Some(id.root.clone());
            }
            if selected_prim_path.as_deref() != Some(id.path.as_slice()) {
                *selected_prim_path = Some(id.path.clone());
            }
        }
        None => {
            if selected_generator.is_some() {
                *selected_generator = None;
            }
            if selected_prim_path.is_some() {
                *selected_prim_path = None;
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
                            .color(crate::ui::theme::current(ui.ctx()).text_weak),
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
                catalogue_menu(ui, "", |_slug, g| {
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
        // Cap-gated (#841): "Save to Inventory" used to insert
        // unconditionally, blowing past the 50-item cap the gift-accept
        // path enforces — sanitize then silently deleted the overflow on
        // the next login. Disabled (with the reason) instead of hidden.
        let cap = crate::config::state::MAX_INVENTORY_ITEMS;
        let stash_full = menu_inventory.is_some_and(|inv| inv.0.generators.len() >= cap);
        if ui
            .add_enabled(
                menu_inventory.is_some() && !stash_full,
                egui::Button::new("Save to Inventory"),
            )
            .on_hover_text("Copy this node into your inventory as a reusable blueprint")
            .on_disabled_hover_text(if stash_full {
                format!("Inventory full ({cap}/{cap}) — remove an item first")
            } else {
                "Inventory not loaded".to_owned()
            })
            .clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::SaveToInventory(menu_id.clone()));
            ui.close();
        }
        // A single-root source (avatar visuals) refuses root removal —
        // hide the item instead of offering a silent no-op (#830).
        // `menu_allow_rename` mirrors `allow_multiple_roots`.
        if (!menu_is_root || menu_allow_rename)
            && crate::ui::affordances::danger_menu_button(ui, "Delete").clicked()
        {
            *pending.borrow_mut() = Some(PendingAction::Delete(menu_id.clone()));
            ui.close();
        }
    };

    let is_container = allows_children(&node.kind);
    // Render the expand/collapse marker only for nodes that *have* children
    // (#719): a container-capable kind with an empty `children` list is a
    // directory in principle but shows no triangle, matching how a file
    // browser draws an empty folder as a plain row. Trees start collapsed
    // (`default_open(false)`); the in-world pick path re-opens a picked
    // node's ancestors so its row stays visible.
    if is_container && !node.children.is_empty() {
        builder.node(
            NodeBuilder::dir(id)
                .label(label)
                .default_open(false)
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
        // Leaf row (no triangle). Two cases collapse here: no-children kinds
        // (Water, Unknown), which reject every drop INTO them, and an *empty*
        // container, which still accepts drops so it can be populated by
        // dragging (its context menu keeps "+ Add child" too). `drop_allowed`
        // also governs the drop marker, so a leaf kind shows a hard "no" at
        // hover while an empty container shows the drop affordance.
        // `apply_reparent` re-checks `allows_children` defensively regardless.
        builder.node(
            NodeBuilder::leaf(id)
                .label(label)
                .drop_allowed(is_container)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #828: selection sync mirrors the tree into the gizmo-read fields —
    /// and, by construction (no `dirty` parameter), can never arm the
    /// debounce that recompiles + broadcasts the record.
    #[test]
    fn selection_sync_updates_fields_and_clears_stale_state() {
        let mut selected_generator: Option<String> = None;
        let mut selected_prim_path: Option<Vec<usize>> = None;
        let mut tree_state = TreeViewState::default();

        // A valid selection lands in both fields.
        let id = GenNodeId::child("oak".to_string(), vec![1, 0]);
        tree_state.set_selected(vec![id.clone()]);
        sync_selection_fields(
            Some(id),
            &mut selected_generator,
            &mut selected_prim_path,
            &mut tree_state,
        );
        assert_eq!(selected_generator.as_deref(), Some("oak"));
        assert_eq!(selected_prim_path, Some(vec![1, 0]));

        // A stale/no selection clears the fields AND the widget state.
        sync_selection_fields(
            None,
            &mut selected_generator,
            &mut selected_prim_path,
            &mut tree_state,
        );
        assert_eq!(selected_generator, None);
        assert_eq!(selected_prim_path, None);
        assert!(tree_state.selected().is_empty());
    }
}
