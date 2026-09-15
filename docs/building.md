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
cargo run --bin render -- --terrain 3        # the room's GROUND: heightmap + splat
cargo run --bin render -- --wear satchel      # a wearable, actually worn
cargo run --bin render -- --generator /tmp/x.json  # a dumped + edited Generator
```

When more than one subject is given the highest-precedence one wins:
`--generator` > `--world` > `--terrain` > `--room` > `--prim` > `--wear` >
`--catalogue` > `--avatar`,
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
top_hue,top_shade,leg_hue,leg_shade` - the avatar editor's four axes, each
0..1, the flag repeated once per body in seed order - changes their
clothes, which no seed does: a reroll never touches the outfit, so every
seeded body ships in the engine's one default). The first seed is the body
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
the width its content needs); `--editor-ui-scale` is the Settings window's
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
# Dump a catalogue entry's generator JSON (edit + re-render via --generator):
cargo run --bin render -- --dump --catalogue neon_kiosk
# Print one avatar's resolved outfit (chassis / style / socio tiers / slot→slug):
cargo run --bin render -- --outfit 7
# Scan seeds for one that rolls a styled part (capped by --family-count):
cargo run --bin render -- --find-part boat_bow_ram
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
# PNG frame directories (as --keep-frames writes them) → one GIF at --fps:
cargo run --bin render -- --stitch /tmp/a-frames,/tmp/b-frames --out /tmp/ab.gif
```

`--outfit`, `--find-part`, `--describe`, `--room-census`, `--foundation-audit`
and `--gateway-fit` only roll records or build catalogue trees, so they return
quickly; `--scatter-census`, `--scatter-plot` and `--settlement-drop` rebuild
each seed's heightmap, which costs a few seconds per seed.

**Session logs** - the app records an append-only NDJSON session log
(`diagnostics/session-latest.jsonl` on native; downloadable from the
Diagnostics panel on web). [diagnostics.md](diagnostics.md) documents the file
locations, environment overrides, schema, and the analyzer.
