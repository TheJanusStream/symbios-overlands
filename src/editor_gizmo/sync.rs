//! Per-frame `GizmoTarget` attachment / detachment plumbing. Computes
//! which entity should carry the gizmo this frame (a placement, a room
//! prim — closest to the camera if multiple instances exist — or an
//! avatar visual prim) and toggles `GizmoTarget` accordingly. The
//! world-space-detach trick (bake `GlobalTransform` into local
//! `Transform`, drop `ChildOf`) lives here so the gizmo (which only
//! reads local `Transform`) renders at the entity's actual world pose.

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use transform_gizmo_bevy::{EnumSet, GizmoMode, GizmoOptions, GizmoTarget};

use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::{ActiveTarget, GizmoDetachedPrim, GizmoFramePref, determine_active_target};

/// Keep the `GizmoTarget` component in sync with whichever editor has a
/// selection this frame.
///
/// Uses `try_insert` / `try_remove` because a UI edit that mutates the
/// live record can despawn every entity downstream before re-spawning
/// fresh ones. Without the `try_` variants the query's stale entity IDs
/// would panic when their insert/remove commands applied against
/// already-despawned indices. Tolerating the race here is safe — the
/// next frame's sync pass sees the newly-spawned entity and re-attaches
/// `GizmoTarget` on it.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
pub(super) fn sync_gizmo_selection(
    mut commands: Commands,
    room_state: Res<RoomEditorState>,
    avatar_state: Res<AvatarEditorState>,
    frame_pref: Res<GizmoFramePref>,
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
    avatar_prim_query: Query<(
        Entity,
        &AvatarVisualPrim,
        &GlobalTransform,
        Has<GizmoTarget>,
        Has<GizmoDetachedPrim>,
        Option<&ChildOf>,
    )>,
    detached_query: Query<&GizmoDetachedPrim>,
    global_tf: Query<&GlobalTransform>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
) {
    // No `is_changed()` guard. The earlier optimization missed the case
    // where a drag commit flips only the *record's* change tick (the
    // commit path doesn't touch the editor state), so on the next
    // frame's `rebuild_local_visuals` the freshly-spawned entity has no
    // gizmo and the editor's tick is unchanged → sync would skip and
    // the gizmo would never come back. Running every frame is cheap —
    // just iteration over a few small queries — and keeps the gizmo
    // tracking the selection through every respawn.

    // Per-frame: push the current orientation preference into the
    // gizmo's global config. Cheap to set unconditionally —
    // `GizmoOptions` change-detects on field write inside
    // `transform-gizmo-bevy`.
    gizmo_options.gizmo_orientation = frame_pref.0;

    let active = determine_active_target(&room_state, &avatar_state);

    let cam_pos = camera_query
        .single()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);

    // --- Resolve which prim entity (if any) should carry the gizmo ----------
    // Room prim: closest live instance of the UI-selected (generator_ref,
    // path) pair to the camera, only when the Room editor is active and
    // the Generators tab is showing.
    let target_room_prim =
        if active == ActiveTarget::Room && room_state.selected_tab == EditorTab::Generators {
            match (
                room_state.selected_generator.as_ref(),
                room_state.selected_prim_path.as_ref(),
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

    // Avatar prim: the unique entity matching the selected path. The
    // `AvatarVisualPrim` component is only attached to local-player
    // visuals (see `world_builder::compile::spawn_generator`), so a
    // single match is the local avatar's own node — no proximity scan.
    let target_avatar_prim = if active == ActiveTarget::Avatar {
        match avatar_state.selected_prim_path.as_ref() {
            Some(path) => avatar_prim_query
                .iter()
                .find_map(|(entity, marker, _, _, _, _)| {
                    if marker.path == *path {
                        Some(entity)
                    } else {
                        None
                    }
                }),
            None => None,
        }
    } else {
        None
    };

    // --- Placements (room only) --------------------------------------------
    let want_placement_gizmo =
        active == ActiveTarget::Room && room_state.selected_tab == EditorTab::Placements;
    let mut placement_selected = false;

    for (entity, marker, has_gizmo) in placement_query.iter() {
        let is_selected = want_placement_gizmo && room_state.selected_placement == Some(marker.0);
        if is_selected {
            placement_selected = true;
        }
        if is_selected && !has_gizmo {
            commands.entity(entity).try_insert(GizmoTarget::default());
        } else if !is_selected && has_gizmo {
            commands.entity(entity).try_remove::<GizmoTarget>();
        }
    }

    let is_room_prim_selected = target_room_prim.is_some();
    let is_avatar_prim_selected = target_avatar_prim.is_some();

    // Restrict gizmo modes per the type of thing selected. Placements
    // can't scale (their generator's construct tree owns shape). Prims
    // can translate / rotate / scale except for blueprint roots, which
    // are locked to rotate + scale — translating the root would just
    // shift the whole subtree relative to its own origin. Avatar
    // visuals follow the same root rule; their root translation lives in
    // the chassis (anchored by locomotion physics).
    if placement_selected {
        let mut modes = EnumSet::new();
        modes.insert_all(GizmoMode::all_translate());
        modes.insert_all(GizmoMode::all_rotate());
        gizmo_options.gizmo_modes = modes;
    } else if is_room_prim_selected {
        let is_root = room_state
            .selected_prim_path
            .as_ref()
            .map(|p| p.is_empty())
            .unwrap_or(false);
        gizmo_options.gizmo_modes = prim_modes(is_root);
    } else if is_avatar_prim_selected {
        let is_root = avatar_state
            .selected_prim_path
            .as_ref()
            .map(|p| p.is_empty())
            .unwrap_or(false);
        gizmo_options.gizmo_modes = prim_modes(is_root);
    }

    // --- Room prims (attach / detach + parent baking) ----------------------
    for (entity, _marker, gt, has_gizmo, is_detached, child_of) in prim_query.iter() {
        let is_target = target_room_prim == Some(entity);
        attach_or_release_prim(
            &mut commands,
            entity,
            is_target,
            has_gizmo,
            is_detached,
            gt,
            child_of,
            &detached_query,
            &global_tf,
        );
    }

    // --- Avatar prims (same machinery, separate query) ---------------------
    for (entity, _marker, gt, has_gizmo, is_detached, child_of) in avatar_prim_query.iter() {
        let is_target = target_avatar_prim == Some(entity);
        attach_or_release_prim(
            &mut commands,
            entity,
            is_target,
            has_gizmo,
            is_detached,
            gt,
            child_of,
            &detached_query,
            &global_tf,
        );
    }
}

/// Mode set for a prim selection — root prims (path == []) are locked to
/// rotate + scale; descendants get the full T+R+S triad.
fn prim_modes(is_root: bool) -> EnumSet<GizmoMode> {
    let mut modes = EnumSet::new();
    if !is_root {
        modes.insert_all(GizmoMode::all_translate());
    }
    modes.insert_all(GizmoMode::all_rotate());
    modes.insert_all(GizmoMode::all_scale());
    modes
}

/// Attach the gizmo to `entity` (detaching it from its parent and baking
/// world pose into local) when it becomes the target, and reverse the
/// process when another entity takes over or the selection clears.
/// Shared by both room and avatar prim queries — the only difference is
/// the marker carried on the entity, which doesn't affect attach/detach.
#[allow(clippy::too_many_arguments)]
fn attach_or_release_prim(
    commands: &mut Commands,
    entity: Entity,
    is_target: bool,
    has_gizmo: bool,
    is_detached: bool,
    gt: &GlobalTransform,
    child_of: Option<&ChildOf>,
    detached_query: &Query<&GizmoDetachedPrim>,
    global_tf: &Query<&GlobalTransform>,
) {
    if is_target && !has_gizmo {
        // Attach: bake `GlobalTransform` into local `Transform` so the
        // gizmo (which only reads local) renders at the actual world
        // position regardless of how deep in the hierarchy this entity
        // lives.
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
            // Already parentless (unusual). Plain attach so the gizmo
            // still appears at the current world pose.
            commands
                .entity(entity)
                .insert(world_tf)
                .try_insert(GizmoTarget::default());
        }
    } else if !is_target && (has_gizmo || is_detached) {
        // Release: restore the prim to its original hierarchy. We
        // recompute its would-be local transform from whatever world
        // pose it ended up at (including any unfinished drag) so a
        // deselect without a drag commit leaves the visible scene
        // unchanged.
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
                // Parent already despawned (mid-rebuild race). Drop
                // markers; the about-to-run rebuild will sweep this
                // entity through its own cleanup.
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
