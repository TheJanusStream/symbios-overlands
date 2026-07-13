//! Wasm Web Worker backend for [`crate::offload`].
//!
//! Runs each [`GenJob`] on a [`gen_worker`] one-shot worker (spawned via
//! `gloo-worker`), off the main thread. The worker JS (`gen-worker.js`,
//! emitted by `wasm-bindgen --target web` from the `gen-worker` crate) must be
//! deployed beside the app's JS — see `deploy.yml`.
//!
//! Workers are **pooled**, not spawned per job (#802): a fresh `Worker` costs
//! worker-thread creation + `gen-worker.js` module fetch/eval + wasm
//! instantiation, which the audio diagnostics measured at 130 ms–1.0 s per
//! voice bake — dwarfing the ~ms of actual synth compute. A finished bridge
//! goes back to a small idle pool and the next job reuses it, so steady-state
//! jobs pay only the round-trip + compute. Concurrent bursts beyond the pool
//! still spawn on demand (a job never queues behind another), and completions
//! past [`MAX_IDLE_WORKERS`] are dropped — the bridge `Drop` sends `Destroy`,
//! terminating that worker exactly as the old spawn-per-job path did.

use std::cell::RefCell;

use gen_jobs::{GenJob, GenResult};
use gloo_worker::Spawnable;
use gloo_worker::oneshot::OneshotBridge;

/// Idle workers kept warm for the next job. Sized for the app's biggest
/// natural burst short of a boot/region load (which fires heightmap + four
/// splat layers at once): those five spawn concurrently, three come back to
/// the pool, and later one-off jobs — voice bakes above all — always find a
/// warm worker. Each idle worker retains its wasm instance (a few MB at the
/// peak job it ran), so the cap keeps the residual footprint bounded.
const MAX_IDLE_WORKERS: usize = 3;

thread_local! {
    /// Warm, idle worker bridges. Main-thread only (wasm is single-threaded
    /// and `offload` + its `spawn_local` continuations all run there); borrows
    /// are scoped strictly around pool ops, never held across an await.
    static IDLE_WORKERS: RefCell<Vec<OneshotBridge<gen_worker::GenWorker>>> =
        const { RefCell::new(Vec::new()) };
}

/// Take a warm worker from the pool, or spawn a fresh one if none is idle.
///
/// `gloo-worker` defaults (`as_module = true`, `with_loader = false`) match a
/// `wasm-bindgen --target web` build: it generates the worker bootstrap and
/// imports `gen-worker.js` as an ES module. Messages use
/// [`gen_worker::MsgpackCodec`] (not gloo's default Bincode, which can't decode
/// the audio cores' internally-tagged enums).
fn take_or_spawn() -> OneshotBridge<gen_worker::GenWorker> {
    IDLE_WORKERS
        .with(|pool| pool.borrow_mut().pop())
        .unwrap_or_else(|| {
            gen_worker::GenWorker::spawner()
                .encoding::<gen_worker::MsgpackCodec>()
                .spawn("./gen-worker.js")
        })
}

/// Return a finished worker to the idle pool, or drop it (terminating the
/// worker) once the pool is full.
fn recycle(bridge: OneshotBridge<gen_worker::GenWorker>) {
    IDLE_WORKERS.with(|pool| {
        let mut pool = pool.borrow_mut();
        if pool.len() < MAX_IDLE_WORKERS {
            pool.push(bridge);
        }
        // else: dropped here — the last bridge's Drop destroys the worker.
    });
}

/// Run one job on a pooled gen-worker and await its result.
///
/// A bridge is exclusively owned while its job runs — a concurrent job takes a
/// different pooled bridge or spawns its own worker — so a long heightmap can
/// never head-of-line-block a voice bake. If the run panics (worker error),
/// the bridge is simply not recycled and the next job spawns fresh.
pub async fn run_on_worker(job: GenJob) -> GenResult {
    let mut bridge = take_or_spawn();
    let result = bridge.run(job).await;
    recycle(bridge);
    result
}
