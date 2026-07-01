# Diagnostic session log

The game records an append-only stream of typed **session events** — everything
notable between launch and exit (loading progress, peer join/leave, record
fetches, offloaded jobs, portal hops, anomalies, periodic metric snapshots).
The stream has three consumers that all read the *same* records, so they can
never disagree:

1. the in-game **Diagnostics panel** (a bounded tail view — Identity tab → *Event Log*),
2. a durable **NDJSON file** a coding agent reads for a post-mortem (native), and
3. the offline **`--analyze-session` analyzer** (Pillar B).

The module lives in [`src/diagnostics/`](../src/diagnostics/); the event model
is [`src/diagnostics/event.rs`](../src/diagnostics/event.rs).

## Where to find it

### Native

The sink appends one JSON line per event to a directory (default `diagnostics/`,
**relative to the working directory** — the repo root in normal use):

| File | Purpose |
| --- | --- |
| `diagnostics/session-latest.jsonl` | **Stable path — always the newest run.** Refreshed (copied) on every flush. Point an agent here. |
| `diagnostics/session-<start>[-<did>].jsonl` | Timestamped per-session file; the DID slug is included once the session authenticates. |
| `diagnostics/session-panic-<pid>-<millis>.jsonl` | Written by the panic hook if the process crashes: the recent event tail plus a synthetic crash marker (`seq: 18446744073709551615`, i.e. `u64::MAX`), so the tail survives an unflushed `BufWriter`. |

The `diagnostics/` directory is **git-ignored** ([`.gitignore`](../.gitignore))
and, unlike `target/`, survives `cargo clean` — a post-mortem file is not wiped
by an unrelated rebuild.

Flushing is best-effort and automatic: at least every
`FLUSH_INTERVAL_SECS` (2 s) or every `FLUSH_EVERY_N_EVENTS` (64) events,
whichever comes first, plus a final `SessionEnd` record on clean exit. A hard
kill therefore loses at most a couple of seconds of tail.

### WASM (web build)

There is no filesystem, so the in-memory ring buffer *is* the log. Open the
**Diagnostics panel → Identity tab → “Download session log”** to save a
`.jsonl` file that is byte-for-byte compatible with the native one (same
analyzer reads both).

## Environment controls

| Variable | Effect |
| --- | --- |
| `SYMBIOS_DIAG_DIR=<path>` | Write the log to `<path>` instead of `diagnostics/` (e.g. a durable location outside the repo). |
| `SYMBIOS_DIAG=0` | Disable native file persistence entirely (tests / CI). The in-memory ring + GUI still work. |

Constants: [`src/config.rs` → `config::diagnostics`](../src/config.rs).

## JSONL schema

Every line is one [`SessionEvent`](../src/diagnostics/event.rs) object. Blank
lines are ignored and a torn/renamed/truncated line is skipped-and-counted
rather than fatal, so a crashed-mid-write tail still analyzes.

```jsonc
{
  "seq": 0,                    // gap-free per-process counter (detects a torn tail)
  "t_mono_secs": 12.34,        // session-relative seconds (monotonic; Time::elapsed_secs_f64)
  "wall_ms": 1751500000000,    // unix-epoch millis for cross-run correlation, or null
  "subsystem": "Network",      // Loading | Network | Offload | Runtime | Session
  "category":  "Peer",         // Lifecycle | Fetch | Generation | Audio | Peer | Transport |
                               //   Offer | Chat | Social | Job | Physics | Asset | Perf |
                               //   Portal | Anomaly | Snapshot
  "severity":  "Info",         // Trace | Info | Warn | Error | Critical
  "payload":   { "kind": "PeerJoined", "peer": "peer:3" }
}
```

- **`payload`** is internally tagged: the `kind` field names the variant and the
  remaining fields are that variant's data. `subsystem` and `category` are
  *derived* from the payload, so the three form a consistent
  `subsystem × category × severity` filter triple.
- Build/environment context is carried by `StartupSnapshot` records: a
  `Boot`-phase one is the process's first event (`seq: 0`) with version, git
  sha, arch, profile and wasm flag, and a `Session`-phase one follows on login
  once the DID/relay are known. Together they key a run to a build + identity.
- Periodic `MetricsSnapshot` lines (severity `Trace`) are file/analyzer-only
  telemetry and are filtered out of the in-game Event Log.

The full set of `kind` values (several dozen — `LoadingGate*`, `RecordFetch*`,
`ItemOffer*`, `AvatarFetch*`, `Portal*`, `InvariantViolation`, …) is the
`EventPayload` enum in [`src/diagnostics/event.rs`](../src/diagnostics/event.rs);
that file is the authoritative schema.

## Reading a log

```sh
cargo run --bin render -- --analyze-session diagnostics/session-latest.jsonl
```

prints a post-mortem: a build/session header, a health `[Verdict]`, and an
`[Invariant Violations]` section (see the Pillar B analyzer). The same NDJSON is
the input to every analyzer subcommand.
