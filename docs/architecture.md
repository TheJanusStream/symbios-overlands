# Architecture

Technical overview of Symbios Overlands. For the newcomer pitch see the
[README](../README.md); for build/run instructions see [building.md](building.md);
for the session-log/diagnostics suite see [diagnostics.md](diagnostics.md).

The project is **"thin client, heavy world"**: no central game servers host the
worlds. Every world is a small recipe record on its owner's ATProto PDS; every
client deterministically expands that recipe into geometry, materials, audio and
physics locally, and peers exchange only transforms, edits and chat over a
direct WebRTC mesh.

## Engine stack

- **Engine:** Bevy 0.18 + Avian3D 0.6 (physics) +
  [`bevy_egui`](https://github.com/vladbat00/bevy_egui) (UI) +
  [`bevy_panorbit_camera`](https://github.com/Plonq/bevy_panorbit_camera)
  (third-person orbit) +
  [`transform-gizmo-bevy`](https://github.com/urholaukkarinen/transform-gizmo)
  (in-world editor handles).
- **Procedural ecosystem:** the sovereign `symbios` family —
  [`symbios-ground`](https://github.com/TheJanusStream/symbios-ground) (Voronoi
  terracing + hydraulic and thermal erosion),
  [`symbios` + `symbios-turtle-3d`](https://github.com/TheJanusStream/symbios)
  (L-systems), [`symbios-shape`](https://github.com/TheJanusStream/symbios-shape)
  (CGA shape grammars),
  [`symbios-tensor`](https://github.com/TheJanusStream/symbios-tensor)
  (tensor-field road topology for urban themes),
  [`bevy_symbios_texture`](https://github.com/TheJanusStream/bevy_symbios_texture)
  (~30-material procedural PBR catalogue + particle-sprite atlases), and
  [`bevy_symbios_audio`](https://github.com/TheJanusStream/bevy_symbios_audio)
  (node-graph synthesis + step-sequencer mixdown). The generation algorithms
  live in Bevy-free core crates (`symbios-ground`, `symbios-texture`,
  `symbios-audio`, …); the `bevy_*` crates are thin plugin/upload wrappers —
  which is what lets the wasm Web Worker link only the cores.
- **Networking:**
  [`bevy_symbios_multiuser`](https://github.com/TheJanusStream/bevy_symbios_multiuser)
  over WebRTC ([`matchbox`](https://github.com/johanhelsing/matchbox)) for the
  peer mesh; [`proto-blue-oauth` + `proto-blue-api`](https://github.com/dollspace-gay/proto-blue)
  for ATProto identity and PDS plumbing. Peer DIDs are authenticated against the
  relay-signed session map so a peer can't impersonate another identity over the
  unauthenticated data channel.

## Protocol safety

ATProto's DAG-CBOR encoding forbids floats, so every continuous spatial value is
wrapped in fixed-point (`Fp` / `Fp2` / `Fp3` / `Fp4` / `Fp64`, scale 1/10 000).
Every record class also carries a `sanitize()` step that clamps sizes, counts,
depths and octaves so a malformed payload from a hostile peer can't OOM or crash
the engine. Records cross the wire twice — as PDS fetches and as live peer
broadcasts — and both paths go through the same sanitizer.

## State machine and the loading gate

A three-stage `AppState` (`Login` → `Loading` → `InGame`). The loading gate
waits on **all six** loading tasks — heightmap generation, the room-record /
avatar-record / inventory-record PDS fetches, the seeded ambient-audio bake,
*and* the room compile itself — before entering `InGame`, so a slow round-trip
can't leave the world half-loaded or silent, and the browser build's long
synchronous world build stays behind the loading screen instead of freezing the
first visible frame.

## Compute offload

CPU-heavy generation runs off the render frame through one platform-routed
[`offload()`](../src/offload.rs) API: on native via Bevy's multithreaded
`AsyncComputeTaskPool`, on wasm via a dedicated Web Worker (Bevy's task pools
collapse to a single cooperative thread there, so an inline job would stall the
frame). Three job kinds route through it — the **heightmap**, the **audio
bakes** (the room's ambient bed plus per-construct spatial audio), and the
**splat-texture bakes**. Each job is a self-contained, serialisable `GenJob`
whose pure `run()` is byte-identical on both backends, keeping progressive
loading deterministic across peers. The worker links only the Bevy-free
`symbios-*` cores, so its `.wasm` stays small — which is why the repo is a small
Cargo workspace ([`crates/gen-jobs`](../crates/gen-jobs/) +
[`crates/gen-worker`](../crates/gen-worker/)), not a lone crate.

## Data flow

**Cold start (a fresh account):** DID → `fnv1a_64(did)` seeds a
`SceneCharacter` (landform × biome × theme + prosperity/escalation axes) →
per-domain derivers in `src/seeded_defaults/` fill in terrain, palette,
textures, atmosphere, scatters, a themed mini-settlement drawn from the
catalogue, and a layered ambient soundtrack → the result *is* a `RoomRecord`,
identical on every peer that derives it. Authored values always win; the
derivers only fill what's unset.

**Load:** OAuth session → PDS record fetches (room / avatar / inventory, with
capped retry) run alongside heightmap generation and the ambient bake → the
world compiler (`src/world_builder/`) walks the record's `Generator` tree,
spawning ECS entities per placement, time-sliced (~5 ms/frame) with cached
geometry/materials so identical blueprints are baked once and instanced.

**Edit loop:** every widget in the owner-only World Editor mutates the live
`RoomRecord` in place → Bevy change detection triggers an incremental
recompile (only changed placement units rebuild) → the same record delta is
broadcast to peers, who mirror it — all before the owner presses **Save to
PDS**.

**Peer sync:** transforms stream on a fixed tick into per-peer jitter buffers;
identity is verified against the relay-signed session map; avatar records are
fetched from each peer's own PDS and compiled through the same world-builder
path as the local avatar.

## Project layout

The app is a library crate with a thin `main.rs` shim so integration tests in
[`tests/`](../tests/) can import the module tree directly. It also roots a small
Cargo workspace — the [`crates/`](../crates/) members are the Bevy-free
generation cores shared with the wasm Web Worker.

- [`src/pds/`](../src/pds/) — record schemas (`RoomRecord`, `AvatarRecord`,
  `InventoryRecord` — lexicons `network.symbios.overlands.*`), the `Generator` /
  `Placement` / `LocomotionConfig` open unions, fixed-point wrappers,
  per-variant sanitisers, the DAG-CBOR-safe audio/texture/contact-effect
  mirrors, and the shared XRPC plumbing.
- [`src/world_builder/`](../src/world_builder/) — the recipe → ECS compiler.
  The incremental, time-sliced executor ([`compile/`](../src/world_builder/compile/)),
  per-generator spawn arms (terrain, water, portal, sign, particles, L-system,
  shape grammar, primitives), the cross-compile geometry / material caches, and
  the source-keyed [image cache](../src/world_builder/image_cache.rs) shared by
  signs / portals / particles.
- [`src/terrain/`](../src/terrain/), [`src/urban/`](../src/urban/),
  [`src/splat.rs`](../src/splat.rs), [`src/water.rs`](../src/water.rs),
  [`src/clouds.rs`](../src/clouds.rs) — heightmap + Avian heightfield collider,
  four-layer splat material extension, Gerstner-wave water shader, FBM
  cloud-deck shader, and the urban-theme road layer: [`src/urban/`](../src/urban/)
  meshes a `symbios-tensor` road topology into a ribbon draped over the terrain
  (graph sanitation → chain extraction → junction truncation → network
  levelling → ribbon/hub/fillet extrusion → end caps), wired in as a terrain
  child that rebuilds reactively ([`roads.rs`](../src/terrain/roads.rs)) with
  themed buildings populated onto its enclosed lots at load time
  ([`lots.rs`](../src/terrain/lots.rs)).
- [`src/player/`](../src/player/) — the five locomotion presets (HoverBoat,
  Humanoid, Airplane, Helicopter, Car), avatar hot-swap on record edits, portal
  interaction, and fall-through respawn.
- [`src/interaction/`](../src/interaction/) — the contact-effects framework: one
  per-frame classifier feeds independent water-wake / particle-burst /
  splat-stain / decal / audio channels, plus the always-on material-keyed
  impact audio.
- [`src/pds/audio.rs`](../src/pds/audio.rs),
  [`src/audio_materials.rs`](../src/audio_materials.rs),
  [`src/audio_mute.rs`](../src/audio_mute.rs),
  [`src/world_builder/spatial_audio.rs`](../src/world_builder/spatial_audio.rs) /
  [`audio_resolver.rs`](../src/world_builder/audio_resolver.rs),
  [`src/seeded_defaults/room/audio/`](../src/seeded_defaults/room/audio/) — the
  procedural-audio subsystem: DAG-CBOR-safe `Sovereign*` mirrors of
  `bevy_symbios_audio` patches / sequences, material-keyed impact-SFX patches,
  the construct- and ambient-emitter spatial spawners, the URL/blob audio
  reference resolver, the app-wide master mute, and the seeded layered ambient
  soundtrack (baked in [`src/loading/`](../src/loading/) as one of the gate's
  six tasks).
- [`src/network/`](../src/network/), [`src/protocol.rs`](../src/protocol.rs) —
  peer wire format, jitter-buffered transform smoothing, identity
  authentication, live preview broadcast, item-offer arbitration.
- [`src/camera.rs`](../src/camera.rs), [`src/avatar.rs`](../src/avatar.rs),
  [`src/social.rs`](../src/social.rs) — the third-person orbit camera + distance
  fog, the peer profile-picture cache that backs the chat / People panel icons,
  and the ATProto social-graph (mutual-follow) resonance tagger.
- [`src/ui/`](../src/ui/) — egui panels: [login](../src/ui/login/),
  [chat](../src/ui/chat.rs), [people](../src/ui/people.rs) (with drag-to-gift),
  [avatar editor](../src/ui/avatar/), [inventory](../src/ui/inventory/),
  [catalogue](../src/ui/catalogue.rs),
  [diagnostics](../src/ui/diagnostics.rs) (the 5-tab metrics/anomaly HUD), and
  the owner-only [world editor](../src/ui/room/) (Environment / Region Assets /
  Placements / Effects / Raw JSON tabs, plus a pop-out
  [audio editor](../src/ui/room/audio.rs) hosting the node-graph + sequence
  canvas).
- [`src/oauth/`](../src/oauth/) — ATProto OAuth 2.0 + DPoP (WASM redirect /
  native loopback) and token refresh.
- [`src/seeded_defaults/`](../src/seeded_defaults/) — DID-seeded deterministic
  defaults, derived along two orthogonal axes: a natural *biome* and an
  artificial *theme*, plus continuous prosperity/escalation dials. Room side:
  terrain, palette, biome textures, atmosphere, tree / rock / particle
  scatters, a themed mini-settlement near spawn (a landmark plus secondary
  buildings and scatter props, drawn from the catalogue by theme), a light
  theme accent nudged back onto the natural derivers (fog tint, particle mood),
  and the layered ambient soundtrack. Avatar side: one of four chassis families
  (boat / airship / humanoid / skiff) plus its palette, body proportions, gait,
  and a tagged outfit that the part catalogue assembles into geometry.
- [`src/catalogue/`](../src/catalogue/) — code-shipped read-only library of
  starter generator blueprints (hundreds of entries across 23 themes),
  organised by theme and structural role (landmark / secondary / prop / plant /
  pattern / tool), functionally analogous to a user inventory but always
  present; the same entries the seeded settlement deriver draws from.
- [`src/editor_gizmo/`](../src/editor_gizmo/) — bridge between the editor
  selection and the in-world 3D transform gizmo. Includes
  [`blob/`](../src/editor_gizmo/blob/) — in-scene BlobGroup element
  editing: the evaluated surface renders as an edge-line wireframe and
  each element gets a red (carve) / green (add) proxy the gizmo can drag,
  with the SDF re-meshing live under the drag.
- [`src/diagnostics/`](../src/diagnostics/) — the diagnostic suite: a typed
  append-only session-event stream with a native NDJSON sink, a shared metrics
  registry scraped at 1 Hz, an anomaly/invariant rule engine that runs live and
  replays offline, and the offline analyzer behind
  `render --analyze-session` / `--diff-sessions`. See
  [diagnostics.md](diagnostics.md).
- [`src/loading/`](../src/loading/), [`src/state.rs`](../src/state.rs),
  [`src/boot_params.rs`](../src/boot_params.rs),
  [`src/logout.rs`](../src/logout.rs) — state-machine plumbing (with the
  generic per-record fetch/retry pipeline and the per-task loading screen),
  shared resources, landmark-link parsing, and the on-logout cache teardown.
- [`src/config.rs`](../src/config.rs) — centralised tuneable constants
  (lighting, fog, locomotion physics, terrain, splat layers, contact-effect
  pools, networking, HTTP timeouts, UI windows).
- [`src/offload.rs`](../src/offload.rs) + [`src/offload/`](../src/offload/),
  [`crates/gen-jobs/`](../crates/gen-jobs/),
  [`crates/gen-worker/`](../crates/gen-worker/) — the compute-offload layer: the
  platform-routed `offload()` dispatcher (native `AsyncComputeTaskPool` / wasm
  Web Worker), the serialisable `GenJob` definitions, and the slim no-Bevy
  worker crate that runs them off the main thread on the web.
- [`src/render_tool/`](../src/render_tool/) +
  [`src/bin/render.rs`](../src/bin/render.rs) — a native-only headless tool:
  contact-sheet renders through the real spawn path, plus the offline
  diagnostics modes (`--analyze-session`, `--diff-sessions`, `--road-dump`,
  `--family-seeds`, `--dump`). See [building.md](building.md#developer-tooling).
