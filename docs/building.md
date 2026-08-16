# Building & running

How to run Symbios Overlands natively, build the WebAssembly bundle, and use
the developer tooling. For what the project *is*, see the [README](../README.md);
for how it's put together, see [architecture.md](architecture.md).

To meet other players the client connects to a `bevy_symbios_multiuser` relay;
the login UI pre-fills a default public instance (editable in the login form).

## Native

```bash
cargo run --release --bin symbios-overlands
```

(The `--bin` is required — the crate ships a second binary, the headless
[render tool](#developer-tooling), so a bare `cargo run` is ambiguous.)

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
cargo run --release --bin symbios-overlands -- \
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

[`deploy.yml`](../.github/workflows/deploy.yml) pins the same version (#1075) —
it used to install the CLI unpinned, which put the workflow one upstream
release away from a broken deploy. The pin is a literal in the workflow and the
crate version lives in `Cargo.lock`, so they can still drift apart: after a
`wasm-bindgen` bump, update both. If a deploy starts producing glue that fails
at `init`, check that pair first.

## Working against a sibling crate

Every `symbios-*` dependency is a published crates.io version, including the
avatar pair (`symbios-avatar`, `bevy_symbios_avatar`) that shipped as path deps
through epic #1054 and were published as 0.1.0 for this release (#1075). To
develop one of them against overlands without publishing, add a temporary
override to the workspace root `Cargo.toml` rather than editing the dependency
tables:

```toml
[patch.crates-io]
symbios-avatar = { path = "../symbios-avatar" }
bevy_symbios_avatar = { path = "../bevy_symbios_avatar" }
```

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

**Never run the full suite under plain `--release`** (#1064). `[profile.release]`
is tuned for one artifact — the wasm bundle `wasm-bindgen` ships — and carries
`lto = "fat"` with `codegen-units = 1`. `cargo test --release` inherits that, so
each of the ~24 test binaries pays a whole-program LTO link of the entire Bevy
engine. Measured on one binary, same machine, same relink:

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
-- --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test` on
every push and pull request — a stray blank line fails the build before a
single test runs. `cargo doc` and the wasm check are local conventions CI does
not cover, so run them yourself before pushing. The wasm check is deliberately
`--workspace`: `--lib` alone would skip `gen-worker`'s binary target, and a
break there surfaces only when the Pages deploy runs `wasm-bindgen` over a
`.wasm` that was never built.

One test is `#[ignore]`d on purpose: `plain_rigid_body_disabled_cycle` in
[`tests/freeze_rigid_body.rs`](../tests/freeze_rigid_body.rs) reproduces the
upstream avian 0.6 island-corruption bug that the avatar-freeze path works
around. Run `cargo test --test freeze_rigid_body -- --ignored` after an avian
or Bevy bump — if it passes, the workaround can go.

Note: [`.cargo/config.toml`](../.cargo/config.toml) pins `build.jobs = 6` —
each test target links a full Bevy binary, and an uncapped parallel link can
exhaust RAM on smaller machines. It also carries the
`getrandom_backend="wasm_js"` rustflag the wasm build needs: `symbios-avatar`
pulls `getrandom` 0.3 transitively, 0.3 refuses to build for
`wasm32-unknown-unknown` without a cfg naming its backend, and cargo configs do
not propagate from a dependency to its dependents (#1055). A wasm build run
from outside the repo root will not see either setting.

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
cargo run --bin render -- --generator /tmp/x.json  # a dumped + edited Generator
```

When more than one subject is given the highest-precedence one wins:
`--generator` > `--room` > `--prim` > `--catalogue` > `--avatar`, with the
no-render modes below running ahead of all of them.

`--avatar` draws `Generator` trees, so since #1060 it covers the *vehicle*
seeds only — boat, airship and skiff. A humanoid seed rolls a rigged
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
