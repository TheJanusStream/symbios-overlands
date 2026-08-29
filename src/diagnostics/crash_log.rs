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
//! Since #1145 the tail is also the wasm capture's **terminal record**. Every
//! recovered tail used to end without a `SessionEnd`, so the analyzer printed
//! `exit: — no SessionEnd record (crash or truncated log)` for a perfectly
//! normal tab close, a Rust panic's location never reached the capture at all
//! (`console_error_panic_hook` prints to the console and nothing else), and
//! the final ≤5 s — the events nearest the fault — were always lost to the
//! timer. A panic hook and a `pagehide` listener now append a marker and
//! flush synchronously, so the three exits read apart: a panic reason means a
//! Rust panic, `pagehide` means the tab was closed, and no marker at all
//! means neither hook got to run — an OOM trap or a browser kill.
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

/// The session's rolling NDJSON tail: one serialized event per entry, evicted
/// from the front once the total passes [`MAX_PERSIST_BYTES`].
///
/// Wasm-only in production (native has a real file sink), but defined at file
/// level so the eviction rule is unit-tested on the host.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Default)]
struct Tail {
    lines: std::collections::VecDeque<String>,
    bytes: usize,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl Tail {
    const fn new() -> Self {
        Tail {
            lines: std::collections::VecDeque::new(),
            bytes: 0,
        }
    }

    /// Append one line. The running byte total counts the newline the render
    /// will add, so the bound matches what actually reaches `localStorage`.
    fn push(&mut self, line: &str) {
        self.bytes += line.len() + 1;
        self.lines.push_back(line.to_owned());
        while self.bytes > MAX_PERSIST_BYTES {
            match self.lines.pop_front() {
                Some(dropped) => self.bytes -= dropped.len() + 1,
                None => break,
            }
        }
    }

    fn ndjson(&self) -> String {
        let mut out = String::with_capacity(self.bytes);
        for line in &self.lines {
            out.push_str(line);
            out.push('\n');
        }
        out
    }
}

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
    use std::cell::{Cell, RefCell};

    use bevy::prelude::*;

    use super::{CURRENT_KEY, MAX_PERSIST_BYTES, PREVIOUS_KEY, Tail, tail_at_line_boundary};
    use crate::diagnostics::event::{EventPayload, SessionEvent, Severity};

    thread_local! {
        /// Byte length of the recovered previous-session tail, cached at boot
        /// so the Diagnostics panel can render its button without re-reading
        /// (and copying) the stored string every frame.
        static PREVIOUS_BYTES: Cell<usize> = const { Cell::new(0) };
    }

    thread_local! {
        /// The session's NDJSON tail, one serialized event per entry, bounded
        /// by [`MAX_PERSIST_BYTES`].
        ///
        /// Serialized once at write time rather than rebuilt from the ring on
        /// every persist. That is what makes the tail reachable from a panic
        /// hook and a `pagehide` listener at all — neither can touch a Bevy
        /// `Resource` — and it is also why the ≤5 s the timer used to lose is
        /// no longer lost: the marker is appended to a tail that is already
        /// current. (Not rebuilding an O(ring) `String` every 5 s is #1136's
        /// concern; this shares the mechanism but not that issue's remaining
        /// question of whether metric snapshots belong in the tail at all.)
        static TAIL: RefCell<Tail> = RefCell::new(Tail::new());
    }

    /// Append one serialized event to the rolling tail. Called by
    /// [`crate::diagnostics::panic::shadow_push`] for every recorded event.
    pub fn push_tail_line(line: &str) {
        TAIL.with(|t| t.borrow_mut().push(line));
    }

    fn storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    /// Write the current tail to `localStorage`. Synchronous, which is what
    /// lets a hook running as the tab dies still land its bytes.
    fn store_tail(tail: &str) -> bool {
        let Some(store) = storage() else {
            return false;
        };
        store
            .set_item(CURRENT_KEY, tail_at_line_boundary(tail, MAX_PERSIST_BYTES))
            .is_ok()
    }

    /// Append a terminal `SessionEnd` marker to the tail and flush it.
    ///
    /// `seq` says which hook wrote it — the whole point is that the reader can
    /// tell a panic from a tab close from neither. Best effort throughout: an
    /// OOM trap will skip this entirely, and that absence is itself the third
    /// discriminator.
    fn write_terminal_record(seq: u64, reason: String) {
        let ev = SessionEvent::new(
            seq,
            crate::diagnostics::panic::marker_ts(),
            crate::diagnostics::log::wall_now_ms(),
            Severity::Critical,
            EventPayload::SessionEnd { reason },
        );
        if let Ok(line) = serde_json::to_string(&ev) {
            push_tail_line(&line);
        }
        let _ = store_tail(&TAIL.with(|t| t.borrow().ndjson()));
    }

    /// Install the two hooks that give a wasm session a terminal record: a
    /// panic hook chained after `console_error_panic_hook`, and a `pagehide`
    /// listener for the clean close.
    ///
    /// `pagehide` rather than `beforeunload`: it is the event browsers
    /// actually fire on mobile and on bfcache eviction, where `unload` and
    /// `beforeunload` are unreliable or skipped outright. The app's existing
    /// `beforeunload` listener guards unsaved edits and is unrelated.
    pub fn install_terminal_hooks() {
        use wasm_bindgen::JsCast;
        use wasm_bindgen::closure::Closure;

        let prev = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            write_terminal_record(
                crate::diagnostics::event::CRASH_MARKER_SEQ,
                crate::diagnostics::panic::format_panic_reason(
                    crate::diagnostics::panic::panic_message(info),
                    info.location().map(|l| (l.file(), l.line())),
                ),
            );
            prev(info);
        }));

        let Some(window) = web_sys::window() else {
            return;
        };
        let closure = Closure::<dyn FnMut()>::new(move || {
            write_terminal_record(
                crate::diagnostics::event::CLOSE_MARKER_SEQ,
                String::from("pagehide"),
            );
        });
        if let Err(e) =
            window.add_event_listener_with_callback("pagehide", closure.as_ref().unchecked_ref())
        {
            warn!("failed to install diagnostics pagehide hook: {e:?}");
        }
        // Leaked deliberately: the listener must outlive every frame of the
        // page, and there is nothing to unregister it from.
        closure.forget();
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
    pub fn persist_session_tail(mut disarmed: Local<bool>) {
        if *disarmed {
            return;
        }
        if !store_tail(&TAIL.with(|t| t.borrow().ndjson())) {
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
    install_terminal_hooks, persist_session_tail, previous_session_log, previous_session_log_bytes,
    push_tail_line, recover_previous_session_log,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// #1145. The tail is serialized once at write time so a panic hook and a
    /// `pagehide` listener — neither of which can touch a Bevy `Resource` —
    /// have something current to append a terminal marker to. It has to stay
    /// bounded on its own, because nothing rebuilds it from the ring any more.
    #[test]
    fn the_rolling_tail_evicts_oldest_first_and_stays_under_the_cap() {
        let mut tail = Tail::new();
        let line = "x".repeat(1000);
        // Comfortably past the cap, so eviction has to have happened.
        for _ in 0..(MAX_PERSIST_BYTES / 1000 + 10) {
            tail.push(&line);
        }
        let nd = tail.ndjson();
        assert!(
            nd.len() <= MAX_PERSIST_BYTES,
            "{} bytes is over the {MAX_PERSIST_BYTES} cap",
            nd.len()
        );
        assert!(
            nd.lines().all(|l| l == line),
            "every surviving entry is a whole line — the cut is never mid-event"
        );
    }

    /// A tail under the cap keeps everything, in order: the newest events are
    /// the ones nearest a fault, but a report that dropped the older ones
    /// could not show what led there.
    #[test]
    fn a_small_tail_keeps_every_line_in_order() {
        let mut tail = Tail::new();
        tail.push("{\"a\":1}");
        tail.push("{\"b\":2}");
        assert_eq!(tail.ndjson(), "{\"a\":1}\n{\"b\":2}\n");
    }

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
