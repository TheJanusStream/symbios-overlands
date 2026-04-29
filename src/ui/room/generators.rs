//! Generators tab — unified tree-view sidebar on the left, per-node detail
//! editor on the right. The sidebar lists every named generator in
//! [`RoomRecord::generators`] as a tree root; each root recursively shows
//! its `children` so the entire generator hierarchy is browsable from one
//! place. Selecting a row in the tree drives both the on-screen editor and
//! the 3D gizmo target — `RoomEditorState::selected_generator` and
//! `selected_prim_path` are derived from the tree's selection each frame so
//! `editor_gizmo` can attach the gizmo to the matching live entity.
//!
//! Structural operations (`+ Add child`, `Rename`, `Save to Inventory`, `−
//! Delete`) live in the per-row right-click context menu. The context-menu
//! closures store a [`PendingAction`] into a shared [`RefCell`]; once the
//! tree-view widget finishes rendering, the action is drained and applied
//! with `&mut record` access. Root deletes additionally sweep dangling
//! `Placement` references and `traits` mappings keyed on the deleted
//! generator name, so dropping a generator never leaves orphan references
//! that the world compiler would log as "unknown generator_ref".
//!
//! Phase 3 will enable drag-to-reparent on the same tree.

use std::cell::RefCell;

use bevy_egui::egui;
use egui_ltreeview::{Action, DirPosition, NodeBuilder, TreeView};

use crate::pds::{
    AlphaModeKind, AnimationFrameMode, EmitterShape, Fp, Fp2, Fp3, Fp4, Generator, GeneratorKind,
    ParticleBlendMode, Placement, RoomRecord, SignSource, SimulationSpace,
    SovereignMaterialSettings, TextureAtlas, TextureFilter, WaterSurface,
};
use crate::state::LiveInventoryRecord;
use crate::ui::inventory::is_drop_placeable;

// `GenNodeId` is defined on `super` (the room editor's mod.rs) and
// re-exported here so external callers (e.g. `ui::avatar`) can address
// it as `ui::room::generators::GenNodeId` without reaching back into
// the room module's top-level surface.
pub use super::GenNodeId;
use super::construct::{
    AVATAR_KINDS, ROOM_CHILD_KINDS, ROOM_ROOT_KINDS, allows_children, draw_torture,
    draw_universal_material, generator_kind_picker, make_default_for_kind,
};
use super::lsystem::draw_lsystem_forge;
use super::shape::draw_shape_forge;
use super::terrain::draw_terrain_forge;
use super::widgets::{color_picker_rgba, drag_u32, draw_transform, fp_slider, unique_key};

/// Convenience alias so the per-tab function signature stays readable.
type TreeViewState = egui_ltreeview::TreeViewState<GenNodeId>;

// ---------------------------------------------------------------------------
// Generator-tree abstraction
// ---------------------------------------------------------------------------

/// Tree-source abstraction for the unified generator editor. Implemented
/// by [`RoomTreeSource`] (multi-root [`RoomRecord::generators`] HashMap
/// plus dangling-reference sweeps for placements/traits) and — in a later
/// avatar-unification phase — `AvatarVisualsTreeSource` (single-root
/// `AvatarRecord::visuals` with a stricter allowed-kinds set).
///
/// The trait deliberately exposes only the structural operations the
/// editor needs: root listing, root mutation (with implementation-specific
/// reference sweeps hidden behind [`Self::remove_root`]), and the
/// allowed-kinds vocabulary at root vs. child positions. Inventory access
/// stays *outside* the trait because the borrow patterns it needs (an
/// independent `&mut LiveInventoryRecord` held alongside the source's own
/// `&mut`) don't fit cleanly under partial-borrow rules.
pub(crate) trait GeneratorTreeSource {
    /// Names of every top-level root, in display order. The room source
    /// returns its HashMap keys sorted; an avatar source returns a single
    /// fixed name.
    fn root_names(&self) -> Vec<String>;
    fn get_root(&self, name: &str) -> Option<&Generator>;
    fn get_root_mut(&mut self, name: &str) -> Option<&mut Generator>;
    /// `true` when the source can hold more than one root. Drives the "+
    /// New" toolbar's behaviour and the inner→root drag-promotion path.
    fn allow_multiple_roots(&self) -> bool;
    /// Append a new top-level root. Implementations are free to pick a
    /// fresh unique name based on `prefix`. Returns the assigned name, or
    /// `None` when the source forbids multi-roots and one already exists.
    fn add_root(&mut self, prefix: &str, generator: Generator) -> Option<String>;
    /// Remove a top-level root, sweeping any implementation-specific
    /// references (Placements, traits, ...). Returns the extracted
    /// generator if it existed.
    fn remove_root(&mut self, name: &str) -> Option<Generator>;
    /// Allowed kind tags at the root of the tree.
    fn allowed_kinds_for_root(&self) -> &'static [&'static str];
    /// Allowed kind tags at child positions inside the tree.
    fn allowed_kinds_for_child(&self) -> &'static [&'static str];
}

/// `GeneratorTreeSource` adapter for the room editor: directly mutates
/// `RoomRecord::generators` and runs [`sweep_root_refs`] on root removal
/// so dangling Placement / traits entries don't survive a delete or
/// drag-out-to-promote.
pub(crate) struct RoomTreeSource<'a> {
    pub(crate) record: &'a mut RoomRecord,
}

impl<'a> RoomTreeSource<'a> {
    pub(crate) fn new(record: &'a mut RoomRecord) -> Self {
        Self { record }
    }
}

impl GeneratorTreeSource for RoomTreeSource<'_> {
    fn root_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.record.generators.keys().cloned().collect();
        names.sort();
        names
    }
    fn get_root(&self, name: &str) -> Option<&Generator> {
        self.record.generators.get(name)
    }
    fn get_root_mut(&mut self, name: &str) -> Option<&mut Generator> {
        self.record.generators.get_mut(name)
    }
    fn allow_multiple_roots(&self) -> bool {
        true
    }
    fn add_root(&mut self, prefix: &str, generator: Generator) -> Option<String> {
        let name = unique_key(&self.record.generators, prefix);
        self.record.generators.insert(name.clone(), generator);
        Some(name)
    }
    fn remove_root(&mut self, name: &str) -> Option<Generator> {
        let removed = self.record.generators.remove(name);
        if removed.is_some() {
            sweep_root_refs(self.record, name);
        }
        removed
    }
    fn allowed_kinds_for_root(&self) -> &'static [&'static str] {
        ROOM_ROOT_KINDS
    }
    fn allowed_kinds_for_child(&self) -> &'static [&'static str] {
        ROOM_CHILD_KINDS
    }
}

/// `GeneratorTreeSource` adapter for the avatar visuals tree. Wraps the
/// single `Generator` root from `AvatarRecord::visuals` and exposes it
/// under a fixed display name (`"visuals"`). Refuses every multi-root
/// operation: the avatar always has exactly one visual root. Allowed
/// kinds are primitives only — see [`AVATAR_KINDS`] for the rationale.
pub(crate) struct AvatarVisualsTreeSource<'a> {
    pub(crate) visuals: &'a mut Generator,
}

impl<'a> AvatarVisualsTreeSource<'a> {
    pub(crate) fn new(visuals: &'a mut Generator) -> Self {
        Self { visuals }
    }

    /// Fixed root key the avatar tree exposes through the source. The
    /// underlying `AvatarRecord` doesn't actually carry per-root names —
    /// it has a single anonymous root — but the tree-view widget keys on
    /// `(root, path)` so we hand it a stable string here.
    pub(crate) const ROOT_NAME: &'static str = "visuals";
}

impl GeneratorTreeSource for AvatarVisualsTreeSource<'_> {
    fn root_names(&self) -> Vec<String> {
        vec![Self::ROOT_NAME.to_string()]
    }
    fn get_root(&self, name: &str) -> Option<&Generator> {
        if name == Self::ROOT_NAME {
            Some(self.visuals)
        } else {
            None
        }
    }
    fn get_root_mut(&mut self, name: &str) -> Option<&mut Generator> {
        if name == Self::ROOT_NAME {
            Some(self.visuals)
        } else {
            None
        }
    }
    fn allow_multiple_roots(&self) -> bool {
        false
    }
    fn add_root(&mut self, _prefix: &str, _generator: Generator) -> Option<String> {
        // Single-root sources never accept new roots. Drag-promotion
        // (inner → root) is filtered out upstream by
        // `allow_multiple_roots == false`.
        None
    }
    fn remove_root(&mut self, _name: &str) -> Option<Generator> {
        // Removing the avatar's only root would leave the chassis with no
        // visuals — refuse and let the caller treat the operation as a
        // no-op. The root delete menu item still appears because hiding
        // it would require a separate trait method; clicking it just
        // does nothing.
        None
    }
    fn allowed_kinds_for_root(&self) -> &'static [&'static str] {
        AVATAR_KINDS
    }
    fn allowed_kinds_for_child(&self) -> &'static [&'static str] {
        AVATAR_KINDS
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_generators_tab(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    tree_view_state: &mut TreeViewState,
    renaming_generator: &mut Option<(String, String)>,
    inventory: Option<&mut LiveInventoryRecord>,
    dirty: &mut bool,
) {
    // Inventory now flows only into the tree panel (for the root-level
    // "+ From Inventory" toolbar, the per-row "+ From Inventory" submenu,
    // and the apply step's "Save to Inventory" write). The detail panel
    // never touches inventory anymore — its inventory-child picker moved
    // into the row context menu in issue #159.
    egui::SidePanel::left("generators_tree_panel")
        .resizable(true)
        .default_width(260.0)
        .min_width(180.0)
        .show_inside(ui, |ui| {
            draw_tree_panel(
                ui,
                source,
                selected_generator,
                selected_prim_path,
                tree_view_state,
                renaming_generator,
                inventory,
                dirty,
            );
        });

    egui::CentralPanel::default().show_inside(ui, |ui| {
        draw_detail_panel(ui, source, selected_generator, selected_prim_path, dirty);
    });
}

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
    /// [`available_kinds_for`] — the apply step calls
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

// ---------------------------------------------------------------------------
// Left: tree panel
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn draw_tree_panel(
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
fn apply_reparent(
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
fn is_ancestor_of(ancestor: &GenNodeId, descendant: &GenNodeId) -> bool {
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

/// Remove every `Placement` whose `generator_ref` matches the deleted root
/// and drop the matching `traits` entry. Keeps `Placement::Unknown` (the
/// forward-compat catch-all) since we can't see its `generator_ref` field.
/// Mirrors the integrity-preservation discipline of the rename modal's
/// commit path.
fn sweep_root_refs(record: &mut RoomRecord, deleted_root: &str) {
    record.placements.retain(|p| match p {
        Placement::Absolute { generator_ref, .. }
        | Placement::Scatter { generator_ref, .. }
        | Placement::Grid { generator_ref, .. } => generator_ref != deleted_root,
        Placement::Unknown => true,
    });
    record.traits.remove(deleted_root);
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

// ---------------------------------------------------------------------------
// Right: detail panel
// ---------------------------------------------------------------------------

/// Renders only the *content* of the selected node — kind picker,
/// transform, per-kind detail editor — plus a header that names the node
/// and shows its path. Every structural operation (Add child / Add child
/// from Inventory / Rename / Save to Inventory / Delete) lives in the
/// per-row context menu on the tree panel; this function never mutates
/// the tree shape.
fn draw_detail_panel(
    ui: &mut egui::Ui,
    source: &mut dyn GeneratorTreeSource,
    selected_generator: &mut Option<String>,
    selected_prim_path: &mut Option<Vec<usize>>,
    dirty: &mut bool,
) {
    let Some(id) = current_id(selected_generator, selected_prim_path) else {
        ui.vertical_centered(|ui| {
            ui.add_space(40.0);
            ui.label(
                egui::RichText::new("Select a generator from the tree to edit.")
                    .color(egui::Color32::GRAY),
            );
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("Right-click any tree row for: + Add child / Rename / Save to Inventory / − Delete.")
                    .small()
                    .color(egui::Color32::GRAY),
            );
        });
        return;
    };

    let is_root = id.path.is_empty();
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
            ui.label(egui::RichText::new(format!("({})", kind_tag)).color(egui::Color32::GRAY));
        } else {
            ui.heading(kind_tag);
            ui.label(
                egui::RichText::new(format!("path: /{}", path_string(&id.path)))
                    .small()
                    .color(egui::Color32::GRAY),
            );
        }
    });

    ui.separator();

    let salt = node_salt(&id);

    if let Some(node) = find_node_mut(source, &id) {
        ui.horizontal(|ui| {
            ui.label("Kind:");
            generator_kind_picker(ui, &mut node.kind, allowed_kinds, &salt, dirty);
        });

        ui.add_space(4.0);
        draw_transform(ui, &mut node.transform, dirty);
        ui.add_space(4.0);
        ui.separator();

        egui::ScrollArea::vertical()
            .id_salt(("gen_detail_scroll", &salt))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                draw_generator_detail(ui, &salt, &mut node.kind, dirty);
            });
    }
}

/// Snapshot of the currently-selected node id, derived from
/// `(selected_generator, selected_prim_path)`. Returns `None` when nothing
/// is selected.
fn current_id(
    selected_generator: &Option<String>,
    selected_prim_path: &Option<Vec<usize>>,
) -> Option<GenNodeId> {
    match (selected_generator.as_ref(), selected_prim_path.as_ref()) {
        (Some(root), Some(path)) => Some(GenNodeId::child(root.clone(), path.clone())),
        _ => None,
    }
}

impl GenNodeId {
    fn parent_id(&self) -> Option<Self> {
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
fn path_string(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("/")
}

/// Stable per-node salt for egui IDs. Includes the root key + child path
/// so collapsing one Cuboid never affects a sibling Cuboid drawn with the
/// same widget set.
fn node_salt(id: &GenNodeId) -> String {
    let mut s = format!("gen_{}", id.root);
    for i in &id.path {
        s.push('_');
        s.push_str(&i.to_string());
    }
    s
}

/// Walk `(root, path)` from the source's roots to a `&Generator`. Returns
/// `None` if the root key is missing or any child index is out of bounds.
fn find_node<'a>(source: &'a dyn GeneratorTreeSource, id: &GenNodeId) -> Option<&'a Generator> {
    let mut node = source.get_root(&id.root)?;
    for &i in &id.path {
        node = node.children.get(i)?;
    }
    Some(node)
}

/// Mutable counterpart of [`find_node`]. Splits the same `(root, path)`
/// walk into the matching `&mut Generator`.
fn find_node_mut<'a>(
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
// Per-kind detail dispatcher (unchanged from prior versions — every kind
// editor lives in a sibling module and this function picks the right one).
// ---------------------------------------------------------------------------

/// Per-kind variant detail editor. Dispatches into the per-variant forges
/// for Terrain / LSystem / Shape, owns the inline Water / Portal widgets,
/// and uses a shared primitive editor for every parametric shape. Does NOT
/// render the local transform — that's drawn separately in the detail
/// panel header.
///
/// `salt` uniquely identifies this node in egui's ID stack — it's passed
/// through to nested material widgets so collapsing one node never
/// affects another when the same widget type repeats across the tree.
pub(super) fn draw_generator_detail(
    ui: &mut egui::Ui,
    salt: &str,
    kind: &mut GeneratorKind,
    dirty: &mut bool,
) {
    match kind {
        GeneratorKind::Terrain(cfg) => draw_terrain_forge(ui, cfg, dirty),
        GeneratorKind::Water {
            level_offset,
            surface,
        } => {
            draw_water_editor(ui, level_offset, surface, dirty);
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
        } => {
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
                ui.label("X");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[0]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Y");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[1]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
                ui.label("Z");
                if ui
                    .add(egui::DragValue::new(&mut target_pos.0[2]).speed(0.1))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        GeneratorKind::Cuboid {
            size,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_cuboid(ui, size, solid, material, twist, taper, bend, salt, dirty),
        GeneratorKind::Sphere {
            radius,
            resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_sphere(
            ui, radius, resolution, solid, material, twist, taper, bend, salt, dirty,
        ),
        GeneratorKind::Cylinder {
            radius,
            height,
            resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_cylinder(
            ui, radius, height, resolution, solid, material, twist, taper, bend, salt, dirty,
        ),
        GeneratorKind::Capsule {
            radius,
            length,
            latitudes,
            longitudes,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_capsule(
            ui, radius, length, latitudes, longitudes, solid, material, twist, taper, bend, salt,
            dirty,
        ),
        GeneratorKind::Cone {
            radius,
            height,
            resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_cone(
            ui, radius, height, resolution, solid, material, twist, taper, bend, salt, dirty,
        ),
        GeneratorKind::Torus {
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_torus(
            ui,
            minor_radius,
            major_radius,
            minor_resolution,
            major_resolution,
            solid,
            material,
            twist,
            taper,
            bend,
            salt,
            dirty,
        ),
        GeneratorKind::Plane {
            size,
            subdivisions,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_plane(
            ui,
            size,
            subdivisions,
            solid,
            material,
            twist,
            taper,
            bend,
            salt,
            dirty,
        ),
        GeneratorKind::Tetrahedron {
            size,
            solid,
            material,
            twist,
            taper,
            bend,
        } => draw_primitive_tetrahedron(ui, size, solid, material, twist, taper, bend, salt, dirty),
        GeneratorKind::Sign {
            source,
            size,
            uv_repeat,
            uv_offset,
            material,
            double_sided,
            alpha_mode,
            unlit,
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
            salt,
            dirty,
        ),
        GeneratorKind::ParticleSystem {
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            looping,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            blend_mode,
            billboard,
            simulation_space,
            inherit_velocity,
            collide_terrain,
            collide_water,
            collide_colliders,
            bounce,
            friction,
            seed,
            texture,
            texture_atlas,
            frame_mode,
            texture_filter,
        } => draw_generator_particles(
            ui,
            emitter_shape,
            rate_per_second,
            burst_count,
            max_particles,
            looping,
            duration,
            lifetime_min,
            lifetime_max,
            speed_min,
            speed_max,
            gravity_multiplier,
            acceleration,
            linear_drag,
            start_size,
            end_size,
            start_color,
            end_color,
            blend_mode,
            billboard,
            simulation_space,
            inherit_velocity,
            collide_terrain,
            collide_water,
            collide_colliders,
            bounce,
            friction,
            seed,
            texture,
            texture_atlas,
            frame_mode,
            texture_filter,
            salt,
            dirty,
        ),
        GeneratorKind::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown generator type — editable only via the Raw JSON tab.",
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Per-primitive detail editors. Each one owns the shape-specific drag
// widgets, the solid checkbox, the torture triple, and the material panel.
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
fn draw_primitive_cuboid(
    ui: &mut egui::Ui,
    size: &mut Fp3,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Size X/Y/Z:");
        let mut v = size.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *size = Fp3(v);
            *dirty = true;
        }
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_sphere(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        drag_u32(ui, "Ico Res", resolution, 0, 10, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_cylinder(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_capsule(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    length: &mut Fp,
    latitudes: &mut u32,
    longitudes: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Length", length, 0.01, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Lats", latitudes, 2, 64, dirty);
        drag_u32(ui, "Lons", longitudes, 4, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_cone(
    ui: &mut egui::Ui,
    radius: &mut Fp,
    height: &mut Fp,
    resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Radius", radius, 0.01, 100.0, dirty);
        fp_slider(ui, "Height", height, 0.01, 100.0, dirty);
        drag_u32(ui, "Res", resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_torus(
    ui: &mut egui::Ui,
    minor_radius: &mut Fp,
    major_radius: &mut Fp,
    minor_resolution: &mut u32,
    major_resolution: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        fp_slider(ui, "Minor R", minor_radius, 0.01, 50.0, dirty);
        fp_slider(ui, "Major R", major_radius, 0.01, 100.0, dirty);
    });
    ui.horizontal(|ui| {
        drag_u32(ui, "Minor Res", minor_resolution, 3, 64, dirty);
        drag_u32(ui, "Major Res", major_resolution, 3, 128, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_plane(
    ui: &mut egui::Ui,
    size: &mut Fp2,
    subdivisions: &mut u32,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    ui.horizontal(|ui| {
        ui.label("Size X/Z:");
        let mut v = size.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *size = Fp2(v);
            *dirty = true;
        }
        drag_u32(ui, "Subdivs", subdivisions, 0, 32, dirty);
    });
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

#[allow(clippy::too_many_arguments)]
fn draw_primitive_tetrahedron(
    ui: &mut egui::Ui,
    size: &mut Fp,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    fp_slider(ui, "Size", size, 0.01, 100.0, dirty);
    draw_common_primitive(ui, solid, material, twist, taper, bend, salt, dirty);
}

/// Shared tail for every primitive editor: solid checkbox, torture triple,
/// collapsible material panel. Factored out so each per-primitive editor
/// only owns its shape-specific parameter widgets.
#[allow(clippy::too_many_arguments)]
fn draw_common_primitive(
    ui: &mut egui::Ui,
    solid: &mut bool,
    material: &mut crate::pds::SovereignMaterialSettings,
    twist: &mut Fp,
    taper: &mut Fp,
    bend: &mut Fp3,
    salt: &str,
    dirty: &mut bool,
) {
    if ui.checkbox(solid, "Solid (collider)").changed() {
        *dirty = true;
    }
    ui.add_space(2.0);
    draw_torture(ui, twist, taper, bend, dirty);
    ui.add_space(2.0);
    egui::CollapsingHeader::new("Material")
        .id_salt(format!("{}_mat", salt))
        .default_open(false)
        .show(ui, |ui| {
            draw_universal_material(ui, material, salt, dirty);
        });
}

/// Editor for the [`GeneratorKind::Sign`] panel: source picker, panel
/// size, UV repeat / offset, the StandardMaterial toggles
/// (double_sided / unlit / alpha_mode), and the shared material PBR
/// section.
#[allow(clippy::too_many_arguments)]
fn draw_generator_sign(
    ui: &mut egui::Ui,
    source: &mut SignSource,
    size: &mut Fp2,
    uv_repeat: &mut Fp2,
    uv_offset: &mut Fp2,
    material: &mut SovereignMaterialSettings,
    double_sided: &mut bool,
    alpha_mode: &mut AlphaModeKind,
    unlit: &mut bool,
    salt: &str,
    dirty: &mut bool,
) {
    draw_sign_source(ui, source, salt, dirty);
    ui.add_space(4.0);

    ui.horizontal(|ui| {
        ui.label("Panel size X/Z:");
        let mut v = size.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.1).range(0.01..=100.0))
                .changed();
        }
        if changed {
            *size = Fp2(v);
            *dirty = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("UV repeat U/V:");
        let mut v = uv_repeat.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(egui::DragValue::new(axis).speed(0.05).range(0.001..=1000.0))
                .changed();
        }
        if changed {
            *uv_repeat = Fp2(v);
            *dirty = true;
        }
    });

    ui.horizontal(|ui| {
        ui.label("UV offset U/V:");
        let mut v = uv_offset.0;
        let mut changed = false;
        for axis in v.iter_mut() {
            changed |= ui
                .add(
                    egui::DragValue::new(axis)
                        .speed(0.05)
                        .range(-1000.0..=1000.0),
                )
                .changed();
        }
        if changed {
            *uv_offset = Fp2(v);
            *dirty = true;
        }
    });

    ui.add_space(4.0);
    if ui.checkbox(double_sided, "Double-sided").changed() {
        *dirty = true;
    }
    if ui.checkbox(unlit, "Unlit").changed() {
        *dirty = true;
    }

    draw_alpha_mode(ui, alpha_mode, salt, dirty);

    ui.add_space(2.0);
    egui::CollapsingHeader::new("Material")
        .id_salt(format!("{}_sign_mat", salt))
        .default_open(false)
        .show(ui, |ui| {
            // Sign panels paint the loaded image into `base_color_texture`
            // and use the universal material's PBR knobs (tint /
            // emission / roughness / metallic) on top. The procedural
            // texture slot is intentionally hidden — the Sign's source
            // already supplies the texture.
            super::widgets::color_picker(ui, "Tint", &mut material.base_color, dirty);
            super::widgets::color_picker(ui, "Emission", &mut material.emission_color, dirty);
            fp_slider(
                ui,
                "Emission strength",
                &mut material.emission_strength,
                0.0,
                20.0,
                dirty,
            );
            fp_slider(ui, "Roughness", &mut material.roughness, 0.0, 1.0, dirty);
            fp_slider(ui, "Metallic", &mut material.metallic, 0.0, 1.0, dirty);
        });
}

/// Source-variant picker for a Sign generator. Combo box selects the
/// variant (URL / atproto_blob / did_pfp); the per-variant payload
/// fields render below. Switching variants reseeds the payload from the
/// previous variant where possible (e.g. URL → did_pfp keeps the URL
/// in the URL field if the user switches back) — implemented by
/// only overwriting when the variant truly changes.
fn draw_sign_source(ui: &mut egui::Ui, source: &mut SignSource, salt: &str, dirty: &mut bool) {
    let current = match source {
        SignSource::Url { .. } => "URL",
        SignSource::AtprotoBlob { .. } => "ATProto blob",
        SignSource::DidPfp { .. } => "DID profile picture",
        SignSource::Unknown => "Unknown",
    };

    ui.horizontal(|ui| {
        ui.label("Source:");
        egui::ComboBox::from_id_salt(format!("{}_sign_source", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "URL", "URL").clicked()
                    && !matches!(source, SignSource::Url { .. })
                {
                    *source = SignSource::Url { url: String::new() };
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "ATProto blob", "ATProto blob")
                    .clicked()
                    && !matches!(source, SignSource::AtprotoBlob { .. })
                {
                    *source = SignSource::AtprotoBlob {
                        did: String::new(),
                        cid: String::new(),
                    };
                    *dirty = true;
                }
                if ui
                    .selectable_label(current == "DID profile picture", "DID profile picture")
                    .clicked()
                    && !matches!(source, SignSource::DidPfp { .. })
                {
                    *source = SignSource::DidPfp { did: String::new() };
                    *dirty = true;
                }
            });
    });

    match source {
        SignSource::Url { url } => {
            ui.horizontal(|ui| {
                ui.label("URL:");
                if ui
                    .add(egui::TextEdit::singleline(url).hint_text("https://…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        SignSource::AtprotoBlob { did, cid } => {
            ui.horizontal(|ui| {
                ui.label("DID:");
                if ui
                    .add(egui::TextEdit::singleline(did).hint_text("did:plc:…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("CID:");
                if ui
                    .add(egui::TextEdit::singleline(cid).hint_text("bafy…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        SignSource::DidPfp { did } => {
            ui.horizontal(|ui| {
                ui.label("DID:");
                if ui
                    .add(egui::TextEdit::singleline(did).hint_text("did:plc:…"))
                    .changed()
                {
                    *dirty = true;
                }
            });
        }
        SignSource::Unknown => {
            ui.colored_label(
                egui::Color32::from_rgb(220, 160, 80),
                "Unknown source variant — pick one above to replace it.",
            );
        }
    }
}

/// Alpha-mode picker for a Sign generator. Combo selects the variant;
/// when `Mask` is selected, the cutoff slider renders below.
fn draw_alpha_mode(
    ui: &mut egui::Ui,
    alpha_mode: &mut AlphaModeKind,
    salt: &str,
    dirty: &mut bool,
) {
    let current = match alpha_mode {
        AlphaModeKind::Opaque => "Opaque",
        AlphaModeKind::Mask { .. } => "Mask",
        AlphaModeKind::Blend => "Blend",
        AlphaModeKind::Unknown => "Unknown",
    };

    ui.horizontal(|ui| {
        ui.label("Alpha mode:");
        egui::ComboBox::from_id_salt(format!("{}_alpha_mode", salt))
            .selected_text(current)
            .show_ui(ui, |ui| {
                if ui.selectable_label(current == "Opaque", "Opaque").clicked()
                    && !matches!(alpha_mode, AlphaModeKind::Opaque)
                {
                    *alpha_mode = AlphaModeKind::Opaque;
                    *dirty = true;
                }
                if ui.selectable_label(current == "Mask", "Mask").clicked()
                    && !matches!(alpha_mode, AlphaModeKind::Mask { .. })
                {
                    *alpha_mode = AlphaModeKind::Mask { cutoff: Fp(0.5) };
                    *dirty = true;
                }
                if ui.selectable_label(current == "Blend", "Blend").clicked()
                    && !matches!(alpha_mode, AlphaModeKind::Blend)
                {
                    *alpha_mode = AlphaModeKind::Blend;
                    *dirty = true;
                }
            });
    });

    if let AlphaModeKind::Mask { cutoff } = alpha_mode {
        fp_slider(ui, "Mask cutoff", cutoff, 0.0, 1.0, dirty);
    }
}

/// Editor for [`GeneratorKind::ParticleSystem`]. Groups the (large)
/// parameter set into collapsible sections so the panel stays
/// browseable without scrolling: Emitter shape, Spawn, Lifetime / Speed,
/// Dynamics, Visuals, Texture, Inheritance, Collisions. Every parameter is
/// surfaced; the sanitiser owns the bounds.
#[allow(clippy::too_many_arguments)]
fn draw_generator_particles(
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

/// Per-volume water editor: the single `level_offset` slider plus the full
/// [`WaterSurface`] knob set grouped into colour / wave / material sub-panels.
fn draw_water_editor(
    ui: &mut egui::Ui,
    level_offset: &mut Fp,
    surface: &mut WaterSurface,
    dirty: &mut bool,
) {
    fp_slider(ui, "Level offset", level_offset, -20.0, 20.0, dirty);
    ui.add_space(4.0);

    egui::CollapsingHeader::new("Colour")
        .default_open(true)
        .show(ui, |ui| {
            color_picker_rgba(ui, "Shallow (head-on)", &mut surface.shallow_color, dirty);
            color_picker_rgba(ui, "Deep (grazing)", &mut surface.deep_color, dirty);
            ui.label(
                egui::RichText::new(
                    "Alpha controls the opacity at each viewing extreme — shallow is typically \
                     low (transparent looking down), deep is high (opaque at grazing).",
                )
                .small()
                .color(egui::Color32::GRAY),
            );
        });

    egui::CollapsingHeader::new("Waves")
        .default_open(true)
        .show(ui, |ui| {
            fp_slider(
                ui,
                "Scale (amplitude)",
                &mut surface.wave_scale,
                0.0,
                4.0,
                dirty,
            );
            fp_slider(ui, "Speed", &mut surface.wave_speed, 0.0, 4.0, dirty);
            fp_slider(
                ui,
                "Choppiness",
                &mut surface.wave_choppiness,
                0.0,
                1.0,
                dirty,
            );
            ui.label("Wave direction (X / Z)");
            ui.horizontal(|ui| {
                let mut v = surface.wave_direction.0;
                let mut changed = false;
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[0])
                            .speed(0.05)
                            .range(-1.0..=1.0),
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut v[1])
                            .speed(0.05)
                            .range(-1.0..=1.0),
                    )
                    .changed();
                if changed {
                    surface.wave_direction = crate::pds::Fp2(v);
                    *dirty = true;
                }
            });
            fp_slider(ui, "Foam amount", &mut surface.foam_amount, 0.0, 1.0, dirty);
        });

    egui::CollapsingHeader::new("Material")
        .default_open(false)
        .show(ui, |ui| {
            fp_slider(ui, "Roughness", &mut surface.roughness, 0.0, 1.0, dirty);
            fp_slider(ui, "Metallic", &mut surface.metallic, 0.0, 1.0, dirty);
            fp_slider(
                ui,
                "Reflectance (F0)",
                &mut surface.reflectance,
                0.0,
                1.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Flow (physics)")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Force per metre submerged applied along the surface's downhill \
                 tangent. Zero on flat water; ~9.81 ≈ free-fall along the slope.",
            );
            fp_slider(
                ui,
                "Flow strength",
                &mut surface.flow_strength,
                0.0,
                30.0,
                dirty,
            );
        });

    egui::CollapsingHeader::new("Flow (visual)")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(
                "Blends the surface look from classic Gerstner standing waves \
                 (0) toward a river-style flow map (1). Independent of the \
                 physics push — set this without flow strength for a glassy \
                 \"infinity-pool\" effect, or both for a flowing river.",
            );
            fp_slider(ui, "Flow amount", &mut surface.flow_amount, 0.0, 1.0, dirty);
        });
}

// ---------------------------------------------------------------------------
// Tests — exercise `apply_reparent`, `sweep_root_refs`, and the cycle /
// invariant guards. These cover the bug-prone parts of Phase 2 + Phase 3
// so future refactors can't silently regress (a) dangling-Placement
// cleanup or (b) cycle protection.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pds::{Environment, Placement, ScatterBounds, TransformData};
    use std::collections::HashMap;

    fn empty_record() -> RoomRecord {
        RoomRecord {
            lex_type: "network.symbios.room".to_string(),
            environment: Environment::default(),
            generators: HashMap::new(),
            placements: Vec::new(),
            traits: HashMap::new(),
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
        );

        let r = record.generators.get("r").expect("root still there");
        // r now has B and C at the top level; A lives inside C.
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
                level_offset: crate::pds::Fp(0.0),
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
        );

        // Cube is still its own root; water still has zero children.
        assert!(record.generators.contains_key("cube"));
        let water = record.generators.get("water").expect("water still there");
        assert!(water.children.is_empty());
        assert!(!dirty);
    }
}
