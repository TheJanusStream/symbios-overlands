//! Platform-routed CPU-generation offload.
//!
//! [`offload`] takes a self-contained [`GenJob`] and returns a
//! `bevy::tasks::Task<GenResult>` that the caller polls each frame — the same
//! API on every target. On **native** the job runs on the multithreaded
//! `AsyncComputeTaskPool` (Bevy's async-executor task pool), giving real
//! parallelism off the main schedule.
//!
//! On **wasm** Bevy's task pools collapse to a single cooperative thread, so a
//! job run inline would stall the render frame. Instead the wasm backend
//! dispatches the job to a **pooled** Web Worker (the `gen_worker` crate,
//! spawned via `gloo-worker` and kept warm between jobs — see the `worker`
//! submodule for the pool, #802), which runs it on a real worker thread —
//! matching native's off-the-frame progressive loading. The worker links only
//! the Bevy-free [`gen_jobs`] crate, never the engine.
//!
//! **It is no longer small.** Adding `GenJob::AvatarBuild` (#1061) linked the
//! whole avatar engine into it: measured at 839 KB gzipped (3.9 MB raw)
//! against ~16 KB before. That is one lazy fetch, cached, and it buys the
//! browser build the only off-main-thread body build available to it — but it
//! is a real boot cost for a session that never renders a rigged avatar, and
//! splitting the roster across two worker artifacts is filed as #1063.
//!
//! The shared [`gen_jobs::GenJob::run`] guarantees native and worker execution
//! are byte-identical — the determinism the terrain pipeline relies on across
//! peers.

use bevy::tasks::Task;
pub use gen_jobs::{GenJob, GenResult};

/// Dispatch a generation job and return a task to poll for its [`GenResult`].
///
/// Polled with the usual `futures_lite::future::{block_on, poll_once}` idiom,
/// identically on native and wasm.
#[cfg(not(target_arch = "wasm32"))]
pub fn offload(job: GenJob) -> Task<GenResult> {
    bevy::tasks::AsyncComputeTaskPool::get().spawn(async move { job.run() })
}

/// Wasm dispatch: run the job on a Web Worker (off the render thread) and
/// resolve the task when it returns. `spawn_local` drives the worker round-trip
/// on the JS event loop; a oneshot channel bridges the result back into a Bevy
/// `Task` so callers poll it exactly as on native.
#[cfg(target_arch = "wasm32")]
pub fn offload(job: GenJob) -> Task<GenResult> {
    let (tx, rx) = futures_channel::oneshot::channel::<GenResult>();
    wasm_bindgen_futures::spawn_local(async move {
        let result = worker::run_on_worker(job).await;
        // The receiver is dropped only if the terrain task was cancelled; then
        // there is simply nothing to deliver the result to.
        let _ = tx.send(result);
    });
    bevy::tasks::IoTaskPool::get().spawn(async move {
        rx.await
            .expect("gen-worker dropped before returning a result")
    })
}

#[cfg(target_arch = "wasm32")]
mod worker;
