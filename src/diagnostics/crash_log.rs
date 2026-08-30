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

/// The share of [`MAX_PERSIST_BYTES`] reserved for the 1 Hz metric snapshot
/// (#1180).
///
/// On native the snapshot goes to a single overwrite slot, so the drip cannot
/// evict the pre-crash events the dump exists to preserve (#633). Wasm has no
/// file sink, so the snapshot goes into this tail like everything else — and a
/// snapshot line is not small: it carries every series in the registry
/// (~69 of them), so it is kilobytes, once a second. Against one shared budget
/// it wins: within a few minutes the recovered tail is almost entirely vitals
/// and the events AROUND the fault have been evicted by the instrument that
/// was watching for it.
///
/// Two budgets rather than one, each evicting oldest-first inside itself, so
/// neither class can starve the other. The split is asymmetric because the two
/// classes are: real events are sparse — a whole 5-minute session's NDJSON was
/// ~300 KB *including* the drip — so 900 KB of them is more history than an
/// observed session produces, while the vitals are a fixed rate and every byte
/// they get is more of the climb.
///
/// A fixed budget still holds only the most recent minutes of the series. A
/// long climb would want the older samples thinned rather than dropped, which
/// is a different design (and #1190) — this one guarantees that both halves of
/// the evidence are present, which is what was actually missing.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const SNAPSHOT_BUDGET_BYTES: usize = 600_000;

/// The remainder of [`MAX_PERSIST_BYTES`]: real events, including the terminal
/// marker a panic or `pagehide` appends.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const EVENT_BUDGET_BYTES: usize = MAX_PERSIST_BYTES - SNAPSHOT_BUDGET_BYTES;

/// `localStorage` key the running session persists its tail under.
#[cfg(target_arch = "wasm32")]
const CURRENT_KEY: &str = "symbios.diag.session_tail";
/// `localStorage` key the previous session's tail is parked under at boot.
#[cfg(target_arch = "wasm32")]
const PREVIOUS_KEY: &str = "symbios.diag.session_tail.prev";

/// The session's rolling NDJSON tail: one serialized event per entry, held in
/// two independently-evicting queues — real events and metric snapshots — so
/// the high-frequency class cannot consume the other's bytes (#1180).
///
/// Each entry carries its `seq` because the two queues evict at different
/// rates and the analyzer reads NDJSON in seq order, so [`Tail::ndjson`] has
/// to merge them rather than concatenate. Both queues are appended in seq
/// order and evicted from the front, so both stay sorted and the merge is a
/// single pass.
///
/// Wasm-only in production (native has a real file sink), but defined at file
/// level so the eviction rule is unit-tested on the host.
#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
#[derive(Default)]
struct Tail {
    events: std::collections::VecDeque<(u64, String)>,
    event_bytes: usize,
    snapshots: std::collections::VecDeque<(u64, String)>,
    snapshot_bytes: usize,
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
impl Tail {
    const fn new() -> Self {
        Tail {
            events: std::collections::VecDeque::new(),
            event_bytes: 0,
            snapshots: std::collections::VecDeque::new(),
            snapshot_bytes: 0,
        }
    }

    /// Append one real event — anything that is not the 1 Hz metric snapshot,
    /// the terminal marker included.
    fn push_event(&mut self, seq: u64, line: &str) {
        Self::push_bounded(
            &mut self.events,
            &mut self.event_bytes,
            EVENT_BUDGET_BYTES,
            seq,
            line,
        );
    }

    /// Append one metric snapshot, into its own budget.
    fn push_snapshot(&mut self, seq: u64, line: &str) {
        Self::push_bounded(
            &mut self.snapshots,
            &mut self.snapshot_bytes,
            SNAPSHOT_BUDGET_BYTES,
            seq,
            line,
        );
    }

    /// Append to one queue and evict its oldest until it is back inside its
    /// budget. The running byte total counts the newline the render will add,
    /// so the two budgets sum to what actually reaches `localStorage`.
    fn push_bounded(
        queue: &mut std::collections::VecDeque<(u64, String)>,
        bytes: &mut usize,
        budget: usize,
        seq: u64,
        line: &str,
    ) {
        *bytes += line.len() + 1;
        queue.push_back((seq, line.to_owned()));
        while *bytes > budget {
            match queue.pop_front() {
                Some((_, dropped)) => *bytes -= dropped.len() + 1,
                None => break,
            }
        }
    }

    /// The two queues merged back into one seq-ordered NDJSON document — the
    /// order the offline analyzer reads, and the order the events actually
    /// happened in.
    fn ndjson(&self) -> String {
        let mut out = String::with_capacity(self.event_bytes + self.snapshot_bytes);
        let mut events = self.events.iter();
        let mut snapshots = self.snapshots.iter();
        let mut next_event = events.next();
        let mut next_snapshot = snapshots.next();
        loop {
            let from_events = match (next_event, next_snapshot) {
                (Some((e, _)), Some((s, _))) => e <= s,
                (Some(_), None) => true,
                (None, Some(_)) => false,
                (None, None) => break,
            };
            let entry = if from_events {
                let entry = next_event;
                next_event = events.next();
                entry
            } else {
                let entry = next_snapshot;
                next_snapshot = snapshots.next();
                entry
            };
            if let Some((_, line)) = entry {
                out.push_str(line);
                out.push('\n');
            }
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
        /// by [`MAX_PERSIST_BYTES`] across its two class budgets.
        ///
        /// Serialized once at write time rather than rebuilt from the ring on
        /// every persist. That is what makes the tail reachable from a panic
        /// hook and a `pagehide` listener at all — neither can touch a Bevy
        /// `Resource` — and it is also why the ≤5 s the timer used to lose is
        /// no longer lost: the marker is appended to a tail that is already
        /// current. (Not rebuilding an O(ring) `String` every 5 s is #1136's
        /// concern; whether metric snapshots belong in the tail at all was
        /// #1180's, answered by [`SNAPSHOT_BUDGET_BYTES`]: they do, in a
        /// budget of their own.)
        static TAIL: RefCell<Tail> = RefCell::new(Tail::new());
    }

    /// Append one serialized real event to the rolling tail. Called by
    /// [`crate::diagnostics::panic::shadow_push`] for every recorded event.
    pub fn push_tail_line(seq: u64, line: &str) {
        TAIL.with(|t| t.borrow_mut().push_event(seq, line));
    }

    /// Append one serialized metric snapshot, into the tail's snapshot budget
    /// rather than beside the real events (#1180). Called by
    /// [`crate::diagnostics::panic::shadow_push_snapshot`].
    pub fn push_tail_snapshot(seq: u64, line: &str) {
        TAIL.with(|t| t.borrow_mut().push_snapshot(seq, line));
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
            // A real event, not a snapshot: the marker is the single most
            // important line in the capture and must never share a budget
            // with the drip.
            push_tail_line(seq, &line);
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
    push_tail_line, push_tail_snapshot, recover_previous_session_log,
};

#[cfg(test)]
mod tests {
    use super::*;

    /// One padded line of `bytes` total length, tagged with its seq so a test
    /// can tell which entries survived eviction.
    fn line_of(tag: &str, seq: u64, bytes: usize) -> String {
        let head = format!("{{\"{tag}\":{seq},\"pad\":\"");
        let tail = "\"}";
        let pad = bytes.saturating_sub(head.len() + tail.len());
        format!("{head}{}{tail}", "p".repeat(pad))
    }

    /// The seqs of the surviving lines of one class, in render order.
    fn kept(nd: &str, tag: &str) -> Vec<u64> {
        let head = format!("{{\"{tag}\":");
        nd.lines()
            .filter_map(|l| l.strip_prefix(&head))
            .filter_map(|rest| rest.split(',').next())
            .filter_map(|n| n.parse().ok())
            .collect()
    }

    /// #1145. The tail is serialized once at write time so a panic hook and a
    /// `pagehide` listener — neither of which can touch a Bevy `Resource` —
    /// have something current to append a terminal marker to. It has to stay
    /// bounded on its own, because nothing rebuilds it from the ring any more.
    #[test]
    fn the_rolling_tail_evicts_oldest_first_and_stays_under_the_cap() {
        let mut tail = Tail::new();
        let line = "x".repeat(1000);
        // Comfortably past the cap, so eviction has to have happened.
        for seq in 0..(MAX_PERSIST_BYTES as u64 / 1000 + 10) {
            tail.push_event(seq, &line);
        }
        let nd = tail.ndjson();
        assert!(
            nd.len() <= EVENT_BUDGET_BYTES,
            "{} bytes is over the {EVENT_BUDGET_BYTES} event budget",
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
        tail.push_event(0, "{\"a\":1}");
        tail.push_event(1, "{\"b\":2}");
        assert_eq!(tail.ndjson(), "{\"a\":1}\n{\"b\":2}\n");
    }

    /// #1180. Sequence: a browser session runs long enough for the 1 Hz vitals
    /// drip to fill the tail — minutes, not hours, because a snapshot line
    /// carries every series in the registry — and then dies. Against one
    /// shared budget the recovered capture is almost entirely vitals: the
    /// events around the fault have been evicted by the instrument that was
    /// watching for the fault, and the terminal marker is the only real line
    /// left. This is the deployed wasm client's only instrument, and the long
    /// session is exactly when the browser OOM it was built for (#811, #565)
    /// happens.
    ///
    /// Both halves have to come back: the events say what happened, the
    /// series says what led there.
    #[test]
    fn the_vitals_drip_cannot_evict_the_events_it_is_recorded_beside() {
        let mut tail = Tail::new();

        // The event that explains the fault, recorded early and never
        // repeated — the worst case for an oldest-first eviction.
        let fault = line_of("event", 0, 200);
        tail.push_event(0, &fault);

        // Then the drip: twice the WHOLE cap, all of it snapshots.
        let sample_bytes = 4_000;
        let samples = (2 * MAX_PERSIST_BYTES / sample_bytes) as u64;
        for seq in 1..=samples {
            tail.push_snapshot(seq, &line_of("snap", seq, sample_bytes));
        }

        // And the terminal marker the panic hook appends as the tab dies.
        tail.push_event(
            crate::diagnostics::event::CRASH_MARKER_SEQ,
            &line_of("event", crate::diagnostics::event::CRASH_MARKER_SEQ, 200),
        );

        let nd = tail.ndjson();
        assert!(
            nd.len() <= MAX_PERSIST_BYTES,
            "the two budgets still sum to the {MAX_PERSIST_BYTES} cap: {} bytes",
            nd.len()
        );
        assert!(
            nd.lines().any(|l| l == fault),
            "the real event must outlive the drip that buried it"
        );

        let vitals = kept(&nd, "snap");
        assert_eq!(
            vitals.last().copied(),
            Some(samples),
            "and the newest vitals must still be there"
        );
        assert!(
            vitals.len() > 100,
            "a series, not a sample: only {} of {samples} kept",
            vitals.len()
        );
        assert!(
            vitals.windows(2).all(|w| w[1] == w[0] + 1),
            "the snapshot budget evicts oldest-first, so what survives is the \
             most recent unbroken run"
        );

        let events = kept(&nd, "event");
        assert_eq!(
            events,
            vec![0, crate::diagnostics::event::CRASH_MARKER_SEQ],
            "and the terminal marker is still the last line of the capture"
        );
    }

    /// The trade has to hold in both directions: a burst of real events must
    /// not take the vitals series with it either.
    #[test]
    fn a_burst_of_real_events_cannot_evict_the_vitals_series() {
        let mut tail = Tail::new();
        let sample_bytes = 4_000;
        for seq in 0..50 {
            tail.push_snapshot(seq, &line_of("snap", seq, sample_bytes));
        }
        let event = "e".repeat(1_000);
        for seq in 50..(50 + (2 * MAX_PERSIST_BYTES as u64 / 1_000)) {
            tail.push_event(seq, &event);
        }
        assert_eq!(
            kept(&tail.ndjson(), "snap").len(),
            50,
            "no snapshot is evicted while the snapshot budget has room"
        );
    }

    /// The analyzer reads NDJSON in seq order, so two independently-evicting
    /// queues have to be merged, not concatenated.
    #[test]
    fn the_two_budgets_render_merged_in_seq_order() {
        let mut tail = Tail::new();
        tail.push_event(0, "e0");
        tail.push_snapshot(1, "s1");
        tail.push_snapshot(2, "s2");
        tail.push_event(3, "e3");
        tail.push_snapshot(4, "s4");
        assert_eq!(tail.ndjson(), "e0\ns1\ns2\ne3\ns4\n");
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
