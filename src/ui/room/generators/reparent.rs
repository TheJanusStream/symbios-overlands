//! Pure-model reparent/move engine for the unified generator tree: the
//! buffered [`PendingAction`] application, the drag-and-drop
//! [`apply_reparent`] state machine with its cycle / stale-path guards,
//! and the `(root, path)` node-walk helpers. No egui rendering lives here
//! — the tree-panel widget in [`super::tree`] stages actions and this
//! module mutates the [`GeneratorTreeSource`] (#650).

use egui_ltreeview::DirPosition;

use crate::pds::Generator;
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
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
    // Undo-entry label channel (#865). Only set on arms that actually
    // mutate the record (`dirty = true`) — a parked label with no
    // matching change tick would mislabel the NEXT edit.
    label: &mut crate::ui::undo::LabelSlot,
) {
    match action {
        PendingAction::AddChild { parent, kind_tag } => {
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
                let placements = source.placement_ref_count(&id.root);
                let body = if placements > 0 {
                    format!(
                        "Deletes the generator \"{}\" AND removes the {placements} \
                         placement{} referencing it from the world. Undo (Ctrl+Z) \
                         can restore it this session.",
                        id.root,
                        if placements == 1 { "" } else { "s" },
                    )
                } else {
                    format!(
                        "Deletes the generator \"{}\" and everything under it. \
                         Undo (Ctrl+Z) can restore it this session.",
                        id.root
                    )
                };
                confirms
                    .delete
                    .request(format!("Delete \"{}\"?", id.root), body, "Delete", id);
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
    label.set(format!("reparent of {}", new_id.root));
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
        }
    }

    fn cuboid_root() -> Generator {
        Generator::default_cuboid()
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
}
