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
//! * [`primitive`] — detail editors covering all sixteen parametric
//!   primitives, Cuboid through Tube / Bevel / Helix / Superellipsoid /
//!   Spine / Lathe / BlobGroup (Wedge reuses the cuboid editor), + shared
//!   torture+material tail.
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
    /// How many implementation-side references (Placements) a root delete
    /// would cascade through — the number the delete confirm shows
    /// (#838). Sources without side-tables (avatar) report zero.
    fn placement_ref_count(&self, _root: &str) -> usize {
        0
    }
    /// How many more roots [`Self::add_root`] will accept (#1210). The
    /// room answers from [`Cap::Generators`]; a single-root source has no
    /// cap beyond its one root, which `add_root` already refuses.
    ///
    /// [`Cap::Generators`]: crate::ui::room::caps::Cap::Generators
    fn root_capacity_remaining(&self) -> usize {
        usize::MAX
    }
    /// Nodes in `root`'s tree, root included — what
    /// [`Cap::NodesPerGenerator`] bounds (#1210). Zero for an unknown root.
    ///
    /// [`Cap::NodesPerGenerator`]: crate::ui::room::caps::Cap::NodesPerGenerator
    fn node_count(&self, root: &str) -> usize {
        self.get_root(root)
            .map_or(0, crate::ui::room::caps::node_count)
    }
    /// Does this source's record instance its roots through PLACEMENTS
    /// (#1239 f81)?
    ///
    /// The world compiler builds exclusively from `record.placements`, so
    /// a room root with none spawns no entity, no gizmo and no highlight —
    /// "+ New" is the World Editor's primary create button and it appears
    /// to do nothing at all. An avatar's visuals tree has no placement
    /// layer; its roots ARE instanced.
    fn instances_through_placements(&self) -> bool {
        false
    }
    /// Can a viewport click resolve a face back onto THIS tree (#1237
    /// f140)?
    ///
    /// `pick_on_scene_click` records a face on the room-prim branches and
    /// the avatar-visuals branch only, and
    /// [`FacePick::take_for`](crate::editor_gizmo::FacePick::take_for)
    /// matches on the root those calls produce. A worn item's Parts editor
    /// renders the same shared detail panel, so it drew a "Pick from
    /// scene" toggle that could never resolve — and whose stuck arm
    /// suppressed click-to-deselect everywhere in the app. A source that
    /// no branch of the pick can address must say so here.
    fn resolves_face_picks(&self) -> bool {
        true
    }
}

/// Pending destructive tree operations awaiting confirmation (#838):
/// a root delete (cascades through referencing placements), a kind
/// change that discards tuned params or strands children, and a drag
/// that nests a placed root (#1209, the same cascade as the delete).
/// Embedded in each editor's state and threaded through
/// [`draw_generators_tab`].
#[derive(Default)]
pub(crate) struct TreeConfirms {
    pub(crate) delete: crate::ui::confirm::ConfirmState<GenNodeId>,
    pub(crate) kind: crate::ui::confirm::ConfirmState<(GenNodeId, &'static str)>,
    pub(crate) reparent: crate::ui::confirm::ConfirmState<reparent::PendingReparent>,
}

impl TreeConfirms {
    /// Drop every parked payload — an undo restore or a record swap makes
    /// the node ids they were resolved against stale.
    pub(crate) fn cancel_all(&mut self) {
        self.delete.cancel();
        self.kind.cancel();
        self.reparent.cancel();
    }
}

pub(crate) use reparent::request_root_delete;

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
    fn instances_through_placements(&self) -> bool {
        true
    }
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
        // The cap is enforced HERE, at the one insert every add path
        // funnels through (#1210): "+ New", "+ From Inventory", "+ From
        // Catalogue", the scene menu's Create, a drop, a drag-promotion.
        // Inserting a 257th used to let the next sanitize flush delete
        // whichever generator sorted last alphabetically.
        if self.root_capacity_remaining() == 0 {
            return None;
        }
        let name = unique_key(&self.record.generators, prefix);
        self.record.generators.insert(name.clone(), generator);
        Some(name)
    }
    fn root_capacity_remaining(&self) -> usize {
        crate::ui::room::caps::Cap::Generators
            .max()
            .saturating_sub(self.record.generators.len())
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
    fn placement_ref_count(&self, root: &str) -> usize {
        // Mirrors [`sweep_root_refs`]'s match exactly — this count is the
        // "also removes N placements" the delete confirm promises.
        self.record
            .placements
            .iter()
            .filter(|p| match p {
                Placement::Absolute { generator_ref, .. }
                | Placement::Scatter { generator_ref, .. }
                | Placement::Grid { generator_ref, .. } => generator_ref == root,
                Placement::Unknown => false,
            })
            .count()
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

/// `GeneratorTreeSource` adapter for one worn item's tree (#1098): the
/// attachment record's `item` generator under its record key as the root
/// name, so the tree-view keys `(rkey, path)` match the
/// [`AttachmentPrim`](crate::world_builder::AttachmentPrim) markers the
/// spawned prop carries. Single-root, primitives-only — the same
/// vocabulary as the avatar visuals tree, because a worn item is
/// sanitised with the same avatar rules.
pub(crate) struct AttachmentTreeSource<'a> {
    pub(crate) rkey: String,
    pub(crate) item: &'a mut Generator,
}

impl<'a> AttachmentTreeSource<'a> {
    pub(crate) fn new(rkey: &str, item: &'a mut Generator) -> Self {
        Self {
            rkey: rkey.to_string(),
            item,
        }
    }
}

impl GeneratorTreeSource for AttachmentTreeSource<'_> {
    /// No branch of `pick_on_scene_click` records a face for a worn part
    /// (#1237 f140), so the Faces panel's "Pick from scene" toggle is
    /// hidden here rather than offered and unable to work. The face
    /// dropdown next to it is unaffected — overrides on worn parts are
    /// fine, it is only the viewport route that has no return path.
    fn resolves_face_picks(&self) -> bool {
        false
    }
    fn root_names(&self) -> Vec<String> {
        vec![self.rkey.clone()]
    }
    fn get_root(&self, name: &str) -> Option<&Generator> {
        (name == self.rkey).then_some(&*self.item)
    }
    fn get_root_mut(&mut self, name: &str) -> Option<&mut Generator> {
        (name == self.rkey).then_some(&mut *self.item)
    }
    fn allow_multiple_roots(&self) -> bool {
        false
    }
    fn add_root(&mut self, _prefix: &str, _generator: Generator) -> Option<String> {
        None
    }
    fn remove_root(&mut self, _name: &str) -> Option<Generator> {
        // A worn item's root IS the item; taking it off is the Attachments
        // tab's job, not a tree delete.
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
    // One-shot request to focus the tree after an in-world pick (#719), so
    // the picked row highlights like a direct click. Always `false` for the
    // avatar editor, which has no in-world node picking.
    request_focus: bool,
    renaming_generator: &mut Option<(String, String)>,
    inventory: Option<&mut LiveInventoryRecord>,
    audio_editor: &mut super::audio::AudioEditorState,
    // Grammar compile outcomes (#829), rendered by the L-system / Shape
    // forges in the detail panel.
    grammar_diag: &crate::world_builder::grammar_diag::GrammarDiagnostics,
    dirty: &mut bool,
    // In-scene blob element selection (#705), threaded to the BlobGroup
    // detail editor so its rows mirror the scene proxies' gizmo state.
    blob_selected_element: &mut Option<usize>,
    // Pending destructive-tree confirmations (#838): root delete + kind
    // change. Requested inside the tree / detail panels, answered here.
    confirms: &mut TreeConfirms,
    // Toast channel + session clock for structural-op feedback (#841's
    // Save-to-Inventory success/full toasts).
    toasts: &mut crate::ui::toast::Toasts,
    now: f64,
    // Undo-entry label channel (#865), pre-bound to the hosting editor's
    // slot so this shared widget stays editor-agnostic.
    label: &mut crate::ui::undo::LabelSlot,
    // Live road-network stats for the RoadNetwork detail readout (#888).
    // `None` for tree sources that can't grow roads (the avatar editor).
    road_stats: Option<&crate::terrain::RoadPanelStats>,
    // Click-to-pick face selection (#961), shared with the scene click
    // handler that arms it — the Faces panel's other way in.
    face_pick: &mut crate::editor_gizmo::FacePick,
    // The signed-in owner's DID (#1239 f78), for the tree's catalogue
    // menus. See `draw_tree_panel`.
    owner_did: &str,
    // Out-channel (#1239 f81): the root the user asked to be PLACED in the
    // world. Performed by the caller, which is the only level holding the
    // record's placements, the player's pose and the tab state. `None` at
    // sources with no placement layer.
    place_root: &mut Option<String>,
    // Substring filter over root names (#1244 f414).
    filter: &mut String,
    // The editor's one-node clipboard (#1244 f422).
    clipboard: &mut Option<Generator>,
    // The asset caches + retry channel (#1246): every field naming a
    // fetched image or sound says what happened to it.
    assets: &mut super::assets::AssetPanel<'_>,
) {
    // Inventory now flows only into the tree panel (for the root-level
    // "+ From Inventory" toolbar, the per-row "+ From Inventory" submenu,
    // and the apply step's "Save to Inventory" write). The detail panel
    // never touches inventory anymore — its inventory-child picker moved
    // into the row context menu in issue #159.
    egui::Panel::left("generators_tree_panel")
        .resizable(true)
        .default_size(260.0)
        .min_size(180.0)
        .show(ui, |ui| {
            tree::draw_tree_panel(
                ui,
                source,
                selected_generator,
                selected_prim_path,
                tree_view_state,
                renaming_generator,
                inventory,
                request_focus,
                dirty,
                confirms,
                toasts,
                now,
                label,
                owner_did,
                filter,
                clipboard,
            );
        });

    egui::CentralPanel::default().show(ui, |ui| {
        // "This exists but is not in the world" (#1239 f81). The concept
        // split — generator = blueprint, placement = instance — is real
        // and worth keeping, but it was taught only by failure: "+ New"
        // put a row in the tree, selected it, and changed nothing in the
        // 3D view, while "+ From Catalogue" and drag-to-place both DO
        // produce visible objects. That made the primary create button
        // look broken rather than different.
        if source.instances_through_placements()
            && let Some(root) = selected_generator.as_deref()
            && selected_prim_path
                .as_deref()
                .is_some_and(<[usize]>::is_empty)
            && source.placement_ref_count(root) == 0
        {
            let theme = crate::ui::theme::current(ui.ctx());
            ui.horizontal_wrapped(|ui| {
                ui.colored_label(theme.status.warn, "Not in the world yet.");
                if ui
                    .button("Place it at my position")
                    .on_hover_text(
                        "Adds a placement pointing at this item. An item is \
                         the thing you build; a placement is where a copy of \
                         it stands.",
                    )
                    .clicked()
                {
                    *place_root = Some(root.to_string());
                }
            });
            ui.separator();
        }
        detail::draw_detail_panel(
            ui,
            source,
            selected_generator,
            selected_prim_path,
            audio_editor,
            grammar_diag,
            dirty,
            blob_selected_element,
            &mut confirms.kind,
            label,
            road_stats,
            face_pick,
            assets,
        );
    });

    // Answer the confirms parked by the panels above. Rendering them
    // here — with the tree source still in scope — means the payloads
    // can re-resolve their nodes at apply time, so a confirm is safe
    // even if the selection moved while the dialog was up.
    if let Some(id) = confirms.delete.show(ui.ctx(), "tree-delete") {
        // Blast radius measured BEFORE the sweep, so the undo toast can
        // say what the cascade actually took with it.
        let placements = source.placement_ref_count(&id.root);
        label.set(if placements > 0 {
            format!(
                "delete of {} + {placements} placement{}",
                id.root,
                if placements == 1 { "" } else { "s" }
            )
        } else {
            format!("delete of {}", id.root)
        });
        source.remove_root(&id.root);
        *selected_generator = None;
        *selected_prim_path = None;
        tree_view_state.set_selected(Vec::new());
        *dirty = true;
    }
    if let Some((id, kind_tag)) = confirms.kind.show(ui.ctx(), "tree-kind-change")
        && let Some(node) = reparent::find_node_mut(source, &id)
    {
        label.set(format!("kind change to {kind_tag}"));
        super::construct::apply_kind_change(node, kind_tag);
        *dirty = true;
    }
    if let Some(reparent::PendingReparent {
        source: drag_source,
        target,
        position,
    }) = confirms.reparent.show(ui.ctx(), "tree-nest")
    {
        reparent::apply_reparent(
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

#[cfg(test)]
mod attachment_source_tests {
    use super::*;

    /// The worn-item source is single-root under the record key (#1098):
    /// that key is what the spawned `AttachmentPrim` markers carry, so the
    /// tree's `(root, path)` ids and the scene's markers agree by name.
    #[test]
    fn the_attachment_source_is_single_root_under_its_record_key() {
        let mut item = Generator::default_cuboid();
        item.children.push(Generator::default_cuboid());
        let mut source = AttachmentTreeSource::new("3jzfcijpj2z2a", &mut item);
        assert_eq!(source.root_names(), vec!["3jzfcijpj2z2a".to_string()]);
        assert!(source.get_root("3jzfcijpj2z2a").is_some());
        assert!(source.get_root("other").is_none());
        assert!(!source.allow_multiple_roots());
        assert!(
            source.add_root("x", Generator::default_cuboid()).is_none(),
            "a worn item has exactly one root"
        );
        assert!(
            source.remove_root("3jzfcijpj2z2a").is_none(),
            "taking an item off is not a tree delete"
        );
        source
            .get_root_mut("3jzfcijpj2z2a")
            .expect("root")
            .children
            .push(Generator::default_cuboid());
        assert_eq!(item.children.len(), 2, "edits land on the worn copy");
    }

    /// #1237 f140. Sequence: open a worn hat's Parts editor, click "Pick
    /// from scene", click the brim — nothing happens, ever, and the button
    /// stays lit. `pick_on_scene_click` records a face on the room-prim
    /// and avatar-visuals branches only, and `take_for` matches on a root
    /// the worn-part branch never produces. The control was visible,
    /// enabled, unable to work, and its stuck arm suppressed
    /// click-to-deselect everywhere else in the app.
    #[test]
    fn only_trees_a_scene_click_can_address_offer_the_face_picker() {
        let mut record = crate::pds::RoomRecord::default();
        assert!(RoomTreeSource::new(&mut record).resolves_face_picks());

        let mut visuals = Generator::default();
        assert!(
            AvatarVisualsTreeSource::new(&mut visuals).resolves_face_picks(),
            "the avatar visuals branch DOES record faces"
        );

        let mut item = Generator::default();
        assert!(
            !AttachmentTreeSource::new("3jzfcijpj2z2a", &mut item).resolves_face_picks(),
            "a worn part has no return path, so the toggle must not be drawn"
        );
    }

    /// #1239 f78. Sequence: add a theme Monument from the TREE's "+ From
    /// Catalogue" and its portrait panel is permanently blank; add the
    /// same item by dragging it out of the Catalogue window and your own
    /// profile picture appears. Both tree entry points passed the empty
    /// string as the local DID, which every monument threads into a
    /// `SignSource::DidPfp` at stamp time; the fetch for an empty DID
    /// fails silently, so the author saw an unexplained grey rectangle
    /// with no path to a fix short of the Raw JSON tab.
    #[test]
    fn a_catalogue_stamp_from_the_tree_carries_the_owner() {
        let owner = "did:plc:owner";
        let entry = crate::catalogue::ENTRIES
            .iter()
            .find(|e| e.slug() == "ancient_monument")
            .expect("the ancient monument is catalogued");
        let stamped = entry.build(owner);
        let mut dids = Vec::new();
        collect_pfp_dids(&stamped, &mut dids);
        assert!(
            !dids.is_empty(),
            "precondition: this entry personalises on the owner's DID"
        );
        for did in &dids {
            assert_eq!(did, owner, "the tree menu must stamp the real owner");
        }

        // …and the empty DID the tree used to pass is exactly what
        // produces the blank panel.
        let mut blank = Vec::new();
        collect_pfp_dids(&entry.build(""), &mut blank);
        assert!(blank.iter().all(String::is_empty));
    }

    /// Walk a stamped generator for every `SignSource::DidPfp` DID.
    #[cfg(test)]
    fn collect_pfp_dids(node: &Generator, out: &mut Vec<String>) {
        if let crate::pds::GeneratorKind::Sign { source, .. } = &node.kind
            && let crate::pds::generator::SignSource::DidPfp { did } = source
        {
            out.push(did.clone());
        }
        for child in &node.children {
            collect_pfp_dids(child, out);
        }
    }

    /// #1239 f81. Sequence: click "+ New → Cuboid" in the World Editor.
    /// The row appears and is selected, and absolutely nothing changes in
    /// the 3D view — the compiler builds exclusively from
    /// `record.placements`, so a generator with no placement spawns no
    /// entity, and the gizmo has nothing to attach to either. Nothing in
    /// the tree, the detail panel or the empty state mentioned it.
    #[test]
    fn a_room_root_with_no_placement_is_known_to_be_unplaced() {
        let mut record = crate::pds::RoomRecord::default();
        record.placements.clear();
        record.generators.clear();
        {
            let mut source = RoomTreeSource::new(&mut record);
            assert!(source.instances_through_placements());
            let name = source
                .add_root("cuboid", Generator::default())
                .expect("a fresh room takes a root");
            assert_eq!(
                source.placement_ref_count(&name),
                0,
                "'+ New' adds a blueprint, not an instance — this is the banner's trigger"
            );
        }
        // An avatar's visuals tree has no placement layer at all, so the
        // banner must never appear there.
        let mut visuals = Generator::default();
        assert!(!AvatarVisualsTreeSource::new(&mut visuals).instances_through_placements());
        let mut item = Generator::default();
        assert!(
            !AttachmentTreeSource::new("3jzfcijpj2z2a", &mut item).instances_through_placements()
        );
    }
}
