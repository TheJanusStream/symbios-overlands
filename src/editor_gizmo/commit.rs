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
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
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
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
    original_world_tf: Transform,
) -> bool {
    if let Ok((_e, transform, marker, _t)) = placement_query.get(active_entity) {
        let transform = *transform;
        let marker_idx = marker.0;
        if is_copy {
            if let Some(original) = record.placements.get(marker_idx).cloned() {
                let mut new_placement = original;
                if write_transform_into_placement(&mut new_placement, &transform, heightmap) {
                    record.placements.push(new_placement);
                    editor.selected_placement = Some(record.placements.len() - 1);
                    return true;
                }
            }
            return false;
        }
        if let Some(placement) = record.placements.get_mut(marker_idx)
            && write_transform_into_placement(placement, &transform, heightmap)
        {
            return true;
        }
        return false;
    }

    if let Ok((_e, transform, marker, _t, detached)) = prim_query.get(active_entity) {
        let transform = *transform;
        let Some(generator) = record.generators.get_mut(&marker.generator_ref) else {
            return false;
        };

        let new_local = if marker.path.is_empty() {
            // Blueprint ROOT: never reparent against the anchor. The
            // root's anchor-relative pose is `cell_tf * root_tf`, and
            // `cell_tf` carries each Scatter/Grid cell's sample position
            // + random yaw — reparenting would bake THIS instance's cell
            // into the shared blueprint, teleporting/spinning every other
            // instance on the next recompile (#703). The world-space drag
            // delta applied to the authored root is cell-independent.
            root_transform_with_drag_delta(&generator.transform, &original_world_tf, &transform)
        } else {
            let Some(new_local) = resolve_committed_local(&transform, detached, global_tf) else {
                return false;
            };
            new_local
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

/// Commit a blueprint-ROOT drag by applying the drag's world-space delta
/// to the authored root transform (#703).
///
/// The spawn path composes each instance as `anchor ⊗ cell_tf ⊗ root_tf`,
/// where `cell_tf` is identity for an Absolute placement but carries the
/// sample position + random yaw of each Scatter/Grid cell. Reparenting a
/// dragged root against its anchor therefore returns `cell_tf ⊗ new_pose`
/// — one instance's cell baked into the shared blueprint. The delta form
/// sidesteps the cell entirely:
///
/// ```text
/// root_new = root_old ⊗ (world_before⁻¹ ⊗ world_after)
/// ```
///
/// For the dragged instance the recompiled pose is then exactly the pose
/// the user released at (`anchor ⊗ cell ⊗ root_new = world_after`, since
/// `anchor ⊗ cell ⊗ root_old = world_before`), and every sibling instance
/// receives the same local-frame edit.
fn root_transform_with_drag_delta(
    authored: &TransformData,
    world_before: &Transform,
    world_after: &Transform,
) -> Transform {
    let old_root = Transform {
        translation: Vec3::from_array(authored.translation.0),
        rotation: Quat::from_array(authored.rotation.0),
        scale: Vec3::from_array(authored.scale.0),
    };
    let delta = world_before.compute_affine().inverse() * world_after.compute_affine();
    let (scale, rotation, translation) =
        (old_root.compute_affine() * delta).to_scale_rotation_translation();
    Transform {
        translation,
        rotation,
        scale,
    }
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
///
/// `transform` is the anchor's WORLD pose, but a snapped placement's
/// record Y lives in a terrain-relative frame — writing world Y verbatim
/// made every drag of a snapped placement leap by the terrain height on
/// the next recompile (#701). The Y rebase below keeps the two frames
/// straight: sideways drags preserve the surface offset (the object
/// sticks to the terrain), vertical drags adjust it.
fn write_transform_into_placement(
    placement: &mut Placement,
    transform: &Transform,
    heightmap: Option<&crate::terrain::FinishedHeightMap>,
) -> bool {
    match placement {
        Placement::Absolute {
            transform: rec_tf,
            snap_to_terrain,
            ..
        } => {
            let mut translation = transform.translation.to_array();
            if *snap_to_terrain && let Some(hm) = heightmap {
                // The anchor sat at terrain(old x/z) + old offset when the
                // drag started, so subtracting terrain at the OLD x/z
                // (still in the record here) turns the dragged world Y
                // back into "offset + vertical drag delta": pure sideways
                // drags keep the offset, vertical drags change it.
                translation[1] -=
                    hm.world_height_at(rec_tf.translation.0[0], rec_tf.translation.0[2]);
            }
            rec_tf.translation = Fp3(translation);
            rec_tf.rotation = Fp4(transform.rotation.to_array());
            true
        }
        Placement::Grid {
            transform: rec_tf,
            snap_to_terrain,
            ..
        } => {
            let mut translation = transform.translation.to_array();
            if *snap_to_terrain && let Some(hm) = heightmap {
                // Grid compile REPLACES Y with the terrain height; store
                // that height at the NEW spot so the record mirrors what
                // the recompile will render (same rule as the snap
                // toggle, #700).
                translation[1] = hm.world_height_at(translation[0], translation[2]);
            }
            rec_tf.translation = Fp3(translation);
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

#[cfg(test)]
mod tests {
    use super::*;

    /// #703: committing a blueprint-root drag must be cell-independent —
    /// for ANY placement cell (scatter sample offset + random yaw), the
    /// dragged instance recompiles to exactly the released pose, and the
    /// authored root never absorbs the cell. The old reparent-against-the-
    /// anchor path failed this for every non-identity cell.
    #[test]
    fn root_drag_delta_is_cell_independent() {
        let authored = TransformData::from(
            Transform::from_xyz(1.0, 2.0, 3.0)
                .with_rotation(Quat::from_rotation_y(0.4))
                .with_scale(Vec3::splat(1.5)),
        );
        let root_old = Transform {
            translation: Vec3::from_array(authored.translation.0),
            rotation: Quat::from_array(authored.rotation.0),
            scale: Vec3::from_array(authored.scale.0),
        };
        // A scatter-like cell: sample offset + random yaw, composed under
        // a snapped anchor.
        let anchor = Transform::from_xyz(-40.0, 6.5, 12.0);
        let cell = Transform::from_xyz(17.0, 0.0, -9.0).with_rotation(Quat::from_rotation_y(2.1));
        let compose = |a: &Transform, b: &Transform| -> Transform {
            let (scale, rotation, translation) =
                (a.compute_affine() * b.compute_affine()).to_scale_rotation_translation();
            Transform {
                translation,
                rotation,
                scale,
            }
        };

        let world_before = compose(&compose(&anchor, &cell), &root_old);
        // The drag: rotate in place and lift a little.
        let world_after = Transform {
            translation: world_before.translation + Vec3::Y * 2.0,
            rotation: Quat::from_rotation_y(0.7) * world_before.rotation,
            scale: world_before.scale,
        };

        let root_new = root_transform_with_drag_delta(&authored, &world_before, &world_after);

        // Recompiled pose of the dragged instance == the released pose.
        let recompiled = compose(&compose(&anchor, &cell), &root_new);
        assert!(
            recompiled.translation.distance(world_after.translation) < 1e-3,
            "translation diverged: {recompiled:?} vs {world_after:?}"
        );
        assert!(
            recompiled.rotation.angle_between(world_after.rotation) < 1e-3,
            "rotation diverged"
        );
        // The cell never leaks into the blueprint: an identity drag is a
        // no-op on the authored root.
        let unchanged = root_transform_with_drag_delta(&authored, &world_before, &world_before);
        assert!(unchanged.translation.distance(root_old.translation) < 1e-3);
        // f32 affine inverse + quat re-extraction wobbles the last ULP,
        // which `acos` amplifies — 5e-3 rad (~0.3°) is far below anything
        // authoring-visible while still catching a real cell leak (the
        // cell yaw here is 2.1 rad).
        assert!(unchanged.rotation.angle_between(root_old.rotation) < 5e-3);
        assert!(unchanged.scale.distance(root_old.scale) < 1e-3);
    }
}
