//! In-scene BlobGroup element editing (#705).
//!
//! While the editor selection rests on a `GeneratorKind::BlobGroup` node,
//! this module turns the evaluated surface into an edge-line wireframe and
//! spawns one translucent proxy mesh per [`BlobElement`] — green for
//! additive elements, red for carves. Clicking a proxy (or its row in the
//! GUI list) attaches the transform gizmo to it; dragging edits the
//! element's `position` / `rotation` / `radii` and commits into the owning
//! record on release, exactly one record update per gesture — the same
//! contract as the whole-prim gizmo. A throttled re-mesh runs during the
//! drag so the wireframe reshapes under the user's hand without touching
//! the record.
//!
//! ## Sub-module map
//!
//! * [`proxy`] — per-element proxy entities: shared mesh/material assets +
//!   the per-frame reconcile that keeps proxies matching the record.
//! * [`wireframe`] — triangle-edge extraction and the mesh/material swap
//!   that turns the selected blob instance into a line-list wireframe.
//!   (WebGL2 — the wasm deploy target — has no line *polygon* mode, so
//!   wireframe is modelled as real `LineList` geometry instead.)
//! * [`preview`] — throttled in-drag SDF re-mesh feeding fresh edge lines
//!   into the swapped wireframe handle.
//! * [`write`](mod@write) — element ⇄ transform mapping and the drag-end
//!   record writeback (room + avatar).
//!
//! ## Frame order
//!
//! [`resolve_blob_edit`] runs first in the editor-gizmo `PostUpdate`
//! chain: it derives [`BlobEditContext::active`] from the editor states +
//! live records each frame (there is no retained mode — deselecting the
//! node deactivates everything). `proxy::reconcile_blob_proxies` then
//! diffs proxies against the record, `sync` attaches the gizmo (proxy
//! target suppresses the whole-prim target), `drag` runs the gesture, and
//! `wireframe`/`preview` repaint the shell.

pub(super) mod preview;
pub(super) mod proxy;
pub(super) mod wireframe;
pub(super) mod write;

use bevy::prelude::*;
use transform_gizmo_bevy::GizmoTarget;

use crate::pds::generator::{BlobElement, Generator, GeneratorKind};
use crate::state::{LiveAvatarRecord, LiveRoomRecord};
use crate::ui::avatar::AvatarEditorState;
use crate::ui::room::{EditorTab, RoomEditorState};
use crate::world_builder::{AvatarVisualPrim, PrimMarker};

use super::{ActiveTarget, determine_active_target};

/// Identity of the blob node an edit session is attached to. Compared
/// across frames to reset the element selection when the user moves to a
/// different node (or the same path in a different generator).
#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) struct BlobEditKey {
    pub(crate) target: ActiveTarget,
    /// Room: the named generator's key. Avatar: `None` (single visuals root).
    pub(crate) generator_ref: Option<String>,
    /// Child-index chain from the generator/visuals root to the blob node.
    pub(crate) path: Vec<usize>,
}

/// Everything the downstream systems need about the blob node under edit,
/// recomputed from the live record every frame so GUI edits (which mutate
/// the record immediately, debouncing only the change *tick*) are
/// reflected without waiting for a rebuild.
pub(crate) struct ActiveBlobEdit {
    pub(crate) key: BlobEditKey,
    /// Clone of the node's full kind (a `GeneratorKind::BlobGroup`).
    /// Carrying the whole kind — not just the element list — lets the
    /// in-drag preview re-mesh with the node's own resolution + torture
    /// params for exact parity with the committed surface.
    pub(crate) kind: GeneratorKind,
    /// The live instance carrying the gizmo/wireframe/proxies: closest to
    /// the camera when the node is instanced by a Scatter/Grid placement —
    /// the same proximity rule the whole-prim gizmo uses.
    pub(crate) blob_entity: Entity,
}

impl ActiveBlobEdit {
    pub(crate) fn elements(&self) -> &[BlobElement] {
        match &self.kind {
            GeneratorKind::BlobGroup { elements, .. } => elements,
            _ => &[],
        }
    }
}

/// Shared state of the in-scene blob editor. `active` is derived (never
/// retained) each frame by [`resolve_blob_edit`]; `selected_element` is
/// retained input written by scene picks and the GUI element list, and
/// cleared here whenever the key changes or the index falls off the list.
#[derive(Resource, Default)]
pub struct BlobEditContext {
    pub(crate) active: Option<ActiveBlobEdit>,
    /// Which element carries the gizmo. `None` ⇒ the whole-prim gizmo
    /// behaves exactly as before this feature.
    pub selected_element: Option<usize>,
    /// One-shot: the wireframe should re-extract from the blob's real mesh
    /// (set on drag end, when an aborted or committed preview may have
    /// left stale lines in the swapped handle).
    pub(crate) wireframe_dirty: bool,
}

/// Walk a generator tree by child indices. Returns `None` when the path
/// dangles (tree reshaped since the selection was made).
pub(crate) fn node_at_path<'a>(root: &'a Generator, path: &[usize]) -> Option<&'a Generator> {
    let mut current = root;
    for &idx in path {
        current = current.children.get(idx)?;
    }
    Some(current)
}

/// Mutable counterpart of [`node_at_path`], used by the commit writeback.
pub(crate) fn node_at_path_mut<'a>(
    root: &'a mut Generator,
    path: &[usize],
) -> Option<&'a mut Generator> {
    let mut current = root;
    for &idx in path {
        current = current.children.get_mut(idx)?;
    }
    Some(current)
}

/// Derive this frame's [`BlobEditContext::active`] from the editor
/// selections and live records, reset the element selection when the
/// resolved node changes, and honour Escape as "drop back to the
/// whole-prim gizmo".
///
/// Room resolution mirrors `sync_gizmo_selection`'s gates exactly: World
/// Editor window open, Generators tab, and the closest live instance of
/// the `(generator_ref, path)` pair. Avatar resolution needs no panel
/// gate — the avatar editor clears its selection when its window closes.
#[allow(clippy::too_many_arguments)]
pub(super) fn resolve_blob_edit(
    mut ctx: ResMut<BlobEditContext>,
    panels: Res<crate::ui::toolbar::UiPanels>,
    room_state: Res<RoomEditorState>,
    avatar_state: Res<AvatarEditorState>,
    room_record: Option<Res<LiveRoomRecord>>,
    avatar_record: Option<Res<LiveAvatarRecord>>,
    prim_query: Query<(Entity, &PrimMarker, &GlobalTransform)>,
    avatar_prim_query: Query<(Entity, &AvatarVisualPrim)>,
    camera_query: Query<&GlobalTransform, With<Camera3d>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    gizmo_targets: Query<&GizmoTarget>,
) {
    let prev_key = ctx.active.take().map(|a| a.key);

    match determine_active_target(&room_state, &avatar_state) {
        ActiveTarget::Room
            if panels.world_editor && room_state.selected_tab == EditorTab::Generators =>
        {
            if let (Some(generator_ref), Some(path), Some(record)) = (
                room_state.selected_generator.as_ref(),
                room_state.selected_prim_path.as_ref(),
                room_record.as_deref(),
            ) && let Some(node) = record
                .0
                .generators
                .get(generator_ref)
                .and_then(|g| node_at_path(g, path))
                && matches!(node.kind, GeneratorKind::BlobGroup { .. })
            {
                // Closest live instance to the camera — same proximity
                // rule (and same tie-breaking) as the whole-prim gizmo.
                let cam_pos = camera_query
                    .single()
                    .map(|t| t.translation())
                    .unwrap_or(Vec3::ZERO);
                let mut best: Option<(Entity, f32)> = None;
                for (entity, marker, tf) in prim_query.iter() {
                    if marker.generator_ref == *generator_ref && marker.path == *path {
                        let dist_sq = tf.translation().distance_squared(cam_pos);
                        if best.map(|(_, d)| dist_sq < d).unwrap_or(true) {
                            best = Some((entity, dist_sq));
                        }
                    }
                }
                if let Some((blob_entity, _)) = best {
                    ctx.active = Some(ActiveBlobEdit {
                        key: BlobEditKey {
                            target: ActiveTarget::Room,
                            generator_ref: Some(generator_ref.clone()),
                            path: path.clone(),
                        },
                        kind: node.kind.clone(),
                        blob_entity,
                    });
                }
            }
        }
        ActiveTarget::Avatar => {
            if let (Some(path), Some(record)) = (
                avatar_state.selected_prim_path.as_ref(),
                avatar_record.as_deref(),
            ) && let Some(node) = node_at_path(&record.0.visuals, path)
                && matches!(node.kind, GeneratorKind::BlobGroup { .. })
            {
                // AvatarVisualPrim is attached only to the local player's
                // visuals, so a path match is unique — no proximity scan.
                let found = avatar_prim_query
                    .iter()
                    .find_map(|(entity, marker)| (marker.path == *path).then_some(entity));
                if let Some(blob_entity) = found {
                    ctx.active = Some(ActiveBlobEdit {
                        key: BlobEditKey {
                            target: ActiveTarget::Avatar,
                            generator_ref: None,
                            path: path.clone(),
                        },
                        kind: node.kind.clone(),
                        blob_entity,
                    });
                }
            }
        }
        _ => {}
    }

    // Moving to a different node (or clearing the selection) drops the
    // element selection — a retained index against a new element list
    // would gizmo-grab an arbitrary element.
    if ctx.active.as_ref().map(|a| &a.key) != prev_key.as_ref() {
        ctx.selected_element = None;
    }
    // Clamp against GUI removals that shrank the list this frame.
    if let (Some(sel), Some(active)) = (ctx.selected_element, ctx.active.as_ref())
        && sel >= active.elements().len()
    {
        ctx.selected_element = None;
    }

    // Escape (outside a drag — mid-drag Escape is the drag-abort key)
    // steps back from element editing to the whole-prim gizmo.
    if ctx.selected_element.is_some()
        && keyboard.just_pressed(KeyCode::Escape)
        && !gizmo_targets.iter().any(|t| t.is_active())
    {
        ctx.selected_element = None;
    }
}

/// `OnExit(AppState::InGame)` sweep: drop the edit session and any proxy
/// entities that survived the room/avatar teardown (a gizmo-detached proxy
/// has no `ChildOf` link, so the recursive despawn of its blob can miss it).
pub(super) fn cleanup_blob_edit(
    mut commands: Commands,
    mut ctx: ResMut<BlobEditContext>,
    proxies: Query<Entity, With<proxy::BlobElementProxy>>,
) {
    *ctx = BlobEditContext::default();
    for entity in proxies.iter() {
        commands.entity(entity).despawn();
    }
}
