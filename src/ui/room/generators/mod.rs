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
//! closures store a `reparent::PendingAction` into a shared [`std::cell::RefCell`];
//! once the tree-view widget finishes rendering, the action is drained and
//! applied with `&mut record` access. Root deletes additionally sweep
//! dangling `Placement` references and `traits` mappings keyed on the
//! deleted generator name, so dropping a generator never leaves orphan
//! references that the world compiler would log as "unknown generator_ref".
//!
//! ## Sub-module map
//!
//! * [`tree`] — left-hand tree panel widget (rows, context menus, drag
//!   handling).
//! * [`reparent`] — pure-model engine: `reparent::PendingAction`
//!   application, the drag-and-drop reparent state machine, node-walk
//!   helpers, and their unit tests.
//! * [`detail`] — right-hand detail panel + per-kind dispatcher.
//! * [`primitive`] — Cuboid / Sphere / Cylinder / Capsule / Cone / Torus
//!   / Plane / Tetrahedron detail editors + shared torture+material tail.
//! * [`sign`] — Sign-generator panel (source picker, UV, alpha mode).
//! * [`particles`] — ParticleSystem panel (emitter shape, dynamics, atlas).
//! * [`water`] — Water volume editor.

mod detail;
mod particles;
mod primitive;
mod reparent;
mod sign;
mod tree;
mod water;

use bevy_egui::egui;

use crate::pds::{Generator, Placement, RoomRecord};
use crate::state::LiveInventoryRecord;

// `GenNodeId` is defined on `super` (the room editor's mod.rs) and
// re-exported here so external callers (e.g. `ui::avatar`) can address
// it as `ui::room::generators::GenNodeId` without reaching back into
// the room module's top-level surface.
pub use super::GenNodeId;
use super::construct::{AVATAR_KINDS, ROOM_CHILD_KINDS, ROOM_ROOT_KINDS};
use super::widgets::unique_key;

/// Convenience alias so the per-tab function signature stays readable.
type TreeViewState = egui_ltreeview::TreeViewState<GenNodeId>;

// ---------------------------------------------------------------------------
// Generator-tree abstraction
// ---------------------------------------------------------------------------

/// Tree-source abstraction for the unified generator editor. Implemented
/// by [`RoomTreeSource`] (multi-root [`RoomRecord::generators`] HashMap
/// plus dangling-reference sweeps for placements/traits) and
/// [`AvatarVisualsTreeSource`] (single-root `AvatarRecord::visuals` with a
/// stricter allowed-kinds set).
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
    audio_editor: &mut super::audio::AudioEditorState,
    dirty: &mut bool,
    // In-scene blob element selection (#705), threaded to the BlobGroup
    // detail editor so its rows mirror the scene proxies' gizmo state.
    blob_selected_element: &mut Option<usize>,
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
            tree::draw_tree_panel(
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
        detail::draw_detail_panel(
            ui,
            source,
            selected_generator,
            selected_prim_path,
            audio_editor,
            dirty,
            blob_selected_element,
        );
    });
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
