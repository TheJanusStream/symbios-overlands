//! Drag-session state machine: rising-edge (mouse-down on the gizmo,
//! Shift selects copy-on-drag), every-frame Escape watch + ghost
//! drawing, falling-edge commit when `GizmoTarget::is_active()` flips
//! back to `false`. Writebacks are routed to [`commit::commit_room_drag`]
//! / [`commit::commit_avatar_drag`].

use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::RoomRecord;
use crate::state::LiveAvatarRecord;
use crate::ui::room::RoomEditorState;
use crate::world_builder::{AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::commit::{commit_avatar_drag, commit_room_drag};
use super::{ActiveTarget, DragState, GizmoDetachedPrim};

/// Drive the full drag session: detect the rising edge (Shift at drag
/// start chooses copy-on-drag), watch for `Escape` aborts and render the
/// origin-ghost + "+" indicator every frame, then commit (or discard) on
/// the falling edge when the gizmo goes idle.
///
/// Writing during the drag would make the live record's `is_changed()`
/// fire on every frame, which in turn would trigger downstream rebuilds
/// to despawn the dragged entity mid-drag and lose the gizmo's target.
/// Deferring the write to drag-end collapses the whole gesture into a
/// single record update, a single peer broadcast and a single recompile.
///
/// Because prims are detached from their parent while the gizmo is
/// attached, a prim's `Transform` is in world space at commit time. We
/// convert back to local space using the cached parent's
/// `GlobalTransform` before writing into the recipe.
///
/// `GizmoTarget::is_active()` reflects the most recent drag state set by
/// `transform-gizmo-bevy`'s `update_gizmos` system in `Last`. Running in
/// `PostUpdate` means we observe the *previous* frame's `is_active`,
/// which is still `true` on the release frame and flips to `false` the
/// frame after — giving us a clean one-frame-delayed falling edge to
/// commit on.
///
/// Copy-on-drag (room editor only): Shift-held at drag-start clones the
/// placement / room prim at commit time and drops the new copy at the
/// dragged position, leaving the original in place. Blueprint roots
/// force copy off — cloning an entire construct tree sideways is
/// expressed at the placement layer instead. Avatar prims do not support
/// copy-on-drag in v1: there's only one local avatar tree, and the
/// inventory + room placements vocabulary doesn't apply.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn manage_gizmo_drag(
    mut state: Local<DragState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut gizmos: Gizmos,
    mut room_editor: ResMut<RoomEditorState>,
    mut placement_query: Query<
        (Entity, &mut Transform, &PlacementMarker, &GizmoTarget),
        (Without<PrimMarker>, Without<AvatarVisualPrim>),
    >,
    mut prim_query: Query<
        (
            Entity,
            &mut Transform,
            &PrimMarker,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        Without<AvatarVisualPrim>,
    >,
    mut avatar_prim_query: Query<
        (
            Entity,
            &mut Transform,
            &AvatarVisualPrim,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        Without<PrimMarker>,
    >,
    global_tf: Query<&GlobalTransform>,
    room_record: Option<ResMut<RoomRecord>>,
    avatar_record: Option<ResMut<LiveAvatarRecord>>,
) {
    // Find the entity (if any) whose gizmo reports active this frame, and
    // record which target type it belongs to so the falling edge can
    // route the writeback to the right record.
    let mut active_target: Option<(Entity, ActiveTarget)> = None;
    for (entity, _tf, _m, target) in placement_query.iter() {
        if target.is_active() {
            active_target = Some((entity, ActiveTarget::Room));
            break;
        }
    }
    if active_target.is_none() {
        for (entity, _tf, _m, target, _d) in prim_query.iter() {
            if target.is_active() {
                active_target = Some((entity, ActiveTarget::Room));
                break;
            }
        }
    }
    if active_target.is_none() {
        for (entity, _tf, _m, target, _d) in avatar_prim_query.iter() {
            if target.is_active() {
                active_target = Some((entity, ActiveTarget::Avatar));
                break;
            }
        }
    }

    // Rising edge — a new drag just started.
    if state.active_entity.is_none() {
        let Some((entity, target_kind)) = active_target else {
            return;
        };
        let (original_world_tf, is_prim_root) =
            if let Ok((_e, tf, _m, _t)) = placement_query.get(entity) {
                (*tf, false)
            } else if let Ok((_e, tf, marker, _t, _d)) = prim_query.get(entity) {
                (*tf, marker.path.is_empty())
            } else if let Ok((_e, tf, marker, _t, _d)) = avatar_prim_query.get(entity) {
                (*tf, marker.path.is_empty())
            } else {
                return;
            };
        let mut is_copy =
            keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        // A blueprint root has no parent to receive a sibling clone — a
        // "copy of the root" only makes sense at the Placement level.
        // Avatar prims also disable copy: there's no avatar-side
        // equivalent of placements, and the visuals tree is single-
        // rooted per local player.
        if is_prim_root || target_kind == ActiveTarget::Avatar {
            is_copy = false;
        }
        state.active_entity = Some(entity);
        state.original_world_tf = original_world_tf;
        state.is_copy = is_copy;
        state.aborted = false;
        state.target = target_kind;
    }

    let active_entity = state.active_entity.unwrap();
    let is_still_active = active_target.map(|(e, _)| e) == Some(active_entity);

    // Active drag — every frame until the mouse is released.
    if is_still_active {
        if keyboard.just_pressed(KeyCode::Escape) {
            state.aborted = true;
        }

        if state.aborted {
            // Visually snap back to the starting pose. The gizmo's Last-
            // schedule update will keep trying to write the dragged
            // pose, but overwriting here each frame keeps the user's
            // feedback pinned to "nothing happened" until they release.
            if let Ok((_e, mut tf, _m, _t)) = placement_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            } else if let Ok((_e, mut tf, _m, _t, _d)) = prim_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            } else if let Ok((_e, mut tf, _m, _t, _d)) = avatar_prim_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            }
            return;
        }

        if state.is_copy {
            // Ghost at origin: a wireframe cube + tripod marks where
            // the original sits while the dragged copy is whisked away.
            gizmos.axes(state.original_world_tf, 1.0);
            gizmos.cube(state.original_world_tf, Color::srgb(0.5, 0.5, 0.5));

            // "+" indicator at the dragged position.
            let current_tf = if let Ok((_e, tf, _m, _t)) = placement_query.get(active_entity) {
                Some(*tf)
            } else if let Ok((_e, tf, _m, _t, _d)) = prim_query.get(active_entity) {
                Some(*tf)
            } else {
                None
            };
            if let Some(current_tf) = current_tf {
                gizmos.axes(current_tf, 1.5);
                let center = current_tf.translation + Vec3::Y * 2.0;
                let green = Color::srgb(0.0, 1.0, 0.0);
                gizmos.line(center - Vec3::X * 0.4, center + Vec3::X * 0.4, green);
                gizmos.line(center - Vec3::Z * 0.4, center + Vec3::Z * 0.4, green);
            }
        }
        return;
    }

    // Falling edge — the gizmo went idle. Either commit or discard.
    let was_aborted = state.aborted;
    let is_copy = state.is_copy;
    let drag_target = state.target;
    state.active_entity = None;
    state.aborted = false;
    state.target = ActiveTarget::None;

    if was_aborted {
        return;
    }

    match drag_target {
        ActiveTarget::Room => {
            let Some(mut record) = room_record else {
                return;
            };
            if commit_room_drag(
                active_entity,
                is_copy,
                &placement_query,
                &prim_query,
                &global_tf,
                &mut record,
                &mut room_editor,
            ) {
                info!("Gizmo drag committed (room). Rebuilding world.");
                room_editor.mark_dirty();
                record.set_changed();
            }
        }
        ActiveTarget::Avatar => {
            let Some(mut record) = avatar_record else {
                return;
            };
            if commit_avatar_drag(active_entity, &avatar_prim_query, &global_tf, &mut record) {
                info!("Gizmo drag committed (avatar). Rebuilding visuals.");
                // The avatar editor's UI debounce only runs when widgets
                // fire; a gizmo drag bypasses that path, so explicitly
                // mark the record changed so `rebuild_local_visuals` and
                // `network::broadcast_avatar_state` see a fresh tick.
                record.set_changed();
            }
        }
        ActiveTarget::None => {}
    }
}
