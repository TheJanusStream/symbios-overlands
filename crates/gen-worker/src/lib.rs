//! WASM Web Worker for off-the-main-thread generation.
//!
//! Defines the [`GenWorker`] oneshot worker (one [`gen_jobs::GenJob`] in, one
//! [`gen_jobs::GenResult`] out). The app spawns it via `gloo-worker` on wasm so
//! heavy generation runs on a real worker thread; the binary (`src/main.rs`)
//! registers it as the worker entry point. Everything is `wasm32`-gated so the
//! crate is an empty rlib on native (the app depends on it only on wasm).
//!
//! Messages are Bincode-encoded (gloo's default codec) — `GenJob`/`GenResult`
//! are `Serialize`/`Deserialize`.

#[cfg(target_arch = "wasm32")]
use gloo_worker::oneshot::oneshot;

/// One-shot generation worker: runs the job purely and returns its result. The
/// compute lives in the Bevy-free [`gen_jobs`] crate, so this worker `.wasm`
/// stays tiny (~16 KB gzipped).
#[cfg(target_arch = "wasm32")]
#[oneshot]
pub async fn GenWorker(job: gen_jobs::GenJob) -> gen_jobs::GenResult {
    job.run()
}
