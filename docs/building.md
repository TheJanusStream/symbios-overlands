# Building & running

How to run Symbios Overlands natively, build the WebAssembly bundle, and use
the developer tooling. For what the project *is*, see the [README](../README.md);
for how it's put together, see [architecture.md](architecture.md).

To meet other players the client connects to a `bevy_symbios_multiuser` relay;
the login UI pre-fills a default public instance (editable in the login form).

## Native

```bash
cargo run --profile test-release        # the dev loop — use this one
cargo run --release                     # the shipping build; see the cost below
```

`--profile test-release` is the native dev loop and `--release` is not, which
is the opposite of the usual advice and worth a sentence. `[profile.release]`
here is tuned for exactly one artifact — the wasm bundle `wasm-bindgen` ships —
and carries `lto = "fat"` with `codegen-units = 1`. Measured on this codebase
that is **644 s and 7.97 GB peak RSS** for a link, against **4.75 s and
1.67 GB** for `test-release` ([the table below](#tests-and-quality-gates)).
`test-release` keeps release codegen — the terrain and avatar builds are
genuinely unbearable in debug — and drops only the whole-program link, so it
runs at full speed and relinks in seconds. Reach for `--release` when you want
the artifact, not while you are iterating.

(No `--bin` needed — `default-run` names the app. The crate ships a second
binary, the headless [render tool](#developer-tooling), which still wants
`--bin render`.)

On Linux the build links Bevy's default backends, so their dev packages have to
be present first — ALSA for `bevy_audio`, udev for input enumeration, and
Wayland plus libxkbcommon for `winit`:

```bash
sudo apt-get install -y libasound2-dev libudev-dev libwayland-dev libxkbcommon-dev
```

That is the same set [`ci.yml`](../.github/workflows/ci.yml) installs before it
can build the tests; use your distribution's equivalents elsewhere.

The native build also accepts the same parameters a landmark link encodes:

```bash
cargo run --profile test-release -- \
    --did=did:plc:example \
    --pos=10,5,-3 \
    --rot=90 \
    --pds=https://bsky.social \
    --relay=relay.example.com
```

`--pos` takes `x,z` — height resolved from the heightmap at spawn — or `x,y,z`
for an exact drop; `--rot` is the spawn yaw in degrees. `--pds` wants a URL and
`--relay` a bare host. `--did` alone is enough to drop into someone else's
overland.

## WebAssembly

```bash
rustup target add wasm32-unknown-unknown
# Pin the CLI to the `wasm-bindgen` crate version in Cargo.lock (0.2.127) —
# the CLI refuses a `.wasm` built against a different crate version, so a
# skew between the two breaks the deploy. Bump both together.
cargo install wasm-bindgen-cli --version 0.2.127

# `--workspace` builds the app *and* the off-thread generation Web Worker
# (the slim, no-Bevy `gen-worker`) for wasm in one pass.
cargo build --workspace --release --target wasm32-unknown-unknown

# Two wasm-bindgen passes: the app, then the worker the app spawns as
# `./gen-worker.js` (both land beside each other in ./dist).
wasm-bindgen --out-dir ./dist --target web --no-typescript \
    --out-name symbios-overlands \
    target/wasm32-unknown-unknown/release/symbios-overlands.wasm
wasm-bindgen --out-dir ./dist --target web --no-typescript \
    --out-name gen-worker \
    target/wasm32-unknown-unknown/release/gen-worker.wasm

# index.html imports ./symbios-overlands.js relative to itself, so
# assemble a flat site directory (mirrors .github/workflows/deploy.yml):
cp index.html dist/
cp -r assets dist/
cp assets/client-metadata.json dist/   # OAuth client metadata sits at the site root
```

Serve `./dist` with any static web server (e.g. `python -m http.server -d dist`).

Note that OAuth sign-in can't complete from a locally served bundle: the OAuth
client metadata pins the redirect URI to the public deployment
(`https://thejanusstream.github.io/symbios-overlands`), so the login
round-trip lands there rather than back on `localhost`. The native build
sidesteps this entirely — it registers the loopback-client `client_id` pattern
instead of the hosted document, opens your system browser, and catches the
redirect on a local listener at `http://127.0.0.1:3456/callback` — so native
sign-in works from a checkout as long as port 3456 is free.

[`deploy.yml`](../.github/workflows/deploy.yml) pins the same version.
Unpinned, the workflow would sit one upstream release away from the CLI being
*newer* than the crate. Note what the pin does and does not buy, because
`Cargo.lock` is git-ignored: the CI checkout has no
lockfile, so `cargo build` there resolves `wasm-bindgen` fresh to the newest
semver-compatible release, while the CLI version is a hand-maintained literal
in the workflow. The two are pinned together *today* (both 0.2.127) and drift
apart on the next upstream release. Patch-level skew has been tolerated in
practice across several deploys, so this is a latent risk rather than a
standing breakage — but if a deploy starts producing glue that fails at
`init`, check that pair first, and bump the workflow literal to whatever a
fresh local resolve puts in `Cargo.lock`.

## Working against a sibling crate

Every `symbios-*` dependency is a published crates.io version, the avatar pair
(`symbios-avatar`, `bevy_symbios_avatar`) included. Overlands tracks whatever
version `Cargo.toml` pins — read it there, not here, so this page cannot go
stale against it.

To develop one of them against overlands without publishing, add a temporary
override to the workspace root `Cargo.toml` rather than editing the dependency
tables:

```toml
[patch.crates-io]
symbios-avatar = { path = "../symbios-avatar" }
bevy_symbios_avatar = { path = "../bevy_symbios_avatar" }
```

**A patch whose version does not match what the graph asks for is silently
ignored.** Cargo does not error on it — it quietly uses the registry crate
instead, so a green run can be testing against a sibling that is not in the
build at all. Check `Cargo.lock` names the path override before believing a
result that depends on it.

Keep the patch out of any commit that is going to be deployed — it is invisible
in the dependency list, and a build that resolves it will not reproduce
anywhere else. `crates/gen-jobs` depends on `symbios-avatar` too (with the
`serde-avatar` feature, for `GenJob::AvatarBuild`); a root `[patch.crates-io]`
covers the whole workspace, so it needs no separate override.

## Tests and quality gates

```bash
cargo fmt --all -- --check                             # formatting (CI blocks on it)
cargo clippy --all-targets -- -D warnings              # lint, exactly as CI runs it
cargo test --lib                                       # unit tests (fast path)
cargo nextest run --cargo-profile test-release         # the full suite (see below)
cargo test --profile test-release --doc                # doctests: nextest cannot run them
cargo doc --no-deps --document-private-items           # docs (kept warning-free)
cargo check --workspace --target wasm32-unknown-unknown # app + worker still build for web
```

**`cargo test --lib` is a separate gate, not a subset of the nextest run.**
nextest forks a process per test, so every process-global — the panic shadow,
the allocation counters, the offload census, Bevy's task pools — gets a fresh
copy, and a test that depends on one passes there unconditionally. CI runs
bare `cargo test`, which threads the whole lib through a single process, and
that is the only runner able to see a test reading state another test wrote.
After touching anything process-global, run it more than once.

Every one of those bare invocations covers `crates/gen-jobs` as well as the
app, because `[workspace] default-members` names both. Without it a
non-virtual workspace selects the root package alone, and gen-jobs' tests —
the determinism and worker-wire round trips that are the *only* check of the
native/wasm byte-identical claim in `src/offload.rs` — run in neither the
local gate nor CI. If you add a crate under `crates/` and it has tests, add it
to `default-members` or it is untested by default.

One check is not a cargo subcommand:

```bash
cargo tree -i openssl   # must report only what proto-blue drags in, and no more
```

Overlands declares reqwest with `default-features = false, features =
["rustls-tls"]`, but `proto-blue-common`/`-oauth`/`-xrpc` declare it *with*
defaults, which unifies `default-tls` back into the graph — and reqwest picks
native-tls whenever `default-tls` is present. So OpenSSL is linked, and
without care it is the backend every native PDS and OAuth request uses.
`default_client` calls `.use_rustls_tls()` explicitly, which fixes the runtime
choice but not the graph: getting OpenSSL out needs proto-blue to declare
`default-features = false` upstream. Run the line above after a
dependency bump so a *new* path to native-tls is noticed rather than
inherited. On wasm none of this applies — reqwest ignores TLS features there,
which is why it went unseen for so long.

The gate profile runs `debug-assertions` and `overflow-checks` **on**, and
must keep doing so. A bare `inherits = "release"` turns both off, which would
leave the `debug_assert!` sites in the crate silent locally while CI's plain
`cargo test` runs them in debug — two gates disagreeing about what the code
asserts, with the weaker one being the one anybody runs.

**Never run the full suite under plain `--release`.** `[profile.release]` is
tuned for one artifact — the wasm bundle `wasm-bindgen` ships — and carries
`lto = "fat"` with `codegen-units = 1`. `cargo test --release` inherits that,
so every test binary pays a whole-program LTO link of the entire Bevy engine.
Measured on one binary, same machine, same relink:

| profile | wall | peak RSS |
| --- | --- | --- |
| `release` (fat LTO) | 644 s | 7.97 GB |
| `test-release` | 4.75 s | 1.67 GB |

At `build.jobs = 6` that is six concurrent 8 GB links — which is what the jobs
pin below is really protecting against. `[profile.test-release]` keeps release
codegen (the avatar and terrain builds in the suite are unbearable in debug) and
drops only the whole-program link. Note that `[profile.bench]` does **not** work
as an override here: despite the folklore, cargo builds `--release` test targets
with `profile.release`.

`cargo nextest run` does not run doctests — that is a known upstream limitation,
not a configuration gap — so the separate `--doc` line above is part of the gate,
not optional. Without it the single live doctest in `src/diagnostics/anomaly/`
stops being covered.

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs `cargo fmt --all
-- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and the
doc gate on every push and pull request — a stray blank line fails the build
before a single test runs — plus a separate `wasm` job running the wasm32
check. The doc gate is where link rot gets caught, and the wasm check is the
cheap half of the deploy — both are enforced rather than left as local
conventions, because a local convention is the first step skipped on a busy
day.

The wasm check is deliberately `--workspace` rather than riding on
`default-members`: `--lib` alone would skip `gen-worker`'s binary target, and
a break there surfaces only when the Pages deploy runs `wasm-bindgen` over a
`.wasm` that was never built.

CI does **not** pass `--locked`, and should not: `Cargo.lock` is untracked by
decision (see `.gitignore`), so `--locked` would fail outright for want of a
lockfile to honour.

### The dependency-bump checklist

Two things the seven-command gate does not cover.

**Fire the avian canary.** One test is `#[ignore]`d on purpose:
`plain_rigid_body_disabled_cycle` in
[`tests/freeze_rigid_body.rs`](../tests/freeze_rigid_body.rs) reproduces an
unfixed upstream island-corruption bug — inserting and then removing
`RigidBodyDisabled` on a body with touching contacts. Both the avatar
visuals-edit freeze and the deferred collider rebuild that rides on it exist
only to route around it. Nothing in the gate or in ci.yml runs ignored tests,
so it is fired by hand:

```bash
cargo nextest run --cargo-profile test-release \
    --run-ignored ignored-only -E 'test(plain_rigid_body_disabled_cycle)'
```

The test's own doc block records which avian version it was last run against
and what happened. While it still fails, both workarounds stay; if it ever
*passes*, upstream has fixed the bug and they can be retired.

Beside it, `the_canary_names_the_avian_version_the_build_actually_resolved` is
not ignored, runs in every gate and costs nothing: it fails the moment
`avian3d` resolves to a version the canary has not been run against
(`build.rs` reads that version out of `Cargo.lock`, the only place a dependent
can see a dependency's resolved version). It cannot fire the canary for you —
what it does is stop a bump from being silent, which is how an ignored test
becomes a comment.

**Re-read the doc prose.** `cargo doc` checks that links resolve, not that
sentences are true, so a bump that changes a version literal or an upstream
file path leaves the `//!` headers and this file describing the old world.
Nothing in the gate can catch that.

The other three `#[ignore]`d tests are probes rather than canaries — they
assert nothing and print measurements — so nothing is owed for them.

### One integration target

Every file in `tests/` is a `mod` of [`tests/main.rs`](../tests/main.rs), which
is the only `[[test]]` target the package declares, and both binaries carry
`test = false`. Each integration target statically links the whole engine, so a
target per file would mean two dozen Bevy links on any `cargo test` that
touches the lib; one target links once. Measured at `build.jobs = 6`, `touch
src/lib.rs && cargo test --no-run` costs **39 s** rather than **1 m 56 s**, and
1 m 34 s of CPU rather than 7 m 45 s.

Because there is one target, targeting a single file's tests is a name filter
rather than a target flag — the module path is part of every test's name:

```bash
cargo test --test integration pds_sanitize
cargo nextest run -E 'test(publish_snapshot::)'
```

`tests/main.rs` carries the rule for what may be added there: one target means
one process under `cargo test`, so a test that needs a private copy of a
process-global — the panic shadow, the allocation counters, the offload census
— is not safe as a module and needs its own target with the reason written
down. nextest still runs each test in its own process either way.

Note: [`.cargo/config.toml`](../.cargo/config.toml) pins `build.jobs = 6` —
each target links a full Bevy binary, and an uncapped parallel link can exhaust
RAM on smaller machines. With one integration target the suite itself is cheap,
but the app and the render bin still link full engines beside it. The same file
carries the `getrandom_backend="wasm_js"` rustflag the wasm build needs:
`symbios-avatar` pulls `getrandom` 0.3 transitively, 0.3 refuses to build for
`wasm32-unknown-unknown` without a cfg naming its backend, and cargo configs do
not propagate from a dependency to its dependents. A wasm build run from
outside the repo root will not see either setting.

## Cargo features

The crate ships one optional feature, `alloc-trace` (native only). It wraps the
global allocator and prints a backtrace for every allocation of 16 MiB or more —
the first 24 in full, then a one-line size report every 128th, so a per-frame
churn doesn't drown stderr:

```bash
cargo run --release --bin symbios-overlands --features alloc-trace 2>/tmp/alloc.log
```

It exists to put names to the giant-buffer churn the wasm allocation tracker can
only count: wasm cannot produce a callstack, and the same code runs natively. No
`RUST_BACKTRACE` needed — the tracer force-captures. Off by default and
zero-cost in ordinary builds; see [diagnostics.md](diagnostics.md) for the
memory metrics it complements.

## Developer tooling

**Headless render tool** — renders any avatar / catalogue entry / primitive /
room / dumped generator JSON through the real spawn path into a multi-angle
contact-sheet PNG, so geometry and materials can be validated without in-game
screenshots:

```bash
cargo run --bin render -- --catalogue medieval_castle
cargo run --bin render -- --avatar did:plc:example
cargo run --bin render -- --prim cuboid
cargo run --bin render -- --room 3            # whole seeded room, by seed or DID
cargo run --bin render -- --wear satchel      # a wearable, actually worn
cargo run --bin render -- --generator /tmp/x.json  # a dumped + edited Generator
```

When more than one subject is given the highest-precedence one wins:
`--generator` > `--room` > `--prim` > `--wear` > `--catalogue` > `--avatar`,
with the no-render modes below running ahead of all of them. That order is
asserted by `render_tool`'s own tests, so it is checkable rather than a claim.

`--wear <slug>` is the attachments instrument, and the surface the
catalogue-item wear loop is judged from. It dresses seeded rigged bodies in a
catalogue wearable and sheets one body per row, so a garment is seen on the
anatomy it has to fit rather than floating alone:

```bash
cargo run --bin render -- --wear satchel --wear-bodies 6
cargo run --bin render -- --wear satchel --wear-socket hand_r
```

`--wear-bodies N` sets how many bodies (default 4). `--wear-socket <engine
socket>` overrides the entry's own `wear_socket()`, which is also how you sheet
an entry that has no wear socket at all — without it such a slug is refused by
name. Sheets are labelled `wear-<slug>-<socket>`.

`--avatar` draws `Generator` trees, so it covers the *vehicle* seeds only —
boat, airship and skiff. A humanoid seed rolls a rigged
`symbios-avatar` body with no tree to walk, and the tool refuses it by name
rather than rendering an empty sheet; the sibling `bevy_symbios_avatar`
viewer's own `--shot` capture is that body's instrument. `--family-seeds` will
find you a vehicle seed to render.

Sheets land in `/tmp/avatar-render/<label>.png`. `--out` replaces that whole
path — it names a `.png` file rather than a directory, and its parent must
already exist — and `--size` sets the per-tile pixel side (default 512, rounded
down to a multiple of 64 so the GPU readback needs no row padding). `--prim`
also accepts the cut/deform overrides (`--hollow`, `--twist`, `--pathcut`, …)
listed by `--help`.

Three more flags change what a sheet contains rather than where it lands.
`--elev <deg>` lifts the camera off its default low orbit (roughly 13°) — the
only way to see into anything open-topped, a brazier or a well or a bowl.
`--ages 2,3,5,7` renders one row per L-system iteration count, every row framed
at one shared camera distance so relative plant size across ages stays honest.
`--variant <name>`, alongside `--catalogue <plant-slug>`, applies that entry's
named material re-skin before rendering — materials only, never geometry, so it
composes with `--ages`; `--variant list` prints the entry's variants and exits.

Plant work has its own guide: [lsystem-playbook.md](lsystem-playbook.md) takes a
species request to a finished grammar — the four mileage levers, the engine's
traps, and the `--dump` → edit → `--generator` loop that iterates a grammar
without recompiling.

The same binary hosts the offline, no-render text modes. None of these stands up
a render app, so they all run on a machine with no GPU:

```bash
# Post-mortem of a session log (see docs/diagnostics.md):
cargo run --bin render -- --analyze-session diagnostics/session-latest.jsonl
# Before/after comparison of two runs:
cargo run --bin render -- --diff-sessions old.jsonl new.jsonl
# Road-network graph diagnostics for a seed or DID:
cargo run --bin render -- --road-dump 1
# List seeds that produce a given avatar chassis family:
cargo run --bin render -- --family-seeds skiff --family-count 8
# Dump a catalogue entry's generator JSON (edit + re-render via --generator):
cargo run --bin render -- --dump --catalogue neon_kiosk
# Print one avatar's resolved outfit (chassis / style / socio tiers / slot→slug):
cargo run --bin render -- --outfit 7
# Scan seeds for one that rolls a styled part (capped by --family-count):
cargo run --bin render -- --find-part boat_bow_ram
# Gateway veil-vs-frame fit report — one slug, or `all`:
cargo run --bin render -- --gateway-fit all
# Plinth-depth audit of every settlement-placeable entry (`all` lists the
# passing rows too; any other word shows only the shortfalls):
cargo run --bin render -- --foundation-audit all
# Terrain drop real seeded settlements span, measured over N seeds:
cargo run --bin render -- --settlement-drop 200
# Analytic entity census over seeds 0..N — what a room will actually spawn:
cargo run --bin render -- --room-census 32
# Placement census — yield, what the slope cutoff costs, Clark–Evans clustering:
cargo run --bin render -- --scatter-census 8
# Plan-view plot of one room's scatters (PNG to --out, else ./scatter-plot.png):
cargo run --bin render -- --scatter-plot 3 --out /tmp/scatter.png
```

`--outfit`, `--find-part`, `--room-census`, `--foundation-audit` and
`--gateway-fit` only roll records or build catalogue trees, so they return
quickly; `--scatter-census`, `--scatter-plot` and `--settlement-drop` rebuild
each seed's heightmap, which costs a few seconds per seed.

**Session logs** — the app records an append-only NDJSON session log
(`diagnostics/session-latest.jsonl` on native; downloadable from the
Diagnostics panel on web). [diagnostics.md](diagnostics.md) documents the file
locations, environment overrides, schema, and the analyzer.
