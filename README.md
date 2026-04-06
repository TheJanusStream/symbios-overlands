# Symbios Overlands

🚧 *Currently in testing* 🚧

A multiplayer physics sandbox where airships sail across procedurally
generated terrain. Built with [Bevy 0.18](https://bevyengine.org/) and
[Avian3D](https://github.com/Jondolf/avian), networked peer-to-peer, and signed
in via [ATProto](https://atproto.com/).

## Overview

Symbios Overlands is an experimental open-world vehicle sim that targets both
native desktop and the browser (WebAssembly). Each player drives a small
solar-sailed rover / airship across a shared, procedurally eroded landscape,
with identity and avatars sourced from their Bluesky / ATProto account.

- **Procedural terrain** — Voronoi terracing with hydraulic + thermal erosion,
  4-layer triplanar splat texturing (grass / dirt / rock / snow).
- **Vehicle physics** — Avian3D rigid-body chassis with ray-cast suspension,
  drive / turn torque, lateral grip, jump, and auto-uprighting.
- **Peer-to-peer multiplayer** — WebRTC-based signalling via a relay, with
  per-peer avatar and vessel-design sync.
- **ATProto identity** — log in with a Bluesky handle + app password; your
  profile picture becomes the airship's sail.
- **In-game tuning UI** — live-editable physics, airship geometry, diagnostics,
  and chat panels (egui).

## Running

### Native

```sh
cargo run --release
```

### Web (WASM)

```sh
cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --out-dir ./dist --target web \
    --out-name symbios-overlands --no-typescript \
    target/wasm32-unknown-unknown/release/symbios-overlands.wasm
cp index.html dist/ && cp -r assets dist/
# Serve dist/ with any static HTTP server.
```

A `main`-branch build is deployed automatically to GitHub Pages via
[.github/workflows/deploy.yml](.github/workflows/deploy.yml).

## Project Layout

| Path | Purpose |
| ---- | ------- |
| [src/main.rs](src/main.rs) | App bootstrap, plugin wiring, state machine |
| [src/terrain.rs](src/terrain.rs) | Heightmap generation + erosion |
| [src/splat.rs](src/splat.rs) | Procedural splat textures |
| [src/rover.rs](src/rover.rs) | Vehicle mesh, physics, controls |
| [src/camera.rs](src/camera.rs) | Orbit camera + atmospheric fog |
| [src/network.rs](src/network.rs) | P2P sync of transforms, chat, avatars |
| [src/avatar.rs](src/avatar.rs) | ATProto profile fetch + sail texture |
| [src/protocol.rs](src/protocol.rs) | Wire messages between peers |
| [src/state.rs](src/state.rs) | Global resources + `AppState` |
| [src/config.rs](src/config.rs) | All tunable constants (central) |
| [src/ui/](src/ui/) | egui panels: login, chat, diagnostics, airship, physics |
| [assets/shaders/](assets/shaders/) | WGSL shaders for splat + water |

All tuneable numbers (suspension stiffness, terrain scale, fog density, airship
geometry, etc.) live in [src/config.rs](src/config.rs) — start there when
tweaking.

## License

See [LICENSE](LICENSE).
