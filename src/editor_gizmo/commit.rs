//! Drag-commit writebacks into [`RoomRecord`] / [`LiveAvatarRecord`].
//! Converts the post-drag world-space `Transform` (prims are detached
//! while the gizmo is attached) back into a local-space transform via
//! the cached parent's `GlobalTransform`, then walks the recipe by
//! `path` to overwrite the target node.

use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::{Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::state::LiveAvatarRecord;
use crate::ui::room::RoomEditorState;
use crate::world_builder::{AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::GizmoDetachedPrim;

/// Commit a finished drag against the room record. Handles the placement
/// vs prim split and the copy-on-drag clone path. Returns `true` when
/// the record was actually mutated — the caller is responsible for
/// flagging the resource as changed (`set_changed()` is on `ResMut`,
/// not on the inner type, so it has to live at the system boundary).
#[allow(clippy::type_complexity)]
pub(super) fn commit_room_drag(
    active_entity: Entity,
    is_copy: bool,
    placement_query: &Query<
        (Entity, &mut Transform, &PlacementMarker, &GizmoTarget),
        (Without<PrimMarker>, Without<AvatarVisualPrim>),
    >,
    prim_query: &Query<
        (
            Entity,
            &mut Transform,
            &PrimMarker,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        Without<AvatarVisualPrim>,
    >,
    global_tf: &Query<&GlobalTransform>,
    record: &mut RoomRecord,
    editor: &mut RoomEditorState,
) -> bool {
    if let Ok((_e, transform, marker, _t)) = placement_query.get(active_entity) {
        let transform = *transform;
        let marker_idx = marker.0;
        if is_copy {
            if let Some(original) = record.placements.get(marker_idx).cloned() {
                let mut new_placement = original;
                if write_transform_into_placement(&mut new_placement, &transform) {
                    record.placements.push(new_placement);
                    editor.selected_placement = Some(record.placements.len() - 1);
                    return true;
                }
            }
            return false;
        }
        if let Some(placement) = record.placements.get_mut(marker_idx)
            && write_transform_into_placement(placement, &transform)
        {
            return true;
        }
        return false;
    }

    if let Ok((_e, transform, marker, _t, detached)) = prim_query.get(active_entity) {
        let transform = *transform;
        let Some(new_local) = resolve_committed_local(&transform, detached, global_tf) else {
            return false;
        };

        let Some(generator) = record.generators.get_mut(&marker.generator_ref) else {
            return false;
        };

        if is_copy && !marker.path.is_empty() {
            if let Some(new_idx) = append_sibling_at_path(generator, &marker.path, new_local) {
                if let Some(path) = editor.selected_prim_path.as_mut()
                    && let Some(last) = path.last_mut()
                {
                    *last = new_idx;
                }
                return true;
            }
            return false;
        }

        return commit_transform_at_path(generator, &marker.path, new_local);
    }

    false
}

/// Commit a finished drag against the avatar's visuals tree. Returns
/// `true` when the record was mutated (caller flips the change tick).
/// No copy path here — see `manage_gizmo_drag`'s rising-edge note.
#[allow(clippy::type_complexity)]
pub(super) fn commit_avatar_drag(
    active_entity: Entity,
    avatar_prim_query: &Query<
        (
            Entity,
            &mut Transform,
            &AvatarVisualPrim,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        Without<PrimMarker>,
    >,
    global_tf: &Query<&GlobalTransform>,
    record: &mut LiveAvatarRecord,
) -> bool {
    let Ok((_e, transform, marker, _t, detached)) = avatar_prim_query.get(active_entity) else {
        return false;
    };
    let transform = *transform;

    let Some(new_local) = resolve_committed_local(&transform, detached, global_tf) else {
        return false;
    };

    commit_transform_at_path(&mut record.0.visuals, &marker.path, new_local)
}

/// Convert a post-drag world-space `Transform` back into the local-space
/// transform expected by the recipe. Returns `None` if the original
/// parent has despawned mid-drag (a peer state update or background
/// recompile lands while the user is dragging) — committing in that
/// case would write a world pose into a local-transform field and
/// irreversibly corrupt the recipe.
fn resolve_committed_local(
    transform: &Transform,
    detached: Option<&GizmoDetachedPrim>,
    global_tf: &Query<&GlobalTransform>,
) -> Option<Transform> {
    let Some(detached) = detached else {
        return Some(*transform);
    };
    match global_tf.get(detached.original_parent) {
        Ok(parent_gt) => Some(GlobalTransform::from(*transform).reparented_to(parent_gt)),
        Err(_) => {
            warn!(
                "Gizmo commit skipped: original parent despawned during drag — \
                 record left unchanged"
            );
            None
        }
    }
}

/// Walk a generator tree by `path` and overwrite the target node's
/// transform. Returns `false` if the path is invalid (e.g. the tree was
/// reshaped mid-drag). Shared by room and avatar commit paths.
fn commit_transform_at_path(
    generator: &mut Generator,
    path: &[usize],
    new_local: Transform,
) -> bool {
    let mut current = generator;
    for &idx in path {
        if idx >= current.children.len() {
            return false;
        }
        current = &mut current.children[idx];
    }
    current.transform = TransformData::from(new_local);
    true
}

/// Append a sibling clone of the node at `path`, with `new_local` as the
/// clone's transform. Returns the new sibling's child-index on success;
/// `None` if `path` is empty (root has no parent to clone into) or
/// invalid. Used only by the room copy-on-drag path — avatar prims do
/// not support copy.
fn append_sibling_at_path(
    generator: &mut Generator,
    path: &[usize],
    new_local: Transform,
) -> Option<usize> {
    if path.is_empty() {
        return None;
    }
    let parent_path = &path[..path.len() - 1];
    let child_idx = *path.last().unwrap();

    let mut parent = generator;
    for &idx in parent_path {
        if idx >= parent.children.len() {
            return None;
        }
        parent = &mut parent.children[idx];
    }
    if child_idx >= parent.children.len() {
        return None;
    }
    let mut new_child = parent.children[child_idx].clone();
    new_child.transform = TransformData::from(new_local);
    parent.children.push(new_child);
    Some(parent.children.len() - 1)
}

/// Copy the translation + rotation from `transform` into `placement`.
/// Scale is intentionally ignored: placements don't scale (their
/// generator's construct tree owns shape), and the placement gizmo
/// modes don't expose a scale handle. Returns `false` for
/// `Placement::Unknown` (no schema to write into).
fn write_transform_into_placement(placement: &mut Placement, transform: &Transform) -> bool {
    match placement {
        Placement::Absolute {
            transform: rec_tf, ..
        } => {
            rec_tf.translation = Fp3(transform.translation.to_array());
            rec_tf.rotation = Fp4(transform.rotation.to_array());
            true
        }
        Placement::Grid {
            transform: rec_tf, ..
        } => {
            rec_tf.translation = Fp3(transform.translation.to_array());
            rec_tf.rotation = Fp4(transform.rotation.to_array());
            true
        }
        Placement::Scatter { bounds, .. } => {
            match bounds {
                crate::pds::ScatterBounds::Circle { center, .. } => {
                    center.0[0] = transform.translation.x;
                    center.0[1] = transform.translation.z;
                }
                crate::pds::ScatterBounds::Rect {
                    center, rotation, ..
                } => {
                    center.0[0] = transform.translation.x;
                    center.0[1] = transform.translation.z;
                    let (yaw, _, _) = transform.rotation.to_euler(EulerRot::YXZ);
                    rotation.0 = yaw;
                }
            }
            true
        }
        Placement::Unknown => false,
    }
}
