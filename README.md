# Symbios Overlands 🚧 Currently in testing

A decentralized, physics-driven multiplayer sandbox for the ATProto network.

Automatically [compiled to WASM and deployed to Github Pages](https://thejanusstream.github.io/symbios-overlands).

**Symbios Overlands** is the flagship showcase application for the Symbios ecosystem. Built on the [Bevy](https://bevyengine.org/) engine, it combines deterministic procedural generation (`symbios-ground`) with peer-to-peer WebRTC networking and federated identity (`bevy_symbios_multiuser`).

Players authenticate via Bluesky/ATProto, spawn a physics-enabled amphibious "hover-rover" adorned with their profile picture, and explore a shared, mathematically identical landscape while chatting.

There is no central game server. There is no competitive objective. It is a space to hang out, drift over procedural dunes, sail the seas, and talk.

## Key Features

* **Sovereign Identity & Avatars:** Authenticate directly via your ATProto PDS. The game automatically resolves your DID, fetches your profile picture (bypassing CORS via `sync.getBlob` on WASM), and mounts it as the double-sided "sail" of your rover.
* **Social Graph Resonance:** The environment actively queries the ATProto social graph (`app.bsky.graph.getRelationships`). If a peer in the room is a "Mutual" (you follow each other), their rover's mast tip emits a warm, identifying glow.
* **P2P WebRTC Networking:** Powered by `bevy_symbios_multiuser`. High-frequency physics transforms are broadcast over Unreliable data channels, while chat and identity data are guaranteed via Reliable channels.
* **Kinematic Spline Smoothing:** Remote peers aren't just snapped to incoming packets. Transforms are pushed to a 100 ms jitter buffer and interpolated frame-by-frame with a hand-rolled cubic Hermite spline using central-difference velocity tangents, completely masking WebRTC network jitter for a buttery-smooth visual experience. Smoothing can be toggled off in the Airship Design → Networking panel to expose raw packet jitter for debugging.
* **Bandwidth Throttling:** When a rover comes to a stop (e.g., parking for a "campfire" chat), the client automatically drops its transform broadcast rate from 60Hz to 2Hz, preserving bandwidth and reducing the deserialization CPU load for all connected peers.
* **Amphibious Raycast Rovers:** Custom vehicles built on [Avian3D](https://github.com/Jondolf/avian). To navigate jagged procedural terrain, vehicles use a raycast suspension system (Hooke's Law + Damping). When entering the procedural ocean, the forces seamlessly transition to Archimedean buoyancy. Drive the dunes, sail the seas.
* **Deterministic Procedural Terrain:** Powered by `symbios-ground` and `bevy_symbios_ground`. Each room is seeded by an FNV-1a hash of the owner's DID, so every client visiting the same overland generates a mathematically identical landscape — Voronoi terracing, hydraulic erosion, then thermal erosion — with triplanar PBR splat textures (grass / dirt / rock / snow) blended from a heightmap-derived weight map.

* **Sovereign Room Customisation:** The environment itself is an ATProto record. Room owners see an in-game "Room Settings" panel that lets them adjust the water level and sun colour and publish the result to their own PDS as a `network.symbios.overlands.room` record. Changes are broadcast live to every connected peer over a Reliable data channel and saved to the owner's repo via `com.atproto.repo.putRecord`. Guests fetch the record on load and only the authenticated owner can publish (enforced both client-side and by the PDS).

* **In-Room Chat:** An egui chat window streams Reliable messages between everyone in the room, labelled with each sender's Bluesky handle and a session-relative timestamp. Muting a peer from the Diagnostics panel hides their vessel and silences their messages locally. Incoming chat payloads are hard-clipped at 512 bytes on the receiver to neutralise malicious jumbo packets.

## Architecture

Overlands utilizes a **Sovereign Broker** pattern:

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

| Input | Action |
| --- | --- |
| **W / S** | Drive forward / reverse |
| **A / D** | Yaw left / right (turn torque) |
| **Space** | Vertical thrust (jump / hop) |
| **Right mouse drag** | Orbit camera around the rover |
| **Middle mouse drag** | Pan camera |
| **Mouse wheel** | Zoom camera |

The orbit camera follows the rover's yaw automatically, so steering always rotates the world around you rather than flipping the view.

## Repository Layout

```text
src/
├── main.rs              App wiring: plugins, state machine, loading-gate, lighting
├── config.rs            Centralised tuneable constants (no magic numbers in modules)
├── state.rs             ECS resources, components, and the AppState enum
├── protocol.rs          Serde-tagged network message enum + AirshipParams
├── network.rs           P2P broadcast, jitter buffer, Hermite smoothing, mute sync
├── pds.rs               ATProto DID / PDS resolution and room-record read/write
├── avatar.rs            Bluesky profile picture fetch + sail-material swap
├── social.rs            Async `app.bsky.graph.getRelationships` resonance query
├── rover.rs             Airship mesh build, raycast suspension, drive, buoyancy
├── terrain.rs           Heightmap generation, collider, splat textures, water volume
├── camera.rs            Pan-orbit camera that follows the local rover
├── splat.rs             `ExtendedMaterial` binding for the splat terrain shader
├── water.rs             `ExtendedMaterial` binding for the animated water shader
├── logout.rs             InGame → Login cleanup: despawn entities, tear down socket
└── ui/
    ├── login.rs         ATProto login form + auth task polling
    ├── diagnostics.rs   Peer list, mute toggles, event log, logout button
    ├── chat.rs          Reliable chat window with length-capped input
    ├── airship.rs       Airship design sliders + smoothing toggle
    ├── physics.rs       Runtime rover physics tuning sliders
    └── room.rs          Owner-only room settings editor (water level, sun colour)
```
