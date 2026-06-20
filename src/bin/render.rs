//! Thin entry point for the headless render tool. All logic lives in
//! [`symbios_overlands::render_tool`] (in the library, so it can reach the
//! crate-internal spawn machinery). Native-only; the web deploy builds the
//! same crate to wasm, where this is a stub.

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    symbios_overlands::render_tool::run();
}

#[cfg(target_arch = "wasm32")]
fn main() {
    eprintln!("render harness is native-only; nothing to do on wasm");
}
