# Symbios Overlands

A peer-to-peer spatial web of user-owned virtual worlds for the ATProto network.

🌍 **[Enter the Overlands (Live Browser / WASM Demo)](https://thejanusstream.github.io/symbios-overlands)**

**Symbios Overlands** transforms your ATProto decentralized identity (DID) into a persistent, 3D virtual world. Built in Rust using the [Bevy](https://bevyengine.org/) engine, it acts as a true, sovereign spatial web.

There are no central game servers to shut down, and no walled gardens. Every artifact—from the shape of your avatar to the layout of your terrain—is authored as a data-driven recipe and stored exclusively on your own ATProto Personal Data Server (PDS). You own your space, your body, and your creations.

## Core Features

* **Your DID is Your Domain:** Authenticate securely via ATProto OAuth 2.0 + DPoP. The app never sees your password. Your room is deterministically seeded by your DID, meaning every user has a unique homeworld from the moment they log in.
* **Live World Building:** Your world is a JSON recipe (`network.symbios.overlands.room`). Use the in-world UI and 3D transform gizmos to sculpt terrain, adjust water levels, and place objects (Absolute, Scatter, or Grid arrays). Changes serialize to your PDS and stream to visiting peers instantly.
* **The Seamless Spatial Web:** Walk through physical portal doorways to travel to other users' DIDs. The engine hot-swaps the PDS data and the WebRTC mesh in the background, allowing you to traverse the federated network without ever hitting a loading screen.
* **Persistent Inventory:** Stash custom-tuned generators (like a procedural tree or a complex architectural blueprint) into your personal inventory (`network.symbios.overlands.inventory`). Carry your creations across the network to deploy in any room you visit.
* **Peer-to-Peer Presence:** A lightweight broker server handles the initial SDP handshake and identity verification, then steps aside. All 60Hz physics transforms, spatial syncing, and chat messages flow directly between peers over WebRTC.
* **Parametric Avatars:** Embody an amphibious `HoverRover` or a bipedal `Humanoid`. Your profile picture is fetched directly from your PDS and worn as a sail or badge. Every physical dimension and material is mutable and portable.

## Architecture

The project is built on a "Thin Client, Heavy World" philosophy:

* **Engine:** Bevy 0.18 + Avian3D 0.6 (Physics) + `bevy_egui` (UI).
* **Procedural Ecosystem:** Relies on the sovereign `symbios` crates (`symbios-ground`, `bevy_symbios_texture`, etc.) for deterministic terrain and L-System derivation.
* **Networking:** `matchbox` (WebRTC) + `proto-blue` (ATProto/OAuth).
* **Protocol Safety:** ATProto's DAG-CBOR encoding strictly forbids floating-point numbers. Overlands wraps all continuous spatial data in fixed-point (`Fp`) structures, safely serializing complex 3D state to the PDS without violating protocol rules.

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
