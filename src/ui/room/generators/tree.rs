//! Tree-panel widget: tree-view sidebar, drag-and-drop reparent, per-row
//! context menu (`+ Add child` / `Rename` / `Save to Inventory` / `−
//! Delete`), and the buffered [`PendingAction`] application that mutates
//! the [`GeneratorTreeSource`] after the tree-build closure releases its
//! shared borrow.

use std::cell::RefCell;

use bevy_egui::egui;
use egui_ltreeview::{Action, DirPosition, NodeBuilder, TreeView};

use crate::pds::Generator;
use crate::state::LiveInventoryRecord;
use crate::ui::inventory::is_drop_placeable;

use super::super::construct::{allows_children, make_default_for_kind};
use super::super::widgets::unique_key;
use super::{GenNodeId, GeneratorTreeSource, TreeViewState};

/// Out-of-band signal stored by a per-row context menu and applied after
/// the tree-view widget finishes drawing. Each menu closure captures
/// `&RefCell<Option<PendingAction>>` (a shared borrow), writes its action
/// on click via `borrow_mut`, and `draw_tree_panel` drains the cell with
/// `take()` once `show_state` returns. Buffering the actions like this
/// keeps the closures synchronous and side-effect-free against the
/// shared `&RoomRecord` borrow held during the tree build.
enum PendingAction {
    /// Append a freshly-defaulted child of the chosen kind to `parent`.
    /// `kind_tag` is one of the `&'static str` tags returned by
    /// `available_kinds_for` — the apply step calls
    /// [`make_default_for_kind`] to materialise the variant's seed value.
    AddChild {
        parent: GenNodeId,
        kind_tag: &'static str,
    },
    /// Append a *clone* of an inventory entry as a child of `parent`. The
    /// clone happens at click time inside the context-menu closure (where
    /// `&LiveInventoryRecord` is in scope), so the apply step doesn't need
    /// to re-borrow inventory and never has to look the entry up by name.
    /// The generator payload is boxed so the enum's stack footprint stays
    /// small — `Generator` carries a deep tree and would otherwise dwarf
    /// every other variant.
    AddChildFromInventory {
        parent: GenNodeId,
        generator: Box<Generator>,
    },
    Rename(String),
    SaveToInventory(GenNodeId),
    Delete(GenNodeId),
    /// Reparent triggered by drag-and-drop. `target` is the destination
    /// directory (or the virtual root for a top-level drop) and `position`
    /// places the source among `target`'s children.
    Reparent {
        source: GenNodeId,
        target: GenNodeId,
        position: DirPosition<GenNodeId>,
    },
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

/// Drain a single buffered context-menu action and mutate the source in
/// the right way for it. Encapsulates the structural-edit machinery — add
/// child / rename / save to inventory / delete — so the tree-build pass
/// stays a pure read of the source's roots.
#[allow(clippy::too_many_arguments)]
fn apply_pending(
    action: PendingAction,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
    renaming_generator: &mut Option<(String, String)>,
    inventory: Option<&mut LiveInventoryRecord>,
    dirty: &mut bool,
) {
    match action {
        PendingAction::AddChild { parent, kind_tag } => {
            if let Some(node) = find_node_mut(source, &parent)
                && allows_children(&node.kind)
            {
                let new_kind = make_default_for_kind(kind_tag);
                node.children.push(Generator::from_kind(new_kind));
                let new_idx = node.children.len() - 1;
                let mut new_path = parent.path.clone();
                new_path.push(new_idx);
                let new_id = GenNodeId::child(&parent.root, new_path.clone());
                *selected_generator = Some(parent.root.clone());
                *selected_prim_path = Some(new_path);
                tree_view_state.set_openness(parent, true);
                tree_view_state.set_one_selected(new_id);
                *dirty = true;
            }
        }
        PendingAction::AddChildFromInventory { parent, generator } => {
            if let Some(node) = find_node_mut(source, &parent)
                && allows_children(&node.kind)
            {
                node.children.push(*generator);
                let new_idx = node.children.len() - 1;
                let mut new_path = parent.path.clone();
                new_path.push(new_idx);
                let new_id = GenNodeId::child(&parent.root, new_path.clone());
                *selected_generator = Some(parent.root.clone());
                *selected_prim_path = Some(new_path);
                tree_view_state.set_openness(parent, true);
                tree_view_state.set_one_selected(new_id);
                *dirty = true;
            }
        }
        PendingAction::Rename(root_name) => {
            // The actual key migration + Placement / traits rewrite lives
            // in the rename modal in `super::room_admin_ui`; we just open
            // the modal with the current name pre-filled.
            *renaming_generator = Some((root_name.clone(), root_name));
        }
        PendingAction::SaveToInventory(id) => {
            if let Some(inv) = inventory
                && let Some(node) = find_node(&*source, &id)
            {
                let prefix = if id.path.is_empty() {
                    id.root.clone()
                } else {
                    node.kind_tag().to_lowercase()
                };
                let safe_name = unique_key(&inv.0.generators, &prefix);
                inv.0.generators.insert(safe_name, node.clone());
                *dirty = true;
            }
        }
        PendingAction::Delete(id) => {
            if id.path.is_empty() {
                // Root delete — `remove_root` also sweeps dangling
                // Placements + traits referencing this generator name on
                // implementations that carry such tables (room source).
                // On sources with no such side-tables (avatar source) the
                // sweep is a no-op.
                source.remove_root(&id.root);
            } else if let Some(parent_id) = id.parent_id() {
                let last_idx = *id.path.last().expect("non-root has non-empty path");
                if let Some(parent) = find_node_mut(source, &parent_id)
                    && last_idx < parent.children.len()
                {
                    parent.children.remove(last_idx);
                }
            }
            *selected_generator = None;
            *selected_prim_path = None;
            tree_view_state.set_selected(Vec::new());
            *dirty = true;
        }
        PendingAction::Reparent {
            source: drag_source,
            target,
            position,
        } => {
            apply_reparent(
                source,
                selected_generator,
                selected_prim_path,
                tree_view_state,
                drag_source,
                target,
                position,
                dirty,
            );
        }
    }
}

/// Apply a single drag-and-drop reparent. Handles the four kinds of
/// movement that the unified tree allows:
///
/// * **inner → inner** — move a child subtree to a different parent in
///   the same root tree, or to a different root tree entirely.
/// * **inner → root** — promote a child subtree to a brand-new top-level
///   generator, auto-keyed via `unique_key` from its kind tag.
/// * **root → inner** — demote a top-level generator into a child of
///   some node. The departing root's `Placement` references and `traits`
///   mapping are swept (same discipline as a root delete) so we never
///   leave an orphan.
/// * **root → root** — a no-op. Top-level generators live in a `HashMap`
///   that has no order, so reordering at the root is meaningless.
///
/// Cycle protection: a node can't be reparented into itself or any of
/// its descendants. The check is conservative — when in doubt we drop
/// the move.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_reparent(
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
    drag_source: GenNodeId,
    mut target: GenNodeId,
    mut position: DirPosition<GenNodeId>,
    dirty: &mut bool,
) {
    if drag_source.is_virtual_root() {
        return;
    }
    // Self-move and ancestor-into-descendant moves would create cycles.
    if drag_source == target || is_ancestor_of(&drag_source, &target) {
        return;
    }

    let target_is_virtual = target.is_virtual_root();

    // Reject "Inside" drops on nodes whose kind disallows children. The
    // tree itself uses `drop_allowed(false)` to prevent this in the UX,
    // but a defensive check at apply time keeps the model consistent
    // even if a future widget version emits the move anyway.
    if !target_is_virtual {
        let Some(target_node) = find_node(&*source, &target) else {
            return;
        };
        if !allows_children(&target_node.kind) {
            return;
        }
    }

    // root → virtual-root reorder is meaningless on multi-root sources
    // (HashMap has no order) and impossible on single-root sources. Also
    // skips the redundant remove + sweep pass we'd otherwise trigger.
    if drag_source.path.is_empty() && target_is_virtual {
        return;
    }

    // Promotion to a new top-level root requires multi-root support; on
    // single-root sources (e.g. avatar visuals) bail before any extraction
    // so the move is a no-op.
    if target_is_virtual && !source.allow_multiple_roots() {
        return;
    }

    // Phase 1: extract the source subtree. For root sources we pull
    // through `remove_root` (which also sweeps any implementation-specific
    // dangling references); for child sources we splice out of the
    // parent's children Vec.
    let extracted: Generator = if drag_source.path.is_empty() {
        let Some(node) = source.remove_root(&drag_source.root) else {
            return;
        };
        node
    } else {
        let Some(parent_id) = drag_source.parent_id() else {
            return;
        };
        let last_idx = *drag_source
            .path
            .last()
            .expect("non-root has non-empty path");
        let Some(parent) = find_node_mut(source, &parent_id) else {
            return;
        };
        if last_idx >= parent.children.len() {
            return;
        }
        let extracted = parent.children.remove(last_idx);

        // Removing index `last_idx` from `parent_id`'s children shifts the
        // index of every later sibling — and the index at the matching
        // depth of every descendant of those siblings — down by one. Any
        // GenNodeId that still carries a pre-removal path through that
        // parent is now stale and would either resolve to the wrong node
        // or fail `find_node` outright (silently dropping the extracted
        // subtree). Rewrite `target` and any anchor in `position` so the
        // Phase-2 lookups land on the correct post-removal nodes.
        adjust_path_after_removal(&mut target, &parent_id, last_idx);
        match &mut position {
            DirPosition::Before(anchor) | DirPosition::After(anchor) => {
                adjust_path_after_removal(anchor, &parent_id, last_idx);
            }
            DirPosition::First | DirPosition::Last => {}
        }

        extracted
    };

    // Phase 2: insert at the destination.
    let new_id = if target_is_virtual {
        // Promotion to top-level. Auto-key from the kind tag — matches
        // the "+ New" toolbar's behaviour.
        let prefix = extracted.kind_tag().to_lowercase();
        let Some(new_name) = source.add_root(&prefix, extracted) else {
            // Source refused the add (e.g. single-root already filled).
            // We've already removed the source subtree above; in that
            // unusual case the data loss is intentional — a no-op exit
            // would silently undo the user's drag.
            return;
        };
        GenNodeId::root(new_name)
    } else {
        // Drop into an existing dir. Translate `DirPosition` to a plain
        // index in `target.children`. Phase 1 already rewrote `target`
        // and any anchor in `position` to their post-removal coordinates,
        // so `target_children_len` and `sibling_index_in` already report
        // the correct post-removal layout — no further index fix-up is
        // needed even when source and target share a parent.
        let target_children_len = match find_node(&*source, &target) {
            Some(n) => n.children.len(),
            None => return,
        };

        let mut idx = match position {
            DirPosition::First => 0,
            DirPosition::Last => target_children_len,
            DirPosition::Before(anchor) => {
                sibling_index_in(&*source, &target, &anchor).unwrap_or(target_children_len)
            }
            DirPosition::After(anchor) => sibling_index_in(&*source, &target, &anchor)
                .map(|i| i + 1)
                .unwrap_or(target_children_len),
        };
        idx = idx.min(target_children_len);

        let Some(target_node) = find_node_mut(source, &target) else {
            return;
        };
        target_node.children.insert(idx, extracted);

        let mut new_path = target.path.clone();
        new_path.push(idx);
        GenNodeId::child(target.root.clone(), new_path)
    };

    // Selection follows the moved subtree. Also clear stale tree-view
    // openness state on the old id by simply not referencing it again.
    *selected_generator = Some(new_id.root.clone());
    *selected_prim_path = Some(new_id.path.clone());
    tree_view_state.set_one_selected(new_id);
    *dirty = true;
}

/// Rewrite `id` so it still names the same node after the child at
/// `removed_idx` was spliced out of `parent_id.children`. Removing
/// `parent.children[removed_idx]` shifts every later sibling down by
/// one; if `id`'s path runs through `parent_id` at a sibling index
/// greater than `removed_idx`, decrement that one digit so the post-
/// removal lookup lands on the right node. No-ops for ids in unrelated
/// roots, ids that don't pass through `parent_id`, and the virtual
/// root sentinel.
fn adjust_path_after_removal(id: &mut GenNodeId, parent_id: &GenNodeId, removed_idx: usize) {
    if id.is_virtual_root() {
        return;
    }
    if id.root != parent_id.root {
        return;
    }
    let depth = parent_id.path.len();
    if id.path.len() <= depth {
        return;
    }
    if id.path[..depth] != parent_id.path[..] {
        return;
    }
    if id.path[depth] > removed_idx {
        id.path[depth] -= 1;
    }
}

/// True when `ancestor` is on the path from a root to `descendant`. Used
/// to reject reparent moves that would create a cycle (drag a node into
/// one of its own descendants).
pub(super) fn is_ancestor_of(ancestor: &GenNodeId, descendant: &GenNodeId) -> bool {
    if ancestor.is_virtual_root() {
        // The virtual root is the ancestor of every real node, but
        // dropping a root *into* the virtual root is a no-op handled
        // separately, so reporting `true` here would needlessly block
        // promotion-from-inner moves. Restrict the meaningful check to
        // proper-prefix relationships within the same tree.
        return false;
    }
    if ancestor.root != descendant.root {
        return false;
    }
    descendant.path.starts_with(&ancestor.path) && descendant.path.len() > ancestor.path.len()
}

/// Find the index of `child` within `parent.children`, or `None` if
/// `child` is not in fact a direct child of `parent`. Used to translate
/// `DirPosition::Before(anchor) / After(anchor)` into a numeric index.
fn sibling_index_in(
    source: &dyn GeneratorTreeSource,
    parent: &GenNodeId,
    child: &GenNodeId,
) -> Option<usize> {
    if child.root != parent.root {
        return None;
    }
    if child.path.len() != parent.path.len() + 1 {
        return None;
    }
    if !child.path.starts_with(&parent.path) {
        return None;
    }
    let parent_node = find_node(source, parent)?;
    let last_idx = *child.path.last()?;
    if last_idx < parent_node.children.len() {
        Some(last_idx)
    } else {
        None
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
                        *pending.borrow_mut() = Some(PendingAction::AddChildFromInventory {
                            parent: menu_id.clone(),
                            generator: Box::new(g.clone()),
                        });
                        ui.close();
                    }
                }
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

/// Snapshot of the currently-selected node id, derived from
/// `(selected_generator, selected_prim_path)`. Returns `None` when nothing
/// is selected.
pub(super) fn current_id(
    selected_generator: &Option<String>,
    selected_prim_path: &Option<Vec<usize>>,
) -> Option<GenNodeId> {
    match (selected_generator.as_ref(), selected_prim_path.as_ref()) {
        (Some(root), Some(path)) => Some(GenNodeId::child(root.clone(), path.clone())),
        _ => None,
    }
}

impl GenNodeId {
    pub(super) fn parent_id(&self) -> Option<Self> {
        if self.path.is_empty() {
            return None;
        }
        let mut parent = self.path.clone();
        parent.pop();
        Some(GenNodeId::child(&self.root, parent))
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

/// Walk `(root, path)` from the source's roots to a `&Generator`. Returns
/// `None` if the root key is missing or any child index is out of bounds.
pub(super) fn find_node<'a>(
    source: &'a dyn GeneratorTreeSource,
    id: &GenNodeId,
) -> Option<&'a Generator> {
    let mut node = source.get_root(&id.root)?;
    for &i in &id.path {
        node = node.children.get(i)?;
    }
    Some(node)
}

/// Mutable counterpart of [`find_node`]. Splits the same `(root, path)`
/// walk into the matching `&mut Generator`.
pub(super) fn find_node_mut<'a>(
    source: &'a mut dyn GeneratorTreeSource,
    id: &GenNodeId,
) -> Option<&'a mut Generator> {
    let mut node = source.get_root_mut(&id.root)?;
    for &i in &id.path {
        node = node.children.get_mut(i)?;
    }
    Some(node)
}
