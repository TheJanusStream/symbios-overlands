//! Applying an undo/redo step back onto the live records (#863).
//!
//! The restore write follows the Load-from-PDS shape exactly (the
//! contract the #817 survey identified as load-bearing): the whole
//! snapshot is written through the **bypassed** borrow and then a
//! single explicit `set_changed()` fires. Downstream, the world
//! compiler / terrain / roads / splat reactors all fingerprint-diff, so
//! one tick reconciles only what actually differs; the network side
//! sees one throttled `RoomStateUpdate` (or one `AvatarStateUpdate` —
//! that path has no second-stage throttle, which is precisely why a
//! restore must never tick per frame). [`UndoHistory::undo`] /
//! [`redo`](UndoHistory::redo) armed a one-shot suppression, so the
//! capture system swallows this tick instead of re-recording it.
//!
//! After the record write, each editor's `restore_from_undo` re-seeds
//! its selection + `egui_ltreeview` state from the entry's snapshot
//! (the `reparent.rs` fixup pattern), validating every index/path
//! against the restored record first — anything that no longer resolves
//! falls back to a clean deselect rather than pointing the gizmo at the
//! wrong node. It also cancels parked confirm-dialog payloads and zeros
//! the widget debounce so a pending burst can't double-fire a phantom
//! entry.

use bevy::ecs::change_detection::DetectChangesMut;

use crate::pds::Generator;
use crate::state::{LiveAvatarRecord, LiveRoomRecord};

use super::super::avatar::AvatarEditorState;
use super::super::room::{GenNodeId, RoomEditorState};
use super::{AvatarUndoHistory, RoomUndoHistory, UndoHistory};

/// Which way to move through the history.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepKind {
    Undo,
    Redo,
}

/// Walk `path` through a generator's child chain. `true` when every
/// index resolves — the same walk `commit_transform_at_path` does at
/// gizmo-commit time, used here to validate a restored selection.
pub(crate) fn node_path_valid(root: &Generator, path: &[usize]) -> bool {
    let mut node = root;
    for &idx in path {
        match node.children.get(idx) {
            Some(child) => node = child,
            None => return false,
        }
    }
    true
}

/// Whether a tree-view id still resolves in `generators` after a
/// wholesale record replace. The virtual-root sentinel is never a real
/// selection.
pub(crate) fn room_node_id_valid(
    generators: &std::collections::HashMap<String, Generator>,
    id: &GenNodeId,
) -> bool {
    !id.is_virtual_root()
        && generators
            .get(&id.root)
            .is_some_and(|g| node_path_valid(g, &id.path))
}

/// Apply one undo/redo step to the room record. Returns the label of
/// the stepped edit for the "Undid: …" / "Redid: …" toast (#864), or
/// `None` when the history has nothing in that direction (the caller
/// decides how to surface a no-op).
pub fn step_room(
    kind: StepKind,
    history: &mut RoomUndoHistory,
    record: &mut impl DetectChangesMut<Inner = LiveRoomRecord>,
    editor: &mut RoomEditorState,
) -> Option<String> {
    let (snapshot, selection, label) = take_step(kind, history)?;
    // The load-bearing write shape — see module docs.
    record.bypass_change_detection().0 = snapshot;
    record.set_changed();
    editor.restore_from_undo(&record.bypass_change_detection().0, &selection);
    Some(label)
}

/// Avatar counterpart of [`step_room`]. Same contract; the avatar's
/// visual rebuild (`rebuild_local_visuals`) is a full respawn per tick,
/// which one tick per keypress keeps acceptable.
pub fn step_avatar(
    kind: StepKind,
    history: &mut AvatarUndoHistory,
    record: &mut impl DetectChangesMut<Inner = LiveAvatarRecord>,
    editor: &mut AvatarEditorState,
) -> Option<String> {
    let (snapshot, selection, label) = take_step(kind, history)?;
    record.bypass_change_detection().0 = snapshot;
    record.set_changed();
    editor.restore_from_undo(&record.bypass_change_detection().0, &selection);
    Some(label)
}

/// Move the cursor and clone the target entry out, releasing the
/// history borrow before the caller starts writing resources.
fn take_step<R, S>(kind: StepKind, history: &mut UndoHistory<R, S>) -> Option<(R, S, String)>
where
    R: Clone + Send + Sync + 'static,
    S: Clone + Send + Sync + 'static,
{
    let (entry, label) = match kind {
        StepKind::Undo => history.undo()?,
        StepKind::Redo => history.redo()?,
    };
    Some((
        entry.record.clone(),
        entry.selection.clone(),
        label.to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use bevy::prelude::*;

    use super::*;
    use crate::pds::{ContactEffects, Environment, Fp, Placement, RoomRecord, TransformData};
    use crate::ui::undo::{Observation, RoomSelection};

    /// Minimal hand-built room record: one generator `"tg"` with
    /// `children` leaf cuboids, and `placements` absolute placements
    /// referencing it. Cheap and deterministic, unlike the seeded
    /// procedural defaults.
    fn tiny_record(children: usize, placements: usize) -> RoomRecord {
        let mut root = Generator::default_cuboid();
        root.children = (0..children).map(|_| Generator::default_cuboid()).collect();
        let mut generators = HashMap::new();
        generators.insert("tg".to_string(), root);
        RoomRecord {
            lex_type: String::new(),
            environment: Environment::default(),
            generators,
            placements: (0..placements)
                .map(|_| Placement::Absolute {
                    generator_ref: "tg".to_string(),
                    transform: TransformData::default(),
                    snap_to_terrain: true,
                    avoid_water: false,
                    avoid_water_clearance: Fp(0.0),
                })
                .collect(),
            traits: HashMap::new(),
            contact_effects: ContactEffects::default(),
            default_landing: None,
        }
    }

    fn selection(children_path: Vec<usize>, placement: Option<usize>) -> RoomSelection {
        RoomSelection {
            generator: Some("tg".to_string()),
            placement,
            prim_path: Some(children_path.clone()),
            tree: vec![GenNodeId::child("tg", children_path)],
        }
    }

    #[test]
    fn path_walk_validates_against_the_tree() {
        let record = tiny_record(2, 0);
        let root_gen = &record.generators["tg"];
        assert!(node_path_valid(root_gen, &[]));
        assert!(node_path_valid(root_gen, &[1]));
        assert!(!node_path_valid(root_gen, &[2]));
        assert!(!node_path_valid(root_gen, &[0, 0]));
        assert!(room_node_id_valid(
            &record.generators,
            &GenNodeId::child("tg", vec![0])
        ));
        assert!(!room_node_id_valid(
            &record.generators,
            &GenNodeId::child("gone", vec![0])
        ));
        assert!(!room_node_id_valid(
            &record.generators,
            &GenNodeId::default()
        ));
    }

    /// Change-tick probe: counts how many frames saw the room record
    /// marked changed.
    #[derive(Resource, Default)]
    struct Ticks(usize);

    fn probe(record: Res<LiveRoomRecord>, mut ticks: ResMut<Ticks>) {
        if record.is_changed() {
            ticks.0 += 1;
        }
    }

    fn app_with(record: RoomRecord) -> App {
        let mut app = App::new();
        app.init_resource::<Ticks>();
        app.init_resource::<RoomEditorState>();
        app.insert_resource(LiveRoomRecord(record));
        app.add_systems(Update, probe);
        // Drain the insertion tick so the counter only sees the restore.
        app.update();
        app.world_mut().resource_mut::<Ticks>().0 = 0;
        app
    }

    fn seeded_history(
        base: &RoomRecord,
        edited: &RoomRecord,
        sel: RoomSelection,
    ) -> RoomUndoHistory {
        let mut history = RoomUndoHistory::default();
        history.reset(
            Some("did:test:room"),
            base.clone(),
            RoomSelection::default(),
        );
        history.observe(
            Some("did:test:room"),
            Observation::Edit("add child".into()),
            || edited.clone(),
            || sel,
        );
        history
    }

    #[test]
    fn restore_fires_exactly_one_tick_and_replaces_the_record() {
        let base = tiny_record(1, 1);
        let edited = tiny_record(3, 2);
        let mut app = app_with(edited.clone());
        let mut history = seeded_history(&base, &edited, selection(vec![2], Some(1)));

        {
            let world = app.world_mut();
            world.resource_scope::<RoomEditorState, _>(|world, mut editor| {
                let mut record = world.resource_mut::<LiveRoomRecord>();
                let label = step_room(StepKind::Undo, &mut history, &mut record, &mut editor);
                assert_eq!(label.as_deref(), Some("add child"));
            });
        }
        app.update();
        app.update();
        assert_eq!(
            app.world().resource::<Ticks>().0,
            1,
            "a restore must produce exactly one change tick"
        );
        assert_eq!(
            app.world().resource::<LiveRoomRecord>().0.placements.len(),
            1,
            "record contents rolled back to the snapshot"
        );
        assert!(
            step_room(
                StepKind::Undo,
                &mut history,
                &mut app.world_mut().resource_mut::<LiveRoomRecord>(),
                &mut RoomEditorState::default(),
            )
            .is_none(),
            "baseline reached — no further undo"
        );
    }

    #[test]
    fn redo_reapplies_the_edit() {
        let base = tiny_record(1, 1);
        let edited = tiny_record(3, 2);
        let mut app = app_with(edited.clone());
        let mut history = seeded_history(&base, &edited, selection(vec![2], Some(1)));

        let world = app.world_mut();
        world.resource_scope::<RoomEditorState, _>(|world, mut editor| {
            let mut record = world.resource_mut::<LiveRoomRecord>();
            step_room(StepKind::Undo, &mut history, &mut record, &mut editor);
            let label = step_room(StepKind::Redo, &mut history, &mut record, &mut editor);
            assert_eq!(label.as_deref(), Some("add child"));
            assert_eq!(record.0.placements.len(), 2);
            // The redo target's selection resolves in the redone record,
            // so it re-seeds rather than deselecting.
            assert_eq!(editor.selected_generator.as_deref(), Some("tg"));
            assert_eq!(editor.selected_prim_path.as_deref(), Some(&[2][..]));
            assert_eq!(editor.selected_placement, Some(1));
        });
    }

    #[test]
    fn valid_selection_is_reseeded_including_tree_state() {
        let base = tiny_record(2, 1);
        let edited = tiny_record(3, 1);
        let mut app = app_with(edited.clone());
        // Selection stored with the BASELINE points at child 1 — valid in
        // the baseline we restore to.
        let mut history = RoomUndoHistory::default();
        history.reset(
            Some("did:test:room"),
            base.clone(),
            selection(vec![1], Some(0)),
        );
        history.observe(
            Some("did:test:room"),
            Observation::Edit("edit".into()),
            || edited.clone(),
            RoomSelection::default,
        );

        let world = app.world_mut();
        world.resource_scope::<RoomEditorState, _>(|world, mut editor| {
            let mut record = world.resource_mut::<LiveRoomRecord>();
            step_room(StepKind::Undo, &mut history, &mut record, &mut editor);
            assert_eq!(editor.selected_generator.as_deref(), Some("tg"));
            assert_eq!(editor.selected_prim_path.as_deref(), Some(&[1][..]));
            assert_eq!(editor.selected_placement, Some(0));
            let snap = editor.undo_selection();
            assert_eq!(snap.tree, vec![GenNodeId::child("tg", vec![1])]);
        });
    }

    #[test]
    fn dangling_selection_falls_back_to_deselect() {
        // The edited state's selection points at child 4 / placement 3,
        // which don't exist in the baseline — restoring must deselect,
        // not point the gizmo at a wrong or missing node.
        let base = tiny_record(1, 1);
        let edited = tiny_record(5, 4);
        let mut app = app_with(edited.clone());
        let mut history = RoomUndoHistory::default();
        history.reset(
            Some("did:test:room"),
            base.clone(),
            selection(vec![4], Some(3)),
        );
        history.observe(
            Some("did:test:room"),
            Observation::Edit("edit".into()),
            || edited.clone(),
            || selection(vec![4], Some(3)),
        );

        let world = app.world_mut();
        world.resource_scope::<RoomEditorState, _>(|world, mut editor| {
            let mut record = world.resource_mut::<LiveRoomRecord>();
            // Poison the baseline's stored selection too: entry selections
            // are validated against the RESTORED record, whatever they say.
            step_room(StepKind::Undo, &mut history, &mut record, &mut editor);
            assert_eq!(editor.selected_generator, None);
            assert_eq!(editor.selected_prim_path, None);
            assert_eq!(editor.selected_placement, None);
            assert!(editor.undo_selection().tree.is_empty());
        });
    }
}
