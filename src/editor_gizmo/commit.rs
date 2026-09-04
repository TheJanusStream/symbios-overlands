//! Drag-commit writebacks into [`RoomRecord`] / [`LiveAvatarRecord`].
//! Converts the post-drag world-space `Transform` (prims are detached
//! while the gizmo is attached) back into a local-space transform via
//! the cached parent's `GlobalTransform`, then walks the recipe by
//! `path` to overwrite the target node.

use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::{Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::player::attachments::LocalAttachment;
use crate::state::LiveAvatarRecord;
use crate::ui::room::RoomEditorState;
use crate::world_builder::{AttachmentPrim, AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::GizmoDetachedPrim;
use super::blob::proxy::BlobElementProxy;

/// What a finished gizmo drag actually did to the record (#1237 f144,
/// #1243 f150).
///
/// Every commit path in this module could already refuse — the original
/// parent despawned mid-drag, a path went stale under a recompile, a
/// duplicate hit the element cap — and every refusal was a `warn!` to a
/// console the user does not have. `manage_gizmo_drag` wrote every branch
/// as `if commit_*(…) { … }` with no `else`, and `sync` keeps the dragged
/// entity detached at its dropped pose until the selection changes, so the
/// scene went on showing the move as having succeeded while the record
/// held the old value. The user's model of "what is saved" diverged from
/// the record, and they published the wrong thing.
///
/// One vocabulary rather than a message per call site, because the three
/// refusals are one event as far as the user is concerned: the thing you
/// dragged is not where you left it.
///
/// A refusal ALSO forces a rebuild at the call site (`set_changed()` on
/// the untouched record), because saying so is only half the fix: the
/// scene must stop showing the move that did not happen.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum DragOutcome {
    /// The record was updated as the gesture asked.
    Committed,
    /// The record WAS updated, but a Shift-duplicate became a plain move:
    /// the blob's element list is full (#1243 f150). The worse of the two
    /// possible mistakes — the original was moved rather than copied.
    CopyDegradedToMove,
    /// Nothing was written. The object was rebuilt or reshaped under the
    /// drag.
    Refused,
}

impl DragOutcome {
    /// What the user is told, or `None` when the gesture did what it said.
    pub(super) fn toast(self) -> Option<&'static str> {
        match self {
            Self::Committed => None,
            Self::CopyDegradedToMove => Some(
                "This blob is at its element limit — the drag moved the element \
                 instead of copying it.",
            ),
            Self::Refused => Some(
                "That move could not be applied — the object was rebuilt \
                     mid-drag. Try again.",
            ),
        }
    }
}

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
        (
            Without<PrimMarker>,
            Without<AvatarVisualPrim>,
            Without<BlobElementProxy>,
        ),
    >,
    prim_query: &Query<
        (
            Entity,
            &mut Transform,
            &PrimMarker,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        (Without<AvatarVisualPrim>, Without<BlobElementProxy>),
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
            if let Some(new_idx) = append_sibling_at_path(generator, &marker.path, Some(new_local))
            {
                let mut new_path = marker.path.clone();
                if let Some(last) = new_path.last_mut() {
                    *last = new_idx;
                }
                select_copy(
                    editor,
                    &marker.generator_ref,
                    new_path,
                    transform.translation,
                );
                return true;
            }
            return false;
        }

        return commit_transform_at_path(generator, &marker.path, new_local);
    }

    false
}

/// Land the editor on the clone a Shift-copy-drag just made (#1237 f145).
///
/// The copy path used to rewrite `selected_prim_path`'s last index and
/// nothing else — but the TREE is the source of truth: after every draw
/// `draw_tree_panel` reads `tree_view_state.selected()` back over
/// `selected_generator` / `selected_prim_path`, so the next World-Editor
/// frame reverted the selection to the original. The gizmo and the
/// highlight jumped back to the object that had not moved, and the user's
/// next drag silently edited the wrong node — against the documented
/// contract of one of the four gestures the Controls sheet teaches.
///
/// These are the same fields `MenuChoice::DuplicateItem` writes, which is
/// the duplicate path that always got it right.
fn select_copy(
    editor: &mut RoomEditorState,
    generator_ref: &str,
    new_path: Vec<usize>,
    world_pos: Vec3,
) {
    editor.selected_generator = Some(generator_ref.to_string());
    editor.selected_prim_path = Some(new_path.clone());
    editor
        .tree_view_state
        .set_selected(vec![crate::ui::room::GenNodeId::child(
            generator_ref.to_string(),
            new_path.clone(),
        )]);
    editor.pending_tree_focus = true;
    // …and the instance preference, or the gizmo falls back to
    // camera-proximity ranking and can land on a different copy of a
    // scattered generator than the one just dropped. `world_pos` is the
    // detached entity's transform, which for a prim under the gizmo is
    // world-space — the drop point itself.
    editor.preferred_pick = Some(crate::ui::room::PreferredPick {
        generator_ref: generator_ref.to_string(),
        path: new_path,
        pos: world_pos,
    });
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
        (Without<PrimMarker>, Without<BlobElementProxy>),
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

    let Some(visuals) = record.0.body.visuals_mut() else {
        return false;
    };
    commit_transform_at_path(visuals, &marker.path, new_local)
}

/// Commit a finished drag of a PART of a worn prop (#1098) into that
/// attachment record's item tree — the avatar-visuals commit with the
/// tree looked up by record key. The part detached to world like a prim
/// and reparents against its original parent (the prop root or an
/// ancestor part), so nothing here touches the joint's rest frame: that
/// is the whole-prop offset's business.
#[allow(clippy::type_complexity)]
pub(super) fn commit_attachment_part_drag(
    active_entity: Entity,
    part_query: &Query<
        (
            Entity,
            &mut Transform,
            &AttachmentPrim,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        (
            Without<PlacementMarker>,
            Without<PrimMarker>,
            Without<AvatarVisualPrim>,
            Without<BlobElementProxy>,
            Without<LocalAttachment>,
        ),
    >,
    global_tf: &Query<&GlobalTransform>,
    record: &mut LiveAvatarRecord,
) -> bool {
    let Ok((_e, transform, marker, _t, detached)) = part_query.get(active_entity) else {
        return false;
    };
    let transform = *transform;
    let Some(new_local) = resolve_committed_local(&transform, detached, global_tf) else {
        return false;
    };
    let Some(resolved) = record
        .0
        .body
        .rigged_mut()
        .and_then(|rig| rig.resolved.as_mut())
    else {
        return false;
    };
    let Some(worn) = resolved
        .attachments
        .iter_mut()
        .find(|a| a.rkey == marker.rkey)
    else {
        return false;
    };
    commit_transform_at_path(&mut worn.record.item, &marker.path, new_local)
}

/// Commit a finished drag of a worn prop (#1062) into the resolved
/// attachment's `offset`. Returns `true` when the record was mutated.
///
/// **The frame is the carrying joint's REST frame, not the world and not
/// the joint's live pose.** A rig joint is animated; its `GlobalTransform`
/// is wherever this frame's clip put it. The record stores one offset that
/// has to hold for every frame of every clip, and the frame it is authored
/// against is the bind pose the engine spawns joints at — every joint
/// unrotated at its rig position. So the released world pose is reparented
/// against [`LocalAttachment::rest_frame`] (the body root's pose translated
/// by the joint's rig position), *not* against the detached parent the way
/// [`resolve_committed_local`] does for prims. Reparenting against the live
/// joint would bake this instant of the clip into the offset, and the prop
/// would sit correctly only at that one phase of the walk.
///
/// The gizmo is *placed* through the same rest frame on attach (see
/// `sync::attach_or_release_attachment`), so the conversion is exact
/// whatever the body happens to be doing. The bind-pose hold in
/// `player::rigged::drive_rigged_motion` is therefore about the owner
/// seeing the pose they are authoring for, not about making this arithmetic
/// come out right.
///
/// Scale lands through [`crate::pds::AttachmentRecord::sanitize`] per
/// axis (#1095): the gizmo offers the full scale triad (see
/// `sync::attachment_modes`) and the record keeps what was dragged,
/// exactly as a region placement does.
#[allow(clippy::type_complexity)]
pub(super) fn commit_attachment_drag(
    active_entity: Entity,
    attachment_query: &Query<
        (
            Entity,
            &mut Transform,
            &LocalAttachment,
            &GizmoTarget,
            Option<&GizmoDetachedPrim>,
        ),
        (
            Without<PlacementMarker>,
            Without<PrimMarker>,
            Without<AvatarVisualPrim>,
            Without<BlobElementProxy>,
        ),
    >,
    rigged_bodies: &Query<&bevy_symbios_avatar::AvatarBody>,
    global_tf: &Query<&GlobalTransform>,
    record: &mut LiveAvatarRecord,
) -> bool {
    let Ok((_e, transform, worn, _t, detached)) = attachment_query.get(active_entity) else {
        return false;
    };
    if detached.is_none() {
        // The gizmo detaches its target from the hierarchy on attach, and a
        // prop always has a joint parent to be detached from. No marker
        // means the entity lost its parent mid-drag (a body rebuild landing
        // under the gesture) — its `Transform` is then not the world pose
        // this conversion assumes, and committing it would write garbage.
        warn!("Attachment commit skipped: the dragged prop was never detached");
        return false;
    }
    let world = GlobalTransform::from(*transform);

    let (Ok(body), Ok(root_world)) = (
        rigged_bodies.get(worn.rigged_root),
        global_tf.get(worn.rigged_root),
    ) else {
        warn!("Attachment commit skipped: the rigged body despawned during the drag");
        return false;
    };
    let Some(rest) = worn.rest_frame(&body.avatar, root_world) else {
        warn!(
            "Attachment commit skipped: joint {} is not in this rig",
            worn.joint
        );
        return false;
    };

    let Some(resolved) = record
        .0
        .body
        .rigged_mut()
        .and_then(|rig| rig.resolved.as_mut())
    else {
        return false;
    };
    let Some(attachment) = resolved
        .attachments
        .iter_mut()
        .find(|a| a.rkey == worn.rkey)
    else {
        // Detached from the Attachments tab while the drag was in flight.
        return false;
    };
    attachment.record.offset = TransformData::from(world.reparented_to(&rest));
    attachment.record.sanitize();
    true
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
/// irreversibly corrupt the recipe. Shared with the blob-element commit
/// in `drag.rs` (#705), whose proxies detach the same way.
pub(super) fn resolve_committed_local(
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

/// Append a sibling clone of the node at `path`. `new_local` overrides
/// the clone's transform (the copy-on-drag path passes the dragged
/// pose); `None` keeps the original's transform verbatim — the context
/// menu's in-place Duplicate (#824). Returns the new sibling's
/// child-index on success; `None` if `path` is empty (root has no
/// parent to clone into) or invalid. Avatar prims do not support copy.
pub(crate) fn append_sibling_at_path(
    generator: &mut Generator,
    path: &[usize],
    new_local: Option<Transform>,
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
    if let Some(new_local) = new_local {
        new_child.transform = TransformData::from(new_local);
    }
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
    // The ground reading MUST match the compile executor's (#1011): the
    // offset written below is `dragged world Y − ground`, and the compile
    // renders at `ground + offset`, so any disagreement is baked into the
    // record and compounds on every drag. A seeded structure resolves
    // against its whole footprint, not its centre (#1008).
    let radius = crate::world_builder::snap_footprint_radius(placement);
    match placement {
        Placement::Absolute {
            transform: rec_tf,
            snap_to_terrain,
            ..
        } => {
            let mut translation = transform.translation.to_array();
            if *snap_to_terrain && let Some(hm) = heightmap {
                // The anchor sat at ground(old x/z) + old offset when the
                // drag started, so subtracting the ground at the OLD x/z
                // (still in the record here) turns the dragged world Y
                // back into "offset + vertical drag delta": pure sideways
                // drags keep the offset, vertical drags change it.
                translation[1] -= crate::world_builder::snapped_ground_y(
                    &hm.0,
                    rec_tf.translation.0[0],
                    rec_tf.translation.0[2],
                    radius,
                );
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
                // toggle, #700). Grid anchors are point-like, so `radius`
                // is `None` and this is the plain centre sample.
                translation[1] = crate::world_builder::snapped_ground_y(
                    &hm.0,
                    translation[0],
                    translation[2],
                    radius,
                );
            }
            rec_tf.translation = Fp3(translation);
            rec_tf.rotation = Fp4(transform.rotation.to_array());
            true
        }
        Placement::Scatter { bounds, .. } => {
            // Scatter is translate-only (#827, user decision): the gizmo
            // no longer offers rotation handles, and the Rect angle is
            // owned by the Bounds "Rotation (deg)" slider. Deliberately
            // do NOT derive a yaw from the anchor pose here — that wrote
            // the anchor's (identity) rotation over an authored Rect
            // angle on every translate-only drag.
            match bounds {
                crate::pds::ScatterBounds::Circle { center, .. }
                | crate::pds::ScatterBounds::Rect { center, .. } => {
                    center.0[0] = transform.translation.x;
                    center.0[1] = transform.translation.z;
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

#[cfg(test)]
mod scatter_commit_tests {
    use super::*;
    use crate::pds::{BiomeFilter, Fp, Fp2, ScatterBounds};

    /// #827: a translate-only scatter drag moves the bounds centre and
    /// must NOT touch the Rect's authored rotation — the old code derived
    /// yaw from the anchor pose (identity on a fresh spawn) and zeroed
    /// the slider-set angle on every drag.
    #[test]
    fn scatter_rect_translate_preserves_the_authored_angle() {
        let mut placement = Placement::Scatter {
            generator_ref: "g".into(),
            bounds: ScatterBounds::Rect {
                center: Fp2([0.0, 0.0]),
                extents: Fp2([32.0, 16.0]),
                rotation: Fp(0.9),
            },
            count: 8,
            local_seed: 1,
            biome_filter: BiomeFilter::default(),
            snap_to_terrain: true,
            random_yaw: true,
            avoid_urban: false,
            float_on_water: false,
            naturalness: Default::default(),
        };
        let dragged = Transform::from_xyz(15.0, 3.0, -7.5);
        assert!(write_transform_into_placement(
            &mut placement,
            &dragged,
            None
        ));
        match placement {
            Placement::Scatter {
                bounds:
                    ScatterBounds::Rect {
                        center, rotation, ..
                    },
                ..
            } => {
                assert_eq!(center.0, [15.0, -7.5]);
                assert_eq!(rotation.0, 0.9, "authored angle must survive");
            }
            other => panic!("variant changed: {other:?}"),
        }
    }

    /// #1237 f144 / #1243 f150. Sequence: drag a prop, let go; it stays
    /// where you dropped it; later something rebuilds and it snaps back
    /// with no memory of anything having gone wrong. Every commit refusal
    /// in this module was a `warn!` to a console the user does not have,
    /// and `manage_gizmo_drag` wrote every branch with no `else` at all.
    #[test]
    fn every_outcome_but_success_says_something() {
        assert_eq!(DragOutcome::Committed.toast(), None, "no noise on success");
        let refused = DragOutcome::Refused.toast().expect("a refusal must speak");
        assert!(refused.contains("could not be applied"), "{refused}");
        let degraded = DragOutcome::CopyDegradedToMove
            .toast()
            .expect("a copy that became a move must speak");
        assert!(degraded.contains("limit"), "{degraded}");
        assert_ne!(refused, degraded, "they are different events");
    }

    /// #1237 f145. Sequence: Shift-drag a window sub-part to make a second
    /// one. The copy appears where you dropped it, then the gizmo and the
    /// highlight jump back to the ORIGINAL, and your next drag moves the
    /// original instead of the copy. The copy path rewrote
    /// `selected_prim_path`'s last index and nothing else — but
    /// `draw_tree_panel` reads `tree_view_state.selected()` back over
    /// those fields after every draw, so the tree, which still named the
    /// original, won.
    #[test]
    fn a_copy_drag_hands_the_tree_the_clone_not_the_original() {
        let mut editor = RoomEditorState::default();
        editor.selected_generator = Some("house".into());
        editor.selected_prim_path = Some(vec![2, 0]);

        select_copy(&mut editor, "house", vec![2, 1], Vec3::new(4.0, 1.0, -2.0));

        assert_eq!(editor.selected_prim_path.as_deref(), Some(&[2, 1][..]));
        // The tree is the source of truth — it must name the clone, or the
        // very next World-Editor draw reverts the selection.
        assert_eq!(
            editor.tree_view_state.selected().as_slice(),
            [crate::ui::room::GenNodeId::child(
                "house".to_string(),
                vec![2, 1]
            )]
        );
        assert!(editor.pending_tree_focus, "the row has to be revealed");
        let pick = editor.preferred_pick.expect("the instance preference");
        assert_eq!(pick.path, vec![2, 1]);
        assert_eq!(pick.pos, Vec3::new(4.0, 1.0, -2.0));
    }
}
