//! Crash-surviving session-log tail (wasm) — #811.
//!
//! The wasm session log lives only in the in-memory ring; a hard crash (the
//! 4 GiB OOM trap that motivated this) kills the tab before the "Download
//! session log" button can be used, so the exact evidence needed to diagnose
//! the crash dies with it. This module persists the ring's NDJSON tail to
//! `localStorage` every few seconds; on the next boot the previous session's
//! tail is moved aside and offered in the Diagnostics panel as
//! "Download previous session log" — a byte-compatible `.jsonl` the offline
//! analyzer reads like any other capture.
//!
//! Native has a real file sink (`session-latest.jsonl`), so everything here
//! except the pure tail-truncation helper is wasm-gated.

/// Ceiling on the persisted tail. Well under every browser's ~5 MB
/// per-origin `localStorage` quota while holding many minutes of events
/// (a full 5-minute field session measured ~300 KB).
// Consumed by the wasm persist system; native builds carry it (and the
// helper below) only for the unit tests.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const MAX_PERSIST_BYTES: usize = 1_500_000;

/// `localStorage` key the running session persists its tail under.
#[cfg(target_arch = "wasm32")]
const CURRENT_KEY: &str = "symbios.diag.session_tail";
/// `localStorage` key the previous session's tail is parked under at boot.
#[cfg(target_arch = "wasm32")]
const PREVIOUS_KEY: &str = "symbios.diag.session_tail.prev";

/// Last `max` bytes of an NDJSON string, cut forward to the next line
/// boundary so the result starts with a complete event. Slicing after an
/// ASCII `\n` is always a valid `str` boundary, so multi-byte content in
/// event payloads can't panic the cut.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
fn tail_at_line_boundary(nd: &str, max: usize) -> &str {
    if nd.len() <= max {
        return nd;
    }
    let cut = nd.len() - max;
    match nd.as_bytes()[cut..].iter().position(|&b| b == b'\n') {
        Some(nl) => &nd[cut + nl + 1..],
        None => "",
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use std::cell::Cell;

    use bevy::prelude::*;

    use super::{CURRENT_KEY, MAX_PERSIST_BYTES, PREVIOUS_KEY, tail_at_line_boundary};
    use crate::diagnostics::SessionLog;

    thread_local! {
        /// Byte length of the recovered previous-session tail, cached at boot
        /// so the Diagnostics panel can render its button without re-reading
        /// (and copying) the stored string every frame.
        static PREVIOUS_BYTES: Cell<usize> = const { Cell::new(0) };
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    /// Startup: park the last session's persisted tail under [`PREVIOUS_KEY`]
    /// (whether that session crashed or simply closed — the last session is
    /// always recoverable) and clear the way for this session's writer.
    pub fn recover_previous_session_log() {
        let Some(store) = storage() else {
            return;
        };
        let Ok(Some(tail)) = store.get_item(CURRENT_KEY) else {
            return;
        };
        if !tail.is_empty() && store.set_item(PREVIOUS_KEY, &tail).is_ok() {
            PREVIOUS_BYTES.with(|b| b.set(tail.len()));
            info!(
                "previous session log recovered ({} bytes) — Diagnostics → \
                 'Download previous session log'",
                tail.len()
            );
        }
        let _ = store.remove_item(CURRENT_KEY);
    }

    /// Update (timer-gated): persist the ring's NDJSON tail. On a quota error
    /// the system disarms for the rest of the session — a persistently full
    /// origin store would otherwise warn every tick.
    pub fn persist_session_tail(session_log: Res<SessionLog>, mut disarmed: Local<bool>) {
        if *disarmed {
            return;
        }
        let Some(store) = storage() else {
            *disarmed = true;
            return;
        };
        let ndjson = session_log.drain_ndjson();
        let tail = tail_at_line_boundary(&ndjson, MAX_PERSIST_BYTES);
        if store.set_item(CURRENT_KEY, tail).is_err() {
            warn!("session-log tail persistence disabled (localStorage quota?)");
            *disarmed = true;
        }
    }

    /// Byte length of the previous session's recovered tail (0 = none). Cheap
    /// per-frame check for the Diagnostics panel.
    pub fn previous_session_log_bytes() -> usize {
        PREVIOUS_BYTES.with(|b| b.get())
    }

    /// The previous session's recovered tail, read back from storage.
    pub fn previous_session_log() -> Option<String> {
        storage()?.get_item(PREVIOUS_KEY).ok().flatten()
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm::{
    persist_session_tail, previous_session_log, previous_session_log_bytes,
    recover_previous_session_log,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_logs_persist_whole() {
        let nd = "{\"a\":1}\n{\"b\":2}\n";
        assert_eq!(tail_at_line_boundary(nd, 1024), nd);
    }

    #[test]
    fn long_logs_cut_forward_to_a_complete_event() {
        let nd = "{\"first\":1}\n{\"second\":2}\n{\"third\":3}\n";
        // A max that lands mid-"second" must yield only the complete third line.
        let tail = tail_at_line_boundary(nd, "{\"third\":3}\n".len() + 3);
        assert_eq!(tail, "{\"third\":3}\n");
        // Every returned tail starts at a line start.
        assert!(!tail.starts_with(','));
    }

    #[test]
    fn multibyte_payloads_never_split_a_char() {
        // Non-ASCII near the cut point: the boundary search walks to the
        // ASCII newline, so slicing stays on char boundaries.
        let nd = "{\"name\":\"héllo wörld\"}\n{\"tail\":\"ok\"}\n";
        let tail = tail_at_line_boundary(nd, "{\"tail\":\"ok\"}\n".len() + 2);
        assert_eq!(tail, "{\"tail\":\"ok\"}\n");
    }

    #[test]
    fn no_newline_in_window_yields_empty() {
        let nd = "{\"one_enormous_line\":true}";
        assert_eq!(tail_at_line_boundary(nd, 5), "");
    }
}
