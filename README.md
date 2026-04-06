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
* **Kinematic Spline Smoothing:** Remote peers aren't just snapped to incoming packets. Transforms are pushed to a 50ms jitter buffer and interpolated using Cubic Hermite splines (`bevy_math::curve`), completely masking WebRTC network jitter for a buttery-smooth visual experience.
* **Bandwidth Throttling:** When a rover comes to a stop (e.g., parking for a "campfire" chat), the client automatically drops its transform broadcast rate from 60Hz to 2Hz, preserving bandwidth and reducing the deserialization CPU load for all connected peers.
* **Amphibious Raycast Rovers:** Custom vehicles built on [Avian3D](https://github.com/Jondolf/avian). To navigate jagged procedural terrain, vehicles use a raycast suspension system (Hooke's Law + Damping). When entering the procedural ocean, the forces seamlessly transition to Archimedean buoyancy. Drive the dunes, sail the seas.
* **Deterministic Procedural Terrain:** Powered by `bevy_symbios_ground`. Every client generates the exact same eroded landscape and triplanar PBR splat textures from a fixed seed locally.

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
