//! WASM Web Worker for off-the-main-thread generation.
//!
//! Defines the [`GenWorker`] oneshot worker (one [`gen_jobs::GenJob`] in, one
//! [`gen_jobs::GenResult`] out). The app spawns it via `gloo-worker` on wasm so
//! heavy generation runs on a real worker thread; the binary (`src/main.rs`)
//! registers it as the worker entry point. Everything is `wasm32`-gated so the
//! crate is an empty rlib on native (the app depends on it only on wasm).
//!
//! Messages use the [`MsgpackCodec`] (NOT gloo's default Bincode) — see that
//! type for why. `GenJob`/`GenResult` are `Serialize`/`Deserialize`.

#[cfg(target_arch = "wasm32")]
use gloo_worker::oneshot::oneshot;

/// One-shot generation worker: runs the job purely and returns its result. The
/// compute lives in the Bevy-free [`gen_jobs`] crate, so this worker `.wasm`
/// stays slim (no Bevy).
#[cfg(target_arch = "wasm32")]
#[oneshot]
pub async fn GenWorker(job: gen_jobs::GenJob) -> gen_jobs::GenResult {
    job.run()
}

/// MessagePack worker-message codec. gloo-worker's default `Bincode` is **not**
/// self-describing, so it cannot `deserialize_any` — which the audio cores'
/// internally-tagged `#[serde(tag = "...")]` enums (in `AudioPatch`'s node
/// graph) require, panicking the worker with `DeserializeAnyNotSupported`.
/// MessagePack is self-describing AND binary-compact (JSON would bloat the
/// heightmap / WAV / texture payloads). The spawner (app) and registrar
/// (worker bin) MUST use this same codec.
#[cfg(target_arch = "wasm32")]
pub struct MsgpackCodec;

#[cfg(target_arch = "wasm32")]
impl gloo_worker::Codec for MsgpackCodec {
    fn encode<I>(input: I) -> wasm_bindgen::JsValue
    where
        I: serde::Serialize,
    {
        let buf = rmp_serde::to_vec_named(&input).expect("can't serialize a worker message");
        js_sys::Uint8Array::from(buf.as_slice()).into()
    }

    fn decode<O>(input: wasm_bindgen::JsValue) -> O
    where
        O: for<'de> serde::Deserialize<'de>,
    {
        let data = js_sys::Uint8Array::from(input).to_vec();
        rmp_serde::from_slice(&data).expect("can't deserialize a worker message")
    }
}
