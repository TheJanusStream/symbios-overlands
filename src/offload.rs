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
    // The ticket rides inside the future, so it is dropped both when the job
    // finishes and when the caller cancels the `Task` (#1143).
    let ticket = census::start(census::label(&job));
    bevy::tasks::AsyncComputeTaskPool::get().spawn(async move {
        let _ticket = ticket;
        job.run()
    })
}

/// Wasm dispatch: run the job on a Web Worker (off the render thread) and
/// resolve the task when it returns. `spawn_local` drives the worker round-trip
/// on the JS event loop; a oneshot channel bridges the result back into a Bevy
/// `Task` so callers poll it exactly as on native.
#[cfg(target_arch = "wasm32")]
pub fn offload(job: GenJob) -> Task<GenResult> {
    let (tx, rx) = futures_channel::oneshot::channel::<GenResult>();
    // The ticket belongs to the `spawn_local` future, not to the `Task` below:
    // dropping a wasm `Task` does not cancel the work behind it, and it is the
    // worker round-trip that either answers or does not. A `gen-worker.js`
    // that 404s leaves this future parked forever, which is precisely the
    // in-flight entry the watchdog needs to see (#1143).
    let ticket = census::start(census::label(&job));
    wasm_bindgen_futures::spawn_local(async move {
        let _ticket = ticket;
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

/// Process-global census of in-flight offloaded jobs (#1143).
///
/// The problem this exists to solve is that a job which never answers is
/// invisible. On wasm `offload` awaits a oneshot whose sender lives inside a
/// `spawn_local` future that only completes when the worker replies, and
/// `spawn_worker` has no error path at all: a 404 or a CSP refusal on
/// `./gen-worker.js`, or a worker-side OOM, simply never resolves. Nothing
/// observed that — `OffloadJobStarted` had no emit site anywhere in the crate,
/// so even the offline replay rule that exists for this
/// (`offload.task_never_resolves`) had nothing to fold.
///
/// The census is armed inside [`offload`] rather than at the six call sites,
/// which is the whole point: the avatar build (#1061) is the newest and
/// heaviest job in the roster and was the one nobody instrumented. A ledger the
/// dispatch function itself keeps cannot be forgotten by the next caller.
///
/// It is a plain global with no clock of its own beyond a monotonic `Instant`
/// because `offload` has no `World` — the same constraint that shapes
/// [`crate::diagnostics::panic`]. The Bevy side ([`sample_offload_census`])
/// drains it each frame and turns transitions into session events.
///
/// [`sample_offload_census`]: crate::diagnostics::offload_watch::sample_offload_census
pub mod census {
    use std::sync::Mutex;

    use bevy::platform::time::Instant;

    use super::GenJob;

    /// A short, stable name per job kind — the string that appears in
    /// `OffloadJobStarted { job }` and in the watchdog's message. Deliberately
    /// coarse: the census answers "is something stuck", and per-instance
    /// identity is the `id` beside it.
    pub fn label(job: &GenJob) -> &'static str {
        match job {
            GenJob::Heightmap(_) => "heightmap",
            GenJob::AudioBake(_) => "audio_bake",
            GenJob::TextureBake { .. } => "texture_bake",
            GenJob::AvatarBuild { .. } => "avatar_build",
        }
    }

    /// One in-flight job.
    struct Pending {
        id: u64,
        job: &'static str,
        started: Instant,
    }

    /// A start or an end, queued for the next drain by the Bevy sampler.
    pub enum Transition {
        Started {
            id: u64,
            job: &'static str,
        },
        Finished {
            id: u64,
            job: &'static str,
            elapsed_secs: f64,
        },
    }

    struct Census {
        next_id: u64,
        pending: Vec<Pending>,
        transitions: Vec<Transition>,
    }

    static CENSUS: Mutex<Census> = Mutex::new(Census {
        next_id: 0,
        pending: Vec::new(),
        transitions: Vec::new(),
    });

    /// Held for the lifetime of one offloaded job; retires it on drop.
    ///
    /// A guard rather than an explicit `finish()` call because the job's future
    /// has two endings, not one: it completes, or the caller drops the `Task`
    /// and cancels it. Only a `Drop` covers both — and a cancelled job left in
    /// the ledger would have the watchdog reporting a stall that nobody is
    /// waiting on.
    pub struct Ticket {
        id: u64,
    }

    impl Drop for Ticket {
        fn drop(&mut self) {
            let Ok(mut census) = CENSUS.lock() else {
                return;
            };
            let Some(index) = census.pending.iter().position(|p| p.id == self.id) else {
                return;
            };
            let entry = census.pending.remove(index);
            let elapsed_secs = entry.started.elapsed().as_secs_f64();
            census.transitions.push(Transition::Finished {
                id: entry.id,
                job: entry.job,
                elapsed_secs,
            });
        }
    }

    /// Register a job as in flight. The returned [`Ticket`] must be moved into
    /// the future that runs it.
    pub fn start(job: &'static str) -> Ticket {
        let Ok(mut census) = CENSUS.lock() else {
            return Ticket { id: u64::MAX };
        };
        let id = census.next_id;
        census.next_id += 1;
        census.pending.push(Pending {
            id,
            job,
            started: Instant::now(),
        });
        census.transitions.push(Transition::Started { id, job });
        Ticket { id }
    }

    /// Take every transition since the last call.
    pub fn drain_transitions() -> Vec<Transition> {
        match CENSUS.lock() {
            Ok(mut census) => std::mem::take(&mut census.transitions),
            Err(_) => Vec::new(),
        }
    }

    /// Every job still in flight, as `(id, job, age_secs)`.
    ///
    /// Returns all of them, not just the oldest: a stale gen-worker fails every
    /// job, and reporting one of six would understate a broken deploy as a
    /// single slow bake.
    pub fn pending_snapshot() -> Vec<(u64, &'static str, f64)> {
        match CENSUS.lock() {
            Ok(census) => census
                .pending
                .iter()
                .map(|p| (p.id, p.job, p.started.elapsed().as_secs_f64()))
                .collect(),
            Err(_) => Vec::new(),
        }
    }
}
