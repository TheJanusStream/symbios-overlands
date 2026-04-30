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
//! closures store a [`tree::PendingAction`] into a shared [`std::cell::RefCell`];
//! once the tree-view widget finishes rendering, the action is drained and
//! applied with `&mut record` access. Root deletes additionally sweep
//! dangling `Placement` references and `traits` mappings keyed on the
//! deleted generator name, so dropping a generator never leaves orphan
//! references that the world compiler would log as "unknown generator_ref".
//!
//! ## Sub-module map
//!
//! * [`tree`] — left-hand tree panel, drag-and-drop reparent,
//!   [`tree::PendingAction`] application.
//! * [`detail`] — right-hand detail panel + per-kind dispatcher.
//! * [`primitive`] — Cuboid / Sphere / Cylinder / Capsule / Cone / Torus
//!   / Plane / Tetrahedron detail editors + shared torture+material tail.
//! * [`sign`] — Sign-generator panel (source picker, UV, alpha mode).
//! * [`particles`] — ParticleSystem panel (emitter shape, dynamics, atlas).
//! * [`water`] — Water volume editor.

mod detail;
mod particles;
mod primitive;
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
        detail::draw_detail_panel(ui, source, selected_generator, selected_prim_path, dirty);
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

// ---------------------------------------------------------------------------
// Tests — exercise `apply_reparent`, `sweep_root_refs`, and the cycle /
// invariant guards. These cover the bug-prone parts of Phase 2 + Phase 3
// so future refactors can't silently regress (a) dangling-Placement
// cleanup or (b) cycle protection.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::tree::{apply_reparent, is_ancestor_of};
    use super::*;
    use crate::pds::{Environment, GeneratorKind, Placement, ScatterBounds, TransformData};
    use egui_ltreeview::DirPosition;
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
