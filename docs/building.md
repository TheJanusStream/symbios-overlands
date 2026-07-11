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

The native build also accepts the same parameters a landmark link encodes:

```bash
cargo run --release --bin symbios-overlands -- \
    --did=did:plc:example \
    --pos=10,5,-3 \          # x,z (heightmap-resolved) or x,y,z (exact)
    --rot=90 \               # spawn yaw in degrees
    --pds=https://bsky.social \
    --relay=relay.example.com
```

`--did` alone is enough to drop into someone else's overland.

## WebAssembly

```bash
rustup target add wasm32-unknown-unknown
# Keep the CLI version aligned with the `wasm-bindgen` crate in Cargo.lock —
# a version skew can break the generated JS glue.
cargo install wasm-bindgen-cli

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
round-trip lands there rather than back on `localhost`.

## Tests and quality gates

```bash
cargo test --lib                                  # unit tests (fast path)
cargo test                                        # + the integration tests in tests/
cargo clippy --lib --tests                        # lint (kept warning-free)
cargo doc --no-deps --document-private-items      # docs (kept warning-free)
cargo check --lib --target wasm32-unknown-unknown # the web target still compiles
```

Note: [`.cargo/config.toml`](../.cargo/config.toml) pins `build.jobs = 6` —
each integration-test file links a full Bevy binary, and an uncapped parallel
link can exhaust RAM on smaller machines.

## Developer tooling

**Headless render tool** — renders any avatar / catalogue entry / primitive /
room through the real spawn path into a multi-angle contact-sheet PNG, so
geometry and materials can be validated without in-game screenshots:

```bash
cargo run --bin render -- --catalogue medieval_castle
cargo run --bin render -- --avatar did:plc:example
cargo run --bin render -- --prim cuboid
cargo run --bin render -- --room 3      # whole seeded room, by seed or DID
```

Sheets land in `/tmp/avatar-render/<label>.png`; override the directory with
`--out` and the tile size with `--size`. `--prim` also accepts the cut/deform
overrides (`--hollow`, `--twist`, `--pathcut`, …) listed by `--help`.

The same binary hosts the offline, no-render text modes:

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
```

**Session logs** — the app records an append-only NDJSON session log
(`diagnostics/session-latest.jsonl` on native; downloadable from the
Diagnostics panel on web). [diagnostics.md](diagnostics.md) documents the file
locations, environment overrides, schema, and the analyzer.
