//! Worker entry point. When built for wasm + processed by `wasm-bindgen
//! --target web`, the produced JS (`gen-worker.js`) is what the app spawns as a
//! Web Worker; `register()` wires this module up to receive jobs. A no-op on
//! native so the crate stays a buildable workspace member.

#[cfg(target_arch = "wasm32")]
fn main() {
    use gloo_worker::Registrable;
    console_error_panic_hook::set_once();
    gen_worker::GenWorker::registrar().register();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!(
        "gen-worker is a wasm-only Web Worker entry point and does nothing \
         natively; build it with `--target wasm32-unknown-unknown` and process \
         the output with wasm-bindgen (see deploy.yml)."
    );
}
