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
//!
//! ## Sub-module map
//!
//! * [`sync`] — per-frame target-resolution + `GizmoTarget`
//!   attach/detach + the world-space-detach trick.
//! * [`drag`] — drag-session state machine (rising / active / falling
//!   edges), Escape abort, copy-on-drag ghost rendering.
//! * [`commit`] — drag-end writeback into [`crate::pds::RoomRecord`] /
//!   [`crate::state::LiveAvatarRecord`] (placement vs prim split,
//!   copy-on-drag clone, path-walked transform overwrite).
//! * [`blob`] — in-scene BlobGroup element editing (#705): wireframe
//!   surface swap, red/green per-element proxies, per-element gizmo
//!   targeting and the element writeback.

mod blob;
mod commit;
mod drag;
mod sync;

pub use blob::BlobEditContext;

use bevy::ecs::hierarchy::ChildOf;
use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings};
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContexts, egui};
use transform_gizmo_bevy::{GizmoOrientation, GizmoTarget};

use crate::state::AppState;
use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{PlacementMarker, PrimMarker};

/// Owner-facing toggle for how the gizmo's drag axes are oriented.
///
/// `Global` (the default and the v1 behaviour): handles align to world
/// XYZ. Translations, rotations and scales operate along world axes.
///
/// `Local`: handles align to the target entity's `Transform.rotation`.
/// Because prims are detached from their parent into world space when
/// the gizmo attaches (see [`sync`]), that rotation is the *accumulated*
/// product of every parent rotation along the path from the blueprint
/// root — exactly what the owner expects when arranging children of a
/// tilted construct.
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
pub(crate) struct GizmoDetachedPrim {
    pub(crate) original_parent: Entity,
}

/// Which editor currently owns the gizmo. Computed each frame from the
/// two editor states; not stored as a resource because both states are
/// already authoritative.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ActiveTarget {
    #[default]
    None,
    Room,
    Avatar,
}

pub(crate) fn determine_active_target(
    room: &RoomEditorState,
    avatar: &AvatarEditorState,
) -> ActiveTarget {
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
        app.init_resource::<blob::BlobEditContext>()
            .init_resource::<blob::proxy::BlobEditAssets>()
            .add_systems(
                PostUpdate,
                (
                    // Blob-edit context first: sync reads it to suppress the
                    // whole-prim gizmo while an element is selected, and the
                    // proxies must exist before sync can target one.
                    blob::resolve_blob_edit.after(TransformSystems::Propagate),
                    blob::proxy::reconcile_blob_proxies,
                    sync::sync_gizmo_selection,
                    drag::manage_gizmo_drag,
                    blob::wireframe::swap_blob_wireframe,
                    blob::preview::blob_drag_preview,
                )
                    .chain()
                    .run_if(in_state(AppState::InGame)),
            )
            // A left-click into the open 3D scene either PICKS the object under
            // the cursor (#702 — Region Assets / Placements tab, world editor
            // open) or clears the selection when nothing selectable was hit.
            // Runs in `Update` (the egui-pointer guard reads the same frame's
            // context, exactly as `ui::inventory::drop` does) and is gated so a
            // click that lands on a gizmo handle never repicks mid-drag.
            .add_systems(
                Update,
                pick_on_scene_click.run_if(in_state(AppState::InGame)),
            )
            .add_systems(OnExit(AppState::InGame), blob::cleanup_blob_edit);
    }
}

/// Scene click-select (#702): while the World-editor window is open on the
/// Region Assets or Placements tab (and the signed-in user owns the room),
/// a left-click into the 3D viewport raycasts the scene's meshes and
/// selects what it hits — the exact sub-part of an asset on Region Assets,
/// the owning placement on Placements — exactly as if the matching GUI row
/// had been clicked. Hitting nothing selectable (sky, terrain, water,
/// avatars) clears the selection, which makes the gizmo vanish via [`sync`].
/// On any other tab, or with the editor closed, a scene click just clears —
/// the pre-#702 behaviour.
///
/// Mesh raycast, not physics: most catalogue props carry no collider, so
/// `SpatialQuery` would see through them; `MeshRayCast` hits anything
/// rendered.
///
/// **Drag safety.** Picking is suppressed whenever any [`GizmoTarget`]
/// reports `is_focused()` (pointer hovering a handle) or `is_active()` (a
/// drag in progress). `transform-gizmo-bevy` writes both flags in its
/// `Last`-schedule update, so on the mouse-down frame they already reflect
/// the prior frame's hover — and the owner always hovers a handle before
/// pressing — so a click that *starts* a drag is caught here and leaves
/// the selection (and the drag) untouched.
#[allow(clippy::too_many_arguments)]
fn pick_on_scene_click(
    mut contexts: EguiContexts,
    mouse: Res<ButtonInput<MouseButton>>,
    gizmo_targets: Query<&GizmoTarget>,
    panels: Res<crate::ui::toolbar::UiPanels>,
    session: Option<Res<bevy_symbios_multiuser::auth::AtprotoSession>>,
    room_did: Option<Res<crate::state::CurrentRoomDid>>,
    windows: Query<&Window, With<PrimaryWindow>>,
    cameras: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut raycast: MeshRayCast,
    prim_markers: Query<&PrimMarker>,
    placement_markers: Query<&PlacementMarker>,
    parents: Query<&ChildOf>,
    mut room_state: ResMut<RoomEditorState>,
    mut avatar_state: ResMut<AvatarEditorState>,
    (mut blob_ctx, blob_proxies): (
        ResMut<blob::BlobEditContext>,
        Query<&blob::proxy::BlobElementProxy>,
    ),
) {
    // Left button only — the orbit/pan camera owns Right/Middle, so this
    // can never fight a camera gesture.
    if !mouse.just_pressed(MouseButton::Left) {
        return;
    }
    let Ok(ctx) = contexts.ctx_mut() else {
        return;
    };
    // Clicks on the toolbar or any editor window are UI interactions, not
    // a "click into the world" — leave the selection alone.
    if ctx.is_pointer_over_area() {
        return;
    }
    // The click is starting (or continuing) a gizmo interaction — keep the
    // selection so the drag can run.
    if gizmo_targets
        .iter()
        .any(|t| t.is_focused() || t.is_active())
    {
        return;
    }

    // Blob element proxies take pick precedence (#705): while a BlobGroup
    // is under edit, clicking one of its red/green proxy meshes selects
    // that element for the gizmo — in whichever editor owns the session,
    // so this runs before the avatar-clear and the room-editor gates. The
    // proxy carries its own mesh, so the raycast hit *is* the proxy
    // entity (no ancestor walk needed).
    if blob_ctx.active.is_some()
        && let Some(hit) = scene_hit_under_cursor(&windows, &cameras, &mut raycast)
        && let Ok(proxy) = blob_proxies.get(hit)
    {
        blob_ctx.selected_element = Some(proxy.index);
        return;
    }

    // A scene click always takes the avatar editor's selection away —
    // same cross-editor mutex direction as before #702.
    if avatar_state.has_visuals_selection() {
        avatar_state.clear_visuals_selection();
    }

    // Picking needs the editor open on a pickable tab, in a room the user
    // owns (the same gate the editor window itself renders under). In
    // every other situation, keep the old clear-on-click behaviour.
    let owns_room = matches!(
        (session.as_deref(), room_did.as_deref()),
        (Some(s), Some(r)) if s.did == r.0
    );
    let pickable_tab = matches!(
        room_state.selected_tab,
        EditorTab::Generators | EditorTab::Placements
    );
    if !(panels.world_editor && owns_room && pickable_tab) {
        if room_state.has_selection() {
            room_state.clear_selection();
        }
        return;
    }

    // Cursor → world ray → nearest rendered mesh.
    let hit_entity = scene_hit_under_cursor(&windows, &cameras, &mut raycast);

    // Walk from the hit mesh up the hierarchy: the FIRST `PrimMarker` is
    // the exact (deepest) sub-part under the cursor; the `PlacementMarker`
    // sits on the anchor above it. Non-selectable scenery (terrain, water,
    // sky, avatars) carries neither and falls through to a clear.
    let mut picked_prim: Option<PrimMarker> = None;
    let mut picked_placement: Option<usize> = None;
    let mut cursor_entity = hit_entity;
    while let Some(entity) = cursor_entity {
        if picked_prim.is_none()
            && let Ok(marker) = prim_markers.get(entity)
        {
            picked_prim = Some(marker.clone());
        }
        if let Ok(marker) = placement_markers.get(entity) {
            picked_placement = Some(marker.0);
            break; // The anchor is the top of a placement's subtree.
        }
        cursor_entity = parents.get(entity).ok().map(ChildOf::parent);
    }

    match room_state.selected_tab {
        EditorTab::Generators => {
            if let Some(marker) = picked_prim {
                room_state.selected_placement = None;
                room_state.selected_generator = Some(marker.generator_ref.clone());
                room_state.selected_prim_path = Some(marker.path.clone());
                // Mirror a tree-row click so the GUI highlights the node.
                room_state
                    .tree_view_state
                    .set_selected(vec![crate::ui::room::GenNodeId::child(
                        marker.generator_ref,
                        marker.path,
                    )]);
            } else {
                room_state.clear_selection();
            }
        }
        EditorTab::Placements => {
            if let Some(index) = picked_placement {
                room_state.selected_generator = None;
                room_state.selected_prim_path = None;
                room_state.tree_view_state.set_selected(Vec::new());
                room_state.selected_placement = Some(index);
            } else {
                room_state.clear_selection();
            }
        }
        // Unreachable: `pickable_tab` gated above.
        _ => {}
    }
}

/// Cursor position → world ray → nearest rendered mesh under it. Shared
/// by the proxy-precedence branch and the ordinary pick path of
/// [`pick_on_scene_click`] (the two run in sequence, so a blob-edit miss
/// costs one extra raycast per click — clicks, not frames).
fn scene_hit_under_cursor(
    windows: &Query<&Window, With<PrimaryWindow>>,
    cameras: &Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    raycast: &mut MeshRayCast,
) -> Option<Entity> {
    let cursor = windows.single().ok()?.cursor_position()?;
    let (camera, cam_tf) = cameras.single().ok()?;
    let ray = camera.viewport_to_world(cam_tf, cursor).ok()?;
    raycast
        .cast_ray(ray, &MeshRayCastSettings::default())
        .first()
        .map(|(entity, _hit)| *entity)
}

/// Drag session state spanning all the frames between mouse-down and
/// mouse-release on the gizmo. Holds the identity of the entity being
/// dragged, the world-space pose it started at (so `Escape` can snap it
/// back and copy-on-drag can draw a ghost of the origin), whether the
/// drag is in copy-mode (Shift held at drag start), whether the user has
/// aborted this drag with `Escape`, and which editor owns the writeback.
#[derive(Default)]
pub(crate) struct DragState {
    pub(crate) active_entity: Option<Entity>,
    pub(crate) original_world_tf: Transform,
    pub(crate) is_copy: bool,
    pub(crate) aborted: bool,
    pub(crate) target: ActiveTarget,
    /// `Some` while the dragged entity is a blob element proxy (#705):
    /// the routing snapshot for the element writeback. Captured at the
    /// rising edge so mid-drag selection changes can't reroute it.
    pub(crate) blob: Option<blob::write::BlobDragInfo>,
}
