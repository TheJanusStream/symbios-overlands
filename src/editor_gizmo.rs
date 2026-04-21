//! Bridge between the egui world editor's "selected placement" state and the
//! in-world 3D transform gizmo. The gizmo itself is driven by
//! `transform-gizmo-bevy` — this module only handles two things:
//!
//! * attach a `GizmoTarget` to whichever entity corresponds to the placement
//!   the owner clicked in the UI panel, removing it from any previously
//!   selected entity, and
//! * commit the dragged Transform back into the `RoomRecord` on mouse
//!   release so the `world_builder` recompile, the Publish-to-PDS button and
//!   the peer broadcast all see the final pose exactly once per drag.
//!
//! The commit runs in `PostUpdate` so the `Transform` we read is the final
//! value after the gizmo's `Last`-schedule update system has applied the
//! frame's drag delta.

use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::{Placement, RoomRecord, TransformData};
use crate::state::AppState;
use crate::ui::room::RoomEditorState;
use crate::world_builder::PlacementMarker;

pub struct EditorGizmoPlugin;

impl Plugin for EditorGizmoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_gizmo_selection.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            PostUpdate,
            commit_gizmo_drag.run_if(in_state(AppState::InGame)),
        );
    }
}

/// Keep the `GizmoTarget` component in sync with `RoomEditorState::
/// selected_placement`. Runs every frame the UI system flips the editor
/// resource's change tick (which is every frame the editor window is open,
/// since `ResMut` deref flips it unconditionally — the attach/remove guards
/// below keep that idempotent).
///
/// Uses `try_insert` / `try_remove` because a UI edit that changes a
/// construct prim's texture marks `RoomRecord` dirty and `compile_room_record`
/// (running in the same `Update` schedule) despawns every placement entity
/// before re-spawning fresh ones. Without the `try_` variants the query's
/// stale entity IDs would panic when their insert/remove commands applied
/// against already-despawned indices. Tolerating the race here is safe — the
/// next frame's sync pass sees the newly-spawned entity and re-attaches
/// `GizmoTarget` on it.
fn sync_gizmo_selection(
    mut commands: Commands,
    editor_state: Res<RoomEditorState>,
    query: Query<(Entity, &PlacementMarker, Has<GizmoTarget>)>,
) {
    if !editor_state.is_changed() {
        return;
    }

    for (entity, marker, has_gizmo) in query.iter() {
        let is_selected = editor_state.selected_placement == Some(marker.0);

        if is_selected && !has_gizmo {
            commands.entity(entity).try_insert(GizmoTarget::default());
        } else if !is_selected && has_gizmo {
            commands.entity(entity).try_remove::<GizmoTarget>();
        }
    }
}

/// Write the dragged `Transform` back into the `RoomRecord` the instant the
/// owner drops the left mouse button. Writing during the drag would make
/// `RoomRecord::is_changed()` fire on every frame, which in turn triggers
/// `compile_room_record` to despawn the dragged entity mid-drag and lose
/// the gizmo's target. Deferring the write to mouse-release collapses the
/// whole gesture into a single record update, a single peer broadcast and a
/// single recompile.
///
/// `GizmoTarget::is_active()` reflects the most recent drag state set by
/// `transform-gizmo-bevy`'s `update_gizmos` system in `Last`. Running our
/// commit in `PostUpdate` means we observe the *previous* frame's `is_active`
/// — still `true` on the release frame for the entity being dragged — which
/// is exactly the filter we need. Without this guard, a stray left-click on
/// empty space would fire `set_changed()` and uselessly rebuild the world.
fn commit_gizmo_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    query: Query<(&Transform, &PlacementMarker, &GizmoTarget)>,
    record: Option<ResMut<RoomRecord>>,
) {
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let Some(mut record) = record else {
        return;
    };

    let mut committed = false;
    for (transform, marker, target) in query.iter() {
        if !target.is_active() {
            continue;
        }
        if let Some(Placement::Absolute {
            transform: rec_tf, ..
        }) = record.placements.get_mut(marker.0)
        {
            *rec_tf = TransformData::from(*transform);
            committed = true;
        }
    }

    if committed {
        info!("Gizmo drag committed. Rebuilding world.");
        record.set_changed();
    }
}
