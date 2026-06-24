//! Wasm Web Worker backend for [`crate::offload`].
//!
//! Spawns the [`gen_worker`] one-shot worker via `gloo-worker` and runs a
//! single [`GenJob`] on it, off the main thread. The worker JS
//! (`gen-worker.js`, emitted by `wasm-bindgen --target web` from the
//! `gen-worker` crate) must be deployed beside the app's JS — see `deploy.yml`.

use gen_jobs::{GenJob, GenResult};
use gloo_worker::Spawnable;

/// Spawn the gen-worker, run one job on it, and await its result.
///
/// `gloo-worker` defaults (`as_module = true`, `with_loader = false`) match a
/// `wasm-bindgen --target web` build: it generates the worker bootstrap and
/// imports `gen-worker.js` as an ES module. Messages use the default Bincode
/// codec (`GenJob`/`GenResult` are `Serialize`/`Deserialize`).
pub async fn run_on_worker(job: GenJob) -> GenResult {
    let mut bridge = gen_worker::GenWorker::spawner().spawn("./gen-worker.js");
    bridge.run(job).await
}
