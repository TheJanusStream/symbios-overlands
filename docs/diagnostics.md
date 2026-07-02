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

## Reading a log (the analyzer)

The offline analyzer ([Pillar B](../src/diagnostics/analyze.rs)) turns a captured
NDJSON log into an agent-facing post-mortem. It is a no-render subcommand of the
`render` bin, so it needs no GPU and reads the native file or the WASM download
interchangeably. A torn/truncated log is analyzed best-effort (unparseable lines
are counted and surfaced, never fatal).

### Post-mortem — `--analyze-session`

```sh
cargo run --bin render -- --analyze-session diagnostics/session-latest.jsonl
```

prints, in order:

| Section | What it tells you |
| --- | --- |
| header | `session-id` / `did` / `build` (version·sha·arch·profile) / `duration` / `exit` reason (or a crash/truncation note). |
| `[Verdict]` | `HEALTHY`, or the count of `warning` / `error` / `critical` events. |
| `[Event Tallies]` | A `subsystem × severity` matrix + a by-category line — *where* the noise came from. The 1 Hz metric snapshots are excluded (and the count noted) so they don't bury the counts. |
| `[Timeline]` | The milestone events (loading gate, fetches, heightmap/ambient/world-compile, `→ InGame`, portals, segment resets, session end) at their timestamps. |
| `[Loading Gate]` | The Login → Loading → InGame gate time, plus each heavy loading stage's duration distribution (`min/p50/p90/max/mean`). |
| `[Metric Trends]` | The gauge/counter/histogram series charted from the periodic `MetricsSnapshot` records — memory-growth curve, frame-time percentiles, entity/asset drift (the leak signal). |
| `[Invariant Violations]` | The anomaly rules **replayed** over the log (offline re-derived) plus any captured live-only fires — the offline counterpart to the in-game anomaly engine. |

### Filters

Restrict the *analysis* sections to a slice of the log (the header always shows
the full run's identity). An invalid filter name aborts with a clear message.

| Flag | Effect |
| --- | --- |
| `--subsystem <s>` | `loading` \| `network` \| `offload` \| `runtime` \| `session` |
| `--category <c>` | `lifecycle` \| `fetch` \| `generation` \| `audio` \| `peer` \| `transport` \| `offer` \| `chat` \| `social` \| `job` \| `physics` \| `asset` \| `perf` \| `portal` \| `anomaly` \| `snapshot` |
| `--severity <min>` | minimum severity — `trace` \| `info` \| `warn` \| `error` \| `critical` (matches that level *and above*) |
| `--since <secs>` / `--until <secs>` | inclusive session-relative time window (`t_mono_secs`) |

```sh
# Just the network events, warnings and worse, in the first two minutes:
cargo run --bin render -- --analyze-session diagnostics/session-latest.jsonl \
  --subsystem network --severity warn --until 120
```

A `[Filter]` line then documents the active lens and how many of the total events
matched. Only the header (session identity) is derived from the full run;
**every** section below it — `[Verdict]`, `[Event Tallies]`, `[Timeline]`,
`[Loading Gate]`, `[Metric Trends]` and `[Invariant Violations]` — folds only the
matching subset. In particular, a narrow time window (`--since`/`--until`) that
clips the `→ InGame` transition can make `[Loading Gate]` report *“did not reach
InGame (stalled or truncated log)”* for the slice even though the full run reached
it; read the `[Filter]` match count as your cue that these sections describe a
subset, not the whole session.

### Before/after diff — `--diff-sessions`

```sh
cargo run --bin render -- --diff-sessions baseline.jsonl candidate.jsonl
```

diffs two logs (A = baseline, B = candidate) so an agent can confirm a fix in B
improved on A. It prints `[Verdict Delta]` (event counts + an improved/regressed
read), `[Loading Gate Delta]` (gate + per-stage mean A → B), `[Metric Delta]`
(gauge peaks + counter totals A → B), and `[Invariant Delta]` (per-rule fire
counts A → B, tagged `NEW` / `worse` / `resolved` / `better` / `same`, regressions
first). Filters apply to `--analyze-session`, not to the diff.

## Adding an invariant rule

Anomalies are detected by **invariant rules** — one definition runs both live
(GUI badges + logged `InvariantViolation` events) and offline (replayed by the
analyzer). Adding one is a three-step recipe (define a `Rule` with a
`const RuleHeader`; add a `LiveCtx` field only if a new reading is needed; one
`register()` line), with a fully-worked example, in the module docs for
[`src/diagnostics/anomaly/`](../src/diagnostics/anomaly/mod.rs) — run
`cargo doc --no-deps --document-private-items --open` and open the `anomaly`
module.
