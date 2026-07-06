//! Throttled live re-mesh while an element drag is in progress.
//!
//! The record is only written on drag release (one recompile, one peer
//! broadcast — the whole-prim gizmo's contract), so without this the
//! wireframe would stay frozen at the drag's starting shape. Instead,
//! every [`crate::config::ui::blob_edit::PREVIEW_INTERVAL_SECS`] the
//! dragged proxy's transform is folded into a *clone* of the node's kind
//! and re-meshed on the compute pool, and the resulting edge lines are
//! written into the wireframe's existing mesh handle. Resolution is
//! capped at [`crate::config::ui::blob_edit::PREVIEW_MAX_RESOLUTION`] so
//! the rebuild stays drag-smooth (on wasm the "pool" shares the main
//! thread); the committed mesh uses the authored resolution.
//!
//! A result that lands after the drag ended is discarded: the falling
//! edge either committed (record rebuild delivers the exact surface) or
//! aborted (`wireframe_dirty` re-extracts the record-accurate lines), and
//! a stale preview must not overwrite either.

use bevy::prelude::*;
use bevy::tasks::{AsyncComputeTaskPool, Task};
use futures_lite::future;
use transform_gizmo_bevy::GizmoTarget;

use crate::config::ui::blob_edit as cfg;
use crate::pds::generator::GeneratorKind;
use crate::world_builder::build_primitive_mesh;

use super::proxy::BlobElementProxy;
use super::wireframe::{BlobWireframeSwap, edge_line_mesh};
use super::{BlobEditContext, write};

/// In-flight preview re-mesh + dispatch throttle state.
#[derive(Default)]
pub(in crate::editor_gizmo) struct PreviewState {
    last_dispatch_secs: f32,
    task: Option<Task<Option<Mesh>>>,
}

/// Poll the in-flight preview (writing finished edge lines into the
/// wireframe handle while the drag is still live) and dispatch the next
/// one when the throttle allows.
pub(in crate::editor_gizmo) fn blob_drag_preview(
    mut state: Local<PreviewState>,
    time: Res<Time>,
    ctx: Res<BlobEditContext>,
    proxies: Query<(&BlobElementProxy, &Transform, &GizmoTarget)>,
    global_tf: Query<&GlobalTransform>,
    swaps: Query<&BlobWireframeSwap>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let dragged = proxies.iter().find(|(_, _, t)| t.is_active());

    // Drain a finished re-mesh. Only landed while the drag is still live —
    // see module docs for why a post-drag result must be dropped.
    if let Some(task) = &mut state.task
        && let Some(result) = future::block_on(future::poll_once(task))
    {
        state.task = None;
        if dragged.is_some()
            && let (Some(line), Some(active)) = (result, &ctx.active)
            && let Ok(swap) = swaps.get(active.blob_entity)
        {
            // The swap owns a strong handle, so the id is always live —
            // insert can only fail on a dropped handle's stale id.
            let _ = meshes.insert(&swap.line_mesh, line);
        }
    }

    let (Some(active), Some((proxy, proxy_tf, _))) = (&ctx.active, dragged) else {
        return;
    };
    if state.task.is_some()
        || time.elapsed_secs() - state.last_dispatch_secs < cfg::PREVIEW_INTERVAL_SECS
    {
        return;
    }
    // The dragged proxy is world-space (gizmo detach); its blob-local pose
    // is recovered against the blob entity's frame — the same conversion
    // the commit will do on release.
    let Ok(blob_gt) = global_tf.get(active.blob_entity) else {
        return;
    };
    let local = GlobalTransform::from(*proxy_tf).reparented_to(blob_gt);

    let mut kind = active.kind.clone();
    let GeneratorKind::BlobGroup {
        elements,
        resolution,
        ..
    } = &mut kind
    else {
        return;
    };
    let Some(element) = elements.get_mut(proxy.index) else {
        return;
    };
    write::apply_local_to_element(element, &local);
    *resolution = (*resolution).min(cfg::PREVIEW_MAX_RESOLUTION);

    state.last_dispatch_secs = time.elapsed_secs();
    state.task = Some(AsyncComputeTaskPool::get().spawn(async move {
        let mesh = build_primitive_mesh(&kind);
        edge_line_mesh(&mesh)
    }));
}
