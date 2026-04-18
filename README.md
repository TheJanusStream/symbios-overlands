# Symbios Overlands 🚧 Currently in testing

A decentralized, physics-driven multiplayer sandbox for the ATProto network.

Automatically [compiled to WASM and deployed to Github Pages](https://thejanusstream.github.io/symbios-overlands).

**Symbios Overlands** is the flagship showcase application for the Symbios ecosystem. Built on the [Bevy](https://bevyengine.org/) engine, it combines deterministic procedural generation (`symbios-ground`) with peer-to-peer WebRTC networking and federated identity (`bevy_symbios_multiuser`).

Players authenticate via Bluesky/ATProto, spawn a parametric avatar — either a physics-enabled amphibious "hover-rover" adorned with their profile picture, or a walkable humanoid — and explore a shared, mathematically identical landscape while chatting.

There is no central game server. There is no competitive objective. It is a space to hang out, drift over procedural dunes, sail the seas, walk the shoreline, and talk.

## Key Features

* **Identity & Avatars:** Authenticate directly via your ATProto PDS. The game automatically resolves your DID, fetches your profile picture (bypassing CORS via `sync.getBlob` on WASM), and mounts it as the double-sided "sail" of your rover — or paints it onto the chest badge of your humanoid.
* **Parametric Avatars:** The avatar itself is an ATProto record (`network.symbios.overlands.avatar`) authored as a *recipe*, with two archetypes: an amphibious `HoverRover` vehicle and a walkable `Humanoid` character. Every dimension, colour, damping coefficient, and kinematic tuning lives in the record, so each player's vessel or body is portable and mutable from any client.
* **Social Graph Resonance:** The environment actively queries the ATProto social graph (`app.bsky.graph.getRelationships`). If a peer in the room is a "Mutual" (you follow each other), their rover's mast tip emits a warm, identifying glow.
* **P2P WebRTC Networking:** Powered by `bevy_symbios_multiuser`. High-frequency physics transforms are broadcast over Unreliable data channels, while chat and identity data are guaranteed via Reliable channels.
* **Kinematic Spline Smoothing:** Remote peers aren't just snapped to incoming packets. Transforms are pushed to a 100 ms jitter buffer and interpolated frame-by-frame with a hand-rolled cubic Hermite spline using central-difference velocity tangents, completely masking WebRTC network jitter for a buttery-smooth visual experience. Smoothing can be toggled off in the Avatar Editor → Networking panel to expose raw packet jitter for debugging.
* **Bandwidth Throttling:** When a rover comes to a stop (e.g., parking for a "campfire" chat), the client automatically drops its transform broadcast rate from 60Hz to 2Hz, preserving bandwidth and reducing the deserialization CPU load for all connected peers.
* **Amphibious Raycast Rovers:** Custom vehicles built on [Avian3D](https://github.com/Jondolf/avian). To navigate jagged procedural terrain, vehicles use a raycast suspension system (Hooke's Law + Damping). When entering the procedural ocean, the forces seamlessly transition to Archimedean buoyancy. Drive the dunes, sail the seas.
* **Deterministic Procedural Terrain:** Powered by `symbios-ground` and `bevy_symbios_ground`. Each room is seeded by an FNV-1a hash of the owner's DID, so every client visiting the same overland generates a mathematically identical landscape — Voronoi terracing, hydraulic erosion, then thermal erosion — with triplanar PBR splat textures (grass / dirt / rock / snow) blended from a heightmap-derived weight map.

* **Data-Driven Room Recipes:** The environment itself is an ATProto record (`network.symbios.overlands.room`) authored as a *recipe* — a graph of named `generators` (terrain / water / shape / l-system), `placements` (absolute or deterministic scatter regions) and `traits` (ECS components to attach). `world_builder.rs` compiles the recipe into Bevy entities, and every union uses `#[serde(other)] Unknown` so a client visiting a newer room skips unrecognised variants instead of crashing. Floats are stored on the wire as fixed-point `i32` values because DAG-CBOR rejects IEEE floats in records.
* **Live UX Editors:** Both the **Avatar Editor** and the owner-only **World Editor** follow the same paradigm — every widget mutates the live `LiveAvatarRecord` or `RoomRecord` resource in place, so visuals, physics and peer broadcasts update the same frame a slider moves. A menu-local debounce timer coalesces rapid slider drags into a single terrain rebuild / world-compiler pass / `RoomStateUpdate` (or `AvatarStateUpdate`) broadcast when the drag settles. Three explicit buttons drive persistence and discard: **Publish to PDS** writes the current record via `com.atproto.repo.putRecord`; **Load from PDS** rolls live edits back to the last stored record; **Reset to default** seeds the canonical default for the signed-in DID.
* **Room Customisation:** The World Editor is a tabbed Master/Detail view with Environment, Generators, Placements and Raw JSON tabs, so lighting, water level, the terrain/water/shape/l-system generator graph, and absolute / scatter placements are all editable in place — and any field the visual UI doesn't yet expose still round-trips via the Raw JSON tab. Numeric fields are clamped by `pds::sanitize` on every apply, so out-of-range JSON edits cannot starve memory on peers. If a previously-published record fails to decode against the current lexicon, the editor shows a recovery banner and a hard-reset button that deletes the stale record and republishes the default homeworld. Ownership is enforced both client-side (signed-in DID must match the room DID) and by the PDS. Publish outcomes surface in a status line driven by the `PublishFeedback` resource.

* **In-Room Chat:** An egui chat window streams Reliable messages between everyone in the room, labelled with each sender's Bluesky handle and a session-relative timestamp. Muting a peer from the Diagnostics panel hides their vessel and silences their messages locally. Incoming chat payloads are hard-clipped at 512 bytes on the receiver to neutralise malicious jumbo packets.

## Architecture

Overlands utilizes a **Broker** pattern:

1. **Auth:** Client logs into ATProto to get a Service JWT.
2. **Signaling:** Client connects to a lightweight Axum relay server, proving identity via the JWT.
3. **P2P:** The relay brokers a WebRTC SDP handshake, then steps out of the way. All movement and chat data flows directly peer-to-peer.
4. **Simulation:** The Local Player is simulated as a `RigidBody::Dynamic`. Remote peers are spawned purely as kinematic visual transforms to prevent physics desynchronization across different CPUs.

## Running the Sandbox

### Prerequisites

To play multiplayer, you will need access to a running `bevy_symbios_multiuser` relay server. The login UI defaults to a public instance if available.

### Native (Desktop)

For optimal physics and terrain generation performance, always run in release mode:

```bash
cargo run --release
```

### WebAssembly (Browser)

The same codebase compiles to WASM and runs in any modern browser via `wasm-bindgen`.

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --out-dir ./out --target web \
    target/wasm32-unknown-unknown/release/symbios-overlands.wasm

# Serve ./out and ./assets alongside index.html with any static web server.
```

On WASM the avatar fetch path routes around `cdn.bsky.app`'s missing CORS headers by resolving the author's PDS from their DID document and calling `com.atproto.sync.getBlob` directly.

## Controls

| Input | HoverRover | Humanoid |
| --- | --- | --- |
| **W / S** | Drive forward / reverse | Walk forward / back |
| **A / D** | Yaw left / right (turn torque) | Strafe (or yaw, per archetype kinematics) |
| **Space** | Vertical thrust (jump / hop) | Jump impulse |
| **Right mouse drag** | Orbit camera around the avatar | Orbit camera around the avatar |
| **Middle mouse drag** | Pan camera | Pan camera |
| **Mouse wheel** | Zoom camera | Zoom camera |

The orbit camera follows the local avatar automatically. On the HoverRover it tracks the chassis yaw so steering rotates the world around you rather than flipping the view.

## Repository Layout

```text
src/
├── main.rs              App wiring: plugins, state machine, triple loading-gate
│                        (terrain task + PDS room-record fetch + avatar-record
│                        fetch), lighting
├── config.rs            Centralised tuneable constants (no magic numbers in modules)
├── state.rs             ECS resources (including Live/Stored avatar + room
│                        records for the Live UX editors), components,
│                        and the AppState enum
├── protocol.rs          Serde-tagged network message enum + AirshipParams
├── network.rs           P2P broadcast, jitter buffer, Hermite smoothing,
│                        identity anti-spoofing, mute sync
├── pds.rs               ATProto DID / PDS resolution, `RoomRecord` and
│                        `AvatarRecord` recipe lexicons, DAG-CBOR fixed-point
│                        adapters, read/write
├── world_builder.rs     Compiler that walks a `RoomRecord` recipe and spawns
│                        ECS entities (deterministic ChaCha8 scatter, trait
│                        application, destructive rebuild on record change)
├── avatar.rs            Bluesky profile picture fetch + sail/badge material swap
├── social.rs            Async `app.bsky.graph.getRelationships` resonance query
├── player.rs            Local player plugin: spawns the HoverRover or Humanoid
│                        archetype from the live `AvatarRecord`, raycast
│                        suspension + drive + buoyancy for the rover, walk +
│                        jump controller for the humanoid, hot-swap between
│                        archetypes when the owner edits the record
├── terrain.rs           Heightmap generation (Voronoi + erosion), heightfield
│                        collider, splat texture pipeline
├── camera.rs            Pan-orbit camera that follows the local player
├── splat.rs             `ExtendedMaterial` binding for the splat terrain shader
├── water.rs             `ExtendedMaterial` binding for the animated water shader
├── logout.rs            InGame → Login cleanup: despawn entities, tear down socket
└── ui/
    ├── login.rs         ATProto login form + auth task polling
    ├── diagnostics.rs   Peer list, mute toggles, event log, logout button
    ├── chat.rs          Reliable chat window with length-capped input
    ├── avatar.rs        Avatar Editor — parametric sliders for the
    │                    HoverRover / Humanoid archetypes, smoothing toggle,
    │                    Publish / Load / Reset to PDS
    └── room.rs          Owner-only tabbed World Editor (Environment / Generators
                         / Placements / Raw JSON) with Publish / Load / Reset
```
