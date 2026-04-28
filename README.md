# Symbios Overlands

A peer-to-peer spatial web of user-owned virtual worlds for the ATProto network.

## 🚧 Prototype in active development 🚧

🌍 **[Enter the Overlands (Live Browser / WASM Demo)](https://thejanusstream.github.io/symbios-overlands)**

**Symbios Overlands** transforms your ATProto decentralized identity (DID) into a persistent, 3D virtual world. Built in Rust using the [Bevy](https://bevyengine.org/) engine, it acts as a true, sovereign spatial web.

There are no central game servers to shut down, and no walled gardens. Every artifact—from the shape of your avatar to the layout of your terrain—is authored as a data-driven recipe and stored exclusively on your own ATProto Personal Data Server (PDS). You own your space, your body, and your creations.

## Core Features

* **Your DID is Your Domain:** Authenticate securely via ATProto OAuth 2.0 + DPoP. The app never sees your password. Your room is deterministically seeded by your DID, meaning every user has a unique homeworld from the moment they log in.
* **Live World Building:** Your world is a JSON recipe (`network.symbios.overlands.room`). The owner-only World Editor (Environment / Region Assets / Placements / Raw JSON tabs) sculpts terrain, atmosphere and water, spawns parametric primitives / L-systems / shape grammars / portals, and arranges them via Absolute, Scatter or Grid placements. Every widget mutates the live record in place — the world recompiles, the 3D transform gizmo follows the selection, and remote peers mirror each edit before you press **Publish to PDS**.
* **Hierarchical Generator Engine:** Every generator is a tree. A node carries variant-specific parameters — `Terrain`, `Water`, `Portal`, `LSystem`, `Shape` (CGA shape grammars), or one of the `Cuboid` / `Sphere` / `Cylinder` / `Capsule` / `Cone` / `Torus` / `Plane` / `Tetrahedron` primitives (each with its own twist/taper/bend vertex torture and PBR material) — a local transform, and a `Vec<Generator>` of children. A whole house, tree, or region becomes one named generator you can scatter, grid-array, or stash in your inventory. Strict positional rules keep the wire format unambiguous: `Terrain` may only sit at the root of a named generator (and may carry children — the "region blueprint" shape) and `Water` may only sit as a child of another generator (and is itself a leaf).
* **The Seamless Spatial Web:** Walk through physical portal doorways to travel to other users' DIDs. The engine hot-swaps the PDS data and the WebRTC mesh in the background, allowing you to traverse the federated network without ever hitting a loading screen.
* **Persistent Inventory:** Stash custom-tuned generator hierarchies (a procedural tree, a region blueprint, an L-system) into your personal inventory (`network.symbios.overlands.inventory`). Carry your creations across the network to deploy in your home room — or gift them to fellow travellers by dragging a stash entry onto their row in the People window. The recipient gets an Accept / Decline / Mute & Decline modal; concurrent offers are auto-declined as "busy" so a malicious peer cannot spam dialogs.
* **Peer-to-Peer Presence:** A lightweight broker server handles the initial SDP handshake and identity verification, then steps aside. All 60 Hz physics transforms, spatial syncing, and chat messages flow directly between peers over WebRTC. Peer DIDs are authenticated against the relay-signed session map so a peer cannot impersonate another identity over the unauthenticated data channel.
* **Parametric Avatars:** An avatar is two disjoint halves on the same record: a `visuals` generator tree (cuboids, capsules, L-systems, shape grammars — the same vocabulary as room generators) parented under your chassis, and a `locomotion` tagged-union picking one of five physics presets — `HoverBoat` (4-corner-suspension cuboid with buoyancy + drive), `Humanoid` (upright capsule with walking / wading / swimming modes), `Airplane` (continuous-thrust arcade flight), `Helicopter` (auto-stabilising hover) or `Car` (ground vehicle with handbrake). Every physical dimension and material is mutable and portable, and visual / locomotion edits stream to peers as a live preview before you commit them to your PDS. Your Bluesky profile picture is fetched directly from your PDS and rendered as your icon in the chat HUD and the People panel.
* **Atmosphere & Sky:** A horizontal cloud-deck plane is rendered through a custom WGSL fragment shader that synthesises domain-warped FBM clouds, threshold-shaped by `cover`, softened by `softness`, drifting with `wind_dir × speed`, lit by the sun direction, and faded into the room's distance-fog colour at the horizon. Cover, density, softness, drift, height and tint are all authored on the room's `Environment` and re-uniformed in place when the owner edits them — no mesh rebuild. The deck is pure-fragment work — no compute, no storage textures — so it ships on WebGL2.
* **Shareable Landmark Links:** The Diagnostics window's "Copy Landmark Link" button bundles the destination DID, the local player's current position, and yaw into a URL. Recipients on WASM open it directly in the browser; native users paste the same params into the CLI as `--did=… --pos=… --rot=…` and drop into the linked overland at the linked pose without any extra navigation. `--pds` and `--relay` overrides round-trip the same way.

## Architecture

The project is built on a "Thin Client, Heavy World" philosophy:

* **Engine:** Bevy 0.18 + Avian3D 0.6 (physics) + `bevy_egui` (UI) + `bevy_panorbit_camera` (third-person orbit) + `transform-gizmo-bevy` (in-world editor handles).
* **Procedural Ecosystem:** The sovereign `symbios` family powers every recipe — `symbios-ground` for deterministic terrain (Voronoi terracing, hydraulic and thermal erosion), `symbios` + `symbios-turtle-3d` for L-system derivation, `symbios-shape` + `bevy_symbios_shape` for CGA shape grammars, and `bevy_symbios_texture` for the procedural PBR catalogue (ground, rock, bark, leaf, twig, brick, plank, shingle, marble, ashlar, stained-glass, asphalt, cobblestone, concrete, corrugated, encaustic, iron-grille, metal, pavers, stucco, thatch, wainscoting, window). Every layer mirror exposed in a record is DAG-CBOR safe.
* **Networking:** `bevy_symbios_multiuser` over `matchbox` (WebRTC) for the peer mesh + `proto-blue-oauth` (OAuth 2.0 + DPoP) and `proto-blue-api` (XRPC client) for ATProto identity and PDS plumbing.
* **State Machine:** A three-stage `AppState` (`Login` → `Loading` → `InGame`). The loading gate blocks on **all** of the heightmap task, the room record fetch, the avatar record fetch, and the inventory record fetch before gameplay starts, so a slow PDS round-trip cannot leave the world half-loaded or let a "Publish" click clobber a real record with a default.
* **Protocol Safety:** ATProto's DAG-CBOR encoding strictly forbids floating-point numbers. Overlands wraps all continuous spatial data in fixed-point (`Fp` / `Fp2` / `Fp3` / `Fp4` / `Fp64`) structures, safely serialising complex 3D state to the PDS without violating protocol rules. Every record class also carries a `sanitize()` step that clamps grid sizes, scatter counts, L-system iterations, generator-tree depth and node count, and PBR octaves so a hostile or malformed payload from the network cannot OOM or crash the engine.

## Project Layout

* [src/main.rs](src/main.rs) / [src/lib.rs](src/lib.rs) — Binary shim and library entry: App wiring, lighting + sky-cuboid + cloud-deck setup, three-stage state machine, and the record-fetch retry/backoff for the room / avatar / inventory loading gate.
* [src/state.rs](src/state.rs) — `AppState`, the live/stored record resources, marker components, jitter buffer, chat + diagnostics logs, and item-offer bookkeeping.
* [src/boot_params.rs](src/boot_params.rs) — URL query string (WASM) and CLI args (native) for landmark deep links: destination DID, spawn pose, PDS / relay overrides, plus the `Copy Landmark Link` clipboard helper.
* [src/pds/](src/pds/) — Sovereign record schemas (`RoomRecord`, `AvatarRecord`, `InventoryRecord`), the `Generator` / `GeneratorKind` / `Placement` / `LocomotionConfig` open unions, fixed-point wire types (`Fp`/`Fp2`/`Fp3`/`Fp4`/`Fp64`), the per-variant `sanitize` clamps, and the shared XRPC plumbing (DID resolution, `FetchError`, `PutOutcome`).
* [src/world_builder/](src/world_builder/) — Recipe → ECS compiler: the recursive `compile_room_record` system (`compile.rs`), the L-system / shape-grammar / primitive / portal / material spawn arms, the cross-compile geometry & material caches, and `avatar_spawn.rs` — the avatar-side wrapper that re-uses the same dispatch arms with `SpawnCtx::avatar_mode = true` so visuals trees skip room-only behaviours (RoomEntity tag, per-prim colliders).
* [src/terrain.rs](src/terrain.rs) / [src/splat.rs](src/splat.rs) / [src/water.rs](src/water.rs) / [src/clouds.rs](src/clouds.rs) — Heightmap generation + Avian heightfield collider, the four-layer splat material extension, the Gerstner-wave water shader extension, and the FBM cloud-deck shader extension.
* [src/player/](src/player/) — Local player rig: locomotion preset hot-swap, [`hover_boat`](src/player/hover_boat.rs) / [`humanoid`](src/player/humanoid.rs) / [`airplane`](src/player/airplane.rs) / [`helicopter`](src/player/helicopter.rs) / [`car`](src/player/car.rs) physics presets, the [`visuals`](src/player/visuals.rs) generator-tree spawner, and [`portal`](src/player/portal.rs) interaction / inter-room travel.
* [src/avatar.rs](src/avatar.rs) — Bluesky profile-picture fetch and the per-DID image / `egui::TextureId` cache that backs the chat and People panels' author icons.
* [src/camera.rs](src/camera.rs) — Third-person orbit camera (`bevy_panorbit_camera`), distance fog, and chassis-yaw-following so steering rotates the world around the player.
* [src/network.rs](src/network.rs) / [src/protocol.rs](src/protocol.rs) — P2P message wire format, jitter-buffered transform smoothing, peer avatar cache, live preview broadcast for room / avatar edits, and item-offer routing.
* [src/social.rs](src/social.rs) — Asynchronous `app.bsky.graph.getRelationships` query that tags each remote peer with a [`SocialResonance`] component (Mutual / None / Unknown), reserved for future chat / People-panel adornments.
* [src/ui/](src/ui/) — Egui panels: login, diagnostics, chat, people, [avatar editor](src/ui/avatar.rs) (Visuals + Locomotion tabs), inventory, and the owner-only [world editor](src/ui/room/) (Environment / Region Assets / Placements / Raw JSON tabs).
* [src/oauth.rs](src/oauth.rs) — ATProto OAuth 2.0 + DPoP flow (WASM redirect / native loopback) plus token refresh on 401 / DPoP-nonce challenge.
* [src/editor_gizmo.rs](src/editor_gizmo.rs) — Bridge between the editor selection and the in-world 3D transform gizmo, including the proximity-targeting + world-space-detach trick that keeps the gizmo on the closest live instance of a many-placement generator.
* [src/logout.rs](src/logout.rs) — Cleanup on `OnExit(InGame)`: despawns world entities, drops session + room/avatar/inventory resources, and clears the per-DID material caches so a re-login can't render the previous user's peers.
* [src/config.rs](src/config.rs) — Centralised tuneable constants for lighting (sun, ambient, sky, cloud deck), camera + fog, locomotion physics, terrain generation, splat layers, networking, HTTP timeouts, and UI windows.
* [tests/](tests/) — Integration suite: record round-trips, sanitiser bounds, OAuth flow, biome filters, fixed-point precision, open-union forward compatibility, and the boot-param parser.

## Running Locally

To interact with other players, the client must connect to a `bevy_symbios_multiuser` relay server. The login UI defaults to a public instance if one is available.

### Native (Desktop)

For optimal physics and terrain generation performance, run in release mode:

```bash
cargo run --release
```

The native build accepts the same parameters as a hosted landmark link:

```bash
cargo run --release -- \
    --did=did:plc:example \
    --pos=10,5,-3 \    # x,z (heightmap-resolved) or x,y,z (exact)
    --rot=90 \         # spawn yaw in degrees
    --pds=https://bsky.social \
    --relay=relay.example.com
```

`--did` is sufficient to drop into someone else's overland; everything else is optional.

### WebAssembly (Browser)

The exact same codebase compiles to WASM and runs natively in modern browsers.

```bash
# Install prerequisites
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.120

# Build
cargo build --release --target wasm32-unknown-unknown

# Generate Bindings
wasm-bindgen --out-dir ./out --target web \
    target/wasm32-unknown-unknown/release/symbios-overlands.wasm
```

Serve `./out` and `./assets` alongside `index.html` using any static web server (e.g., `python -m http.server`).
