//! Pure-model reparent/move engine for the unified generator tree: the
//! buffered [`PendingAction`] application, the drag-and-drop
//! [`apply_reparent`] state machine with its cycle / stale-path guards,
//! and the `(root, path)` node-walk helpers. No egui rendering lives here
//! — the tree-panel widget in [`super::tree`] stages actions and this
//! module mutates the [`GeneratorTreeSource`] (#650).

use bevy::math::Affine3A;
use bevy::prelude::{Quat, Transform, Vec3};
use egui_ltreeview::DirPosition;

use crate::pds::{Generator, TransformData};
use crate::state::LiveInventoryRecord;

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
pub(super) enum PendingAction {
    /// Append a freshly-defaulted child of the chosen kind to `parent`.
    /// `kind_tag` is one of the `&'static str` tags returned by
    /// `available_kinds_for` — the apply step calls
    /// [`make_default_for_kind`] to materialise the variant's seed value.
    AddChild {
        parent: GenNodeId,
        kind_tag: &'static str,
    },
    /// Append a prebuilt generator (an inventory-entry *clone* or a fresh
    /// catalogue stamp) as a child of `parent`. The payload is materialised
    /// at click time inside the context-menu closure (where
    /// `&LiveInventoryRecord` / the catalogue entry is in scope), so the
    /// apply step doesn't need to re-borrow anything and never has to look
    /// the entry up by name. The generator payload is boxed so the enum's
    /// stack footprint stays small — `Generator` carries a deep tree and
    /// would otherwise dwarf every other variant.
    AddChildPrebuilt {
        parent: GenNodeId,
        generator: Box<Generator>,
    },
    Rename(String),
    /// Duplicate a node in place (#1244 f422): an inner node becomes a
    /// coincident sibling, a root becomes a new root under a fresh name.
    /// Copying a sub-assembly is the core reuse gesture of any large-world
    /// editor, and the one surface that can reach an unplaced or
    /// off-screen generator — the tree — could not do it: drag resolves
    /// only to `Action::Move`, so the gesture the owner WILL try is
    /// destructive to the source, and the only Duplicate in the app
    /// required physically finding the object in the world and
    /// right-clicking it.
    Duplicate(GenNodeId),
    /// Put a deep copy of a node on the editor's clipboard (#1244 f422),
    /// so a copy can cross roots without round-tripping through the
    /// inventory's 50 slots and a different window.
    Copy(GenNodeId),
    /// Append the clipboard's node as a child of `parent`.
    PasteChild(GenNodeId),
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

/// Drain a single buffered context-menu action and mutate the source in
/// the right way for it. Encapsulates the structural-edit machinery — add
/// child / rename / save to inventory / delete — so the tree-build pass
/// stays a pure read of the source's roots.
#[allow(clippy::too_many_arguments)]
pub(super) fn apply_pending(
    action: PendingAction,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
    renaming_generator: &mut Option<(String, String)>,
    inventory: Option<&mut LiveInventoryRecord>,
    dirty: &mut bool,
    confirms: &mut super::TreeConfirms,
    toasts: &mut crate::notify::Toasts,
    now: f64,
    // Undo-entry label channel (#865). Only set on arms that actually
    // mutate the record (`dirty = true`) — a parked label with no
    // matching change tick would mislabel the NEXT edit.
    label: &mut crate::ui::undo::LabelSlot,
    // The editor's one-node clipboard (#1244 f422).
    clipboard: &mut Option<Generator>,
) {
    match action {
        PendingAction::AddChild { parent, kind_tag } => {
            if let Some(reason) = child_refusal(&*source, &parent, 0, 1) {
                toasts.warn(reason, now);
                return;
            }
            if let Some(node) = find_node_mut(source, &parent)
                && allows_children(&node.kind)
            {
                let new_kind = make_default_for_kind(kind_tag);
                label.set(format!("add of {kind_tag}"));
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
        PendingAction::AddChildPrebuilt { parent, generator } => {
            if let Some(reason) = child_refusal(
                &*source,
                &parent,
                super::super::caps::subtree_depth(&generator),
                super::super::caps::node_count(&generator),
            ) {
                toasts.warn(reason, now);
                return;
            }
            if let Some(node) = find_node_mut(source, &parent)
                && allows_children(&node.kind)
            {
                label.set(format!("add of {}", generator.kind_tag()));
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
        PendingAction::Duplicate(id) => {
            let Some(node) = find_node(&*source, &id).cloned() else {
                return;
            };
            if id.path.is_empty() {
                // A single-root source (the avatar's visuals tree, a worn
                // item's parts) has nowhere to put a second root, and
                // `add_root` refuses — say THAT rather than the generator
                // cap, which is not why.
                if !source.allow_multiple_roots() {
                    toasts.warn(
                        "This tree holds one top-level item — duplicate a part \
                         inside it instead.",
                        now,
                    );
                    return;
                }
                // A root's duplicate is a new root: `add_root` mints a
                // fresh unique name and refuses at the generator cap.
                let kind = node.kind_tag().to_string();
                match source.add_root(&id.root, node) {
                    Some(new_name) => {
                        label.set(format!("duplicate of {}", id.root));
                        *selected_generator = Some(new_name.clone());
                        *selected_prim_path = Some(Vec::new());
                        tree_view_state.set_one_selected(GenNodeId::root(new_name));
                        *dirty = true;
                    }
                    None => toasts.warn(
                        format!(
                            "Couldn't duplicate this {kind} — {}",
                            super::super::caps::Cap::Generators.full_reason()
                        ),
                        now,
                    ),
                }
                return;
            }
            let parent = GenNodeId::child(&id.root, id.path[..id.path.len() - 1].to_vec());
            if let Some(reason) = child_refusal(
                &*source,
                &parent,
                super::super::caps::subtree_depth(&node),
                super::super::caps::node_count(&node),
            ) {
                toasts.warn(reason, now);
                return;
            }
            let Some(root) = source.get_root_mut(&id.root) else {
                return;
            };
            // The same helper the in-world "Duplicate item" uses, so the
            // two doors onto one gesture cannot drift.
            let Some(new_idx) = crate::editor_gizmo::append_sibling_at_path(root, &id.path, None)
            else {
                return;
            };
            let mut new_path = id.path.clone();
            if let Some(last) = new_path.last_mut() {
                *last = new_idx;
            }
            label.set(format!("duplicate of {}", node.kind_tag()));
            *selected_generator = Some(id.root.clone());
            *selected_prim_path = Some(new_path.clone());
            tree_view_state.set_openness(parent, true);
            tree_view_state.set_one_selected(GenNodeId::child(&id.root, new_path));
            *dirty = true;
        }
        PendingAction::Copy(id) => {
            if let Some(node) = find_node(&*source, &id) {
                let kind = node.kind_tag();
                toasts.info(format!("Copied {kind} — paste it under any row"), now);
                *clipboard = Some(node.clone());
            }
        }
        PendingAction::PasteChild(parent) => {
            let Some(node) = clipboard.clone() else {
                return;
            };
            if let Some(reason) = child_refusal(
                &*source,
                &parent,
                super::super::caps::subtree_depth(&node),
                super::super::caps::node_count(&node),
            ) {
                toasts.warn(reason, now);
                return;
            }
            if let Some(target) = find_node_mut(source, &parent)
                && allows_children(&target.kind)
            {
                label.set(format!("paste of {}", node.kind_tag()));
                target.children.push(node);
                let new_idx = target.children.len() - 1;
                let mut new_path = parent.path.clone();
                new_path.push(new_idx);
                *selected_generator = Some(parent.root.clone());
                *selected_prim_path = Some(new_path.clone());
                tree_view_state.set_openness(parent.clone(), true);
                tree_view_state.set_one_selected(GenNodeId::child(&parent.root, new_path));
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
                // Cap enforcement (#841): the context-menu item is
                // disabled when full, but the buffered action could race
                // a same-frame insert — never exceed the cap here either.
                let cap = crate::config::state::MAX_INVENTORY_ITEMS;
                if inv.0.generators.len() >= cap {
                    toasts.warn(
                        format!("Inventory full ({cap}/{cap}) — item not saved."),
                        now,
                    );
                    return;
                }
                let prefix = if id.path.is_empty() {
                    id.root.clone()
                } else {
                    node.kind_tag().to_lowercase()
                };
                let safe_name = unique_key(&inv.0.generators, &prefix);
                inv.0.generators.insert(safe_name.clone(), node.clone());
                // First feedback this action ever had (#841) — and NO
                // `*dirty = true`: that flag arms the ROOM debounce +
                // peer broadcast, but this mutation touched only the
                // inventory record (its own dirty state is derived
                // live-vs-stored and needs no flag).
                toasts.success(
                    format!("Saved as \"{safe_name}\" — open Inventory to place or gift it."),
                    now,
                );
            }
        }
        PendingAction::Delete(id) => {
            if id.path.is_empty() {
                // Root delete — CASCADING: `remove_root`
                // also sweeps every Placement + traits entry referencing
                // this generator name (a 200-tree scatter dies with it).
                // Since #838 it never fires from the click itself: park it
                // behind the shared confirm, which names the blast radius.
                // `draw_generators_tab` performs the delete on confirm.
                request_root_delete(&mut confirms.delete, &*source, &id.root);
                return;
            } else if let Some(parent_id) = id.parent_id() {
                let last_idx = *id.path.last().expect("non-root has non-empty path");
                if let Some(parent) = find_node_mut(source, &parent_id)
                    && last_idx < parent.children.len()
                {
                    label.set(format!(
                        "delete of {}",
                        parent.children[last_idx].kind_tag()
                    ));
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
            // A root dropped INTO another node stops being a root, and
            // `remove_root` sweeps every placement that put it in the
            // world — the same cascade as a root delete, which is
            // confirmed. The drag was not (#1209): a 200-oak scatter
            // vanished behind an undo entry that read "reparent of". It
            // parks behind the same kind of confirm, naming the count;
            // `draw_generators_tab` applies it on confirm.
            // Caps first (#1210): a drop that would land the subtree past
            // the nesting or node cap used to be amputated by the next
            // flush, and a promotion to root at the generator cap went
            // through `add_root`'s refusal AFTER extraction — deleting the
            // subtree outright. Refused here, with the reason, before any
            // mutation.
            if let Some(reason) = reparent_refusal(&*source, &drag_source, &target) {
                toasts.warn(reason, now);
                return;
            }
            let placements = if drag_source.path.is_empty() && !target.is_virtual_root() {
                source.placement_ref_count(&drag_source.root)
            } else {
                0
            };
            if placements > 0 {
                confirms.reparent.request(
                    format!("Nest \"{}\"?", drag_source.root),
                    nest_warning(&drag_source.root, placements),
                    "Nest anyway",
                    PendingReparent {
                        source: drag_source,
                        target,
                        position,
                    },
                );
                return;
            }
            apply_reparent(
                source,
                selected_generator,
                selected_prim_path,
                tree_view_state,
                drag_source,
                target,
                position,
                dirty,
                label,
            );
        }
    }
}

/// Why a subtree of `depth` / `nodes` cannot be added under `parent`
/// (#1210): the sanitiser's depth and per-generator node caps, asked BEFORE
/// the insert instead of enforced by deletion a quarter second after it.
fn child_refusal(
    source: &dyn GeneratorTreeSource,
    parent: &GenNodeId,
    depth: usize,
    nodes: usize,
) -> Option<String> {
    use super::super::caps::{Cap, fits_under};
    if !fits_under(parent.path.len(), depth) {
        return Some(Cap::Depth.full_reason());
    }
    if source.node_count(&parent.root) + nodes > Cap::NodesPerGenerator.max() {
        return Some(Cap::NodesPerGenerator.full_reason());
    }
    None
}

/// Why a drag from `drag_source` to `target` is refused (#1210): a
/// promotion to root at the generator cap, or a landing that would put the
/// moved subtree past the nesting or node cap of its new root. A move
/// inside one root cannot change that root's node count.
fn reparent_refusal(
    source: &dyn GeneratorTreeSource,
    drag_source: &GenNodeId,
    target: &GenNodeId,
) -> Option<String> {
    use super::super::caps::{Cap, node_count, subtree_depth};
    let moved = find_node(source, drag_source)?;
    if target.is_virtual_root() {
        return (source.allow_multiple_roots() && source.root_capacity_remaining() == 0)
            .then(|| Cap::Generators.full_reason());
    }
    let nodes = if drag_source.root == target.root {
        0
    } else {
        node_count(moved)
    };
    child_refusal(source, target, subtree_depth(moved), nodes)
}

/// A drag-and-drop move parked behind the nest confirm (#1209), replayed
/// through [`apply_reparent`] when the owner says yes.
#[derive(Clone, Debug)]
pub(crate) struct PendingReparent {
    pub(crate) source: GenNodeId,
    pub(crate) target: GenNodeId,
    pub(crate) position: DirPosition<GenNodeId>,
}

/// The body of the nest confirm: what nesting `root` takes with it.
fn nest_warning(root: &str, placements: usize) -> String {
    format!(
        "Nesting \"{root}\" under another node removes the {placements} placement{} that put it \
         in the world — the same as deleting it and re-adding it as a child. Undo (Ctrl+Z) can \
         restore it this session.",
        if placements == 1 { "" } else { "s" },
    )
}

/// Park the cascading root delete behind the shared confirm, naming the
/// blast radius (#838). ONE builder for every door onto the cascade
/// (#1209): the tree's `− Delete` and the scene menu's "Delete item (and
/// its placements)" — which used to run the identical sweep with no
/// confirmation at all. Answered in `draw_generators_tab`.
pub(crate) fn request_root_delete(
    confirm: &mut crate::ui::confirm::ConfirmState<GenNodeId>,
    source: &dyn GeneratorTreeSource,
    root: &str,
) {
    let placements = source.placement_ref_count(root);
    let body = if placements > 0 {
        format!(
            "Deletes the item \"{root}\" AND removes the {placements} \
             placement{} referencing it from the world. Undo (Ctrl+Z) \
             can restore it this session.",
            if placements == 1 { "" } else { "s" },
        )
    } else {
        format!(
            "Deletes the item \"{root}\" and everything under it. \
             Undo (Ctrl+Z) can restore it this session."
        )
    };
    confirm.request(
        format!("Delete \"{root}\"?"),
        body,
        "Delete",
        GenNodeId::root(root),
    );
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
    label: &mut crate::ui::undo::LabelSlot,
) {
    if drag_source.is_virtual_root() {
        return;
    }
    // Self-move and ancestor-into-descendant moves would create cycles.
    if drag_source == target || is_ancestor_of(&drag_source, &target) {
        return;
    }

    // Dropping a node immediately before or after *itself* is a no-op.
    // The anchor still carries the pre-removal path, so an `After(self)`
    // would (post-extraction) resolve to the slot the right sibling
    // shifted into and insert the node one position too far right —
    // `adjust_path_after_removal` deliberately leaves the removed index
    // unchanged, so `sibling_index_in` can't compensate. Bail before any
    // mutation rather than fix it up downstream.
    match &position {
        DirPosition::Before(anchor) | DirPosition::After(anchor) if anchor == &drag_source => {
            return;
        }
        _ => {}
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

    // The frame the source currently hangs in, captured *before* any
    // extraction — for a cross-root move the old chain may not survive
    // Phase 1. Phase 3 uses it to keep the subtree looking exactly where
    // it looked, so a drag in the tree changes the hierarchy and nothing
    // else (#926).
    let old_parent_world = if drag_source.path.is_empty() {
        Some(Affine3A::IDENTITY)
    } else {
        drag_source
            .parent_id()
            .and_then(|p| chain_affine(&*source, &p))
    };

    // Blast radius of a root → inner move, measured BEFORE the sweep so
    // the undo entry can say what the cascade took (#1209) — "reparent of
    // <root>" hid a 200-placement loss.
    let swept = if drag_source.path.is_empty() && !target_is_virtual {
        source.placement_ref_count(&drag_source.root)
    } else {
        0
    };
    let nested_root = drag_source.root.clone();

    // Phase 1: extract the source subtree. For root sources we pull
    // through `remove_root` (which also sweeps any implementation-specific
    // dangling references); for child sources we splice out of the
    // parent's children Vec.
    let mut extracted: Generator = if drag_source.path.is_empty() {
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

    // Phase 1b: rebase the subtree's local transform into the destination
    // frame, so the drop preserves its world pose. `target` is already in
    // post-removal coordinates, so the new chain resolves correctly.
    let new_parent_world = if target_is_virtual {
        Some(Affine3A::IDENTITY)
    } else {
        chain_affine(&*source, &target)
    };
    if let (Some(old_parent), Some(new_parent)) = (old_parent_world, new_parent_world) {
        rebase_local(&mut extracted.transform, old_parent, new_parent);
    }

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
    label.set(if swept > 0 {
        format!(
            "nest of {nested_root} + {swept} placement{}",
            if swept == 1 { "" } else { "s" }
        )
    } else {
        format!("reparent of {}", new_id.root)
    });
    tree_view_state.set_one_selected(new_id);
    *dirty = true;
}

/// The affine transform of `id`'s frame, accumulated from its root
/// generator down through every ancestor's local transform (the root's own
/// transform included — the spawner applies it too). Returns `None` if any
/// link in the chain is missing.
///
/// "World" here means *the root generator's frame*, deliberately stopping
/// short of the `Placement` that anchors that root into the region. A root
/// can be placed many times over — a `Scatter` stamps one generator across
/// hundreds of poses — so there is no single world pose to preserve
/// against. Within one root tree (the overwhelmingly common drag, and the
/// only kind an avatar's single-root tree has) the placement is a shared
/// prefix that cancels out of the rebase entirely, so this is exact.
fn chain_affine(source: &dyn GeneratorTreeSource, id: &GenNodeId) -> Option<Affine3A> {
    if id.is_virtual_root() {
        return Some(Affine3A::IDENTITY);
    }
    let mut node = source.get_root(&id.root)?;
    let mut acc = affine_of(&node.transform);
    for &i in &id.path {
        node = node.children.get(i)?;
        acc *= affine_of(&node.transform);
    }
    Some(acc)
}

fn affine_of(t: &TransformData) -> Affine3A {
    Affine3A::from_scale_rotation_translation(
        Vec3::from_array(t.scale.0),
        Quat::from_array(t.rotation.0),
        Vec3::from_array(t.translation.0),
    )
}

/// Rewrite `local` so that `new_parent * local` reproduces the world pose
/// `old_parent * local` had before the move.
///
/// Bails without touching `local` when the new parent chain is singular
/// (a zero scale anywhere above the drop target) or when the result is not
/// finite: an un-rebased drop is merely surprising, whereas writing a NaN
/// transform into the record poisons the node for good. Decomposing back
/// to TRS also drops any shear the chain introduced — only reachable via
/// non-uniform scale combined with rotation, which the runtime's own
/// `Transform` propagation cannot represent either.
fn rebase_local(local: &mut TransformData, old_parent: Affine3A, new_parent: Affine3A) {
    if new_parent.matrix3.determinant().abs() < 1e-9 {
        return;
    }
    let rebased = new_parent.inverse() * old_parent * affine_of(local);
    let (scale, rotation, translation) = rebased.to_scale_rotation_translation();
    if !translation.is_finite() || !scale.is_finite() || !rotation.is_finite() {
        return;
    }
    *local = TransformData::from(Transform {
        translation,
        rotation,
        scale,
    });
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

// ---------------------------------------------------------------------------
// Tests — exercise `apply_reparent`, `sweep_root_refs`, and the cycle /
// invariant guards. These cover the bug-prone parts of Phase 2 + Phase 3
// so future refactors can't silently regress (a) dangling-Placement
// cleanup or (b) cycle protection.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::{RoomTreeSource, sweep_root_refs};
    use super::*;
    use crate::pds::{
        Environment, GeneratorKind, Placement, RoomRecord, ScatterBounds, TransformData,
    };
    use std::collections::HashMap;

    fn empty_record() -> RoomRecord {
        RoomRecord {
            lex_type: "network.symbios.room".to_string(),
            environment: Environment::default(),
            generators: HashMap::new(),
            placements: Vec::new(),
            traits: HashMap::new(),
            contact_effects: Default::default(),
            default_landing: None,
            opaque_refs: Default::default(),
        }
    }

    fn cuboid_root() -> Generator {
        Generator::default_cuboid()
    }

    /// #1209, finding 75. Sequence: drag the "oak" root onto another
    /// node in the tree to tidy the hierarchy. `remove_root` swept every
    /// placement referencing it — 200 scattered oaks gone with no
    /// warning, behind an undo entry that read "reparent of host". The
    /// same cascade reached via `− Delete` is confirmed with a count. The
    /// drop now parks behind a confirm naming the count, applies only on
    /// yes, and the undo entry names the cascade.
    #[test]
    fn nesting_a_placed_root_is_confirmed_and_its_undo_entry_names_the_cascade() {
        let mut record = empty_record();
        record.generators.insert("host".into(), cuboid_root());
        record.generators.insert("oak".into(), cuboid_root());
        record.placements.push(absolute_pointing_at("oak"));
        record.placements.push(absolute_pointing_at("oak"));

        let mut tvs = TreeViewState::default();
        let mut sel_gen = None;
        let mut sel_path = None;
        let mut renaming = None;
        let mut dirty = false;
        let mut confirms = super::super::TreeConfirms::default();
        let mut toasts = crate::notify::Toasts::default();
        let mut labels = crate::ui::undo::PendingUndoLabels::default();
        apply_pending(
            PendingAction::Reparent {
                source: GenNodeId::root("oak"),
                target: GenNodeId::root("host"),
                position: DirPosition::Last,
            },
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            &mut renaming,
            None,
            &mut dirty,
            &mut confirms,
            &mut toasts,
            0.0,
            &mut labels.slot(crate::ui::shortcuts::EditorKind::World),
            &mut None,
        );
        assert!(!dirty, "nothing moves on the drop itself");
        assert!(record.generators.contains_key("oak"));
        assert_eq!(
            record.placements.len(),
            2,
            "the placements survive the drop"
        );
        assert!(
            confirms.reparent.is_pending(),
            "the drop parks behind the nest confirm"
        );

        // A root with NO placements nests on the drop, as before.
        record.placements.clear();
        apply_pending(
            PendingAction::Reparent {
                source: GenNodeId::root("oak"),
                target: GenNodeId::root("host"),
                position: DirPosition::Last,
            },
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            &mut renaming,
            None,
            &mut dirty,
            &mut confirms,
            &mut toasts,
            0.0,
            &mut labels.slot(crate::ui::shortcuts::EditorKind::World),
            &mut None,
        );
        assert!(dirty);
        assert_eq!(record.generators["host"].children.len(), 1);

        // Confirmed (what `draw_generators_tab` replays): the cascade
        // happens, and the undo entry says what it took.
        let mut record = empty_record();
        record.generators.insert("host".into(), cuboid_root());
        record.generators.insert("oak".into(), cuboid_root());
        record.placements.push(absolute_pointing_at("oak"));
        record.placements.push(absolute_pointing_at("oak"));
        let mut labels = crate::ui::undo::PendingUndoLabels::default();
        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::root("oak"),
            GenNodeId::root("host"),
            DirPosition::Last,
            &mut dirty,
            &mut labels.slot(crate::ui::shortcuts::EditorKind::World),
        );
        assert!(
            record.placements.is_empty(),
            "the cascade is the same as a delete"
        );
        assert_eq!(
            labels.peek_room(),
            Some("nest of oak + 2 placements"),
            "the undo entry names the blast radius"
        );
    }

    /// #1210, findings 83 / #411 and 410. Sequence: drop a two-level
    /// subtree under a node at depth 15, or promote a subtree to root
    /// with 256 generators already. The first landed and was amputated by
    /// the next flush; the second went through `add_root` AFTER
    /// extraction — `None` at the cap, and the comment said the data loss
    /// was intentional. Both are refused before any mutation, with the
    /// cap's sentence.
    #[test]
    fn a_drop_past_a_cap_is_refused_before_extraction() {
        use super::super::super::caps::Cap;
        let mut record = empty_record();
        // A chain root: depth 0..=15.
        let mut chain = cuboid_root();
        let mut cursor = &mut chain;
        for _ in 0..15 {
            cursor.children = vec![cuboid_root()];
            cursor = &mut cursor.children[0];
        }
        record.generators.insert("chain".into(), chain);
        let mut two_deep = cuboid_root();
        two_deep.children = vec![cuboid_root()];
        two_deep.children[0].children = vec![cuboid_root()];
        record.generators.insert("bush".into(), two_deep);

        let deep_parent = GenNodeId::child("chain", vec![0; 15]);
        let refusal = reparent_refusal(
            &RoomTreeSource::new(&mut record),
            &GenNodeId::child("bush", vec![0]),
            &deep_parent,
        )
        .expect("a two-level subtree cannot hang at depth 16");
        assert_eq!(refusal, Cap::Depth.full_reason());
        // A leaf CAN hang there (depth 16 exists; it just keeps no children).
        assert_eq!(
            reparent_refusal(
                &RoomTreeSource::new(&mut record),
                &GenNodeId::child("bush", vec![0, 0]),
                &deep_parent,
            ),
            None
        );

        // Promotion at the generator cap.
        for i in 0..Cap::Generators.max() {
            record
                .generators
                .entry(format!("filler_{i}"))
                .or_insert_with(cuboid_root);
        }
        assert_eq!(
            RoomTreeSource::new(&mut record).root_capacity_remaining(),
            0
        );
        let refusal = reparent_refusal(
            &RoomTreeSource::new(&mut record),
            &GenNodeId::child("bush", vec![0]),
            &GenNodeId::default(),
        )
        .expect("promotion refused at the cap");
        assert_eq!(refusal, Cap::Generators.full_reason());
        assert!(
            RoomTreeSource::new(&mut record)
                .add_root("more", cuboid_root())
                .is_none(),
            "and the one insert every add path uses refuses too"
        );
        assert_eq!(
            record.generators["bush"].children.len(),
            1,
            "nothing extracted"
        );
    }

    /// #1209, finding 143. The scene menu's "Delete item (and its
    /// placements)" shares this builder with the tree's `− Delete`, so
    /// the two doors onto the cascade cannot word it differently.
    #[test]
    fn the_root_delete_confirm_names_the_blast_radius_from_one_builder() {
        let mut record = empty_record();
        record.generators.insert("oak".into(), cuboid_root());
        record.placements.push(absolute_pointing_at("oak"));
        let mut confirm = crate::ui::confirm::ConfirmState::<GenNodeId>::default();
        request_root_delete(&mut confirm, &RoomTreeSource::new(&mut record), "oak");
        assert!(confirm.is_pending());
        assert_eq!(
            record.placements.len(),
            1,
            "nothing happens until the answer"
        );
    }

    fn absolute_pointing_at(name: &str) -> Placement {
        Placement::Absolute {
            generator_ref: name.to_string(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: crate::pds::Fp(0.0),
        }
    }

    fn scatter_pointing_at(name: &str) -> Placement {
        Placement::Scatter {
            generator_ref: name.to_string(),
            bounds: ScatterBounds::default(),
            count: 1,
            local_seed: 0,
            biome_filter: Default::default(),
            snap_to_terrain: true,
            random_yaw: true,
            avoid_urban: false,
            float_on_water: false,
            naturalness: Default::default(),
        }
    }

    fn grid_pointing_at(name: &str) -> Placement {
        Placement::Grid {
            generator_ref: name.to_string(),
            transform: TransformData::default(),
            counts: [1, 1, 1],
            gaps: crate::pds::Fp3([1.0, 1.0, 1.0]),
            snap_to_terrain: true,
            random_yaw: false,
        }
    }

    /// `sweep_root_refs` removes every variant of placement that targets the
    /// deleted root and drops the matching `traits` entry. Forward-compat
    /// `Placement::Unknown` rows survive (we can't see their `generator_ref`).
    #[test]
    fn sweep_root_refs_removes_placements_and_traits() {
        let mut record = empty_record();
        record
            .generators
            .insert("victim".to_string(), cuboid_root());
        record
            .generators
            .insert("survivor".to_string(), cuboid_root());
        record.placements.push(absolute_pointing_at("victim"));
        record.placements.push(scatter_pointing_at("victim"));
        record.placements.push(grid_pointing_at("survivor"));
        record.placements.push(Placement::Unknown);
        record.traits.insert(
            "victim".to_string(),
            vec!["collider_heightfield".to_string()],
        );
        record
            .traits
            .insert("survivor".to_string(), vec!["sensor".to_string()]);

        sweep_root_refs(&mut record, "victim");

        assert_eq!(record.placements.len(), 2, "victim refs should be gone");
        for p in &record.placements {
            match p {
                Placement::Absolute { generator_ref, .. }
                | Placement::Scatter { generator_ref, .. }
                | Placement::Grid { generator_ref, .. } => {
                    assert_ne!(generator_ref, "victim");
                }
                Placement::Unknown => {}
            }
        }
        assert!(!record.traits.contains_key("victim"));
        assert!(record.traits.contains_key("survivor"));
    }

    /// #926: a drag changes the hierarchy, not the appearance. Moving a
    /// node between two parents with different poses must rewrite its local
    /// transform so its world pose is bit-for-bit where it was — including
    /// when the two chains differ in rotation and scale, not just position.
    #[test]
    fn reparent_preserves_world_pose_across_posed_parents() {
        let posed = |t: Transform| Generator {
            transform: TransformData::from(t),
            ..cuboid_root()
        };

        let mut record = empty_record();
        let mut host = posed(Transform::from_xyz(1.0, 0.0, -2.0));
        // from[0] — the donor parent, rotated and scaled.
        host.children.push(posed(
            Transform::from_xyz(3.0, 1.0, 0.5)
                .with_rotation(Quat::from_rotation_y(0.7))
                .with_scale(Vec3::splat(2.0)),
        ));
        // from[0][0] — the node that will be dragged.
        host.children[0].children.push(posed(
            Transform::from_xyz(0.25, 0.5, -0.75).with_rotation(Quat::from_rotation_x(0.3)),
        ));
        // to[1] — the receiving parent, posed differently again.
        host.children.push(posed(
            Transform::from_xyz(-4.0, 2.5, 6.0)
                .with_rotation(Quat::from_rotation_z(-1.1))
                .with_scale(Vec3::new(0.5, 0.5, 0.5)),
        ));
        record.generators.insert("host".to_string(), host);

        let dragged = GenNodeId::child("host", vec![0, 0]);
        let world_before = {
            let src = RoomTreeSource::new(&mut record);
            chain_affine(&src, &dragged).expect("chain resolves")
        };

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("host".to_string());
        let mut sel_path = Some(vec![0, 0]);
        let mut dirty = false;
        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            dragged,
            GenNodeId::child("host", vec![1]),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );
        assert!(dirty, "the move should have happened");

        let landed = GenNodeId::child("host", vec![1, 0]);
        let world_after = {
            let src = RoomTreeSource::new(&mut record);
            chain_affine(&src, &landed).expect("chain resolves at the new home")
        };

        let (s0, r0, t0) = world_before.to_scale_rotation_translation();
        let (s1, r1, t1) = world_after.to_scale_rotation_translation();
        assert!(
            t0.abs_diff_eq(t1, 1e-4),
            "world translation moved: {t0:?} -> {t1:?}"
        );
        assert!(
            s0.abs_diff_eq(s1, 1e-4),
            "world scale moved: {s0:?} -> {s1:?}"
        );
        // Quaternions double-cover rotations, so q and -q are the same pose.
        assert!(
            r0.abs_diff_eq(r1, 1e-4) || r0.abs_diff_eq(-r1, 1e-4),
            "world rotation moved: {r0:?} -> {r1:?}"
        );

        // ...and it really did move: the raw local transform had to change
        // for the world pose to survive.
        let local_after = find_node(&RoomTreeSource::new(&mut record), &landed)
            .expect("node is there")
            .transform
            .translation
            .0;
        assert!(
            local_after != [0.25, 0.5, -0.75],
            "local transform was left untouched — nothing was rebased"
        );
    }

    /// A drop into the *same* parent (a pure reorder) has identical old and
    /// new chains, so the rebase must be a no-op rather than drifting the
    /// node through repeated round-trips.
    #[test]
    fn reorder_within_one_parent_leaves_local_transform_alone() {
        let mut record = empty_record();
        let mut host = Generator {
            transform: TransformData::from(
                Transform::from_xyz(2.0, 3.0, 4.0).with_rotation(Quat::from_rotation_y(0.9)),
            ),
            ..cuboid_root()
        };
        for i in 0..2 {
            host.children.push(Generator {
                transform: TransformData::from(Transform::from_xyz(i as f32, 1.5, -0.5)),
                ..cuboid_root()
            });
        }
        record.generators.insert("host".to_string(), host);

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("host".to_string());
        let mut sel_path = Some(vec![0]);
        let mut dirty = false;
        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::child("host", vec![0]),
            GenNodeId::root("host"),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        let moved = &record.generators["host"].children[1];
        let t = moved.transform.translation.0;
        assert!(
            (t[0] - 0.0).abs() < 1e-5 && (t[1] - 1.5).abs() < 1e-5 && (t[2] + 0.5).abs() < 1e-5,
            "a pure reorder must not touch the local transform, got {t:?}"
        );
    }

    /// Cycle protection: a node is its own ancestor in the trivial sense, so
    /// dropping it onto itself is rejected. Dropping into a descendant of
    /// itself (a true cycle) is also rejected. Sibling drops are allowed.
    #[test]
    fn is_ancestor_of_recognises_proper_descendant() {
        let root = GenNodeId::root("a");
        let child = GenNodeId::child("a", vec![0]);
        let grandchild = GenNodeId::child("a", vec![0, 1]);
        let other_root = GenNodeId::root("b");

        assert!(is_ancestor_of(&root, &child));
        assert!(is_ancestor_of(&root, &grandchild));
        assert!(is_ancestor_of(&child, &grandchild));
        // Self is *not* a proper ancestor — `apply_reparent` checks for
        // self-equality separately.
        assert!(!is_ancestor_of(&root, &root));
        assert!(!is_ancestor_of(&child, &root));
        assert!(!is_ancestor_of(&root, &other_root));
    }

    /// Inner → root promotion: dropping a child into the virtual root
    /// auto-keys it from the kind tag and registers it in
    /// `record.generators`. Selection follows the new id.
    #[test]
    fn reparent_inner_to_virtual_root_promotes_child() {
        let mut record = empty_record();
        let mut parent = cuboid_root();
        parent
            .children
            .push(Generator::from_kind(GeneratorKind::default_cuboid()));
        record.generators.insert("parent".to_string(), parent);

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("parent".to_string());
        let mut sel_path = Some(vec![0]);
        let mut dirty = false;

        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::child("parent", vec![0]),
            GenNodeId::default(),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        // parent's child list shrinks; a new top-level "cuboid" appears.
        let parent_after = record.generators.get("parent").expect("parent still there");
        assert!(parent_after.children.is_empty());
        assert!(record.generators.contains_key("cuboid"));
        // Selection should now name the promoted root.
        assert_eq!(sel_gen.as_deref(), Some("cuboid"));
        assert_eq!(sel_path.as_deref(), Some(&[][..]));
        assert!(dirty);
    }

    /// Root → inner demotion: dragging a top-level generator into another
    /// node's children removes the HashMap entry, sweeps placements/traits
    /// targeting the demoted root, and inserts the subtree as a child of
    /// the target.
    #[test]
    fn reparent_root_to_inner_demotes_and_sweeps_refs() {
        let mut record = empty_record();
        record.generators.insert("host".to_string(), cuboid_root());
        record
            .generators
            .insert("victim".to_string(), cuboid_root());
        record.placements.push(absolute_pointing_at("victim"));
        record
            .traits
            .insert("victim".to_string(), vec!["sensor".to_string()]);

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("victim".to_string());
        let mut sel_path = Some(Vec::new());
        let mut dirty = false;

        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::root("victim"),
            GenNodeId::root("host"),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        assert!(!record.generators.contains_key("victim"));
        let host = record.generators.get("host").expect("host still there");
        assert_eq!(host.children.len(), 1);
        // Dangling refs: gone.
        assert!(record.placements.is_empty());
        assert!(record.traits.is_empty());
        // Selection follows the moved subtree into its new home.
        assert_eq!(sel_gen.as_deref(), Some("host"));
        assert_eq!(sel_path.as_deref(), Some(&[0usize][..]));
        assert!(dirty);
    }

    /// Root → root reorder is a no-op: `record.generators` is a `HashMap`
    /// with no order, so dragging one root next to another can't change
    /// anything observable. The handler must NOT extract+reinsert (that
    /// would needlessly sweep refs and break selection).
    #[test]
    fn reparent_root_to_virtual_root_is_noop() {
        let mut record = empty_record();
        record.generators.insert("a".to_string(), cuboid_root());
        record.generators.insert("b".to_string(), cuboid_root());
        record.placements.push(absolute_pointing_at("a"));

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("a".to_string());
        let mut sel_path = Some(Vec::new());
        let mut dirty = false;

        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::root("a"),
            GenNodeId::default(),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        assert!(record.generators.contains_key("a"));
        assert!(record.generators.contains_key("b"));
        assert_eq!(record.placements.len(), 1);
        assert!(!dirty, "no-op reparent must not flip dirty");
    }

    /// Cycle protection: dragging a node into one of its own descendants
    /// would create a loop in the tree. The handler rejects it without
    /// mutating anything.
    #[test]
    fn reparent_into_own_descendant_is_rejected() {
        let mut record = empty_record();
        let mut root = cuboid_root();
        root.children
            .push(Generator::from_kind(GeneratorKind::default_cuboid()));
        record.generators.insert("a".to_string(), root);

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("a".to_string());
        let mut sel_path = Some(Vec::new());
        let mut dirty = false;

        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::root("a"),
            GenNodeId::child("a", vec![0]),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        // `a` still exists with its child, no churn.
        let a = record.generators.get("a").expect("a still there");
        assert_eq!(a.children.len(), 1);
        assert!(!dirty);
    }

    /// Regression: dragging a node "Inside" a sibling that comes *after*
    /// it in the same parent's children must not silently drop the
    /// extracted subtree. The pre-fix code resolved the target with the
    /// stale pre-removal path, which was either out-of-bounds (None,
    /// hits the early-return and deletes the dragged subtree) or pointed
    /// at the *next* sibling and dropped into the wrong node.
    #[test]
    fn reparent_inside_later_sibling_lands_in_correct_node() {
        let mut record = empty_record();
        let mut root = cuboid_root();
        // Three children A, B, C under "r".
        root.children
            .push(Generator::from_kind(GeneratorKind::default_cuboid()));
        root.children
            .push(Generator::from_kind(GeneratorKind::default_cuboid()));
        root.children
            .push(Generator::from_kind(GeneratorKind::default_cuboid()));
        record.generators.insert("r".to_string(), root);

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("r".to_string());
        let mut sel_path = Some(vec![0]);
        let mut dirty = false;

        // Drag A (path [0]) inside C (path [2], originally — after A is
        // extracted C lives at [1]).
        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::child("r", vec![0]),
            GenNodeId::child("r", vec![2]),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        let r = record.generators.get("r").expect("root still there");
        assert_eq!(
            r.children.len(),
            2,
            "extracting A should leave two top-level children"
        );
        assert_eq!(
            r.children[1].children.len(),
            1,
            "A should land inside what used to be C, not get dropped"
        );
        assert_eq!(
            r.children[0].children.len(),
            0,
            "B (now at index 0) must be untouched"
        );
        assert_eq!(sel_path.as_deref(), Some(&[1usize, 0][..]));
        assert!(dirty);
    }

    /// Regression: `DirPosition::After(anchor)` where the anchor is a
    /// sibling that follows the dragged node must drop at the correct
    /// post-removal index. With five children A,B,C,D,E and B dragged
    /// "After E", the result should be A,C,D,E,B — not A,C,D,B,E.
    #[test]
    fn reparent_after_later_sibling_uses_post_removal_index() {
        let mut record = empty_record();
        let mut root = cuboid_root();
        for _ in 0..5 {
            root.children
                .push(Generator::from_kind(GeneratorKind::default_cuboid()));
        }
        record.generators.insert("r".to_string(), root);
        // Tag each child via its translation.x so we can verify the
        // final order without depending on a per-node id field.
        for (i, c) in record
            .generators
            .get_mut("r")
            .unwrap()
            .children
            .iter_mut()
            .enumerate()
        {
            c.transform.translation = crate::pds::Fp3([i as f32, 0.0, 0.0]);
        }

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("r".to_string());
        let mut sel_path = Some(vec![1]);
        let mut dirty = false;

        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::child("r", vec![1]),
            GenNodeId::root("r"),
            DirPosition::After(GenNodeId::child("r", vec![4])),
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        let r = record.generators.get("r").expect("root still there");
        let order: Vec<i32> = r
            .children
            .iter()
            .map(|c| c.transform.translation.0[0] as i32)
            .collect();
        assert_eq!(order, vec![0, 2, 3, 4, 1]);
        assert_eq!(sel_path.as_deref(), Some(&[4usize][..]));
        assert!(dirty);
    }

    /// `drop_allowed(false)` on Water/Unknown is a UX-side guard; the
    /// model-side check in `apply_reparent` is the second line of defence.
    /// Dropping "Inside" a Water node must be rejected even if the widget
    /// somehow emits the move (e.g., a future widget version).
    #[test]
    fn reparent_inside_no_children_kind_is_rejected() {
        let mut record = empty_record();
        record.generators.insert(
            "water".to_string(),
            Generator::from_kind(GeneratorKind::Water {
                surface: crate::pds::WaterSurface::default(),
            }),
        );
        record.generators.insert("cube".to_string(), cuboid_root());

        let mut tvs = TreeViewState::default();
        let mut sel_gen = Some("cube".to_string());
        let mut sel_path = Some(Vec::new());
        let mut dirty = false;

        apply_reparent(
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            GenNodeId::root("cube"),
            GenNodeId::root("water"),
            DirPosition::Last,
            &mut dirty,
            &mut crate::ui::undo::PendingUndoLabels::default()
                .slot(crate::ui::shortcuts::EditorKind::World),
        );

        // Cube is still its own root; water still has zero children.
        assert!(record.generators.contains_key("cube"));
        let water = record.generators.get("water").expect("water still there");
        assert!(water.children.is_empty());
        assert!(!dirty);
    }

    /// #1244 f422. Sequence: build a good window frame inside "house_a"
    /// and want a copy inside "house_b". Dragging it in the tree MOVES it
    /// out of house_a (egui_ltreeview resolves only `Action::Move`, and
    /// `apply_reparent` is extract-then-insert), and the only Duplicate in
    /// the app was on the in-world right-click menu — which cannot reach
    /// an unplaced or off-screen generator at all. The only cross-root
    /// copy route was Save to Inventory → + From Inventory, consuming one
    /// of 50 slots and round-tripping through another window.
    #[test]
    fn copy_and_paste_move_a_subtree_between_roots_without_touching_the_source() {
        let mut record = RoomRecord::default();
        record.generators.clear();
        record.placements.clear();
        record.generators.insert(
            "house_a".into(),
            Generator {
                children: vec![Generator::default()],
                ..Generator::default()
            },
        );
        record
            .generators
            .insert("house_b".into(), Generator::default());
        let mut sel_gen = None;
        let mut sel_path = None;
        let mut tvs = TreeViewState::default();
        let mut renaming = None;
        let mut dirty = false;
        let mut confirms = crate::ui::room::generators::TreeConfirms::default();
        let mut toasts = crate::notify::Toasts::default();
        let mut labels = crate::ui::undo::PendingUndoLabels::default();
        let mut clipboard: Option<Generator> = None;

        let mut apply =
            |action: PendingAction, record: &mut RoomRecord, clipboard: &mut Option<Generator>| {
                apply_pending(
                    action,
                    &mut RoomTreeSource::new(record),
                    &mut sel_gen,
                    &mut sel_path,
                    &mut tvs,
                    &mut renaming,
                    None,
                    &mut dirty,
                    &mut confirms,
                    &mut toasts,
                    0.0,
                    &mut labels.slot(crate::ui::shortcuts::EditorKind::World),
                    clipboard,
                );
            };

        apply(
            PendingAction::Copy(GenNodeId::child("house_a", vec![0])),
            &mut record,
            &mut clipboard,
        );
        assert!(clipboard.is_some(), "Copy holds a deep copy");
        assert_eq!(
            record.generators["house_a"].children.len(),
            1,
            "Copy is not a move — the source is untouched"
        );

        apply(
            PendingAction::PasteChild(GenNodeId::root("house_b")),
            &mut record,
            &mut clipboard,
        );
        assert_eq!(
            record.generators["house_b"].children.len(),
            1,
            "the copy crossed roots without the inventory"
        );
        assert_eq!(record.generators["house_a"].children.len(), 1);
        assert!(dirty);
    }

    /// #1244 f422, the in-place half: an inner node duplicates as a
    /// coincident SIBLING (the same helper the in-world Duplicate uses,
    /// so the two doors onto one gesture cannot drift), and a root
    /// duplicates as a new root under a fresh unique name.
    #[test]
    fn duplicate_adds_a_sibling_for_an_inner_node_and_a_root_for_a_root() {
        let mut record = RoomRecord::default();
        // `RoomRecord::default()` is a seeded world, not an empty one.
        record.generators.clear();
        record.placements.clear();
        record.generators.insert(
            "house".into(),
            Generator {
                children: vec![Generator::default()],
                ..Generator::default()
            },
        );
        let mut sel_gen = None;
        let mut sel_path = None;
        let mut tvs = TreeViewState::default();
        let mut renaming = None;
        let mut dirty = false;
        let mut confirms = crate::ui::room::generators::TreeConfirms::default();
        let mut toasts = crate::notify::Toasts::default();
        let mut labels = crate::ui::undo::PendingUndoLabels::default();
        let mut clipboard = None;

        apply_pending(
            PendingAction::Duplicate(GenNodeId::child("house", vec![0])),
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            &mut renaming,
            None,
            &mut dirty,
            &mut confirms,
            &mut toasts,
            0.0,
            &mut labels.slot(crate::ui::shortcuts::EditorKind::World),
            &mut clipboard,
        );
        assert_eq!(record.generators["house"].children.len(), 2);
        assert_eq!(
            sel_path.as_deref(),
            Some(&[1][..]),
            "the selection lands on the copy, ready to drag apart"
        );

        apply_pending(
            PendingAction::Duplicate(GenNodeId::root("house")),
            &mut RoomTreeSource::new(&mut record),
            &mut sel_gen,
            &mut sel_path,
            &mut tvs,
            &mut renaming,
            None,
            &mut dirty,
            &mut confirms,
            &mut toasts,
            0.0,
            &mut labels.slot(crate::ui::shortcuts::EditorKind::World),
            &mut clipboard,
        );
        assert_eq!(record.generators.len(), 2, "a root duplicates as a root");
        assert_ne!(
            sel_gen.as_deref(),
            Some("house"),
            "…under a fresh name, and the selection follows it"
        );
    }
}
