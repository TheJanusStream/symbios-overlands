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

mod commit;
mod drag;
mod sync;

use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::egui;
use transform_gizmo_bevy::GizmoOrientation;

use crate::state::AppState;
use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::RoomEditorState;

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
        app.add_systems(
            PostUpdate,
            (
                sync::sync_gizmo_selection.after(TransformSystems::Propagate),
                drag::manage_gizmo_drag,
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
pub(crate) struct DragState {
    pub(crate) active_entity: Option<Entity>,
    pub(crate) original_world_tf: Transform,
    pub(crate) is_copy: bool,
    pub(crate) aborted: bool,
    pub(crate) target: ActiveTarget,
}
