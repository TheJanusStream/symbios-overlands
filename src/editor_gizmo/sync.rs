//! Per-frame `GizmoTarget` attachment / detachment plumbing. Computes
//! which entity should carry the gizmo this frame (a placement, a room
//! prim — closest to the camera if multiple instances exist — or an
//! avatar visual prim) and toggles `GizmoTarget` accordingly. The
//! world-space-detach trick (bake `GlobalTransform` into local
//! `Transform`, drop `ChildOf`) lives here so the gizmo (which only
//! reads local `Transform`) renders at the entity's actual world pose.

use bevy::ecs::hierarchy::ChildOf;
use bevy::prelude::*;
use transform_gizmo_bevy::{EnumSet, GizmoMode, GizmoOptions, GizmoOrientation, GizmoTarget};

use crate::pds::Placement;
use crate::pds::generator::BlobShape;
use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{AvatarVisualPrim, PlacementMarker, PrimMarker};

use super::blob::{BlobEditContext, proxy::BlobElementProxy};
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
    panels: Res<crate::ui::toolbar::UiPanels>,
    room_state: Res<RoomEditorState>,
    avatar_state: Res<AvatarEditorState>,
    blob_ctx: Res<BlobEditContext>,
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
    proxy_query: Query<(
        Entity,
        &BlobElementProxy,
        &GlobalTransform,
        &Transform,
        Has<GizmoTarget>,
        Has<GizmoDetachedPrim>,
        Option<&ChildOf>,
    )>,
    // Bundled to stay under Bevy's 16-parameter ceiling.
    (detached_query, global_tf): (Query<&GizmoDetachedPrim>, Query<&GlobalTransform>),
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    // Any entity still carrying gizmo state a deselect would need to tear down.
    gizmoed: Query<(), Or<(With<GizmoTarget>, With<GizmoDetachedPrim>)>>,
    // Live gizmo flags, for the mid-drag resolution freeze (#822).
    active_gizmos: Query<&GizmoTarget>,
    // The live record, for variant-aware placement gizmo modes (#827).
    room_record: Option<Res<crate::state::LiveRoomRecord>>,
) {
    // No `is_changed()` guard. The earlier optimization missed the case
    // where a drag commit flips only the *record's* change tick (the
    // commit path doesn't touch the editor state), so on the next
    // frame's `rebuild_local_visuals` the freshly-spawned entity has no
    // gizmo and the editor's tick is unchanged → sync would skip and
    // the gizmo would never come back. Running every frame keeps the
    // gizmo tracking the selection through every respawn; the cost is a
    // linear pass over the placement/prim queries, which scales with the
    // room's population (every `PrimMarker` node — not a fixed small set
    // when a room carries a dense scatter).

    // Per-frame: push the current orientation + snap preferences into
    // the gizmo's global config. Cheap to set unconditionally —
    // `GizmoOptions` change-detects on field write inside
    // `transform-gizmo-bevy`. Snap increments come from the same
    // resource the World/Local toggle edits (#827); the upstream angle
    // option is radians.
    gizmo_options.gizmo_orientation = frame_pref.orientation;
    gizmo_options.snapping = frame_pref.snap;
    gizmo_options.snap_distance = frame_pref.snap_distance;
    gizmo_options.snap_angle = frame_pref.snap_angle_deg.to_radians();
    gizmo_options.snap_scale = frame_pref.snap_scale;

    let mut active = determine_active_target(&room_state, &avatar_state);
    // The room gizmo exists only while the World-editor window is open
    // (#702) — a selection may survive the window closing (so reopening
    // restores it), but the gizmo itself detaches. The tab gates below
    // (Region Assets → prims, Placements → placements) already restrict
    // WHICH room selection can carry it.
    if active == ActiveTarget::Room && !panels.world_editor {
        active = ActiveTarget::None;
    }

    // Idle fast path (#640): nothing selected AND nothing still carrying gizmo
    // state means every loop below is a full no-op pass over the room's
    // population (`prim_query` scales with a dense scatter). The transition
    // frame that clears a selection still has `active == None` but non-empty
    // `gizmoed` (the previously-selected entity keeps its `GizmoTarget` /
    // `GizmoDetachedPrim` until this system releases it), so the guard is false
    // there and the release/reparent path runs exactly as before.
    if active == ActiveTarget::None && gizmoed.is_empty() {
        return;
    }

    let cam_pos = camera_query
        .single()
        .map(|t| t.translation())
        .unwrap_or(Vec3::ZERO);

    // --- Blob element proxy (#705) -------------------------------------------
    // A selected BlobGroup element steals the gizmo from the node itself:
    // the whole-prim targets below are suppressed and the proxy entity
    // (spawned by `blob::proxy::reconcile_blob_proxies` earlier this
    // frame) becomes the sole target. Shape rides along to pick the mode
    // set — rotating a sphere is meaningless, so spheres only expose
    // translate + uniform scale.
    let target_proxy: Option<(Entity, BlobShape)> =
        match (blob_ctx.active.as_ref(), blob_ctx.selected_element) {
            (Some(edit), Some(sel)) => edit.elements().get(sel).and_then(|element| {
                proxy_query.iter().find_map(|(entity, proxy, ..)| {
                    (proxy.blob_entity == edit.blob_entity && proxy.index == sel)
                        .then_some((entity, element.shape))
                })
            }),
            _ => None,
        };

    // Element editing always drives the gizmo in the element's LOCAL frame,
    // overriding the World/Local toggle (#708). The toggle governs the
    // whole-prim and placement gizmos; for a per-element edit, local is both
    // correct and the useful behaviour:
    //
    // * SCALE — `transform-gizmo`'s Global-orientation scale is lossy for a
    //   rotated target: it derives the new size from the column lengths of
    //   `diag(world_scale) · (R · diag(scale))` — a sheared matrix — and
    //   keeps the old rotation, so a world-axis stretch of a rotated
    //   ellipsoid comes out wrong (a 45°-rotated sphere stretched on world-X
    //   collapses to a symmetric disc). The Local path is a clean per-axis
    //   multiply along the element's own axes, which is exactly what
    //   sculpting an element's proportions wants. (A true world-axis stretch
    //   would have to re-orient the element — rarely the intent.)
    // * TRANSLATE / ROTATE — local is coherent for positioning/orienting a
    //   mass inside its blob, and for an *unrotated* element local ≡ world,
    //   so nothing changes in the common case.
    if target_proxy.is_some() {
        gizmo_options.gizmo_orientation = GizmoOrientation::Local;
    }

    // Mid-drag resolution freeze (#822): while any gizmo drag is in
    // flight, the target must not be re-resolved. Without this, the
    // camera-proximity scan below can re-rank instances mid-gesture (the
    // dragged instance moves past a sibling, or scroll-zoom shifts the
    // camera) — `attach_or_release_prim` would then strip `GizmoTarget`
    // from the dragged entity, which the drag system reads as a falling
    // edge and commits the drag mid-air. `is_active` is written in the
    // gizmo crate's `Last` schedule, so it reads one frame stale — the
    // release frame therefore runs one extra frozen frame before normal
    // resolution resumes, which is harmless. Selection can't change
    // mid-drag (scene picks are drag-suppressed and the mouse is held on
    // a handle), so freezing attach/release entirely is safe.
    if active_gizmos.iter().any(|t| t.is_active()) {
        return;
    }

    // --- Resolve which prim entity (if any) should carry the gizmo ----------
    // Room prim: the live instance of the UI-selected (generator_ref,
    // path) pair nearest the owner's last scene-click (#822 — so the
    // clicked instance hosts the gizmo, surviving the respawns a drag
    // commit triggers because the preference is a position, not an
    // entity id), falling back to camera proximity for GUI-originated
    // selections. Only when the Room editor is active and the Generators
    // tab is showing.
    let target_room_prim = if target_proxy.is_some() {
        None
    } else if active == ActiveTarget::Room && room_state.selected_tab == EditorTab::Generators {
        match (
            room_state.selected_generator.as_ref(),
            room_state.selected_prim_path.as_ref(),
        ) {
            (Some(generator_ref), Some(path)) => {
                let reference_pos = room_state
                    .preferred_pick
                    .as_ref()
                    .filter(|p| p.generator_ref == *generator_ref && p.path == *path)
                    .map(|p| p.pos)
                    .unwrap_or(cam_pos);
                let mut best_entity = None;
                let mut best_dist_sq = f32::MAX;
                for (entity, marker, tf, _, _, _) in prim_query.iter() {
                    if marker.generator_ref == *generator_ref && marker.path == *path {
                        let dist_sq = tf.translation().distance_squared(reference_pos);
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
    let target_avatar_prim = if target_proxy.is_some() {
        None
    } else if active == ActiveTarget::Avatar {
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
    // can't scale (their generator's construct tree owns shape) and get
    // a variant-aware set — Scatter is translate-only (#827, see
    // `placement_modes`). Prims can translate / rotate / scale except
    // for blueprint roots, which are locked to rotate + scale —
    // translating the root would just shift the whole subtree relative
    // to its own origin. Avatar visuals follow the same root rule; their
    // root translation lives in the chassis (anchored by locomotion
    // physics). Blob elements get a shape-specific set (see
    // `element_modes`).
    if let Some((_, shape)) = target_proxy {
        gizmo_options.gizmo_modes = element_modes(shape);
    } else if placement_selected {
        let placement = room_record.as_ref().and_then(|record| {
            room_state
                .selected_placement
                .and_then(|idx| record.0.placements.get(idx))
        });
        gizmo_options.gizmo_modes = placement_modes(placement);
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

    // --- Blob element proxies (#705, same machinery again) -----------------
    // The proxy is a child of the blob prim entity, so the detach trick
    // bakes its world pose exactly like a nested prim's; release restores
    // it under the blob for the reconcile pass to keep in sync.
    for (entity, _proxy, gt, local, has_gizmo, is_detached, child_of) in proxy_query.iter() {
        let is_target = target_proxy.map(|(e, _)| e) == Some(entity);
        // Proxies are spawned by `reconcile_blob_proxies` in `PostUpdate`
        // *after* `TransformSystems::Propagate`, so a freshly (re)spawned
        // proxy — as happens when a drag-commit rebuilds the blob and the
        // still-selected element re-targets it the same frame — carries an
        // identity `GlobalTransform` this pass. Baking that would teleport
        // the proxy to the world origin (#706). Recompose the true world
        // pose from the blob parent's already-propagated `GlobalTransform`
        // and the proxy's local transform instead; for an already-detached
        // proxy (mid-drag, no `ChildOf`) its own GT is authoritative.
        let effective_gt = match child_of {
            Some(co) => global_tf
                .get(co.parent())
                .map(|parent_gt| parent_gt.mul_transform(*local))
                .unwrap_or(*gt),
            None => *gt,
        };
        attach_or_release_prim(
            &mut commands,
            entity,
            is_target,
            has_gizmo,
            is_detached,
            &effective_gt,
            child_of,
            &detached_query,
            &global_tf,
        );
    }
}

/// Mode set for a placement selection (#827): only the gestures the
/// commit actually keeps, so no drag silently evaporates.
///
/// * `Absolute` / `Grid` — translate + rotate (both written verbatim;
///   scale is owned by the generator tree, as before).
/// * `Scatter` — translate ONLY (user decision, 2026-07-16): the GUI
///   shows bounds, not a transform, and a Rect's angle is the Bounds
///   "Rotation (deg)" slider — a gizmo rotation was discarded (Circle)
///   or half-kept (Rect yaw), reading as a bug.
/// * `Unknown` — nothing: the commit refuses to write into a schema it
///   doesn't know, so no handle should promise otherwise.
/// * `None` (record momentarily unavailable) — the pre-#827 set, so a
///   transient lookup miss doesn't strip handles mid-session.
fn placement_modes(placement: Option<&Placement>) -> EnumSet<GizmoMode> {
    let mut modes = EnumSet::new();
    match placement {
        Some(Placement::Scatter { .. }) => {
            modes.insert_all(GizmoMode::all_translate());
        }
        Some(Placement::Unknown) => {}
        Some(Placement::Absolute { .. }) | Some(Placement::Grid { .. }) | None => {
            modes.insert_all(GizmoMode::all_translate());
            modes.insert_all(GizmoMode::all_rotate());
        }
    }
    modes
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

/// Mode set for a blob element (#705). Every shape translates. A sphere
/// gets the full scale triad (uniform *and* per-axis): dragging one axis
/// stretches it, and the commit promotes it to an ellipsoid so per-axis
/// size works (#707). It still gets no rotation — a sphere's orientation is
/// meaningless to the SDF, and once promoted the ellipsoid picks up rotate
/// on its next selection. `Unknown` (forward-compat) stays uniform-only so
/// a gizmo drag can't reshape a construct an older client authored.
/// Capsules and ellipsoids get the full triad.
fn element_modes(shape: BlobShape) -> EnumSet<GizmoMode> {
    let mut modes = EnumSet::new();
    modes.insert_all(GizmoMode::all_translate());
    match shape {
        BlobShape::Unknown => {
            modes.insert(GizmoMode::ScaleUniform);
        }
        BlobShape::Sphere => {
            modes.insert_all(GizmoMode::all_scale());
        }
        BlobShape::Capsule
        | BlobShape::Ellipsoid
        | BlobShape::Box
        | BlobShape::Cylinder
        | BlobShape::Torus
        | BlobShape::Cone => {
            modes.insert_all(GizmoMode::all_rotate());
            modes.insert_all(GizmoMode::all_scale());
        }
    }
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

#[cfg(test)]
mod repro_tests {
    use super::*;
    use crate::editor_gizmo::blob::proxy::{BlobEditAssets, reconcile_blob_proxies};
    use crate::editor_gizmo::blob::{ActiveBlobEdit, BlobEditContext, BlobEditKey};
    use crate::pds::generator::{BlobElement, GeneratorKind};
    use crate::pds::types::{Fp, Fp3, Fp4};
    use bevy::MinimalPlugins;
    use bevy::asset::AssetPlugin;
    use bevy::transform::TransformPlugin;

    fn sphere_kind(pos: [f32; 3]) -> GeneratorKind {
        let mut kind = GeneratorKind::default_primitive_for_tag("BlobGroup").unwrap();
        if let GeneratorKind::BlobGroup { elements, .. } = &mut kind {
            *elements = vec![BlobElement {
                shape: BlobShape::Sphere,
                position: Fp3(pos),
                rotation: Fp4([0.0, 0.0, 0.0, 1.0]),
                radii: Fp3([0.3, 0.3, 0.3]),
                subtract: false,
                blend: Fp(0.1),
            }];
        }
        kind
    }

    fn panels() -> crate::ui::toolbar::UiPanels {
        crate::ui::toolbar::UiPanels {
            chat: false,
            people: false,
            avatar: true,
            world_editor: true,
            inventory: false,
            catalogue: false,
            diagnostics: false,
            controls: false,
            controls_seen: true,
        }
    }

    /// The regression this whole issue is about (#706): dragging a blob
    /// element and releasing must land it exactly where the gizmo left it,
    /// even when the blob node lives deep under a translated/rotated
    /// hierarchy. Runs the real `reconcile` + `sync` systems with real
    /// transform propagation, then reproduces the drag-release commit math
    /// and asserts the element's new local position round-trips back to the
    /// dragged world pose through the parent chain.
    #[test]
    fn element_drag_release_round_trips_under_a_transform_hierarchy() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<GizmoOptions>()
            .init_resource::<GizmoFramePref>()
            .init_resource::<RoomEditorState>()
            .init_resource::<BlobEditContext>()
            .init_resource::<BlobEditAssets>()
            .insert_resource(panels());
        // Avatar selection so `determine_active_target` == Avatar and the
        // idle fast-path doesn't bail before the proxy loop runs.
        let mut avatar_state = AvatarEditorState::default();
        avatar_state.selected_prim_path = Some(vec![]);
        app.insert_resource(avatar_state);

        app.add_systems(Update, reconcile_blob_proxies);
        app.add_systems(
            PostUpdate,
            sync_gizmo_selection.after(bevy::transform::TransformSystems::Propagate),
        );

        // Hierarchy: anchor (translate + yaw) -> blob node (its own offset).
        let anchor = app
            .world_mut()
            .spawn(Transform::from_xyz(10.0, 5.0, 20.0).with_rotation(Quat::from_rotation_y(0.9)))
            .id();
        let blob = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 2.0, 0.0),
                ChildOf(anchor),
                AvatarVisualPrim { path: vec![] },
            ))
            .id();

        let element_local = [1.0f32, 0.0, 0.0];
        {
            let mut ctx = app.world_mut().resource_mut::<BlobEditContext>();
            ctx.active = Some(ActiveBlobEdit {
                key: BlobEditKey {
                    target: ActiveTarget::Avatar,
                    generator_ref: None,
                    path: vec![],
                },
                kind: sphere_kind(element_local),
                blob_entity: blob,
            });
        }

        // Frame 1: reconcile spawns the proxy (selected_element = None so it
        // isn't the target yet). Frame 2: propagate its GlobalTransform.
        app.update();
        app.update();

        // The proxy exists, is a child of the blob, and sits at the element's
        // world position through the whole chain.
        let proxy = app
            .world_mut()
            .query_filtered::<Entity, With<BlobElementProxy>>()
            .single(app.world())
            .expect("proxy spawned");
        let blob_gt = *app.world().get::<GlobalTransform>(blob).unwrap();
        let proxy_world_at_rest = blob_gt * Vec3::from_array(element_local);
        let proxy_gt = app.world().get::<GlobalTransform>(proxy).unwrap();
        assert!(
            proxy_gt.translation().distance(proxy_world_at_rest) < 1e-4,
            "proxy not placed at element world pos: {:?} vs {:?}",
            proxy_gt.translation(),
            proxy_world_at_rest
        );

        // Select the element → sync attaches the gizmo (detach). Frame to run
        // sync, frame to apply its commands + re-propagate.
        app.world_mut()
            .resource_mut::<BlobEditContext>()
            .selected_element = Some(0);
        app.update();
        app.update();

        // THE CRUX: the attached proxy must be detached WITH the blob entity
        // recorded as its original parent. If this is missing, the commit
        // treats the proxy's world transform as a blob-local one and the
        // element jumps by the parent offset — the reported bug.
        let detached = app
            .world()
            .get::<GizmoDetachedPrim>(proxy)
            .expect("proxy must carry GizmoDetachedPrim after attach");
        assert_eq!(
            detached.original_parent, blob,
            "original_parent must be the blob entity"
        );
        assert!(
            app.world().get::<ChildOf>(proxy).is_none(),
            "detached proxy must have no ChildOf"
        );

        // Simulate a drag: move the proxy in WORLD space (it's a detached
        // root now, so its Transform is world).
        let dragged_world = proxy_world_at_rest + Vec3::new(0.0, 0.0, 3.0);
        app.world_mut()
            .get_mut::<Transform>(proxy)
            .unwrap()
            .translation = dragged_world;
        app.update(); // propagate the dragged world pose

        // Reproduce the drag-release commit math (mirrors `drag.rs`'s
        // `resolve_committed_local` → `apply_local_to_element`).
        let proxy_tf = *app.world().get::<Transform>(proxy).unwrap();
        let blob_gt_now = *app.world().get::<GlobalTransform>(blob).unwrap();
        let committed_local = GlobalTransform::from(proxy_tf).reparented_to(&blob_gt_now);

        // The element's committed local position, rendered back through the
        // blob's world transform, must land exactly at the dragged world pose.
        let rebuilt_world = blob_gt_now * committed_local.translation;
        assert!(
            rebuilt_world.distance(dragged_world) < 1e-3,
            "element jumped on release: committed local {:?} rebuilds to {:?}, expected {:?}",
            committed_local.translation,
            rebuilt_world,
            dragged_world
        );
    }

    /// The real bug (#706): a proxy spawned by `reconcile_blob_proxies`
    /// (which runs in `PostUpdate` AFTER `TransformSystems::Propagate`) and
    /// targeted the SAME frame — exactly what happens when a drag-commit
    /// rebuilds the blob and respawns the proxy — is baked by `sync` while
    /// its `GlobalTransform` is still the spawn-time identity, teleporting
    /// it to the world origin instead of the element's real world pose.
    ///
    /// This mirrors the real plugin schedule (reconcile → sync, both after
    /// Propagate). The fix must make the attach bake the proxy against the
    /// blob's world transform, independent of when the proxy itself was
    /// last propagated.
    #[test]
    fn fresh_proxy_targeted_same_frame_bakes_at_real_world_not_origin() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<GizmoOptions>()
            .init_resource::<GizmoFramePref>()
            .init_resource::<RoomEditorState>()
            .init_resource::<BlobEditContext>()
            .init_resource::<BlobEditAssets>()
            .insert_resource(panels());
        let mut avatar_state = AvatarEditorState::default();
        avatar_state.selected_prim_path = Some(vec![]);
        app.insert_resource(avatar_state);

        // Real plugin ordering: reconcile then sync, both AFTER Propagate.
        app.add_systems(
            PostUpdate,
            (reconcile_blob_proxies, sync_gizmo_selection)
                .chain()
                .after(bevy::transform::TransformSystems::Propagate),
        );

        let anchor = app
            .world_mut()
            .spawn(Transform::from_xyz(10.0, 5.0, 20.0))
            .id();
        let blob = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 2.0, 0.0),
                ChildOf(anchor),
                AvatarVisualPrim { path: vec![] },
            ))
            .id();
        app.update(); // propagate the blob's own GlobalTransform

        let element_local = [1.0f32, 0.0, 0.0];
        {
            let mut ctx = app.world_mut().resource_mut::<BlobEditContext>();
            ctx.active = Some(ActiveBlobEdit {
                key: BlobEditKey {
                    target: ActiveTarget::Avatar,
                    generator_ref: None,
                    path: vec![],
                },
                kind: sphere_kind(element_local),
                blob_entity: blob,
            });
            // Element already selected: the proxy is spawned AND targeted in
            // the same PostUpdate — the post-commit-rebuild scenario.
            ctx.selected_element = Some(0);
        }

        app.update(); // reconcile spawns proxy; sync attaches it same frame
        app.update(); // propagate whatever pose the attach baked

        let proxy = app
            .world_mut()
            .query_filtered::<Entity, With<BlobElementProxy>>()
            .single(app.world())
            .expect("proxy spawned");
        let blob_gt = *app.world().get::<GlobalTransform>(blob).unwrap();
        let expected_world = blob_gt * Vec3::from_array(element_local);
        let proxy_world = app
            .world()
            .get::<GlobalTransform>(proxy)
            .unwrap()
            .translation();
        assert!(
            proxy_world.distance(expected_world) < 1e-3,
            "fresh proxy baked at {:?}, expected {:?} (origin bake = the #706 jump)",
            proxy_world,
            expected_world
        );
    }

    /// #708: element editing must force the gizmo into the element's LOCAL
    /// frame regardless of the World/Local toggle (the gizmo's Global-frame
    /// scale is lossy for a rotated element). Selecting an element flips the
    /// orientation to Local even when the user picked World; deselecting
    /// restores their preference for the whole-prim gizmo.
    #[test]
    fn element_editing_forces_local_gizmo_orientation() {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
            .init_asset::<Mesh>()
            .init_asset::<StandardMaterial>()
            .init_resource::<GizmoOptions>()
            .init_resource::<GizmoFramePref>()
            .init_resource::<RoomEditorState>()
            .init_resource::<BlobEditContext>()
            .init_resource::<BlobEditAssets>()
            .insert_resource(panels());
        let mut avatar_state = AvatarEditorState::default();
        avatar_state.selected_prim_path = Some(vec![]);
        app.insert_resource(avatar_state);
        // The user has the World (Global) frame selected.
        app.world_mut().resource_mut::<GizmoFramePref>().orientation = GizmoOrientation::Global;

        app.add_systems(Update, reconcile_blob_proxies);
        app.add_systems(
            PostUpdate,
            sync_gizmo_selection.after(bevy::transform::TransformSystems::Propagate),
        );

        let blob = app
            .world_mut()
            .spawn((
                Transform::from_xyz(0.0, 1.0, 0.0).with_rotation(Quat::from_rotation_y(0.9)),
                AvatarVisualPrim { path: vec![] },
            ))
            .id();
        {
            let mut ctx = app.world_mut().resource_mut::<BlobEditContext>();
            ctx.active = Some(ActiveBlobEdit {
                key: BlobEditKey {
                    target: ActiveTarget::Avatar,
                    generator_ref: None,
                    path: vec![],
                },
                kind: sphere_kind([0.5, 0.0, 0.0]),
                blob_entity: blob,
            });
        }
        // Spawn + propagate the proxy, then select the element.
        app.update();
        app.update();
        app.world_mut()
            .resource_mut::<BlobEditContext>()
            .selected_element = Some(0);
        app.update();
        assert_eq!(
            app.world().resource::<GizmoOptions>().gizmo_orientation,
            GizmoOrientation::Local,
            "element scaling must be forced to the local frame",
        );

        // Deselect the element → the whole-prim gizmo takes over and the
        // user's World preference is honoured again.
        app.world_mut()
            .resource_mut::<BlobEditContext>()
            .selected_element = None;
        app.update();
        assert_eq!(
            app.world().resource::<GizmoOptions>().gizmo_orientation,
            GizmoOrientation::Global,
            "the World/Local toggle must still govern the whole-prim gizmo",
        );
    }
}

#[cfg(test)]
mod mode_tests {
    use super::*;
    use crate::pds::{BiomeFilter, Fp, Fp3, ScatterBounds, TransformData};

    fn scatter() -> Placement {
        Placement::Scatter {
            generator_ref: "g".into(),
            bounds: ScatterBounds::default(),
            count: 4,
            local_seed: 1,
            biome_filter: BiomeFilter::default(),
            snap_to_terrain: true,
            random_yaw: true,
            avoid_urban: false,
        }
    }

    fn absolute() -> Placement {
        Placement::Absolute {
            generator_ref: "g".into(),
            transform: TransformData::default(),
            snap_to_terrain: true,
            avoid_water: false,
            avoid_water_clearance: Fp(0.0),
        }
    }

    fn grid() -> Placement {
        Placement::Grid {
            generator_ref: "g".into(),
            transform: TransformData::default(),
            counts: [2, 1, 2],
            gaps: Fp3([2.0, 2.0, 2.0]),
            snap_to_terrain: true,
            random_yaw: false,
        }
    }

    /// #827 (user decision): scatter placements are translate-only — a
    /// rotation gesture had no honest commit (Circle discarded it, Rect
    /// half-kept it and clobbered the authored slider angle).
    #[test]
    fn scatter_gets_translate_only_handles() {
        let modes = placement_modes(Some(&scatter()));
        for m in GizmoMode::all_translate() {
            assert!(modes.contains(m), "missing translate mode {m:?}");
        }
        for m in GizmoMode::all_rotate() {
            assert!(!modes.contains(m), "scatter must not rotate: {m:?}");
        }
        for m in GizmoMode::all_scale() {
            assert!(!modes.contains(m), "placements never scale: {m:?}");
        }
    }

    #[test]
    fn absolute_and_grid_keep_translate_plus_rotate() {
        for placement in [absolute(), grid()] {
            let modes = placement_modes(Some(&placement));
            for m in GizmoMode::all_translate()
                .iter()
                .chain(GizmoMode::all_rotate())
            {
                assert!(modes.contains(m), "{placement:?} missing {m:?}");
            }
            for m in GizmoMode::all_scale() {
                assert!(!modes.contains(m));
            }
        }
    }

    #[test]
    fn unknown_gets_no_handles_and_none_falls_back() {
        assert!(placement_modes(Some(&Placement::Unknown)).is_empty());
        // Transient record-lookup miss keeps the pre-#827 behaviour.
        let fallback = placement_modes(None);
        assert!(fallback.contains(GizmoMode::TranslateX));
        assert!(fallback.contains(GizmoMode::RotateY));
    }
}
