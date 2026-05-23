# Symbios Overlands

> A peer-to-peer spatial web of user-owned virtual worlds for the ATProto network.

🌍 **[Enter the Overlands (Live Browser / WASM Demo)](https://thejanusstream.github.io/symbios-overlands)**

> 🚧 Prototype in active development

## What it is

Sign in with your ATProto identity, walk into a 3D world that belongs to you. Edit the terrain, scatter buildings, dress your avatar, then step through a portal into someone else's overland — all without ever hitting a loading screen. There are no central game servers hosting the worlds; only a small broker for the WebRTC handshake. Once peers connect, every transform, edit and chat message flows directly between them.

Built in Rust on the [Bevy](https://bevyengine.org/) engine. The same binary runs natively or in any modern browser via WASM.

## Core ideas

**Your DID is your domain.** Authenticate via ATProto OAuth 2.0 + DPoP — the app never sees a password. Your world is deterministically seeded from your DID, so a brand-new user already has a unique homeworld before they touch the editor.

**Worlds are recipes, not assets.** A room is a JSON record (`network.symbios.overlands.room`) carrying a tree of `Generator` nodes — terrain, water, portals, parametric primitives, L-systems, CGA shape grammars, image-bearing signs, and CPU particle emitters. Every widget in the owner-only World Editor mutates the live record in place: the world recompiles, the 3D transform gizmo follows the selection, and remote peers mirror each edit before you press **Publish to PDS**. A whole region — a house, a forest, a market square — becomes one named generator you can scatter, grid-array, or stash in your inventory.

**Avatars are recipes too.** A `visuals` generator tree (same vocabulary as a room) is parented under one of five physics presets — `HoverBoat`, `Humanoid`, `Airplane`, `Helicopter` or `Car`. Visual and locomotion edits stream to peers as a live preview before you commit them.

**The web is seamless.** Walk through a portal doorway and the engine hot-swaps the destination PDS data and the peer mesh in the background. Shareable landmark links bundle a destination DID, position and yaw into a URL (or CLI flags on native) so anyone can drop into a specific spot in someone else's world.

**Contact effects bring it to life.** Every avatar — yours and every peer's — is classified against the surface beneath it each frame, and the contact drives a stack of effects: Gerstner-wave water wakes, transient particle bursts, persistent splat-stains baked into the terrain, fading projected decals, and spatial audio cues. Wakes and stains are always-on; particle / decal / audio channels are PDS-authored per room.

**Persistence and gifting.** Inventories live on your PDS (`network.symbios.overlands.inventory`). Stash a custom-tuned tree or a whole region blueprint, carry it across the network, and drag it onto a peer's row in the People panel to gift it. A code-shipped Catalogue ships a small starter set (villa, castle, several L-system trees, a teleporter) alongside whatever you've authored.

## Architecture

The project is "thin client, heavy world":

- **Engine:** Bevy 0.18 + Avian3D 0.6 (physics) + [`bevy_egui`](https://github.com/vladbat00/bevy_egui) (UI) + [`bevy_panorbit_camera`](https://github.com/Plonq/bevy_panorbit_camera) (third-person orbit) + [`transform-gizmo-bevy`](https://github.com/urholaukkarinen/transform-gizmo) (in-world editor handles).
- **Procedural ecosystem:** the sovereign `symbios` family — [`symbios-ground`](https://github.com/TheJanusStream/symbios-ground) (Voronoi terracing + hydraulic and thermal erosion), [`symbios` + `symbios-turtle-3d`](https://github.com/TheJanusStream/symbios) (L-systems), [`symbios-shape`](https://github.com/TheJanusStream/symbios-shape) (CGA shape grammars), and [`bevy_symbios_texture`](https://github.com/TheJanusStream/bevy_symbios_texture) (~23-material procedural PBR catalogue).
- **Networking:** [`bevy_symbios_multiuser`](https://github.com/TheJanusStream/bevy_symbios_multiuser) over WebRTC ([`matchbox`](https://github.com/johanhelsing/matchbox)) for the peer mesh; [`proto-blue-oauth` + `proto-blue-api`](https://github.com/dollspace-gay/proto-blue) for ATProto identity and PDS plumbing. Peer DIDs are authenticated against the relay-signed session map so a peer can't impersonate another identity over the unauthenticated data channel.
- **Protocol safety.** ATProto's DAG-CBOR encoding forbids floats, so every continuous spatial value is wrapped in fixed-point (`Fp` / `Fp2` / `Fp3` / `Fp4` / `Fp64`). Every record class also carries a `sanitize()` step that clamps sizes, counts, depths and octaves so a malformed payload from a hostile peer can't OOM or crash the engine.
- **State machine.** A three-stage `AppState` (`Login` → `Loading` → `InGame`). The loading gate waits on the heightmap *and* the room / avatar / inventory PDS fetches before gameplay starts, so a slow round-trip can't leave the world half-loaded.

## Project layout

The crate is a library with a thin `main.rs` shim so integration tests in [`tests/`](tests/) can import the module tree directly.

- [`src/pds/`](src/pds/) — record schemas (`RoomRecord`, `AvatarRecord`, `InventoryRecord`), the `Generator` / `Placement` / `LocomotionConfig` open unions, fixed-point wrappers, per-variant sanitisers, and the shared XRPC plumbing.
- [`src/world_builder/`](src/world_builder/) — the recipe → ECS compiler. Per-generator spawn arms (terrain, water, portal, sign, particles, L-system, shape grammar, primitives), the cross-compile geometry / material caches, and the source-keyed [image cache](src/world_builder/image_cache.rs) shared by signs / portals / particles.
- [`src/terrain.rs`](src/terrain.rs), [`src/splat.rs`](src/splat.rs), [`src/water.rs`](src/water.rs), [`src/clouds.rs`](src/clouds.rs) — heightmap + Avian heightfield collider, four-layer splat material extension, Gerstner-wave water shader, FBM cloud-deck shader.
- [`src/player/`](src/player/) — the five locomotion presets and portal interaction.
- [`src/interaction/`](src/interaction/) — the contact-effects framework: one classifier feeds independent water-wake / particle-burst / splat-stain / decal / audio channels.
- [`src/network/`](src/network/), [`src/protocol.rs`](src/protocol.rs) — peer wire format, jitter-buffered transform smoothing, identity authentication, live preview broadcast, item-offer arbitration.
- [`src/ui/`](src/ui/) — egui panels: [login](src/ui/login/), [chat](src/ui/chat.rs), [people](src/ui/people.rs) (with drag-to-gift), [avatar editor](src/ui/avatar/), [inventory](src/ui/inventory/), [catalogue](src/ui/catalogue.rs), [diagnostics](src/ui/diagnostics.rs), and the owner-only [world editor](src/ui/room/) (Environment / Region Assets / Placements / Effects / Raw JSON tabs).
- [`src/oauth/`](src/oauth/) — ATProto OAuth 2.0 + DPoP (WASM redirect / native loopback) and token refresh.
- [`src/seeded_defaults/`](src/seeded_defaults/) — DID-seeded deterministic defaults for terrain, palette, atmosphere, avatar body / palette / gait. Record-authored values always win; the derivers fill in only what's unset, so a fresh account is already a fully-furnished room.
- [`src/catalogue/`](src/catalogue/) — code-shipped read-only library of starter generator blueprints, functionally analogous to a user inventory but always present.
- [`src/editor_gizmo/`](src/editor_gizmo/) — bridge between the editor selection and the in-world 3D transform gizmo.
- [`src/loading.rs`](src/loading.rs), [`src/state.rs`](src/state.rs), [`src/boot_params.rs`](src/boot_params.rs), [`src/logout.rs`](src/logout.rs) — state-machine plumbing, shared resources, landmark-link parsing, and the on-logout cache teardown.
- [`src/config.rs`](src/config.rs) — centralised tuneable constants (lighting, fog, locomotion physics, terrain, splat layers, contact-effect pools, networking, HTTP timeouts, UI windows).

## Running locally

To meet other players the client connects to a `bevy_symbios_multiuser` relay; the login UI defaults to a public instance if one is available.

### Native

```bash
cargo run --release
```

The native build also accepts the same parameters a landmark link encodes:

```bash
cargo run --release -- \
    --did=did:plc:example \
    --pos=10,5,-3 \          # x,z (heightmap-resolved) or x,y,z (exact)
    --rot=90 \               # spawn yaw in degrees
    --pds=https://bsky.social \
    --relay=relay.example.com
```

`--did` alone is enough to drop into someone else's overland.

### WebAssembly

```bash
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.122

cargo build --release --target wasm32-unknown-unknown
wasm-bindgen --out-dir ./out --target web \
    target/wasm32-unknown-unknown/release/symbios-overlands.wasm
```

Serve `./out` and `./assets` alongside `index.html` with any static web server (e.g. `python -m http.server`).
