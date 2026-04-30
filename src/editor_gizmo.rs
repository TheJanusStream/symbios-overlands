//! Bridge between the egui editors' selection state and the in-world 3D
//! transform gizmo. The gizmo itself is driven by `transform-gizmo-bevy`
//! — this module:
//!
//! * attaches a `GizmoTarget` to whichever entity the owner is currently
//!   editing, in either editor — Room editor: a `Placement::Absolute`
//!   root, or any node inside a named generator's tree; Avatar editor:
//!   any node in the local player's `visuals` tree — and removes it
//!   from any previously-selected entity.
//! * commits the dragged `Transform` back into the live record on mouse
//!   release — `RoomRecord` for room edits, `LiveAvatarRecord` for avatar
//!   edits — so the downstream recompile, the Publish-to-PDS button and
//!   the peer broadcast all see the final pose exactly once per drag.
//!
//! **Single-active-target invariant.** The two editors implement a click-
//! site mutex (selecting a row in one editor clears the other's
//! selection). This module reads both editor states each frame and
//! dispatches gizmo plumbing to whichever has a non-empty selection. If
//! both somehow do (mid-frame race), avatar wins — the locomotion-freeze
//! gate is keyed on avatar selection, so deferring to it preserves
//! physics behaviour.
//!
//! **Proximity Gizmo (room only).** A named room generator can be
//! referenced by many placements (e.g. a scatter of 50 houses). Attaching
//! a `GizmoTarget` to every live instance of a UI-selected node would
//! make `transform-gizmo-bevy` group the selection and rotate the group
//! around its centroid. Instead, we find the instance *closest to the
//! camera* and attach the gizmo there alone. When the drag commits, the
//! record update triggers a full room recompile and every other instance
//! reappears with the updated blueprint. Avatar visuals are local-only
//! and singular, so no proximity scan is needed there.
//!
//! **World-space detach.** `transform-gizmo-bevy` reads the target
//! entity's *local* `Transform` and treats it as the world pose — it has
//! no notion of `GlobalTransform`. A child prim deep in the blueprint
//! hierarchy would therefore render its gizmo at the prim's local offset
//! rather than at its actual world position. To bridge that, we remove
//! the prim's `ChildOf` link while the gizmo is attached and bake its
//! `GlobalTransform` into its local `Transform` so the two coincide. On
//! deselect (or commit) we walk back through the stored parent's
//! `GlobalTransform` to write a valid local transform into the recipe
//! (commit) or reparent and convert back (plain deselect). The same
//! detach/reattach machinery serves both room and avatar prims.
//!
//! The commit runs in `PostUpdate` so the `Transform` we read is the
//! final value after the gizmo's `Last`-schedule update system has
//! applied the frame's drag delta.

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::egui;
use transform_gizmo_bevy::{EnumSet, GizmoMode, GizmoOptions, GizmoOrientation, GizmoTarget};

use crate::pds::{Fp3, Fp4, Generator, Placement, RoomRecord, TransformData};
use crate::state::{AppState, LiveAvatarRecord};
use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{AvatarVisualPrim, PlacementMarker, PrimMarker};

/// Owner-facing toggle for how the gizmo's drag axes are oriented.
///
/// `Global` (the default and the v1 behaviour): handles align to world
/// XYZ. Translations, rotations and scales operate along world axes.
///
/// `Local`: handles align to the target entity's `Transform.rotation`.
/// Because prims are detached from their parent into world space when
/// the gizmo attaches (see `attach_or_release_prim`), that rotation is
/// the *accumulated* product of every parent rotation along the path
/// from the blueprint root — exactly what the owner expects when
/// arranging children of a tilted construct.
///
/// Wraps `transform_gizmo_bevy::GizmoOrientation` so call sites don't
/// have to know about the upstream type.
#[derive(Resource, Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct GizmoFramePref(pub GizmoOrientation);

/// Tab-bar widget shared by both editor panels: a two-state toggle
/// between World and Local gizmo orientation. Lives next to each
/// editor's tab strip so the owner can flip it without leaving the
/// panel they're editing in.
pub fn draw_gizmo_frame_toggle(ui: &mut egui::Ui, pref: &mut GizmoFramePref) {
    let is_global = pref.0 == GizmoOrientation::Global;
    ui.label("Gizmo:");
    if ui.selectable_label(is_global, "World").clicked() {
        pref.0 = GizmoOrientation::Global;
    }
    if ui.selectable_label(!is_global, "Local").clicked() {
        pref.0 = GizmoOrientation::Local;
    }
}

/// Marker attached to a prim while it is serving as the gizmo target.
/// Stores the parent entity we removed it from so a plain deselect (tab
/// change, panel collapse, mutex hand-off) can reattach the prim to its
/// original place in the hierarchy without forcing a record-driven
/// rebuild.
#[derive(Component)]
struct GizmoDetachedPrim {
    original_parent: Entity,
}

/// Which editor currently owns the gizmo. Computed each frame from the
/// two editor states; not stored as a resource because both states are
/// already authoritative.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
enum ActiveTarget {
    #[default]
    None,
    Room,
    Avatar,
}

fn determine_active_target(room: &RoomEditorState, avatar: &AvatarEditorState) -> ActiveTarget {
    // Avatar takes precedence over room when both somehow have a
    // selection at the same time. The click-site mutex in each editor
    // should keep this from happening, but if it does, deferring to
    // avatar lines up with the locomotion-freeze gate (which already
    // reads avatar state) so physics behaviour is consistent.
    if avatar.has_visuals_selection() {
        ActiveTarget::Avatar
    } else if room.selected_placement.is_some() || room.selected_prim_path.is_some() {
        ActiveTarget::Room
    } else {
        ActiveTarget::None
    }
}

pub struct EditorGizmoPlugin;

impl Plugin for EditorGizmoPlugin {
    fn build(&self, app: &mut App) {
        // Both gizmo systems live in `PostUpdate`. Two reasons:
        //
        // 1. `sync_gizmo_selection`'s detach path bakes the target
        //    entity's `GlobalTransform` into its local `Transform` so
        //    the gizmo (which only reads local) renders at the actual
        //    world pose. `GlobalTransform` is updated by Bevy's
        //    `TransformSystems::Propagate` in `PostUpdate`. Running
        //    sync in `Update` means we read *last frame's* GT; for an
        //    entity that was respawned this frame (because a drag
        //    commit triggered a record-driven rebuild), GT is still
        //    the spawn-time default (Identity), and the bake puts the
        //    prim at world origin instead of where it actually lives.
        //    Running sync after `TransformSystems::Propagate` reads
        //    this frame's GT, so freshly-spawned entities get baked
        //    against their real world pose.
        //
        // 2. `manage_gizmo_drag` already runs in `PostUpdate` to read
        //    the gizmo crate's previous-frame `is_active` flag.
        //    Keeping the two together lets us order them explicitly
        //    (`sync` before `drag`) so a freshly-attached gizmo is
        //    visible to the drag system on the very same frame.
        app.add_systems(
            PostUpdate,
            (
                sync_gizmo_selection.after(TransformSystems::Propagate),
                manage_gizmo_drag,
            )
                .chain()
                .run_if(in_state(AppState::InGame)),
        );
    }
}

/// Drag session state spanning all the frames between mouse-down and
/// mouse-release on the gizmo. Holds the identity of the entity being
/// dragged, the world-space pose it started at (so `Escape` can snap it
/// back and copy-on-drag can draw a ghost of the origin), whether the
/// drag is in copy-mode (Shift held at drag start), whether the user has
/// aborted this drag with `Escape`, and which editor owns the writeback.
#[derive(Default)]
struct DragState {
    active_entity: Option<Entity>,
    original_world_tf: Transform,
    is_copy: bool,
    aborted: bool,
    target: ActiveTarget,
}

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
fn sync_gizmo_selection(
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
fn manage_gizmo_drag(
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

/// Commit a finished drag against the room record. Handles the placement
/// vs prim split and the copy-on-drag clone path. Returns `true` when
/// the record was actually mutated — the caller is responsible for
/// flagging the resource as changed (`set_changed()` is on `ResMut`,
/// not on the inner type, so it has to live at the system boundary).
#[allow(clippy::type_complexity)]
fn commit_room_drag(
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
fn commit_avatar_drag(
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
