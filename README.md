# Symbios Overlands

A peer-to-peer spatial web of user-owned virtual worlds for the ATProto network.

## 🚧 Prototype in active development 🚧

🌍 **[Enter the Overlands (Live Browser / WASM Demo)](https://thejanusstream.github.io/symbios-overlands)**

**Symbios Overlands** transforms your ATProto decentralized identity (DID) into a persistent, 3D virtual world. Built in Rust using the [Bevy](https://bevyengine.org/) engine, it acts as a true, sovereign spatial web.

There are no central game servers to shut down, and no walled gardens. Every artifact—from the shape of your avatar to the layout of your terrain—is authored as a data-driven recipe and stored exclusively on your own ATProto Personal Data Server (PDS). You own your space, your body, and your creations.

## Core Features

* **Your DID is Your Domain:** Authenticate securely via ATProto OAuth 2.0 + DPoP. The app never sees your password. Your room is deterministically seeded by your DID, meaning every user has a unique homeworld from the moment they log in.
* **Live World Building:** Your world is a JSON recipe (`network.symbios.overlands.room`). The owner-only World Editor sculpts terrain, atmosphere and water, spawns parametric primitives / L-systems / portals, and arranges them via Absolute, Scatter or Grid placements. Every widget mutates the live record in place — the world recompiles, the 3D transform gizmo follows the selection, and remote peers mirror each edit before you press **Publish to PDS**.
* **Hierarchical Generator Engine:** Every generator is a tree. A node carries variant-specific parameters — `Terrain`, `Water`, `Portal`, `LSystem`, or one of the `Cuboid` / `Sphere` / `Cylinder` / `Capsule` / `Cone` / `Torus` / `Plane` / `Tetrahedron` primitives (each with its own twist/taper/bend vertex torture and PBR material) — a local transform, and a `Vec<Generator>` of children. A whole house, tree, or region becomes one named generator you can scatter, grid-array, or stash in your inventory. Strict positional rules keep the wire format unambiguous: `Terrain` may only sit at the root of a named generator (and may carry children — the "region blueprint" shape) and `Water` may only sit as a child of another generator (and is itself a leaf).
* **The Seamless Spatial Web:** Walk through physical portal doorways to travel to other users' DIDs. The engine hot-swaps the PDS data and the WebRTC mesh in the background, allowing you to traverse the federated network without ever hitting a loading screen.
* **Persistent Inventory:** Stash custom-tuned generator hierarchies (a procedural tree, a region blueprint, an L-system) into your personal inventory (`network.symbios.overlands.inventory`). Carry your creations across the network to deploy in your home room — or gift them to fellow travellers by dragging a stash entry onto their row in the People window. The recipient gets an Accept / Decline / Mute & Decline modal; concurrent offers are auto-declined as "busy" so a malicious peer cannot spam dialogs.
* **Peer-to-Peer Presence:** A lightweight broker server handles the initial SDP handshake and identity verification, then steps aside. All 60 Hz physics transforms, spatial syncing, and chat messages flow directly between peers over WebRTC. Peer DIDs are authenticated against the relay-signed session map so a peer cannot impersonate another identity over the unauthenticated data channel.
* **Parametric Avatars:** Embody an amphibious `HoverRover` or a bipedal `Humanoid`. Your profile picture is fetched directly from your PDS and worn as a sail or badge. Every physical dimension and material is mutable and portable, and edits stream to peers as a live preview before you commit them to your PDS.

## Architecture

The project is built on a "Thin Client, Heavy World" philosophy:

* **Engine:** Bevy 0.18 + Avian3D 0.6 (physics) + `bevy_egui` (UI) + `transform-gizmo-bevy` (in-world editor handles).
* **Procedural Ecosystem:** The sovereign `symbios` family powers every recipe — `symbios-ground` for deterministic terrain (Voronoi terracing, hydraulic and thermal erosion), `symbios` + `symbios-turtle-3d` for L-system derivation, and `bevy_symbios_texture` for the procedural PBR catalogue (ground, rock, bark, leaf, twig, brick, plank, shingle, marble, ashlar, stained-glass, etc.). Every layer mirror exposed in a record is DAG-CBOR safe.
* **Networking:** `bevy_symbios_multiuser` over `matchbox` (WebRTC) for the peer mesh + `proto-blue` for ATProto OAuth and PDS XRPC plumbing.
* **State Machine:** A three-stage `AppState` (`Login` → `Loading` → `InGame`). The loading gate blocks on **all** of the heightmap task, the room record fetch, the avatar record fetch, and the inventory record fetch before gameplay starts, so a slow PDS round-trip cannot leave the world half-loaded or let a "Publish" click clobber a real record with a default.
* **Protocol Safety:** ATProto's DAG-CBOR encoding strictly forbids floating-point numbers. Overlands wraps all continuous spatial data in fixed-point (`Fp` / `Fp2` / `Fp3` / `Fp4` / `Fp64`) structures, safely serialising complex 3D state to the PDS without violating protocol rules. Every record class also carries a `sanitize()` step that clamps grid sizes, scatter counts, L-system iterations, generator-tree depth and node count, and PBR octaves so a hostile or malformed payload from the network cannot OOM or crash the engine.

## Project Layout

* [src/lib.rs](src/lib.rs) — App wiring, state machine, record-fetch retry/backoff.
* [src/pds/](src/pds/) — Sovereign record schemas (`RoomRecord`, `AvatarRecord`, `InventoryRecord`), fixed-point types, and the PDS XRPC plumbing.
* [src/world_builder/](src/world_builder/) — Recipe → ECS compiler: hierarchical generator spawner (terrain root anchor + descendants, L-systems, primitives, portals, water volumes) plus the placement-graph dispatcher.
* [src/player/](src/player/) — Local player rig: HoverRover (suspension + buoyancy + drive) and Humanoid (capsule walker), plus portal interaction.
* [src/network.rs](src/network.rs) / [src/protocol.rs](src/protocol.rs) — P2P message wire format, jitter-buffered transform smoothing, item-offer routing.
* [src/ui/](src/ui/) — Egui panels: login, diagnostics, chat, people, avatar editor, world editor (Environment / Generators / Placements / Raw JSON tabs), inventory.
* [src/oauth.rs](src/oauth.rs) — ATProto OAuth 2.0 + DPoP flow (WASM redirect / native loopback).
* [src/editor_gizmo.rs](src/editor_gizmo.rs) — Bridge between the editor selection and the in-world 3D transform gizmo.
* [src/config.rs](src/config.rs) — Centralised tuneable constants for lighting, rover, terrain, networking and UI windows.
* [tests/](tests/) — Integration suite: record round-trips, sanitiser bounds, OAuth flow, biome filters, fixed-point precision, open-union forward compatibility.

## Running Locally

To interact with other players, the client must connect to a `bevy_symbios_multiuser` relay server. The login UI defaults to a public instance if one is available.

### Native (Desktop)

For optimal physics and terrain generation performance, run in release mode:

```bash
cargo run --release
```

### WebAssembly (Browser)

The exact same codebase compiles to WASM and runs natively in modern browsers.

```bash
# Install prerequisites
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.118

# Build
cargo build --release --target wasm32-unknown-unknown

# Generate Bindings
wasm-bindgen --out-dir ./out --target web \
    target/wasm32-unknown-unknown/release/symbios-overlands.wasm
```

Serve `./out` and `./assets` alongside `index.html` using any static web server (e.g., `python -m http.server`).
