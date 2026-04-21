//! Bridge between the egui world editor's selection state and the in-world
//! 3D transform gizmo. The gizmo itself is driven by `transform-gizmo-bevy`
//! — this module handles:
//!
//! * attaching a `GizmoTarget` to whichever entity the owner is currently
//!   editing (a `Placement::Absolute` root, or a single `PrimNode` inside a
//!   `Generator::Construct` blueprint), removing it from any previously
//!   selected entity, and
//! * committing the dragged `Transform` back into the live `RoomRecord` on
//!   mouse release so the `world_builder` recompile, the Publish-to-PDS
//!   button and the peer broadcast all see the final pose exactly once per
//!   drag.
//!
//! **Proximity Gizmo.** A `Construct` can be referenced by many placements
//! (e.g. a scatter of 50 houses). Attaching a `GizmoTarget` to every live
//! instance of a UI-selected prim would make `transform-gizmo-bevy` group
//! the selection and rotate the group around its centroid — breaking the
//! local math the moment you try to nudge a single chimney. Instead, we
//! find the instance *closest to the camera* and attach the gizmo there
//! alone. When the drag commits, the record update triggers a full room
//! recompile and every other instance of the same construct reappears with
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
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::{Generator, Placement, RoomRecord, TransformData};
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
            commit_gizmo_drag.run_if(in_state(AppState::InGame)),
        );
    }
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
#[allow(clippy::type_complexity)]
fn sync_gizmo_selection(
    mut commands: Commands,
    editor_state: Res<RoomEditorState>,
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
    for (entity, marker, has_gizmo) in placement_query.iter() {
        let is_selected = want_placement_gizmo && editor_state.selected_placement == Some(marker.0);
        if is_selected && !has_gizmo {
            commands.entity(entity).try_insert(GizmoTarget::default());
        } else if !is_selected && has_gizmo {
            commands.entity(entity).try_remove::<GizmoTarget>();
        }
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

/// Write the dragged `Transform` back into the `RoomRecord` the instant the
/// owner drops the left mouse button. Writing during the drag would make
/// `RoomRecord::is_changed()` fire on every frame, which in turn triggers
/// `compile_room_record` to despawn the dragged entity mid-drag and lose
/// the gizmo's target. Deferring the write to mouse-release collapses the
/// whole gesture into a single record update, a single peer broadcast and a
/// single recompile.
///
/// Because prims are detached from their parent while the gizmo is attached,
/// a prim's `Transform` is in world space at commit time. We convert back to
/// blueprint-local space using the cached parent's `GlobalTransform` before
/// writing into the recipe.
///
/// `GizmoTarget::is_active()` reflects the most recent drag state set by
/// `transform-gizmo-bevy`'s `update_gizmos` system in `Last`. Running our
/// commit in `PostUpdate` means we observe the *previous* frame's
/// `is_active` — still `true` on the release frame for the entity being
/// dragged — which is exactly the filter we need. Without this guard, a
/// stray left-click on empty space would fire `set_changed()` and uselessly
/// rebuild the world.
fn commit_gizmo_drag(
    mouse: Res<ButtonInput<MouseButton>>,
    placement_query: Query<(&Transform, &PlacementMarker, &GizmoTarget)>,
    prim_query: Query<(
        &Transform,
        &PrimMarker,
        &GizmoTarget,
        Option<&GizmoDetachedPrim>,
    )>,
    global_tf: Query<&GlobalTransform>,
    record: Option<ResMut<RoomRecord>>,
) {
    if !mouse.just_released(MouseButton::Left) {
        return;
    }
    let Some(mut record) = record else {
        return;
    };

    let mut committed = false;

    for (transform, marker, target) in placement_query.iter() {
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

    for (transform, marker, target, detached) in prim_query.iter() {
        if !target.is_active() {
            continue;
        }

        // Convert the post-drag world pose back into blueprint-local space.
        // A detached prim's `Transform` is already world-space (no parent);
        // we divide out the original parent's world pose to recover the
        // local offset the recipe expects. If the prim wasn't detached (edge
        // case: selection raced with a rebuild), we treat its `Transform` as
        // already-local and write it through unchanged.
        let new_local = if let Some(detached) = detached {
            match global_tf.get(detached.original_parent) {
                // Wrap the detached prim's local `Transform` (which is
                // world-space because it has no parent) as a synthetic
                // `GlobalTransform` so we can reuse Bevy's built-in
                // world→local helper.
                Ok(parent_gt) => GlobalTransform::from(*transform).reparented_to(parent_gt),
                Err(_) => *transform,
            }
        } else {
            *transform
        };

        let Some(Generator::Construct { root }) = record.generators.get_mut(&marker.generator_ref)
        else {
            continue;
        };

        // Walk the blueprint tree along the recorded path. A mid-drag edit
        // that removes a sibling could shift indices; bail cleanly if the
        // path no longer resolves rather than panicking.
        let mut current_node = root;
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

    if committed {
        info!("Gizmo drag committed. Rebuilding world.");
        record.set_changed();
    }
}
