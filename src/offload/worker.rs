//! Wasm Web Worker backend for [`crate::offload`].
//!
//! Runs each [`GenJob`] on a [`gen_worker`] one-shot worker (spawned via
//! `gloo-worker`), off the main thread. The worker JS (`gen-worker.js`,
//! emitted by `wasm-bindgen --target web` from the `gen-worker` crate) must be
//! deployed beside the app's JS — see `deploy.yml`.
//!
//! Workers are **pooled and bounded**, not spawned per job (#802/#807): a
//! fresh `Worker` costs worker-thread creation + `gen-worker.js` module
//! fetch/eval + wasm instantiation, which the audio diagnostics measured at
//! 130 ms–1.0 s per voice bake — dwarfing the ~ms of actual synth compute. A
//! finished bridge goes back to a small idle pool and the next job reuses it,
//! so steady-state jobs pay only the round-trip + compute.
//!
//! Concurrency is capped at [`MAX_WORKERS`]: once that many workers exist,
//! further jobs wait for a bridge instead of spawning more (a re-rolled
//! avatar dispatches dozens of texture bakes at once — unbounded spawn-on-
//! demand would flood the browser with worker instantiations, reintroducing
//! the very cost the pool removes). Idle bridges beyond [`MAX_IDLE_WORKERS`]
//! are dropped — the bridge `Drop` sends `Destroy`, terminating that worker
//! exactly as the old spawn-per-job path did.
//!
//! Waiting is **two-lane**, not one FIFO: audio bakes and heightmaps are
//! latency-sensitive (a region re-seed's ambient bed was measured waiting
//! 5 s behind a 50-job surface-texture flood — silent world), while texture
//! bakes are a bulk stream whose only cost is pop-in. [`release`] hands a
//! freed bridge to the urgent lane first, so an audio bake waits at most one
//! in-flight job even mid-flood.

use std::cell::RefCell;
use std::collections::VecDeque;

use gen_jobs::{GenJob, GenResult};
use gloo_worker::Spawnable;
use gloo_worker::oneshot::OneshotBridge;

/// Ceiling on concurrently-live workers (running + idle). Sized so the boot
/// burst (heightmap + four splat layers) mostly runs in parallel while a
/// texture-bake flood from an avatar re-roll queues instead of instantiating
/// a worker per material.
const MAX_WORKERS: usize = 4;

/// Idle workers kept warm for the next job; the rest are dropped on release.
/// Each idle worker retains its wasm instance (a few MB at the peak job it
/// ran), so the cap keeps the residual footprint bounded while voice bakes
/// and one-off textures always find a warm worker.
const MAX_IDLE_WORKERS: usize = 3;

type Bridge = OneshotBridge<gen_worker::GenWorker>;

/// Pool bookkeeping. Main-thread only (wasm is single-threaded and `offload` +
/// its `spawn_local` continuations all run there); borrows are scoped strictly
/// around pool ops, never held across an await.
struct Pool {
    /// Warm bridges awaiting a job.
    idle: Vec<Bridge>,
    /// Live workers: running + idle. Spawning is allowed while `live <
    /// MAX_WORKERS`; a released bridge that is dropped (pool full) decrements.
    live: usize,
    /// Latency-sensitive jobs (audio bakes, heightmaps) waiting for a bridge —
    /// served FIFO, ahead of every bulk waiter.
    urgent_waiters: VecDeque<futures_channel::oneshot::Sender<Bridge>>,
    /// Bulk jobs (texture bakes) waiting for a bridge — served FIFO once the
    /// urgent lane is empty.
    bulk_waiters: VecDeque<futures_channel::oneshot::Sender<Bridge>>,
}

thread_local! {
    static POOL: RefCell<Pool> = const {
        RefCell::new(Pool {
            idle: Vec::new(),
            live: 0,
            urgent_waiters: VecDeque::new(),
            bulk_waiters: VecDeque::new(),
        })
    };
}

/// Latency-sensitive job kinds jump the bulk queue: an ambient / voice bake
/// is the difference between a silent and a sounding world, and a heightmap
/// gates terrain (and everything seated on it) after a region re-seed.
/// Texture bakes only delay their own pop-in.
fn is_urgent(job: &GenJob) -> bool {
    matches!(job, GenJob::AudioBake(_) | GenJob::Heightmap(_))
}

/// Spawn a fresh gen-worker.
///
/// `gloo-worker` defaults (`as_module = true`, `with_loader = false`) match a
/// `wasm-bindgen --target web` build: it generates the worker bootstrap and
/// imports `gen-worker.js` as an ES module. Messages use
/// [`gen_worker::MsgpackCodec`] (not gloo's default Bincode, which can't decode
/// the audio cores' internally-tagged enums).
fn spawn_worker() -> Bridge {
    gen_worker::GenWorker::spawner()
        .encoding::<gen_worker::MsgpackCodec>()
        .spawn("./gen-worker.js")
}

/// Take a warm worker, spawn one (under the cap), or wait in the lane for a
/// release.
async fn acquire(urgent: bool) -> Bridge {
    let (bridge, rx) = POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        if let Some(bridge) = pool.idle.pop() {
            return (Some(bridge), None);
        }
        if pool.live < MAX_WORKERS {
            pool.live += 1;
            return (Some(spawn_worker()), None);
        }
        let (tx, rx) = futures_channel::oneshot::channel();
        if urgent {
            pool.urgent_waiters.push_back(tx);
        } else {
            pool.bulk_waiters.push_back(tx);
        }
        (None, Some(rx))
    });
    match (bridge, rx) {
        (Some(bridge), _) => bridge,
        // The sender lives in the pool until a release hands this waiter a
        // bridge; it is only dropped by that hand-off, so the await cannot
        // fail while the pool exists (thread-local, never torn down).
        (None, Some(rx)) => rx.await.expect("gen-worker pool dropped a waiter"),
        (None, None) => unreachable!("acquire yields a bridge or a waiter"),
    }
}

/// Hand a finished worker to the next waiter (urgent lane first), park it
/// warm, or drop it (terminating the worker) once the idle pool is full.
fn release(mut bridge: Bridge) {
    POOL.with(|pool| {
        let mut pool = pool.borrow_mut();
        // A waiter whose future was dropped (cancelled task) rejects the
        // hand-off; skip to the next, draining urgent before bulk.
        while let Some(tx) = pool
            .urgent_waiters
            .pop_front()
            .or_else(|| pool.bulk_waiters.pop_front())
        {
            match tx.send(bridge) {
                Ok(()) => return,
                Err(rejected) => bridge = rejected,
            }
        }
        if pool.idle.len() < MAX_IDLE_WORKERS {
            pool.idle.push(bridge);
        } else {
            // Dropped here — the last bridge's Drop destroys the worker.
            pool.live -= 1;
        }
    });
}

/// Run one job on a pooled gen-worker and await its result.
///
/// A bridge is exclusively owned while its job runs — a concurrent job takes a
/// different pooled bridge, spawns its own worker under the cap, or waits in
/// its lane. If the run panics (worker error), the bridge is simply not
/// released; the pool's live count stays consumed, which is moot because a
/// worker-side panic aborts the wasm app anyway.
pub async fn run_on_worker(job: GenJob) -> GenResult {
    let urgent = is_urgent(&job);
    let mut bridge = acquire(urgent).await;
    let result = bridge.run(job).await;
    release(bridge);
    result
}
