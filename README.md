# Symbios Overlands

A decentralized, physics-driven multiplayer sandbox for the ATProto network.

**Symbios Overlands** is the flagship showcase application for the Symbios ecosystem. Built on the [Bevy](https://bevyengine.org/) engine, it combines deterministic procedural generation (`symbios-ground`) with peer-to-peer WebRTC networking and federated identity (`bevy_symbios_multiuser`).

Players authenticate via Bluesky/ATProto, spawn a physics-enabled "hover-rover" adorned with their profile picture, and explore a shared, mathematically identical landscape while chatting.

There is no central game server. There is no competitive objective. It is a space to hang out, drift over procedural dunes, and talk.

## The Metaphor: The Kinetic Guestbook

We view this application as a **Kinetic Guestbook**. The landscape itself is fixed and shared; the avatars are dynamic vehicles powered by cryptographic identity. The physical friction of the terrain—the valleys you fall into, the ridges you climb—shapes the flow of the conversation.

It proves our **Thin Client, Heavy World** architecture: because the environment is generated deterministically on the client's CPU, we require zero bandwidth to sync the world state. The network only needs to transmit the "kinetic energy" (physics transforms) and "social energy" (chat).

## Key Features

* **Sovereign Identity:** Authenticate directly via your ATProto PDS (e.g., `bsky.social`). The game automatically fetches your profile picture and mounts it as the "sail" of your rover.
* **P2P WebRTC Networking:** Powered by `bevy_symbios_multiuser`. High-frequency physics transforms (60Hz) are broadcast over Unreliable data channels, while chat and identity data are guaranteed via Reliable channels.
* **Raycast Hover-Rovers:** Custom vehicle controllers built on [Avian3D](https://github.com/Jondolf/avian). To navigate jagged procedural terrain smoothly, vehicles use a raycast suspension system (Hooke's Law + Damping) rather than brittle wheel colliders, resulting in a buttery-smooth, dune-buggy traversal experience.
* **Deterministic Procedural Terrain:** Powered by `bevy_symbios_ground`. Every client generates the exact same eroded landscape from a fixed seed locally.

## Architecture

Overlands utilizes a **Sovereign Broker** pattern:

1. **Auth:** Client logs into ATProto to get a Service JWT.
2. **Signaling:** Client connects to a lightweight Axum relay server, proving identity via the JWT.
3. **P2P:** The relay brokers a WebRTC SDP handshake, then steps out of the way. All 60fps movement and chat data flows directly peer-to-peer.
4. **Simulation:** The Local Player is simulated as a `RigidBody::Dynamic`. Remote peers are spawned purely as visual transforms, interpolated from incoming network packets to prevent physics desynchronization across different CPUs.

## Running the Sandbox

### Prerequisites

To play multiplayer, you will need access to a running `bevy_symbios_multiuser` relay server.

### Native (Desktop)

```bash
# Run in release mode for optimal physics and terrain generation performance
cargo run --release
