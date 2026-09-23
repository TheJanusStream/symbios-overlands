//! Thin entry point for the agent client (#1413). All logic lives in
//! [`symbios_overlands::agent`] (in the library, so it can reach the
//! crate-internal session and world machinery). Unix-only; on every other
//! target, the wasm deploy among them, this is a stub.

#[cfg(unix)]
fn main() -> std::process::ExitCode {
    symbios_overlands::agent::run()
}

#[cfg(not(unix))]
fn main() {
    eprintln!("the agent client runs on Unix-like systems only; nothing to do here");
}
