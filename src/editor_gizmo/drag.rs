//! Drag-session state machine: rising-edge (mouse-down on the gizmo,
//! Shift selects copy-on-drag), every-frame Escape watch + ghost
//! drawing, falling-edge commit when `GizmoTarget::is_active()` flips
//! back to `false`. Writebacks are routed to
//! [`commit::commit_room_drag`](super::commit::commit_room_drag) /
//! [`commit::commit_avatar_drag`](super::commit::commit_avatar_drag).
//!
//! Two gestures are not moves, and write nothing (#1396): a press that
//! never moves the gizmo (the falling edge finds the target where the drag
//! began - see [`moved`]), and a press on a gizmo that appeared under it
//! (never a drag at all - see [`withhold_the_selecting_press`]).

use bevy::ecs::message::Messages;
use bevy::prelude::*;
use transform_gizmo_bevy::{GizmoDragStarted, GizmoTarget};

use crate::player::attachments::LocalAttachment;
use crate::state::{LiveAvatarRecord, LiveRoomRecord};
use crate::ui::room::RoomEditorState;
use crate::world_builder::{AttachmentPrim, AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::blob::BlobEditContext;
use super::blob::proxy::BlobElementProxy;
use super::blob::write::{BlobDragInfo, commit_blob_element_drag};
use super::commit::DragOutcome;
use super::commit::{
    commit_attachment_drag, commit_attachment_part_drag, commit_avatar_drag, commit_room_drag,
    resolve_committed_local,
};
use super::{ActiveTarget, DragState, GizmoDetachedPrim};

/// Say what a finished drag did, when it is not what was asked (#1237
/// f144, #1243 f150). Silent on success - a toast per completed drag
/// would be noise on the app's most-repeated gesture.
fn report_drag(toasts: &mut crate::notify::Toasts, time: &Time, outcome: DragOutcome) {
    if let Some(text) = outcome.toast() {
        toasts.warn(text, time.elapsed_secs_f64());
    }
}

/// Placement anchors under the gizmo. This and the five aliases after it
/// are the kinds a drag session tracks, named because both of its edges
/// look an entity up through them ([`tracked_pose`]). Each holds
/// `&mut Transform`; the `Without`s keep them provably disjoint.
type Placements<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static PlacementMarker,
        &'static GizmoTarget,
    ),
    (
        Without<PrimMarker>,
        Without<AvatarVisualPrim>,
        Without<BlobElementProxy>,
    ),
>;

/// Room prims under the gizmo - see [`Placements`].
type Prims<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static PrimMarker,
        &'static GizmoTarget,
        Option<&'static GizmoDetachedPrim>,
    ),
    (Without<AvatarVisualPrim>, Without<BlobElementProxy>),
>;

/// The local avatar's visuals nodes under the gizmo - see [`Placements`].
type AvatarPrims<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static AvatarVisualPrim,
        &'static GizmoTarget,
        Option<&'static GizmoDetachedPrim>,
    ),
    (Without<PrimMarker>, Without<BlobElementProxy>),
>;

/// Blob element proxies under the gizmo - see [`Placements`].
type Proxies<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static BlobElementProxy,
        &'static GizmoTarget,
        Option<&'static GizmoDetachedPrim>,
    ),
    (
        Without<PlacementMarker>,
        Without<PrimMarker>,
        Without<AvatarVisualPrim>,
    ),
>;

/// Worn props under the gizmo (#1062) - see [`Placements`].
type Attachments<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static LocalAttachment,
        &'static GizmoTarget,
        Option<&'static GizmoDetachedPrim>,
    ),
    (
        Without<PlacementMarker>,
        Without<PrimMarker>,
        Without<AvatarVisualPrim>,
        Without<BlobElementProxy>,
    ),
>;

/// Parts of worn props under the gizmo (#1098) - see [`Placements`].
type AttachmentParts<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        &'static AttachmentPrim,
        &'static GizmoTarget,
        Option<&'static GizmoDetachedPrim>,
    ),
    (
        Without<PlacementMarker>,
        Without<PrimMarker>,
        Without<AvatarVisualPrim>,
        Without<BlobElementProxy>,
        Without<LocalAttachment>,
    ),
>;

/// Where a tracked entity stands, and whether it is a blueprint root. The
/// one lookup both edges of a drag use: the rising edge for the pose the
/// drag starts from (and whether Shift may copy it), the falling edge for
/// whether it moved at all (#1396). `None` for anything else, including an
/// entity that despawned mid-drag.
///
/// A kind the `is_active` scan in [`manage_gizmo_drag`] can report but this
/// cannot find never starts a drag, so its release commits nothing - worn
/// props' parts, from #1098 until #1397.
fn tracked_pose(
    entity: Entity,
    placements: &Placements,
    prims: &Prims,
    avatar_prims: &AvatarPrims,
    attachments: &Attachments,
    parts: &AttachmentParts,
    proxies: &Proxies,
) -> Option<(Transform, bool)> {
    if let Ok((_, tf, ..)) = placements.get(entity) {
        return Some((*tf, false));
    }
    if let Ok((_, tf, marker, ..)) = prims.get(entity) {
        return Some((*tf, marker.path.is_empty()));
    }
    if let Ok((_, tf, marker, ..)) = avatar_prims.get(entity) {
        return Some((*tf, marker.path.is_empty()));
    }
    if let Ok((_, tf, ..)) = attachments.get(entity) {
        return Some((*tf, false));
    }
    if let Ok((_, tf, marker, ..)) = parts.get(entity) {
        return Some((*tf, marker.path.is_empty()));
    }
    proxies.get(entity).ok().map(|(_, tf, ..)| (*tf, false))
}

/// Whether a released gizmo moved its target from where the drag began
/// (#1396).
///
/// transform-gizmo reports a press on a handle as a drag whether or not
/// the pointer then moves, and every commit writes the target's DRAWN pose.
/// For a placement Avoid Water or the terrain snap moved, that is not the
/// recorded pose - so a click on a handle rewrote the record: seed 253's
/// kiosk went from (123.350, -0.35, -5.363) to (129.344, 0.395, -5.624)
/// with nothing dragged. A drag that did not move is a click, and a click
/// writes nothing.
///
/// Measured against the pose taken at the rising edge, when the drag has
/// not yet applied a delta. The tolerances are float noise: at zero delta
/// the gizmo's f32 -> f64 -> f32 round trip is exact, while one pixel of
/// drag on a 720-line frame, with the camera a metre from the target,
/// already moves it about a millimetre.
fn moved(from: &Transform, to: &Transform) -> bool {
    !from.translation.abs_diff_eq(to.translation, 1e-4)
        || !from.rotation.abs_diff_eq(to.rotation, 1e-5)
        || !from.scale.abs_diff_eq(to.scale, 1e-5)
}

/// A gizmo that appears on the frame of a press cannot be grabbed by that
/// press (#1396).
///
/// A left-click pick selects in `Update` and `sync_gizmo_selection`
/// attaches the gizmo in `PostUpdate`; transform-gizmo then starts a drag
/// in `Last` for any press (`GizmoDragStarted`, written on `just_pressed`)
/// that has a handle under the pointer. So the click that picked an object
/// also grabbed whichever handle of its brand-new gizmo it landed on, and
/// the release committed a move: on seed 253's kiosk a 0.65 m jump the
/// pointer never made, on top of the drawn pose [`moved`] is about.
///
/// `pick_on_scene_click`'s drag-safety rule already assumed the opposite:
/// a drag starts only on a handle that was there, and hovered, before the
/// press. Withholding the press on the one frame a gizmo is new makes that
/// true for every route a selection can arrive by. Runs between the attach
/// (so the new target is `Added`) and transform-gizmo's `Last` update.
pub(super) fn withhold_the_selecting_press(
    mouse: Res<ButtonInput<MouseButton>>,
    new_gizmos: Query<(), Added<GizmoTarget>>,
    drag_started: Option<ResMut<Messages<GizmoDragStarted>>>,
) {
    if mouse.just_pressed(MouseButton::Left)
        && !new_gizmos.is_empty()
        && let Some(mut drag_started) = drag_started
    {
        drag_started.clear();
    }
}

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
/// frame after - giving us a clean one-frame-delayed falling edge to
/// commit on.
///
/// Copy-on-drag (room editor only): Shift-held at drag-start clones the
/// placement / room prim at commit time and drops the new copy at the
/// dragged position, leaving the original in place. Blueprint roots
/// force copy off - cloning an entire construct tree sideways is
/// expressed at the placement layer instead. Avatar prims do not support
/// copy-on-drag in v1: there's only one local avatar tree, and the
/// inventory + room placements vocabulary doesn't apply.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub(super) fn manage_gizmo_drag(
    mut state: Local<DragState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut gizmos: Gizmos<super::EditorOverlayGizmos>,
    mut room_editor: ResMut<RoomEditorState>,
    mut blob_ctx: ResMut<BlobEditContext>,
    mut placement_query: Placements,
    mut prim_query: Prims,
    mut avatar_prim_query: AvatarPrims,
    mut proxy_query: Proxies,
    mut attachment_query: Attachments,
    // The rigged bodies worn props hang off - the rig (for its rest joint
    // positions) and the root pose that puts the rest frame in the world.
    // Bundled with the parts-of-worn-props query (#1098) to stay under the
    // 16-parameter ceiling; that query's five `Without`s keep it disjoint
    // from every other `&mut Transform` query above.
    (rigged_bodies, global_tf, mut part_query, mut toasts, time): (
        Query<&bevy_symbios_avatar::AvatarBody>,
        Query<&GlobalTransform>,
        AttachmentParts,
        // #1237 f144 / #1243 f150: every commit refusal in this system was
        // a `warn!` to a console the user does not have, and the scene
        // went on showing the move as having succeeded.
        ResMut<crate::notify::Toasts>,
        Res<Time>,
    ),
    room_record: Option<ResMut<LiveRoomRecord>>,
    avatar_record: Option<ResMut<LiveAvatarRecord>>,
    // For the snapped-placement Y rebase at commit time (#701).
    heightmap: Option<Res<crate::terrain::FinishedHeightMap>>,
    // Undo-entry labels (#865): a gizmo commit names its target.
    mut undo_labels: ResMut<crate::ui::undo::PendingUndoLabels>,
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
    if active_target.is_none() {
        for (entity, _tf, _m, target, _d) in attachment_query.iter() {
            if target.is_active() {
                active_target = Some((entity, ActiveTarget::Attachment));
                break;
            }
        }
    }
    if active_target.is_none() {
        for (entity, _tf, _m, target, _d) in part_query.iter() {
            if target.is_active() {
                active_target = Some((entity, ActiveTarget::AttachmentPart));
                break;
            }
        }
    }
    if active_target.is_none() {
        for (entity, _tf, _p, target, _d) in proxy_query.iter() {
            if target.is_active() {
                // The proxy belongs to whichever editor owns the blob-edit
                // session; the authoritative routing is the session info
                // captured at the rising edge, this tag is only used to
                // pick the drag's editor at that moment.
                let kind = blob_ctx
                    .active
                    .as_ref()
                    .map(|a| a.key.target)
                    .unwrap_or(ActiveTarget::None);
                active_target = Some((entity, kind));
                break;
            }
        }
    }

    // Rising edge - a new drag just started.
    if state.active_entity.is_none() {
        let Some((entity, target_kind)) = active_target else {
            return;
        };
        let Some((original_world_tf, is_prim_root)) = tracked_pose(
            entity,
            &placement_query,
            &prim_query,
            &avatar_prim_query,
            &attachment_query,
            &part_query,
            &proxy_query,
        ) else {
            return;
        };
        let shift = keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        // A blob element drag snapshots its routing at the rising edge so
        // a mid-drag GUI selection change can't reroute the writeback.
        // Shift means "duplicate this element" here (the record-local
        // analogue of copy-on-drag - valid for avatar blobs too, unlike
        // prim copy which needs the placements vocabulary).
        state.blob = proxy_query
            .get(entity)
            .ok()
            .and_then(|(_e, _tf, proxy, _t, _d)| {
                blob_ctx.active.as_ref().map(|a| BlobDragInfo {
                    key: a.key.clone(),
                    index: proxy.index,
                    duplicate: shift,
                })
            });
        let mut is_copy = shift;
        // A blueprint root has no parent to receive a sibling clone - a
        // "copy of the root" only makes sense at the Placement level.
        // Avatar prims also disable copy: there's no avatar-side
        // equivalent of placements, and the visuals tree is single-
        // rooted per local player. Blob elements route Shift through the
        // session info above, not the placement/prim copy path.
        // Worn props join avatar visuals in refusing copy-on-drag: a second
        // prop means a second attachment RECORD (owned copy, minted TID,
        // fan-out cap), which is the Attach row's job, not a drag's. Their
        // parts refuse too (#1397): the part commit has no copy path, so a
        // Shift-drag would draw the copy ghost and then move the part.
        if is_prim_root
            || matches!(
                target_kind,
                ActiveTarget::Avatar | ActiveTarget::Attachment | ActiveTarget::AttachmentPart
            )
            || state.blob.is_some()
        {
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

    // Active drag - every frame until the mouse is released.
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
            } else if let Ok((_e, mut tf, _w, _t, _d)) = attachment_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            } else if let Ok((_e, mut tf, _a, _t, _d)) = part_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            } else if let Ok((_e, mut tf, _p, _t, _d)) = proxy_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            }
            return;
        }

        // A blob element's duplicate routes through `BlobDragInfo` rather
        // than the placement/prim copy path, so `is_copy` is forced false
        // for it above - and the copy FEEDBACK hung off `is_copy` alone,
        // which is why a Shift-drag on an element drew no ghost, no tripod
        // and no "+" at all (#1243 f150). The user had no way to tell,
        // during the gesture, whether they were copying or moving.
        let showing_a_copy =
            state.is_copy || state.blob.as_ref().is_some_and(|info| info.duplicate);
        if showing_a_copy {
            // Ghost at origin: a wireframe cube + tripod marks where
            // the original sits while the dragged copy is whisked away.
            gizmos.axes(state.original_world_tf, 1.0);
            gizmos.cube(state.original_world_tf, Color::srgb(0.5, 0.5, 0.5));

            // "+" indicator at the dragged position.
            let current_tf = if let Ok((_e, tf, _m, _t)) = placement_query.get(active_entity) {
                Some(*tf)
            } else if let Ok((_e, tf, _m, _t, _d)) = prim_query.get(active_entity) {
                Some(*tf)
            } else if let Ok((_e, tf, _p, _t, _d)) = proxy_query.get(active_entity) {
                // The blob element's own proxy (#1243 f150).
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

    // Falling edge - the gizmo went idle. Either commit or discard.
    let was_aborted = state.aborted;
    let is_copy = state.is_copy;
    let drag_target = state.target;
    let blob_info = state.blob.take();
    state.active_entity = None;
    state.aborted = false;
    state.target = ActiveTarget::None;

    // A drag that never moved the gizmo is a click (#1396): discarded like
    // an Escape, Shift or not - a copy dropped exactly on its original is
    // one nobody can see. An entity that despawned mid-drag has no pose to
    // compare and falls through to the commit, which reports the refusal.
    let never_moved = tracked_pose(
        active_entity,
        &placement_query,
        &prim_query,
        &avatar_prim_query,
        &attachment_query,
        &part_query,
        &proxy_query,
    )
    .is_some_and(|(released, _)| !moved(&state.original_world_tf, &released));
    if was_aborted || never_moved {
        if blob_info.is_some() {
            // The in-drag preview may have painted speculative edge lines;
            // repaint from the (unchanged) record.
            blob_ctx.wireframe_dirty = true;
        }
        return;
    }

    // Blob element drags route through the session info, not the
    // placement/prim writeback.
    if let Some(info) = blob_info {
        let committed_local =
            proxy_query
                .get(active_entity)
                .ok()
                .and_then(|(_e, tf, _p, _t, detached)| {
                    resolve_committed_local(tf, detached, &global_tf)
                });
        let Some(local) = committed_local else {
            blob_ctx.wireframe_dirty = true;
            report_drag(&mut toasts, &time, DragOutcome::Refused);
            return;
        };
        let landed = match info.key.target {
            ActiveTarget::Room => room_record.map(|mut record| {
                let landed = commit_blob_element_drag(&info, &local, Some(&mut record.0), None);
                if landed.is_some() {
                    info!("Blob element drag committed (room). Rebuilding world.");
                    undo_labels.set_room("blob element edit");
                    record.set_changed();
                }
                landed
            }),
            ActiveTarget::Avatar => avatar_record.map(|mut record| {
                let landed = commit_blob_element_drag(&info, &local, None, Some(&mut record));
                if landed.is_some() {
                    info!("Blob element drag committed (avatar). Rebuilding visuals.");
                    undo_labels.set_avatar("blob element edit");
                    record.set_changed();
                }
                landed
            }),
            // Blob-element sculpting is addressed by a path into a
            // generator tree; a worn prop's tree lives in its own record and
            // has no element session, so this is unreachable rather than
            // unimplemented.
            ActiveTarget::Attachment | ActiveTarget::AttachmentPart | ActiveTarget::None => None,
        };
        match landed.flatten() {
            // Keep the gizmo on the element the edit landed at - for a
            // Shift-duplicate that's the freshly inserted copy.
            Some(landing) => {
                blob_ctx.selected_element = Some(landing.index);
                report_drag(
                    &mut toasts,
                    &time,
                    if landing.degraded_to_move {
                        DragOutcome::CopyDegradedToMove
                    } else {
                        DragOutcome::Committed
                    },
                );
            }
            None => {
                blob_ctx.wireframe_dirty = true;
                report_drag(&mut toasts, &time, DragOutcome::Refused);
            }
        }
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
                &mut record.0,
                &mut room_editor,
                heightmap.as_deref(),
                state.original_world_tf,
            ) {
                info!("Gizmo drag committed (room). Rebuilding world.");
                let verb = if is_copy { "duplicate" } else { "move" };
                let target = prim_query
                    .get(active_entity)
                    .map(|(_, _, marker, _, _)| marker.generator_ref.clone())
                    .or_else(|_| {
                        placement_query
                            .get(active_entity)
                            .map(|(_, _, marker, _)| format!("placement {}", marker.0))
                    })
                    .unwrap_or_else(|_| "selection".to_string());
                undo_labels.set_room(format!("{verb} of {target}"));
                // No dirty flag to set: the World Editor derives "dirty"
                // from `records_differ(stored, live)`, and this commit
                // just mutated the live record. `set_changed()` still
                // drives the recompile + peer broadcast.
                record.set_changed();
            } else {
                report_drag(&mut toasts, &time, DragOutcome::Refused);
                // The record is UNCHANGED, and `sync` keeps the dragged
                // entity detached at its dropped pose until the selection
                // moves - so without this the scene goes on showing the
                // move until something unrelated recompiles (#1237 f144).
                // Marking the untouched record changed rebuilds from it,
                // which snaps the object back where it really is.
                record.set_changed();
            }
        }
        ActiveTarget::Avatar => {
            let Some(mut record) = avatar_record else {
                return;
            };
            if commit_avatar_drag(active_entity, &avatar_prim_query, &global_tf, &mut record) {
                info!("Gizmo drag committed (avatar). Rebuilding visuals.");
                undo_labels.set_avatar("move of avatar part");
                // The avatar editor's UI debounce only runs when widgets
                // fire; a gizmo drag bypasses that path, so explicitly
                // mark the record changed so `rebuild_local_visuals` and
                // `network::broadcast_avatar_state` see a fresh tick.
                record.set_changed();
            } else {
                report_drag(&mut toasts, &time, DragOutcome::Refused);
                record.set_changed();
            }
        }
        ActiveTarget::Attachment => {
            let Some(mut record) = avatar_record else {
                return;
            };
            if commit_attachment_drag(
                active_entity,
                &attachment_query,
                &rigged_bodies,
                &global_tf,
                &mut record,
            ) {
                info!("Gizmo drag committed (attachment). Re-dressing the body.");
                undo_labels.set_avatar("move of worn prop");
                // Same reason as the avatar branch: a gizmo drag bypasses
                // the editor's widget debounce, so the change tick has to
                // be set here for `sync_rigged_attachments` (re-dress) and
                // `broadcast_avatar_state` (peer preview) to see it.
                record.set_changed();
            } else {
                report_drag(&mut toasts, &time, DragOutcome::Refused);
                record.set_changed();
            }
        }
        ActiveTarget::AttachmentPart => {
            let Some(mut record) = avatar_record else {
                return;
            };
            if commit_attachment_part_drag(active_entity, &part_query, &global_tf, &mut record) {
                info!("Gizmo drag committed (worn item part). Re-dressing the body.");
                undo_labels.set_avatar("move of worn item part");
                record.set_changed();
            } else {
                report_drag(&mut toasts, &time, DragOutcome::Refused);
                record.set_changed();
            }
        }
        ActiveTarget::None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1396: a press that never moved the gizmo is a click, and a click
    /// writes nothing - so "did not move" has to survive float noise, and
    /// any drag a pointer can make has to count as a move. Posed where the
    /// click was measured: seed 253's kiosk, 129 m out, yawed 87.6 degrees.
    #[test]
    fn a_still_press_is_a_click_and_any_real_drag_is_a_move() {
        let start = Transform::from_xyz(129.344, 2.1, -5.624)
            .with_rotation(Quat::from_rotation_y(87.6_f32.to_radians()))
            .with_scale(Vec3::splat(0.94));
        let after = |edit: fn(&mut Transform)| {
            let mut released = start;
            edit(&mut released);
            released
        };

        assert!(!moved(&start, &start), "a press that went nowhere");
        // One ulp at 129 m is about 1.5e-5 m: noise, not a drag.
        assert!(!moved(
            &start,
            &after(|t| t.translation.x = f32::from_bits(t.translation.x.to_bits() + 1))
        ));
        assert!(!moved(
            &start,
            &after(|t| t.rotation.w = f32::from_bits(t.rotation.w.to_bits() + 1))
        ));

        assert!(
            moved(&start, &after(|t| t.translation.z += 0.001)),
            "a millimetre - one pixel at a metre - is a move"
        );
        assert!(
            moved(&start, &after(|t| t.rotate_y(0.1_f32.to_radians()))),
            "a tenth of a degree is a move"
        );
        assert!(
            moved(&start, &after(|t| t.scale *= 1.001)),
            "a 0.1% scale is a move"
        );
    }

    /// #1396: the press that makes a gizmo appear cannot grab it. The
    /// control is the other half of the rule, and without it "no drag ever
    /// starts" would pass too: a press on a gizmo that was already there
    /// still reaches transform-gizmo.
    #[test]
    fn a_press_cannot_grab_a_gizmo_that_appeared_under_it() {
        /// One press, with a gizmo that was there before it or one that
        /// appears with it. Returns the drag starts transform-gizmo would
        /// read in `Last`.
        fn drag_starts(gizmo_appears_with_the_press: bool) -> usize {
            let mut app = App::new();
            app.add_message::<GizmoDragStarted>()
                .init_resource::<ButtonInput<MouseButton>>()
                .add_systems(Update, withhold_the_selecting_press);
            if !gizmo_appears_with_the_press {
                app.world_mut().spawn(GizmoTarget::default());
            }
            // A frame with no press: a gizmo spawned above is old news by
            // the time the button goes down.
            app.update();
            if gizmo_appears_with_the_press {
                app.world_mut().spawn(GizmoTarget::default());
            }
            app.world_mut()
                .resource_mut::<ButtonInput<MouseButton>>()
                .press(MouseButton::Left);
            app.world_mut().write_message(GizmoDragStarted);
            app.update();
            app.world().resource::<Messages<GizmoDragStarted>>().len()
        }

        assert_eq!(
            drag_starts(false),
            1,
            "control: a press on a gizmo that was already there can start a drag"
        );
        assert_eq!(
            drag_starts(true),
            0,
            "the selecting press is withheld from the gizmo it made appear"
        );
    }

    /// What [`tracked_pose`] answers for each of `entities`, asked the way
    /// the drag session asks it: from a system, over the tracked kinds'
    /// queries.
    fn looked_up(
        In(entities): In<Vec<Entity>>,
        placements: Placements,
        prims: Prims,
        avatar_prims: AvatarPrims,
        attachments: Attachments,
        parts: AttachmentParts,
        proxies: Proxies,
    ) -> Vec<Option<Transform>> {
        entities
            .into_iter()
            .map(|entity| {
                tracked_pose(
                    entity,
                    &placements,
                    &prims,
                    &avatar_prims,
                    &attachments,
                    &parts,
                    &proxies,
                )
                .map(|(pose, _)| pose)
            })
            .collect()
    }

    /// #1397: the drag session's lookup finds an entity of every kind its
    /// `is_active` scan can report. A kind the lookup misses never starts a
    /// drag - the rising edge returns - so its release commits nothing: a
    /// worn item's parts moved under the gizmo and were never saved, from
    /// #1098 (189e484) until #1397. The kinds below are the SCAN's, in its
    /// order, not the lookup's own arms - a list copied from the lookup
    /// cannot catch what the lookup forgot - and the scan is read back at
    /// the end, so a kind added to it fails here until it gets a row.
    #[test]
    fn the_drag_lookup_finds_every_kind_the_scan_reports() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        let pose = |i: usize| Transform::from_xyz(i as f32 + 1.0, 2.0, -3.0);
        let kinds = [
            (
                "a placement anchor",
                world
                    .spawn((PlacementMarker(0), pose(0), GizmoTarget::default()))
                    .id(),
            ),
            (
                "a room prim",
                world
                    .spawn((
                        PrimMarker {
                            generator_ref: "house".into(),
                            path: vec![1],
                        },
                        pose(1),
                        GizmoTarget::default(),
                    ))
                    .id(),
            ),
            (
                "a node of your avatar's visuals",
                world
                    .spawn((
                        AvatarVisualPrim { path: vec![0] },
                        pose(2),
                        GizmoTarget::default(),
                    ))
                    .id(),
            ),
            (
                "a worn prop",
                world
                    .spawn((
                        LocalAttachment {
                            rkey: "hat".into(),
                            joint: 0,
                            rigged_root: Entity::PLACEHOLDER,
                            source: None,
                        },
                        pose(3),
                        GizmoTarget::default(),
                    ))
                    .id(),
            ),
            (
                "a part of a worn prop",
                world
                    .spawn((
                        AttachmentPrim {
                            rkey: "hat".into(),
                            path: vec![0],
                        },
                        pose(4),
                        GizmoTarget::default(),
                    ))
                    .id(),
            ),
            (
                "a blob element proxy",
                world
                    .spawn((
                        BlobElementProxy::for_test(0, Entity::PLACEHOLDER),
                        pose(5),
                        GizmoTarget::default(),
                    ))
                    .id(),
            ),
        ];
        // Control: a gizmo target of no tracked kind.
        let stranger = world.spawn((pose(6), GizmoTarget::default())).id();

        let entities: Vec<Entity> = kinds
            .iter()
            .map(|&(_, entity)| entity)
            .chain([stranger])
            .collect();
        let found = world
            .run_system_once_with(looked_up, entities)
            .expect("the lookup runs");
        for (i, (kind, _)) in kinds.iter().enumerate() {
            assert_eq!(
                found[i],
                Some(pose(i)),
                "the drag lookup cannot find {kind}: its drag never starts, and its release saves nothing"
            );
        }
        assert_eq!(
            found[kinds.len()],
            None,
            "control: a gizmo target of no tracked kind is not tracked"
        );

        // The scan, read back: each `is_active` check in it is one kind.
        let scan = include_str!("drag.rs")
            .split("fn manage_gizmo_drag(")
            .nth(1)
            .and_then(|body| body.split("// Rising edge").next())
            .expect("manage_gizmo_drag's is_active scan");
        assert_eq!(
            scan.matches(".is_active()").count(),
            kinds.len(),
            "manage_gizmo_drag's scan reports a kind this test does not spawn: \
             add it above, and to tracked_pose"
        );
    }
}
