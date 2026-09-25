# Building & running

How to run Symbios Overlands natively, build the WebAssembly bundle, and use
the developer tooling. For what the project *is*, see the [README](../README.md);
for how it's put together, see [architecture.md](architecture.md).

To meet other players the client connects to a `bevy_symbios_multiuser` relay;
the login UI pre-fills a default public instance (editable in the login form).

## Native

```bash
cargo run --profile test-release        # the dev loop - use this one
cargo run --release                     # the shipping build; see the cost below
```

`--profile test-release` is the native dev loop and `--release` is not, which
is the opposite of the usual advice and worth a sentence. `[profile.release]`
here is tuned for exactly one artifact - the wasm bundle `wasm-bindgen` ships -
and carries `lto = "fat"` with `codegen-units = 1`. Measured on this codebase
that is **644 s and 7.97 GB peak RSS** for a link, against **4.75 s and
1.67 GB** for `test-release` ([the table below](#tests-and-quality-gates)).
`test-release` keeps release codegen - the terrain and avatar builds are
genuinely unbearable in debug - and drops only the whole-program link, so it
runs at full speed and relinks in seconds. Reach for `--release` when you want
the artifact, not while you are iterating.

(No `--bin` needed - `default-run` names the app. The crate ships a second
binary, the headless [render tool](#developer-tooling), which still wants
`--bin render`.)

The two profiles also differ in what a Bevy **command error** does, which is
deliberate (#1412). Bevy routes an error nothing else handled - a command whose
target was despawned before the queue applied, a system missing a required
parameter - to a fallback handler that panics by default, and that is how a
player's session ended in #1410. The shipped build (`--release`, and the
deployed wasm bundle) installs a handler that logs at `error` instead, so one
stray command cannot abort somebody's session. The dev loop
(`--profile test-release`, which turns `debug_assertions` back on) and the whole
test suite keep the panicking default, because that is where such a failure
should be impossible to walk past. `run()` is the only place allowed to install
the handler, and `gate_contract::only_the_shipped_build_stops_panicking_on_a_command_error`
fails if that stops being true - a handler reachable from a test would leave the
regression tests for #1410 / #1411 asserting nothing.

On Linux the build links Bevy's default backends, so their dev packages have to
be present first - ALSA for `bevy_audio`, udev for input enumeration, and
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

`--pos` takes `x,z` - height resolved from the heightmap at spawn - or `x,y,z`
for an exact drop; `--rot` is the spawn yaw in degrees. `--pds` wants a URL and
`--relay` a bare host. `--did` alone is enough to drop into someone else's
overland.

## WebAssembly

```bash
rustup target add wasm32-unknown-unknown
# Pin the CLI to the `wasm-bindgen` crate version in Cargo.lock (0.2.128) -
# the CLI refuses a `.wasm` built against a different crate version, so a
# skew between the two breaks the deploy. Bump both together.
cargo install wasm-bindgen-cli --version 0.2.128

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
sidesteps this entirely - it registers the loopback-client `client_id` pattern
instead of the hosted document, opens your system browser, and catches the
redirect on a local listener at `http://127.0.0.1:3456/callback` - so native
sign-in works from a checkout as long as port 3456 is free.

[`deploy.yml`](../.github/workflows/deploy.yml) pins the same version.
Unpinned, the workflow would sit one upstream release away from the CLI being
*newer* than the crate. Note what the pin does and does not buy, because
`Cargo.lock` is git-ignored: the CI checkout has no
lockfile, so `cargo build` there resolves `wasm-bindgen` fresh to the newest
semver-compatible release, while the CLI version is a hand-maintained literal
in the workflow. The two are pinned together *today* (both 0.2.128) and drift
apart on the next upstream release. Patch-level skew has been tolerated in
practice across several deploys, so this is a latent risk rather than a
standing breakage - but if a deploy starts producing glue that fails at
`init`, check that pair first, and bump the workflow literal to whatever a
fresh local resolve puts in `Cargo.lock`.

## Working against a sibling crate

Every `symbios-*` dependency is a published crates.io version, the avatar pair
(`symbios-avatar`, `bevy_symbios_avatar`) included. Overlands tracks whatever
version `Cargo.toml` pins - read it there, not here, so this page cannot go
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
ignored.** Cargo does not error on it - it quietly uses the registry crate
instead, so a green run can be testing against a sibling that is not in the
build at all. Check `Cargo.lock` names the path override before believing a
result that depends on it.

Keep the patch out of any commit that is going to be deployed - it is invisible
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
# app + worker still build for web. CI's toolchain action exports this
# RUSTFLAGS, so a warning there is an error; the env replaces
# .cargo/config.toml's per-target rustflags, exactly as it does on CI (#1321)
RUSTFLAGS='-D warnings' cargo check --workspace --target wasm32-unknown-unknown
```

**`cargo test --lib` is a separate gate, not a subset of the nextest run.**
nextest forks a process per test, so every process-global - the panic shadow,
the allocation counters, the offload census, Bevy's task pools - gets a fresh
copy, and a test that depends on one passes there unconditionally. CI runs
bare `cargo test`, which threads the whole lib through a single process, and
that is the only runner able to see a test reading state another test wrote.
After touching anything process-global, run it more than once.

Every one of those bare invocations covers `crates/gen-jobs` as well as the
app, because `[workspace] default-members` names both. Without it a
non-virtual workspace selects the root package alone, and gen-jobs' tests -
the determinism and worker-wire round trips that are the *only* check of the
native/wasm byte-identical claim in `src/offload.rs` - run in neither the
local gate nor CI. If you add a crate under `crates/` and it has tests, add it
to `default-members` or it is untested by default.

One check is not a cargo subcommand:

```bash
cargo tree -i openssl   # must report only what proto-blue drags in, and no more
```

Overlands declares reqwest with `default-features = false, features =
["rustls-tls"]`, but `proto-blue-common`/`-oauth`/`-xrpc` declare it *with*
defaults, which unifies `default-tls` back into the graph - and reqwest picks
native-tls whenever `default-tls` is present. So OpenSSL is linked, and
without care it is the backend every native PDS and OAuth request uses.
`default_client` calls `.use_rustls_tls()` explicitly, which fixes the runtime
choice but not the graph: getting OpenSSL out needs proto-blue to declare
`default-features = false` upstream. Run the line above after a
dependency bump so a *new* path to native-tls is noticed rather than
inherited. On wasm none of this applies - reqwest ignores TLS features there,
which is why it went unseen for so long.

The gate profile runs `debug-assertions` and `overflow-checks` **on**, and
must keep doing so. A bare `inherits = "release"` turns both off, which would
leave the `debug_assert!` sites in the crate silent locally while CI's plain
`cargo test` runs them in debug - two gates disagreeing about what the code
asserts, with the weaker one being the one anybody runs.

**Never run the full suite under plain `--release`.** `[profile.release]` is
tuned for one artifact - the wasm bundle `wasm-bindgen` ships - and carries
`lto = "fat"` with `codegen-units = 1`. `cargo test --release` inherits that,
so every test binary pays a whole-program LTO link of the entire Bevy engine.
Measured on one binary, same machine, same relink:

| profile | wall | peak RSS |
| --- | --- | --- |
| `release` (fat LTO) | 644 s | 7.97 GB |
| `test-release` | 4.75 s | 1.67 GB |

At `build.jobs = 6` that is six concurrent 8 GB links - which is what the jobs
pin below is really protecting against. `[profile.test-release]` keeps release
codegen (the avatar and terrain builds in the suite are unbearable in debug) and
drops only the whole-program link. Note that `[profile.bench]` does **not** work
as an override here: despite the folklore, cargo builds `--release` test targets
with `profile.release`.

`cargo nextest run` does not run doctests - that is a known upstream limitation,
not a configuration gap - so the separate `--doc` line above is part of the gate,
not optional. Without it the single live doctest in `src/diagnostics/anomaly/`
stops being covered.

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs `cargo fmt --all
-- --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test` and the
doc gate on every push and pull request - a stray blank line fails the build
before a single test runs - plus a separate `wasm` job running the wasm32
check. The doc gate is where link rot gets caught, and the wasm check is the
cheap half of the deploy - both are enforced rather than left as local
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
unfixed upstream island-corruption bug - inserting and then removing
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
can see a dependency's resolved version). It cannot fire the canary for you -
what it does is stop a bump from being silent, which is how an ignored test
becomes a comment.

**Re-read the doc prose.** `cargo doc` checks that links resolve, not that
sentences are true, so a bump that changes a version literal or an upstream
file path leaves the `//!` headers and this file describing the old world.
Nothing in the gate can catch that.

**Refresh the hosted-editor glyph list on a `bevy_symbios_avatar` bump.**
The Body tab draws sculpting sections it does not own, from
`bevy_symbios_avatar::editor`. The tofu guard
(`every_ui_label_glyph_is_in_the_base_font_set`) walks `src/ui` plus a list of
paths under this crate, so it structurally cannot see that editor's string
literals - and a glyph the bundled faces cannot draw ships as an empty box
that looks like a styled button until someone renders it (#861, #1105, #1257).
`HOSTED_EDITOR_GLYPHS` in [`src/ui/fonts.rs`](../src/ui/fonts.rs) is the
hand-kept floor that IS checked. After a bump, re-scan the dependency's
`src/editor.rs` for non-ASCII characters in string literals and add any that
are new:

```bash
grep -oP '"(?:[^"\\]|\\.)*"' \
    ~/.cargo/registry/src/*/bevy_symbios_avatar-*/src/editor.rs \
    | grep -P '[^\x00-\x7F]'
```

Mind the multi-line literals: a `\`-continued string spans lines, so a
line-oriented scan misses its tail - which is exactly how the `⚠` at
`editor.rs:606` was nearly left out.

**Refresh the hosted audio-editor glyph list on a `bevy_symbios_audio` bump.**
The same law, for the Audio Editor pop-out the room editor hosts from
`bevy_symbios_audio::ui` (#1318: its instrument-selector pencil shipped as an
empty box, and four more of its symbols turned out to be tofu on inspection).
`HOSTED_AUDIO_EDITOR_GLYPHS`, beside `HOSTED_EDITOR_GLYPHS`, is the floor that
IS checked.

**Ask the crate, not a regex.** Since 0.4.9 the dependency ships the report
itself - `print_editor_glyph_inventory`, an `#[ignore]`d test that lexes the
string literals of its own `src/ui`, decoding `\u{…}` escapes (which is how
that editor writes most of its symbols, and how the pencil hid from a
raw-glyph grep). Run it against the **packaged** bytes of the version being
adopted, so what is scanned is what publishes:

```bash
cd /path/to/bevy_symbios_audio && cargo package
cd target/package/bevy_symbios_audio-<version>
cargo test --jobs 6 --features egui --lib print_editor_glyph_inventory \
    -- --nocapture --ignored
```

It prints one `GLYPH <c> U+XXXX <files>` line per code point. **A regex gets
both directions wrong**, which is why this recipe used to be one and is not
any more. Measured on 0.4.10's packaged `src/ui`:

* a raw-glyph scan reads comments as well as literals, so it adds `–`, `→`
  and `✓` - code points that only ever appear in prose *about* glyphs. Two
  of those three do not even draw in the bundled faces (see below), so the
  list would have gained the tofu it exists to prevent;
* it also *misses* every `\u{…}` escape, which is how most of that editor's
  symbols are written - the die `🎲` does not appear in a raw scan at all;
* and a regex that does decode escapes then picks up `⬅` and `✎` from
  test assertions and doc comments, neither of which anything draws.

Three of the code points the lexer itself finds are test FIXTURES and must
NOT be copied into the hosted list: `é` U+00E9, the die `🎲` U+1F3B2, and
U+270E, which the crate's own font tests assert is *not* drawable.
Everything else it prints belongs on the list; 0.4.10 draws thirteen.

Do not guess coverage from the glyph's looks: the bundled Noto Sans is a
Latin/Greek/Cyrillic face and egui's tail is emoji plus a few icons, so `✔`
draws and `✓` does not, `↔` draws and `←` does not. The test is the probe.

The other three `#[ignore]`d tests are probes rather than canaries - they
assert nothing and print measurements - so nothing is owed for them.

### One integration target

Every file in `tests/` is a `mod` of [`tests/main.rs`](../tests/main.rs), which
is the only `[[test]]` target the package declares, and both binaries carry
`test = false`. Each integration target statically links the whole engine, so a
target per file would mean two dozen Bevy links on any `cargo test` that
touches the lib; one target links once. Measured at `build.jobs = 6`, `touch
src/lib.rs && cargo test --no-run` costs **39 s** rather than **1 m 56 s**, and
1 m 34 s of CPU rather than 7 m 45 s.

Because there is one target, targeting a single file's tests is a name filter
rather than a target flag - the module path is part of every test's name:

```bash
cargo test --test integration pds_sanitize
cargo nextest run -E 'test(publish_snapshot::)'
```

`tests/main.rs` carries the rule for what may be added there: one target means
one process under `cargo test`, so a test that needs a private copy of a
process-global - the panic shadow, the allocation counters, the offload census -
is not safe as a module and needs its own target with the reason written
down. nextest still runs each test in its own process either way.

Note: [`.cargo/config.toml`](../.cargo/config.toml) pins `build.jobs = 6` -
each target links a full Bevy binary, and an uncapped parallel link can exhaust
RAM on smaller machines. With one integration target the suite itself is cheap,
but the app and the render bin still link full engines beside it. The same file
carries the `getrandom_backend="wasm_js"` rustflag for local wasm builds:
`symbios-avatar` pulls `getrandom` 0.3 transitively, early 0.3 refused to build
for `wasm32-unknown-unknown` without a cfg naming its backend, and cargo
configs do not propagate from a dependency to its dependents (#1055). Since
0.3.4 the `wasm_js` feature alone selects that backend, which is why the gate's
wasm line and both CI workflows build without the cfg: their `RUSTFLAGS`
replaces the per-target rustflags, and the graph no longer needs them. A wasm
build run from outside the repo root will not see either setting.

## Cargo features

The crate ships one optional feature, `alloc-trace` (native only). It wraps the
global allocator and prints a backtrace for every allocation of 16 MiB or more -
the first 24 in full, then a one-line size report every 128th, so a per-frame
churn doesn't drown stderr:

```bash
cargo run --release --bin symbios-overlands --features alloc-trace 2>/tmp/alloc.log
```

It exists to put names to the giant-buffer churn the wasm allocation tracker can
only count: wasm cannot produce a callstack, and the same code runs natively. No
`RUST_BACKTRACE` needed - the tracer force-captures. Off by default and
zero-cost in ordinary builds; see [diagnostics.md](diagnostics.md) for the
memory metrics it complements.

## Developer tooling

**Headless render tool** - renders any avatar / catalogue entry / primitive /
room / dumped generator JSON through the real spawn path into a multi-angle
contact-sheet PNG, or the whole compiled world through the game's own
pipeline into a still or an animated clip, so geometry, materials and the
world itself can be validated without in-game screenshots:

```bash
cargo run --bin render -- --catalogue medieval_castle
cargo run --bin render -- --avatar did:plc:example
cargo run --bin render -- --prim cuboid
cargo run --bin render -- --room 3            # whole seeded room, by seed or DID
cargo run --bin render -- --world 3           # the seeded WORLD as the game builds it
cargo run --bin render -- --world did:plc:x --world-record room.json   # ...or an edited record
cargo run --bin render -- --terrain 3        # the room's GROUND: heightmap + splat
cargo run --bin render -- --wear satchel      # a wearable, actually worn
cargo run --bin render -- --generator /tmp/x.json  # a dumped + edited Generator
cargo run --bin render -- --play-view --lineup 12,40,7 --reference-figure
#                                              # several subjects at the
#                                              # chase camera's own range
```

When more than one subject is given the highest-precedence one wins:
`--lineup` > `--generator` > `--world` > `--terrain` > `--room` > `--prim` >
`--wear` > `--catalogue` > `--avatar`,
with the no-render modes below running ahead of all of them. That order is
asserted by `render_tool`'s own tests, so it is checkable rather than a claim.

`--world <seed|did>` (#1349) is the one subject that is *compiled* rather than
spawned. It registers the pipelines the game itself runs - the heightmap +
splat chain, the road re-mesh and the lot layer that grows a district along
the streets, the placement compile with its terrain-aware scatter sampler and
water volumes, and the sun / sky / cloud deck the room's `Environment`
re-tints - exactly as the login backdrop compiles its demo world, then waits
for all of it to *settle* (a compile pass landed, none running, the splat
applied, no road re-mesh or lot re-derive pending, no procedural texture
bake still in flight, and that answer held for forty frames) before
shooting. The camera is the game's: fog, bloom, the
depth prepass shore foam reads, cascaded shadows, MSAA. Framing is a rig
rather than a tile set:

```bash
# A still of seed 3 from 130 m, 28° up, centred on the built-up band:
cargo run --profile test-release --bin render -- --world 3 --focus settlement --dist 130
# A 60-frame orbit clip (12.5 fps, 30° of drift) → /tmp/avatar-render/world-3.gif:
cargo run --profile test-release --bin render -- --world 3 --frames 60 --sweep 30
# Three seeded bodies walking the world together, camera following the first:
cargo run --profile test-release --bin render -- --world 3 --walker 7,12,30 --focus walker \
    --frames 48 --dist 6 --elev 14 --yaw 150
```

`--focus` names what the rig orbits - `origin` (the spawn square; the
default), `landing` (the gateway forecourt), `settlement` (the centroid of the
placed structures), `walker`, or a point `x,z` / `x,y,z` - and `--dist`,
`--elev`, `--yaw` and `--lift` place the camera around it. `--sweep` turns
the yaw over a clip; `--dist-end` and `--elev-end` dolly. The defaults follow
the focus: a vista (`origin`, `landing`, `settlement`) looks 8 m above the
ground and drifts 30° over a clip; the walker and a named point are eye-level
shots that hold their angle. Frames are
`--width` × `--height` (default 896 × 504; the width is forced to a multiple
of 64 so the GPU readback needs no row padding).

`--frames N` turns any single-camera shot into a clip written as a GIF -
a `--world` shot, or a turntable of any single subject
(`--catalogue villa --frames 36` turns once; `--zoom 1.5` sits a third
closer than the sheet fit) - at `--fps` (default 12.5;
GIF counts delays in centiseconds, so 10, 12.5, 20 and 25 land exactly).
The app runs on a hand-driven clock: `Time<Virtual>` is paused at startup and
stepped by `1 / fps` **once per captured frame**, so wind sway, cloud
scroll, water, particle plumes and the walker's gait play at the rate the
GIF does, and frame `k` is the scene at exactly `k / fps` seconds however
long the readback of frame `k − 1` took. The encoder fits one palette to
the whole clip, dithers on a grid fixed to the pixels, and writes only the
pixels that changed since the previous frame, so a still camera over a moving
body costs what the body costs. `--keep-frames` also dumps every frame as
`<out>-frames/frame-NNN.png`, and `--stitch dirA,dirB --out both.gif`
concatenates such directories into one GIF - how a world, a walker and a
turntable become one picture - and `--crossfade N` dissolves each of its
cuts, and the loop seam from the last directory back to the first, over N
blended frames (default 0, a hard cut; every blended frame is a whole-frame
change and costs like one). `--dither` (default 6, in 8-bit steps) is the
one encoder knob: higher smooths sky gradients and costs bytes, since a
dither pattern is exactly the detail LZW cannot fold; a still camera with
only the body, the smoke and the water moving is the other lever, because
unchanged pixels cost nothing.

Whatever the subject, the shutter does not open while a procedural texture
bake is airborne (#1351). A material is spawned in a flat fallback colour and
gets its maps when its bake lands, seconds later on the texture crate's own
thread pool, and no fixed warm-up frame count covers that on a fast GPU - a
tower's glass used to land at frame 14 of a held clip. The warm-up frames
still run (the particle plumes need them), then the tool holds both the
shutter and the clock until the last bake is patched in, so a still shows the
finished material and a clip's timing is unchanged by the wait. The world
mode's progress line reports `bakes_in_flight` alongside the compile state.

For the same reason a clip's one camera is put on its shot pose the frame
the subject is framed and kept there through the warm-up, exactly as the
sheet cameras are, rather than moved there on the capture frame. Until it
moves it sits on a spawn placeholder, and a mesh that gets its material
while it is outside that placeholder's view - a palm's crown 7 m up, the
walker's body spawned 100 m from the origin - is not drawn for the view when
the camera finally turns to it. A frond-less palm or a body-less walker in a
clip whose sheet or later frames look right is this, not a missing asset.

`--walker <seed,...>` (with `--world`) rolls each seed's default body and
walks them from the record's landing toward the origin (`--walk-from x,z` /
`--walk-to x,z` override the line, `--walker-pace` the speed,
`--walker-wear satchel,circlet` dresses every body, and `--walker-outfit
#top,#trousers[,sleeve,leg]` - the two garments' colours in hex and
optionally their lengths as shares of the limb, 0.5 being the elbow or the
knee, the flag repeated once per body in seed order - holds their clothes
fixed; without it each body wears the outfit its seed rolls, since engine
0.10 dresses a re-roll (#1404)). The first seed is the body
`--focus walker` follows; the others walk beside it, `--walker-spread`
metres apart (default 1.6) on alternate sides and each half a metre further
back, so three seeds read as friends walking together (#1352). They are
driven by the same `Drive` / `AvatarDriver` pair the game hangs a local
player on, on the real heightmap, and they start walking `--walker-lead`
seconds (default 1.5) before the first captured frame so a clip opens
mid-stride.

`--editor` (#1353, with `--world`) draws the game's own editing surfaces
into the same frame as the world: the toolbar, the World Editor, the
Catalogue, the toasts and the in-world transform gizmo, registered as the
game's own systems under the game's own run conditions, together with the
undo history, the Catalogue's drop handler, the item-preview stage and the
physics collider tree the drop's ground ray needs. One thing stands in: the
editor is owner-only and no OAuth sign-in can happen in a headless tool, so
an offline session for the world's own DID is signed in, built the way the
crate's tests build theirs, with every URL on `example.invalid`. Nothing it
registers performs network I/O. The editor is laid out for a 1280 x 720
screen or larger - smaller, its windows fill the frame and overlap - so lay a
shot out at 1280 x 720 and write it smaller with `--downscale`:

```bash
# The Placements tab with the landmark selected and its gizmo in the world:
cargo run --profile test-release --bin render -- --world 253 --editor \
    --editor-tab placements --editor-select landmark --width 1280 --height 720
```

`--editor-tab` opens a tab by its label (`environment`, `items`,
`placements`, `effects`, `raw`); `--editor-select <item>` selects an item by
its name in the record (`--describe` lists them), its tree row on Items and
its first placement on Placements; `--editor-window <window>=x,y,w,h` places a
window by its layout key the way a saved layout does (a window still takes
the width its content needs); `--editor-avatar` opens the **Avatar** editor
on its Body tab instead of the World Editor, which is how the sculpting
sections this app hosts from `bevy_symbios_avatar::editor` are checked after
an adapter bump; `--editor-ui-scale` is the Settings window's
Interface scale. `--downscale N` writes any single-camera still or clip N
times smaller than it renders, each pixel the mean of an N x N block.

`--editor-script <file>` plays gestures on the tool's clock. The steps above
a `start` line run during the warm-up and are never captured; every step
below it advances one captured frame at a time:

| Step | What it does |
| --- | --- |
| `hold N` | nothing, for N frames |
| `move <target> [over N]` | glide the pointer onto a target (default one frame) |
| `click <target> [over N]` | glide (default 6 frames), rest a frame, press, release |
| `press` / `release` | the left button, one frame each |
| `type "text"` | select everything in the focused field, then type |
| `scroll N` | wheel the surface under the pointer N egui points further down its list (a negative N goes back up) |
| `drag-gizmo <axis> <metres> over N` | with the button held, pull the gizmo handle that far along x, y or z |
| `start` | where capture begins |

A target is `widget "label"`, the one control whose AccessKit label or value
is that text (a name that matches nothing, or more than one control, stops
the run and lists what it found); `right-of "label"`, the nearest control to
the right of a label on its row; `px x,y`, a frame pixel; `gizmo <axis>`, the
middle of the selected gizmo's arrow for that axis; or `ground x,z`, the
terrain there. The one pointer is written everywhere the game reads a real
one - egui's input, bevy_picking's mouse pointer (which the gizmo hovers
with), the window cursor and the mouse button - and an arrow is painted
where it is, so a clip shows what is being pointed at. Each step logs where
it put the pointer.

**A widget below the fold is found, and clicking it misses.** A scrolled-out
control is clipped, not culled: egui still lays it out and still reports it to
AccessKit, so `widget "label"` resolves happily to a point outside the window
and the click lands on whatever is behind. `move` the pointer over the panel
first, then `scroll` until the control is on screen, and only then click it.
The Avatar editor's Body tab needs this for every section it hosts from
`bevy_symbios_avatar::editor`, which all sit below its identity block
(`--editor-avatar`, #1358). The same property is why a duplicate name cannot
be scrolled away: the Body tab has two controls called `hair`, the seed-lock
toggle and the section header, and the lookup refuses the pair however far the
panel is scrolled - reach one of them with `px` instead.

A gesture's consequences land between captures: a gizmo release commits the
record and the placement is rebuilt over the next frames, and a Catalogue
drop spawns a building whose textures bake for seconds. So a clip holds its
clock and its shutter while a compile pass runs or a texture bake is
airborne, and shoots the next frame only once the scene has caught up - the
warm-up's bake rule applied between every pair of frames, with frame `k`
still the scene at exactly `k / fps` seconds.

`--terrain <seed|did>` is the *ground* instrument (#994), and the only render
mode whose subject is not an object: it builds the room's real heightmap,
bakes the four splat layers and shoots four grazing landscape views across
`--view` metres (default 300). `--room` deliberately puts settlement
structures on a flat plane and skips terrain, so until this existed no splat
could be seen outside the running game:

```bash
cargo run --profile test-release --bin render -- --terrain 7 --view 300 --elev 32
```

It waits for the splat pass to resolve rather than for a frame count - the
material wears a flat placeholder colour until then, and a render that caught
that frame would look finished and show no ground texture at all - and it
frames a *fixed* camera rather than auto-framing the subject's bounds, so two
renders of the same seed are comparable. How much repetition a view shows is a
function of distance: one tile covers `world_extent / tile_scale` metres, which
at the shipped defaults is 11.4 m, so a 300 m view shows about 26 repeats.

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
an entry that has no wear socket at all - without it such a slug is refused by
name. Sheets are labelled `wear-<slug>-<socket>`.

`--avatar` draws `Generator` trees, so it covers the *vehicle* seeds only -
boat, airship and skiff. A humanoid seed rolls a rigged
`symbios-avatar` body with no tree to walk, and the tool refuses it by name
rather than rendering an empty sheet; the sibling `bevy_symbios_avatar`
viewer's own `--shot` capture is that body's instrument. `--family-seeds` will
find you a vehicle seed to render.

`--play-view` is the *play-distance* instrument (#1360), and the frame a
vehicle design is accepted on. Every earlier pass at the seeded craft was
judged on zoomed contact sheets, where a 1.2 cm rail looks like a rail; the
chase camera rests 12 m away on a 45 degree lens, which resolves about 109
pixels a metre at 1080 lines, and that rail is 1.3 px. The preset reads its
distance and pitch from `crate::config::camera` (`ORBIT_RADIUS`,
`ORBIT_PITCH`) so it cannot drift from the game, shoots 1920x1080, and stands
its subjects on a lit ground plane - a hovering hull and a beached one are the
same picture without a contact shadow.

```bash
# The standing comparison: airship, boat, skiff and a 1.75 m figure, at the
# range the player sees them.
cargo run --profile test-release --bin render -- --play-view \
  --lineup 12,40,7 --reference-figure --out /tmp/play.png
# A hand-written prototype beside the seeded craft it is replacing, told
# where to float (a generator file carries no locomotion record):
cargo run --profile test-release --bin render -- --play-view \
  --lineup target/dump/vehicles2026-09/sloopB.json,40 --reference-figure \
  --ride-height 0.35,auto,auto
```

Each `--lineup` entry is a `u64` seed, a path to a `--generator` JSON file, or
a DID, and the slots read left to right in the order typed;
`--reference-figure` appends the mannequin. `--lineup` outranks every other
subject, and works without `--play-view` too - then it sheets four angles per
slot, one row each, the way `--ages` does.

**Where a subject stands.** The view exists to show ride height, so it stands
each subject where the game does rather than resting it on its bounds: a
craft that settles on a suspension goes with its chassis origin at
`half_y + suspension_rest_length - static compression`, read off the
locomotion the same build produced. An airship has no ground ride height at
all (it holds itself up with thrust) and a `--generator` file has no
locomotion record, so those rest on their own drawn bounds unless
`--ride-height` says otherwise - one value for every slot, or a list with
`auto` for the slots that keep their derived height. The log names which rule
each slot landed on, and every slot's origin is placed at exactly the game's
orbit radius from the camera: the line-up stands on an arc, not on a line,
because a 14 m line shot from 12 m puts its outermost subject 16 % further
away than its innermost. A line-up wider than the frame is warned about
rather than quietly shrunk.

**Judging a livery list (#1365).** `--livery <index>` draws every seeded
vehicle subject in the heritage scheme at that index instead of the one its
seed picked, so a curated list can be compared on ONE hull with the
proportions, the stance, the wear and the craft type all held still:

```bash
for i in $(seq 0 6); do
  cargo run --profile test-release --bin render -- \
    --avatar 13 --play-view --livery "$i" --out "/tmp/livery-$i.png"
done
```

The index is into that family's own table in `src/pds/avatar/livery.rs` and
wraps, so a loop that runs past the end draws each scheme once rather than the
last one twice. `--outfit <seed>` prints the scheme a seed picked for itself,
beside its craft type. A `--generator` file carries its own colours and is
unaffected.

`--play-view` is a preset, not a straitjacket: `--yaw` (default 135, the
sheet's three-quarter angle), `--dist`, `--elev`, `--zoom`, `--lift`,
`--width`/`--height` and `--frames` all still apply, so the same flag also
gives the tool its most convenient studio.

A single subject - a catalogue entry, a primitive, a generator, a wearable -
stands in a neutral studio whose backdrop `--backdrop #rrggbb` recolours
(default the blue-grey `#8592b3`); a world and a room paint their own sky
and ignore it.

Sheets land in `/tmp/avatar-render/<label>.png`. `--out` replaces that whole
path - it names a `.png` file rather than a directory, and its parent must
already exist - and `--size` sets the per-tile pixel side (default 512, rounded
down to a multiple of 64 so the GPU readback needs no row padding). `--prim`
also accepts the cut/deform overrides (`--hollow`, `--twist`, `--pathcut`, …)
listed by `--help`.

Three more flags change what a sheet contains rather than where it lands.
`--elev <deg>` lifts the camera off its default low orbit (roughly 13°) - the
only way to see into anything open-topped, a brazier or a well or a bowl.
`--ages 2,3,5,7` renders one row per L-system iteration count, every row framed
at one shared camera distance so relative plant size across ages stays honest.
`--variant <name>`, alongside `--catalogue <plant-slug>`, applies that entry's
named material re-skin before rendering - materials only, never geometry, so it
composes with `--ages`; `--variant list` prints the entry's variants and exits.

Plant work has its own guide: [lsystem-playbook.md](lsystem-playbook.md) takes a
species request to a finished grammar - the four mileage levers, the engine's
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
# ...narrowed to one seeded craft type (#1362) - the survey each craft-type
# slice opens with. A craft type is a property of the SEED, so this answers
# for a type before anything builds it:
cargo run --bin render -- --family-seeds boat --craft longship --family-count 6
# Dump a catalogue entry's generator JSON (edit + re-render via --generator):
cargo run --bin render -- --dump --catalogue neon_kiosk
# Print one avatar's resolved outfit (chassis / style / socio tiers / slot→slug):
cargo run --bin render -- --outfit 7
# Scan seeds for one that rolls a styled part (capped by --family-count):
# The AIRSHIP is the last family with parts, so its slugs are the only ones
# this can find (#1363, #1364): an ornament is the selective case, rolled on
# the ornate tiers alone.
cargo run --bin render -- --find-part airship_orn_lanterns
# Gateway veil-vs-frame fit report - one slug, or `all`:
cargo run --bin render -- --gateway-fit all
# Plinth-depth audit of every settlement-placeable entry (`all` lists the
# passing rows too; any other word shows only the shortfalls):
cargo run --bin render -- --foundation-audit all
# Terrain drop real seeded settlements span, measured over N seeds:
cargo run --bin render -- --settlement-drop 200
# Analytic entity census over seeds 0..N - what a room will actually spawn:
cargo run --bin render -- --room-census 32
# Placement census - yield, what the slope cutoff costs, Clark–Evans clustering:
cargo run --bin render -- --scatter-census 8
# Plan-view plot of one room's scatters (PNG to --out, else ./scatter-plot.png):
cargo run --bin render -- --scatter-plot 3 --out /tmp/scatter.png
# What seeded rooms ARE, before a render: scene roll, fog visibility, sun height,
# cloud cover, water line, landing and placement counts - one line per seed
# for a range, a labelled block for one seed or DID:
cargo run --bin render -- --describe 0..32
cargo run --bin render -- --describe 11
# A world's ground as numbers, no render (#1449): percentiles, the water line
# and the share it floods, the landing, where each placement stands, and at
# each point its height, slope, downhill way and contour yaw (--footprint:
# what a thing that wide rests on), and which ground textures the game blends
# there - each splat layer's share, from its own weight map, and the layer a
# scatter's biome_filter reads (#1461); --plan draws it from above, +X right:
cargo run --bin render -- --world <did> --world-record room.json --terrain-report \
    --at=-27.7,1.9 --footprint 2.5 --plan /tmp/plan.png --focus=-20,20 --span 120
# ...or the same terrain recipe under other seeds, and a contact sheet of them:
cargo run --bin render -- --world <did> --world-record room.json --terrain-report \
    --seed-scan 4..20 --plan /tmp/seeds.png
# Any single subject (--generator, --catalogue, --prim) prints its size first -
# the box its meshes fill, from its origin (#1448):
#   subject size 1.68 x 1.15 x 1.56 m (x, y, z), from [-0.80, 0.00, -0.78] to [0.88, 1.15, 0.78]
# PNG frame directories (as --keep-frames writes them) → one GIF at --fps:
cargo run --bin render -- --stitch /tmp/a-frames,/tmp/b-frames --out /tmp/ab.gif
```

`--outfit`, `--find-part`, `--describe`, `--room-census`, `--foundation-audit`
and `--gateway-fit` only roll records or build catalogue trees, so they return
quickly, and `--terrain-report` builds one heightmap (about half a second);
`--scatter-census`, `--scatter-plot` and `--settlement-drop` rebuild each
seed's heightmap, which costs a few seconds per seed.

**Agent client** (#1413) - a headless Overlands client an AI agent drives
from the command line, signed in as **its own** account. To everyone else in a
world it is a player like any other: it runs the game's own client and sends
exactly what that client sends. Unix-only. An agent that is to drive it
starts at [agent/README.md](agent/README.md): how to run it well, and what
earlier sessions learned, by topic.

```bash
A="cargo run -q --profile test-release --bin agent --"
# Once, by a person: sign the agent's account in in a browser. Use a
# dedicated account, never your own - the relay lets one identity into a
# room once, so an agent signed in as you would take your place.
$A login --account agent.example.com
$A accounts                          # the saved sessions
$A start --admin @you.example.com    # in the background; returns when it listens
$A status                            # who, where, with whom (JSON)
$A events --since 0 --wait 30        # admin chat, arrivals, departures, walks ending
$A say "hello"
$A walk-to -104.9 125.7 --wait       # straight line; ends arrived/stuck/halted
$A walk-to @friend.example.com       # "come here": 3 m short of them (--distance), then face them
$A follow @friend.example.com        # stay about 3 m behind them (--distance, --run)
$A face @friend.example.com          # turn to face them - or a point: face X Z
$A travel @alice.example.com --wait  # a DID, a handle, or `home`
$A look                              # a PNG of the game's own view; prints its path
$A look --view eyes --heading 90     # from the eyes, looking right (or --at X Z)
$A stop                              # never logs out: the session is kept
$A start --offline                   # no account: a stand-in, alone, saves nothing
$A start --offline --stand-in did:plc:agentofflinecar22222222e   # ...that drives a car
$A start --offline --stand-in did:plc:agentofflineair222222222   # ...that flies an airship
# Editing its own world (#1422) - live for whoever is there, kept only once saved:
$A placements --within 40            # what is placed, by index, where it is drawn
$A catalogue lighthouse              # what can be placed, by slug
$A place lighthouse --at -112 122 --yaw 90   # no --at: a few metres ahead
$A move 16 -105.8 126.9              # keeps its height above the ground
$A remove 16                         # `place <name>` puts it back
$A room get /environment/fog_visibility      # the Raw JSON tab's form
$A room set /environment/fog_visibility 3000000   # 300 m: decimals are x 10 000
$A avatar get /body/outfit           # the record, and a rigged body's sculpt
$A undo                              # or `undo avatar`; `redo`, `revert` likewise
$A start --allow-save                # only then may it save:
$A save --wait                       # or `save avatar`
$A travel home --discard-edits       # or --save-edits; without, refused
# Its inventory and gifts (#1423):
$A inventory                         # what it holds, what it wears
$A stash ships_lantern               # a catalogue entry - or a thing in its own world
$A stash Birdhouse --from-avatar /record/body/visuals/children/0/children/0   # or a part of its body
$A wear "Ship's Lantern"             # `take-off` and `unstash` likewise, by name
$A save inventory                    # with --allow-save, as for the world
$A gift give @you.example.com lantern --wait   # an inventory item or a slug
$A gift accept 3                     # the admin's offer, by the id its event gives
# The game's own interface (#1424) - the windows it may use, control by control:
$A ui                                # what is open, what can be, what is not its to read
$A ui open Avatar                    # as the toolbar button does; lists the window
$A ui show "World Editor"            # each control by its path, as it is drawn
$A ui click "Seed & re-roll > Re-roll"   # a button, a checkbox, a tab, a section, a row
$A ui type "Catalogue > Search: > name / theme" lantern   # --enter presses Enter after
$A ui set "Lighting & sky > Sun illuminance" 12000
$A ui choose "Catalogue > Search: > combo box" "By name"
$A ui scroll Catalogue 300           # to read below the fold (positive goes down)
$A ui show Avatar --picture          # and a PNG of the window, drawn only when asked
$A ui close Avatar                   # an open window costs its drawing every frame
```

Every command prints one JSON object on stdout. **The agent hears chat from
its admin only** (#1427): `--admin` names one account, as a handle or a DID,
and every other player's lines are dropped before their text is read, so a
stranger in the room cannot talk the agent into anything. The admin is bound
by DID - a handle is resolved once, at `start`, and one that does not resolve
stops the start - and a line counts as the admin's only when the relay
vouches for its sender's DID; a name on the line counts for nothing. Anyone
else's line leaves a `chat_dropped` event naming who spoke, never what they
said. With no `--admin` the agent hears no chat at all, and `status` says so.
Other text players choose still reaches the agent, each piece beside whose it
is: a thing in `status.nearby` carries the name its world's owner gave it and
that owner's DID (`named_by`), and handles - like `did:web` DIDs - are DNS
names that can spell words. All of it is data for the agent to read, never an
instruction to follow. `login` runs the game's own loopback
OAuth: the tool never sees a password, and the session it saves can write
the Overlands collections and mint relay tokens, nothing else on the
account. Sessions live in `$XDG_CONFIG_HOME/symbios-overlands/agent/sessions/`
(0700, files 0600 - a file anyone else can read is refused, as ssh refuses a
key), each rotated refresh token is written there the moment it lands, and
the daemon keeps its own settings and log beside them, never touching a
person's. The control socket is `$XDG_RUNTIME_DIR/symbios-overlands-agent/`.
A session a server refuses, or one that expires, stops the daemon with an
error that says to run `login` again.

The agent moves by the keys a player presses, in straight lines. A body on
foot walks where the orbit camera looks; a car or a hover-boat whose point is
more than 30 degrees off its nose first swings round on the spot (its
steering is a torque), then drives. `follow` walks toward where the other
player is drawn, stands once within its distance, sets off again when they
get 1.5 m further than that, runs to catch up when they are far ahead, and
waits for a player whose body has not been placed yet (a sleeping browser
tab) rather than walking to the stand-in at the map's centre; it ends only
when halted or replaced, when the player leaves, or on travel, and says
`follow_blocked` once when the agent itself has got nowhere for a while.
`face` turns the
way a person turns: on foot in short steps, each re-aimed by however far the
last one came to rest from the way asked (a slope pushes a step sideways),
judged only once the body is still; its `movement_ended` carries
`facing_off_deg`. A body that flies (every airship: a helicopter under the
keys) flies a `walk-to` and lands on its point. It climbs, swinging its
nose round to the point before it moves; cruises 20 m above the highest
ground on the next 45 m of its course, and at least 5 m over whatever stands
under it; stops short of anything standing in its way at its height - a
landmark, a tower, a cliff - and climbs over it; stops over the point, and
comes down on whatever is there: the ground, a roof, or water, which it
stops on rather than sinks into. It `follow`s by escorting the player 8 m
up, keeping its distance, lands beside them once they have stood still for
five seconds, and takes off again when they move away; it `face`s by
turning on the spot at whatever height it is. It never ends a movement in
the air: halted, stuck, or left by the player it followed, it first comes
straight down where it is (`halt` answers `landing: true`), and the
movement ends once it is down - only another movement or travel cuts it off
at once. A flight is `stuck` when the body has neither moved nor turned for
six seconds, never by how far it still has to go, and its `movement_ended`
carries `height_m`. An airplane (#1431) is flown as the game's airplane
actually flies - its lift is straight up in proportion to its forward speed,
so it holds its height by its throttle and turns by its rudder, its nose
kept level (at the daemon's frame one press of A or D rolls it about 140
degrees). On the ground it swings round to a clear run and takes off; it
comes onto a straight final toward the point from 80 m out, glides down by
slowing, cuts its engine at touchdown and stops within 10 m of the point;
not lined up, or with something on the final, it goes round - out to 170 m
and back - and after two of those it lands where it can and ends `stuck`.
Its engine runs with no key held, so with nothing flying it the agent holds
it cut, or it would take off on its own. It `face`s on the ground only and
does not `follow`. `status.movement`
says what the agent is doing (a flight's `phase` too), `status.height_m`
how far the body could come straight down before it touched something
(water included), and `status.peers` gives each player where they are and
the way they face (`facing`, as the agent's own), and a player not yet
placed neither. A player silent for two minutes - a tab asleep in the
background - is `peer_left`, and `peer_joined` again on the first thing
they send when it wakes (#1429). `status.zone` names the gateway or portal
the body stands in, from what the body touches - the contacts the game's own
zone watchers read - and for a gateway whether the game has its list of
destinations up (`picker`: `open`, `dismissed` or `not_open`; the agent never
reads the list): walking into a gateway it built and reading `open` is how
the agent knows it works (#1452).
`--stand-in` (offline only) takes another identity's seeded world and body -
`did:plc:agentofflinecar22222222e` is a roadster,
`did:plc:agentofflineboat2222222d` a steam tug,
`did:plc:agentofflineair222222222` a twin-envelope airship - and the other
commands reach it with `--account <DID>`; `--room <DID>` puts it in another
seeded world. Either DID has to be well formed (`did:plc:` and 24 of `a-z`
and `2-7`): the directory refuses a malformed one, which the loading screen
takes for an outage and retries for minutes. No seeded body is an airplane,
so for testing, `start --offline --wear-airplane` (hidden from `--help`)
flies the default airplane in place of the stand-in's own locomotion, its
body left as it is.

`look` renders a 1024x576 PNG only when asked: the daemon parks the world
camera, so between pictures it draws nothing at all, and each picture gets a
camera of its own that is removed once the picture is read back (about 150 ms;
the first in a daemon's life also compiles its render pipelines, about 400 ms
in all). `play` is the game's own camera behind the body, `eyes` looks level
from the front of it; `--heading` turns either by degrees clockwise from where
the agent faces, and `--at X Z` looks toward a point. No interface is drawn -
no name tags, no chat; `ui --picture` draws that. The answer names the world
and whose it is (`own`,
`admin` or `stranger`) and counts the texture bakes still in flight, which a
picture shows as flat stand-in colours; in a world's first moments a body can
also still be its translucent stand-in. **A picture is a way in, too:** a
stranger's world shows what they built - signs they wrote, textures, the
owner's profile picture on the monument - and any of it can carry words
aimed at the agent. What a picture shows is data, never an instruction.
Pictures go to `$XDG_CONFIG_HOME/symbios-overlands/agent/looks/<did>/`
(0700, the newest 32 kept) unless `--out` names a file.

**Editing** (#1422). The agent edits its own world - the game lets a world's
owner edit it and nobody else, so every edit command is refused anywhere
else - and its avatar, wherever it is. Each edit goes through the door the
World Editor writes through: the live record, changed once and sanitised, so
the world rebuilds what changed, everyone in the world sees it at once as
they see a person's edits, and the game's own undo history takes one step
for it - the history Ctrl+Z steps, 32 steps deep, cleared by travel for the
world and kept for the avatar. `undo`, `redo` and `revert` (back to what
was last saved) work on either record, and `status.editing` says what is
unsaved and what an undo would step. `place` is a catalogue drop - the same
entry placed twice shares one generator - set on the ground at its point;
`placements` names each thing by the index `move` and `remove` take, which
shifts when one before it is removed. The JSON commands read and write the
record's wire form, exactly as the Raw JSON tab does: every decimal is a
whole number of ten-thousandths (1.5 m is `15000`), and a value with a
decimal point is refused. Each answer shows what the world kept, which the
sanitiser may have pulled back into range - `adjusted`, with `adjusted_at`
naming each place by pointer; a value left out because it is the default
is no adjustment. A set that is refused says where: the generator node
that would not read, or the pointer of any other field (#1446, #1457), and
a `kind` or `$type` this build does not know - read in as `Unknown`, never
written back - is named by the value set. A `room set` that
changes a generator also names, by pointer, each pair of its primitives
drawing faces in one place where they can be seen - one plane, facing one
way (`z_fighting`, with the area): they flicker as anyone moves, which a
still `look` barely shows. `room set` and `avatar set` also weigh the
record as a save would write it - a world as its manifest and one record per
generator, the avatar as one - and name the largest with its size against
the 100 KiB budget (`record_size`); `status.editing.record_size` has the
room, the avatar and the inventory (#1455). A rigged avatar
keeps its body and what it wears in records of their own, so `avatar get`
shows `record`, `body` and `worn`; wearing, taking off and swapping the
body are not JSON edits and are refused, and a sculpt reaches others only
once it is saved. Every edited record is what saving it and reading it back
would give, so nothing it holds is off the wire's grid. **Saving is the
operator's to allow**: without `--allow-save` at `start` the agent edits
freely and what it changes is gone when it stops; with it, `save` writes
through the Save button's own pipeline, refused for the button's reasons -
nothing unsaved, a save already under way, a record past the ceiling - and
where the editor would stop to ask a person, over a saved record that could
not be read when the agent arrived. How a save ends arrives as a `saved` or
`save_failed` event. Offline there is no account, and `save` says so.
Leaving the agent's world would lose its unsaved edits, and nobody is there
to answer the dialog the game raises, so `travel` is refused over them
unless told `--discard-edits` or `--save-edits` (which leaves once the save
has landed). `stop` always stops, and its answer names what it discarded.

**Inventory and gifts** (#1423). `inventory` lists what the agent holds and
what it wears; `stash` copies in a catalogue entry by its slug - a wearable
one stays wearable - or a thing in the agent's own world by its name, and
`unstash` takes an item out, though not while it is worn: `take-off` first.
`stash NAME --from-avatar POINTER` copies in a part of the agent's own body
under that name (#1444): a node of a generator body's tree, by its pointer
as `avatar get /record/body/visuals` shows it, the nodes under it included.
Where it sat on the body is dropped - its origin becomes the item's, so it
stands on the ground where it is placed - and its turn and scale are kept.
The avatar is the one record the agent may edit in anyone's world, so this
is how it builds away from home: on its own body, then `stash`, then `gift
give`.
`wear` and `take-off` are avatar edits, steps of its undo history, saved with
`save avatar`; the inventory itself is saved with `save inventory` and
`revert`ed, and has no undo history, as in the game. **The agent takes gifts
from its admin only**, as it hears chat: anyone else's offer is declined the
moment it lands - reaching them as an ordinary "declined" - and leaves a
`gift_declined` event that names who, never what. The admin's offer arrives
as a `gift_offered` event with its id, the item's name (the admin's words,
as data), what kind of thing it is and whether it can be worn, and waits for
`gift accept` or `gift decline` for the game's own 90 s, then goes back as
unanswered (`gift_offer_closed`); while it waits, any other offer is turned
away as busy, the game's one-at-a-time rule. The daemon draws no offer
dialog - it held every movement key for as long as an offer waited. An
accepted gift goes into the inventory and, if the agent may save, is saved
at once as a person's Accept saves it; if not, it is unsaved, and the answer
says so. `gift give` offers an inventory item or a catalogue entry to a
player in the agent's world through the same path as a drag onto the People
list; a gift is a copy, and the answer comes back as a `gift_answered` event:
`accepted`, or `declined`, `busy`, `unavailable`, `unanswered`, or
`no_answer` when nothing came back in three minutes. A second offer to a
player who has not answered the first waits for that answer.

**The interface** (#1424). `ui` works the game's own windows the way a
person does, through AccessKit - egui's account of every widget it draws,
which is built only while a `ui` command is at work (kept on, it cost half a
point of a core at idle). A control is named by where it is drawn: its
window, the open section it sits in, the words to its left on its row or
just above its group, then its own label - `Settings > Ground avoidance: >
Off` - and never by its value, which the first edit would change. The end of
a path is enough when it names one control, and the icons a label is
dressed in may be left off; a name two controls share is refused with both
paths. Each command works one control by an AccessKit request aimed at that
control alone, after the checks a person's hand meets: one out of view is
scrolled into view first, one under another window has that window raised
first, and one behind a dialog, or greyed out, is refused. A tree's rows
(the Catalogue) take no request, so a pointer selects them, once nothing
else is drawn there. A command leaves no menu open and no field holding the
keyboard - either would stop the agent walking.

**What the agent may read and do there is settled** (#1424). Its windows are
People, Avatar, Inventory, Catalogue, World Editor, Settings, Controls and
the audio editor, the dialogs and menus its own clicks raise in them, and
the toasts. Chat is not one - it holds every line anyone in the room said,
and the agent hears its admin's only - nor Diagnostics, whose event log
names what other players sent, nor the gateway picker, the one surface that
shows Bluesky display names. A window the agent may not read is never read,
not even to suggest a near miss; `ui` names it by its title and says why.
Inside the others, a control whose work has a gate elsewhere is refused and
says what does the work: every Save (`agent save`, `--allow-save`), Visit
(`agent travel`), muting (whom the agent hears is its operator's choice),
and whatever would write the operator's clipboard or open their browser -
the daemon also drops egui's own requests to. The game's own dialogs - the
unsaved-edits dialog before a trip, sign-in-again - are never the agent's
to read or answer. Every answer to a command that could change a record
says which of the world, avatar and inventory records changed (`changed`,
bit for bit) and which were written without changing (`touched`): a panel
drawn with nobody touching it has rewritten a record before (#1390).
`--picture` draws the interface into a PNG beside the agent's `look`s - over
the fog's colour rather than the world, which is `look`'s - when asked and
at no other time. A picture is pixels, so it would show what the listing
never reads: none is taken while Chat, Diagnostics or the gateway picker is
open, or a dialog or menu the agent did not raise is up, and the refusal
says how to clear it (`ui close Chat` - closing reads nothing).

**Session logs** - the app records an append-only NDJSON session log
(`diagnostics/session-latest.jsonl` on native; downloadable from the
Diagnostics panel on web). [diagnostics.md](diagnostics.md) documents the file
locations, environment overrides, schema, and the analyzer.
