//! Bridge between the egui world editor's selection state and the in-world
//! 3D transform gizmo. The gizmo itself is driven by `transform-gizmo-bevy`
//! — this module handles:
//!
//! * attaching a `GizmoTarget` to whichever entity the owner is currently
//!   editing (a `Placement::Absolute` root, or any node inside a named
//!   generator's tree), removing it from any previously selected entity,
//!   and
//! * committing the dragged `Transform` back into the live `RoomRecord` on
//!   mouse release so the `world_builder` recompile, the Publish-to-PDS
//!   button and the peer broadcast all see the final pose exactly once per
//!   drag.
//!
//! **Proximity Gizmo.** A named generator can be referenced by many
//! placements (e.g. a scatter of 50 houses). Attaching a `GizmoTarget` to
//! every live instance of a UI-selected node would make `transform-gizmo-bevy`
//! group the selection and rotate the group around its centroid — breaking
//! the local math the moment you try to nudge a single chimney. Instead,
//! we find the instance *closest to the camera* and attach the gizmo there
//! alone. When the drag commits, the record update triggers a full room
//! recompile and every other instance of the same generator reappears with
//! the updated blueprint.
//!
//! **World-space detach.** `transform-gizmo-bevy` reads the target entity's
//! *local* `Transform` and treats it as the world pose — it has no notion
//! of `GlobalTransform`. A child prim in the blueprint hierarchy would
//! therefore render its gizmo at the chimney's blueprint-local offset
//! (e.g. `(0, 2, 0)`) rather than at the chimney's actual world position
//! atop the targeted house. To bridge that, we remove the prim's `ChildOf`
//! link while the gizmo is attached and bake its `GlobalTransform` into
//! its local `Transform` so the two coincide. On deselect (or commit) we
//! either walk back through the stored parent's `GlobalTransform` to write
//! a valid local transform into the recipe (commit) or reparent and convert
//! back (plain deselect). Each prim now carries its own `RoomEntity` so the
//! compile-pass cleanup sweeps even the detached ones on rebuild.
//!
//! The commit runs in `PostUpdate` so the `Transform` we read is the final
//! value after the gizmo's `Last`-schedule update system has applied the
//! frame's drag delta.

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use transform_gizmo_bevy::{EnumSet, GizmoMode, GizmoOptions, GizmoTarget};

use crate::pds::{Fp3, Fp4, Placement, RoomRecord, TransformData};
use crate::state::AppState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{PlacementMarker, PrimMarker};

/// Marker attached to a prim while it is serving as the gizmo target. Stores
/// the parent entity we removed it from so a plain deselect (tab change,
/// 🎯 toggled off) can reattach the prim to its original place in the
/// hierarchy without forcing a room recompile.
#[derive(Component)]
struct GizmoDetachedPrim {
    original_parent: Entity,
}

pub struct EditorGizmoPlugin;

impl Plugin for EditorGizmoPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            sync_gizmo_selection.run_if(in_state(AppState::InGame)),
        )
        .add_systems(
            PostUpdate,
            manage_gizmo_drag.run_if(in_state(AppState::InGame)),
        );
    }
}

/// Drag session state spanning all the frames between mouse-down and
/// mouse-release on the gizmo. Holds the identity of the entity being
/// dragged, the world-space pose it started at (so `Escape` can snap it
/// back and copy-on-drag can draw a ghost of the origin), whether the
/// drag is in copy-mode (Shift held at drag start), and whether the user
/// has aborted this drag with `Escape`. Lives in a `Local<DragState>` on
/// the manage system so it persists across the multiple frames of a drag.
#[derive(Default)]
struct DragState {
    active_entity: Option<Entity>,
    original_world_tf: Transform,
    is_copy: bool,
    aborted: bool,
}

/// Keep the `GizmoTarget` component in sync with the active editor tab and
/// its selection state.
///
/// Uses `try_insert` / `try_remove` because a UI edit that changes a
/// construct prim's texture marks `RoomRecord` dirty and `compile_room_record`
/// (running in the same `Update` schedule) despawns every room entity before
/// re-spawning fresh ones. Without the `try_` variants the query's stale
/// entity IDs would panic when their insert/remove commands applied against
/// already-despawned indices. Tolerating the race here is safe — the next
/// frame's sync pass sees the newly-spawned entity and re-attaches
/// `GizmoTarget` on it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync_gizmo_selection(
    mut commands: Commands,
    editor_state: Res<RoomEditorState>,
    mut gizmo_options: ResMut<GizmoOptions>,
    placement_query: Query<(Entity, &PlacementMarker, Has<GizmoTarget>)>,
    prim_query: Query<(
        Entity,
        &PrimMarker,
        &GlobalTransform,
        Has<GizmoTarget>,
        Has<GizmoDetachedPrim>,
        Option<&ChildOf>,
    )>,
    detached_query: Query<&GizmoDetachedPrim>,
    global_tf: Query<&GlobalTransform>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
) {
    if !editor_state.is_changed() {
        return;
    }

    let cam_pos = camera_query
        .single()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);

    // Resolve the prim we want to carry the gizmo this frame (Generators tab
    // only). The proximity search picks the live instance closest to the
    // camera so the owner can walk up to any copy of the construct and edit
    // that copy's chimney in place, with the record edit propagating to all
    // other copies on recompile.
    let target_prim = if editor_state.selected_tab == EditorTab::Generators {
        match (
            editor_state.selected_generator.as_ref(),
            editor_state.selected_prim_path.as_ref(),
        ) {
            (Some(generator_ref), Some(path)) => {
                let mut best_entity = None;
                let mut best_dist_sq = f32::MAX;
                for (entity, marker, tf, _, _, _) in prim_query.iter() {
                    if marker.generator_ref == *generator_ref && marker.path == *path {
                        let dist_sq = tf.translation().distance_squared(cam_pos);
                        if dist_sq < best_dist_sq {
                            best_dist_sq = dist_sq;
                            best_entity = Some(entity);
                        }
                    }
                }
                best_entity
            }
            _ => None,
        }
    } else {
        None
    };

    // --- Placements ---------------------------------------------------------
    let want_placement_gizmo = editor_state.selected_tab == EditorTab::Placements;
    let mut placement_selected = false;

    for (entity, marker, has_gizmo) in placement_query.iter() {
        let is_selected = want_placement_gizmo && editor_state.selected_placement == Some(marker.0);
        if is_selected {
            placement_selected = true;
        }
        if is_selected && !has_gizmo {
            commands.entity(entity).try_insert(GizmoTarget::default());
        } else if !is_selected && has_gizmo {
            commands.entity(entity).try_remove::<GizmoTarget>();
        }
    }

    let is_prim_selected = target_prim.is_some();

    // Restrict Gizmo Modes. Placements can't scale (their generator's
    // construct tree owns shape). Prims can translate/rotate/scale except
    // for the blueprint root, which is locked to rotate+scale — translating
    // the root would just shift the entire construct relative to its own
    // origin and is better expressed at the placement level.
    if placement_selected {
        let mut modes = EnumSet::new();
        modes.insert_all(GizmoMode::all_translate());
        modes.insert_all(GizmoMode::all_rotate());
        gizmo_options.gizmo_modes = modes;
    } else if is_prim_selected {
        let is_root = editor_state
            .selected_prim_path
            .as_ref()
            .map(|p| p.is_empty())
            .unwrap_or(false);
        let mut modes = EnumSet::new();
        if !is_root {
            modes.insert_all(GizmoMode::all_translate());
        }
        modes.insert_all(GizmoMode::all_rotate());
        modes.insert_all(GizmoMode::all_scale());
        gizmo_options.gizmo_modes = modes;
    }

    // --- Prims --------------------------------------------------------------
    for (entity, _marker, gt, has_gizmo, is_detached, child_of) in prim_query.iter() {
        let is_target = target_prim == Some(entity);

        if is_target && !has_gizmo {
            // Attach the gizmo. Detach from the parent so the prim's local
            // `Transform` equals its world pose — the gizmo will then render
            // on top of the actual prim no matter how deep in the blueprint
            // hierarchy (anchor → root → ... → this node) it lives.
            let world_tf = gt.compute_transform();
            if let Some(child_of) = child_of {
                commands
                    .entity(entity)
                    .remove::<ChildOf>()
                    .insert(world_tf)
                    .try_insert((
                        GizmoDetachedPrim {
                            original_parent: child_of.parent(),
                        },
                        GizmoTarget::default(),
                    ));
            } else {
                // Already parentless (unusual — the anchor normally owns
                // every prim). Fall back to a plain attach so the gizmo
                // still appears at the current world pose.
                commands
                    .entity(entity)
                    .insert(world_tf)
                    .try_insert(GizmoTarget::default());
            }
        } else if !is_target && (has_gizmo || is_detached) {
            // Restore the prim to its normal hierarchy. We recompute its
            // would-be local transform from whatever world pose it ended up
            // at (including any unfinished drag) so a deselect without a
            // drag commit leaves the visible scene unchanged.
            if let Ok(detached) = detached_query.get(entity) {
                if let Ok(parent_gt) = global_tf.get(detached.original_parent) {
                    let new_local = gt.reparented_to(parent_gt);
                    commands
                        .entity(entity)
                        .try_insert(ChildOf(detached.original_parent))
                        .try_insert(new_local)
                        .remove::<GizmoDetachedPrim>()
                        .try_remove::<GizmoTarget>();
                } else {
                    // Parent already despawned (mid-rebuild race). Drop the
                    // markers; the about-to-run compile pass will finish the
                    // cleanup by tearing the detached prim down too.
                    commands
                        .entity(entity)
                        .remove::<GizmoDetachedPrim>()
                        .try_remove::<GizmoTarget>();
                }
            } else if has_gizmo {
                commands.entity(entity).try_remove::<GizmoTarget>();
            }
        }
    }
}

/// Drive the full drag session: detect the rising edge (Shift at drag start
/// chooses copy-on-drag), watch for `Escape` aborts and render the
/// origin-ghost + "+" indicator every frame, then commit (or discard) on
/// the falling edge when the gizmo goes idle.
///
/// Writing during the drag would make `RoomRecord::is_changed()` fire on
/// every frame, which in turn triggers `compile_room_record` to despawn the
/// dragged entity mid-drag and lose the gizmo's target. Deferring the write
/// to drag-end collapses the whole gesture into a single record update, a
/// single peer broadcast and a single recompile.
///
/// Because prims are detached from their parent while the gizmo is attached,
/// a prim's `Transform` is in world space at commit time. We convert back to
/// blueprint-local space using the cached parent's `GlobalTransform` before
/// writing into the recipe.
///
/// `GizmoTarget::is_active()` reflects the most recent drag state set by
/// `transform-gizmo-bevy`'s `update_gizmos` system in `Last`. Running in
/// `PostUpdate` means we observe the *previous* frame's `is_active`, which
/// is still `true` on the release frame and flips to `false` the frame
/// after — giving us a clean one-frame-delayed falling edge to commit on.
///
/// Copy-on-drag: Shift-held at drag-start clones the placement/prim at
/// commit time and drops the new copy at the dragged position, leaving the
/// original in place. Blueprint roots force copy off — cloning an entire
/// construct tree sideways is expressed at the placement layer instead.
#[allow(clippy::too_many_arguments)]
fn manage_gizmo_drag(
    mut state: Local<DragState>,
    keyboard: Res<ButtonInput<KeyCode>>,
    mut gizmos: Gizmos,
    mut editor_state: ResMut<RoomEditorState>,
    mut placement_query: Query<
        (Entity, &mut Transform, &PlacementMarker, &GizmoTarget),
        Without<PrimMarker>,
    >,
    mut prim_query: Query<(
        Entity,
        &mut Transform,
        &PrimMarker,
        &GizmoTarget,
        Option<&GizmoDetachedPrim>,
    )>,
    global_tf: Query<&GlobalTransform>,
    record: Option<ResMut<RoomRecord>>,
) {
    // Find the entity (if any) whose gizmo reports active this frame.
    let mut active_target: Option<Entity> = None;
    for (entity, _tf, _m, target) in placement_query.iter() {
        if target.is_active() {
            active_target = Some(entity);
            break;
        }
    }
    if active_target.is_none() {
        for (entity, _tf, _m, target, _d) in prim_query.iter() {
            if target.is_active() {
                active_target = Some(entity);
                break;
            }
        }
    }

    // Rising edge — a new drag just started.
    if state.active_entity.is_none() {
        let Some(entity) = active_target else {
            return;
        };
        let (original_world_tf, is_prim_root) =
            if let Ok((_e, tf, _m, _t)) = placement_query.get(entity) {
                (*tf, false)
            } else if let Ok((_e, tf, marker, _t, _d)) = prim_query.get(entity) {
                (*tf, marker.path.is_empty())
            } else {
                return;
            };
        let mut is_copy =
            keyboard.pressed(KeyCode::ShiftLeft) || keyboard.pressed(KeyCode::ShiftRight);
        // A blueprint root has no parent to receive a sibling clone — a
        // "copy of the root" only makes sense at the Placement level. Force
        // is_copy off so the commit path takes the normal in-place update.
        if is_prim_root {
            is_copy = false;
        }
        state.active_entity = Some(entity);
        state.original_world_tf = original_world_tf;
        state.is_copy = is_copy;
        state.aborted = false;
    }

    let active_entity = state.active_entity.unwrap();
    let is_still_active = active_target == Some(active_entity);

    // Active drag — every frame until the mouse is released.
    if is_still_active {
        if keyboard.just_pressed(KeyCode::Escape) {
            state.aborted = true;
        }

        if state.aborted {
            // Visually snap back to the starting pose. The gizmo's Last-
            // schedule update will keep trying to write the dragged pose,
            // but overwriting here each frame keeps the user's feedback
            // pinned to "nothing happened" until they release.
            if let Ok((_e, mut tf, _m, _t)) = placement_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            } else if let Ok((_e, mut tf, _m, _t, _d)) = prim_query.get_mut(active_entity) {
                *tf = state.original_world_tf;
            }
            return;
        }

        if state.is_copy {
            // Ghost at origin: a wireframe cube + tripod marks where the
            // original sits while the dragged copy is whisked away.
            gizmos.axes(state.original_world_tf, 1.0);
            gizmos.cube(state.original_world_tf, Color::srgb(0.5, 0.5, 0.5));

            // "+" indicator at the dragged position: bigger tripod plus a
            // green crossing pair hovering above the cursor to signal
            // "this is a copy" rather than a move.
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
    state.active_entity = None;
    state.aborted = false;

    if was_aborted {
        return;
    }
    let Some(mut record) = record else {
        return;
    };

    let mut committed = false;

    if let Ok((_e, transform, marker, _t)) = placement_query.get(active_entity) {
        let transform = *transform;
        let marker_idx = marker.0;
        if is_copy {
            // Clone the original (unchanged) placement, stamp it with the
            // dragged pose (ignoring scale — placements don't scale), and
            // push it as a new row. UI selection follows the new copy so
            // the gizmo re-attaches to it on the next sync.
            if let Some(original) = record.placements.get(marker_idx).cloned() {
                let mut new_placement = original;
                if write_transform_into_placement(&mut new_placement, &transform) {
                    record.placements.push(new_placement);
                    editor_state.selected_placement = Some(record.placements.len() - 1);
                    committed = true;
                }
            }
        } else if let Some(placement) = record.placements.get_mut(marker_idx)
            && write_transform_into_placement(placement, &transform)
        {
            committed = true;
        }
    } else if let Ok((_e, transform, marker, _t, detached)) = prim_query.get(active_entity) {
        let transform = *transform;

        // Convert the post-drag world pose back into blueprint-local space.
        // A detached prim's `Transform` is already world-space (no parent);
        // divide out the original parent's world pose to recover the local
        // offset the recipe expects.
        //
        // If the parent has despawned mid-drag (a peer `RoomStateUpdate` or a
        // background recompile lands while the user is dragging), we have no
        // way to compute the correct local transform — falling through with
        // the world-space `transform` would write a world pose into the
        // recipe's local-transform field, displacing the prim by the
        // (now-vanished) parent's translation on the next compile and
        // irreversibly corrupting the authored recipe. Skip the commit
        // instead: the user can redo the drag once the recompile settles.
        let new_local = if let Some(detached) = detached {
            match global_tf.get(detached.original_parent) {
                Ok(parent_gt) => GlobalTransform::from(transform).reparented_to(parent_gt),
                Err(_) => {
                    warn!(
                        "Gizmo commit skipped: original parent despawned during drag — \
                         recipe left unchanged"
                    );
                    return;
                }
            }
        } else {
            transform
        };

        if let Some(generator) = record.generators.get_mut(&marker.generator_ref) {
            if is_copy && !marker.path.is_empty() {
                // Append the clone as a sibling of the original child. We
                // forced is_copy=false for roots earlier, so by construction
                // path is non-empty here.
                let parent_path = &marker.path[..marker.path.len() - 1];
                let child_idx = *marker.path.last().unwrap();
                let mut parent_node = &mut *generator;
                let mut valid = true;
                for &idx in parent_path {
                    if idx < parent_node.children.len() {
                        parent_node = &mut parent_node.children[idx];
                    } else {
                        valid = false;
                        break;
                    }
                }
                if valid && child_idx < parent_node.children.len() {
                    let mut new_child = parent_node.children[child_idx].clone();
                    new_child.transform = TransformData::from(new_local);
                    parent_node.children.push(new_child);
                    let new_idx = parent_node.children.len() - 1;
                    if let Some(path) = editor_state.selected_prim_path.as_mut()
                        && let Some(last) = path.last_mut()
                    {
                        *last = new_idx;
                    }
                    committed = true;
                }
            } else {
                // Normal in-place update — walk the path and stamp the new
                // local transform on the target node. An empty `path` means
                // the named generator's own root.
                let mut current_node = &mut *generator;
                let mut valid = true;
                for &idx in &marker.path {
                    if idx < current_node.children.len() {
                        current_node = &mut current_node.children[idx];
                    } else {
                        valid = false;
                        break;
                    }
                }
                if valid {
                    current_node.transform = TransformData::from(new_local);
                    committed = true;
                }
            }
        }
    }

    if committed {
        info!("Gizmo drag committed. Rebuilding world.");
        editor_state.mark_dirty();
        record.set_changed();
    }
}

/// Copy the translation + rotation from `transform` into `placement`. Scale
/// is intentionally ignored: placements don't scale (their generator's
/// construct tree owns shape), and the placement gizmo modes don't expose
/// a scale handle. Returns `false` for `Placement::Unknown` (no schema to
/// write into).
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
