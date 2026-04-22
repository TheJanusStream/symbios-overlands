# Symbios Overlands 🚧 Currently in testing

A decentralized, physics-driven multiplayer sandbox for the ATProto network.

Automatically [compiled to WASM and deployed to Github Pages](https://thejanusstream.github.io/symbios-overlands).

**Symbios Overlands** is the flagship showcase application for the Symbios ecosystem — and, by now, a working sketch of a **sovereign spatial web**. Built on the [Bevy](https://bevyengine.org/) engine, it stitches together deterministic procedural generation (`symbios-ground`, `bevy_symbios`, `bevy_symbios_texture`), peer-to-peer WebRTC networking and federated identity (`bevy_symbios_multiuser`), and the ATProto PDS as a **user-owned data substrate**.

The overlands started life as a physics rover sandbox. It has since grown into a live-editable, multi-room shared reality whose every artifact is authored as an ATProto record on the player's own PDS:

* **Your avatar** is an `AvatarRecord` recipe — a `HoverRover` or `Humanoid` phenotype plus kinematics — with every dimension, material slot, and procedural texture editable from a slider panel that mutates the live resource in place.
* **Your room** is a `RoomRecord` recipe — environment, a generator graph (terrain, water, shape, L-system trees, hierarchical construct assemblies, portals) and Absolute / Scatter / Grid placements (biome-filtered with an allow-list + water relation) — authored through a tabbed Master/Detail editor with an in-world transform gizmo for direct 3D manipulation.
* **Your doorways** are `Portal` generators that teleport on collision, intra- or inter-room, with the destination owner's profile picture painted onto the portal's top face so you can see where you're about to go.

Players authenticate over OAuth 2.0 + DPoP (the app never sees a password), spawn their parametric avatar on a landscape deterministically hashed from the room owner's DID, and meet — every client generating the identical terrain locally, every edit to the world or an avatar streaming live over P2P data channels the same frame a slider moves.

There is no central game server. There is no competitive objective. The overlands are a space to hang out, drift over procedural dunes, sail the seas, walk the shoreline, step between each other's hand-authored rooms, and talk — with the scenery and the vessels themselves living on the players' own ATProto repositories, portable to any client that speaks the lexicon.

## Key Features

* **Identity & Avatars:** Authenticate directly via your ATProto PDS using OAuth 2.0 + DPoP — the app never sees your password. The game automatically resolves your DID, fetches your profile picture (bypassing CORS via `sync.getBlob` on WASM), and mounts it as the double-sided "sail" of your rover — or paints it onto the chest badge of your humanoid.
* **Parametric Avatars:** The avatar itself is an ATProto record (`network.symbios.overlands.avatar`) authored as a *recipe*, with two archetypes: an amphibious `HoverRover` vehicle and a walkable `Humanoid` character. Every dimension, colour, damping coefficient, and kinematic tuning lives in the record, so each player's vessel or body is portable and mutable from any client.
* **Social Graph Resonance:** The environment actively queries the ATProto social graph (`app.bsky.graph.getRelationships`). If a peer in the room is a "Mutual" (you follow each other), their rover's mast tip emits a warm, identifying glow.
* **P2P WebRTC Networking:** Powered by `bevy_symbios_multiuser`. High-frequency physics transforms are broadcast over Unreliable data channels, while chat and identity data are guaranteed via Reliable channels.
* **Kinematic Spline Smoothing:** Remote peers aren't just snapped to incoming packets. Transforms are pushed to a 100 ms jitter buffer and interpolated frame-by-frame with a hand-rolled cubic Hermite spline using central-difference velocity tangents, completely masking WebRTC network jitter for a buttery-smooth visual experience. Smoothing can be toggled off in the Avatar Editor → Networking panel to expose raw packet jitter for debugging.
* **Bandwidth Throttling:** When a rover comes to a stop (e.g., parking for a "campfire" chat), the client automatically drops its transform broadcast rate from 60Hz to 2Hz, preserving bandwidth and reducing the deserialization CPU load for all connected peers.
* **Amphibious Raycast Rovers:** Custom vehicles built on [Avian3D](https://github.com/Jondolf/avian). To navigate jagged procedural terrain, vehicles use a raycast suspension system (Hooke's Law + Damping). When entering the procedural ocean, the forces seamlessly transition to Archimedean buoyancy. Drive the dunes, sail the seas.
* **Deterministic Procedural Terrain:** Powered by `symbios-ground` and `bevy_symbios_ground`. Each room is seeded by an FNV-1a hash of the owner's DID, so every client visiting the same overland generates a mathematically identical landscape — Voronoi terracing, hydraulic erosion, then thermal erosion — with triplanar PBR splat textures (grass / dirt / rock / snow) blended from a heightmap-derived weight map.

* **Data-Driven Room Recipes:** The environment itself is an ATProto record (`network.symbios.overlands.room`) authored as a *recipe* — a graph of named `generators` (terrain / water / shape / l-system / **construct** / portal), `placements` (absolute, biome-filtered deterministic scatter, or regular grid with optional terrain-snap and yaw randomisation) and `traits` (ECS components to attach). The `world_builder` module compiles the recipe into Bevy entities, and every union uses `#[serde(other)] Unknown` so a client visiting a newer room skips unrecognised variants instead of crashing. Floats are stored on the wire as fixed-point `i32` values because DAG-CBOR rejects IEEE floats in records; `u64` seeds are stringified for the same reason (most JSON decoders hop through `f64` and silently truncate values above `2^53`).
* **Hierarchical Construct Primitives:** The `Construct` generator authors a tree of `PrimNode`s — cubes, spheres, cylinders, capsules, cones, and torus shapes — each with its own relative transform, solid-collider flag, and material. Child transforms are interpreted relative to the parent so a rotated assembly stays rigid, and the compiler clamps recursion depth and total node count on every apply so a malicious record cannot exhaust memory on peers.
* **Universal Procedural Materials:** Every material slot in the schema — rover hull / pontoons / mast / struts / sail, humanoid body / head / limbs, every construct node, every L-system prop bucket — carries a full `SovereignMaterialSettings` (`base_color`, `emission`, `roughness`, `metallic`, `uv_scale`) plus an embedded `SovereignTextureConfig` that drives any `bevy_symbios_texture` generator (ashlar, asphalt, bark, brick, cobblestone, concrete, corrugated, encaustic, ground, iron grille, leaf, marble, metal, pavers, plank, rock, shingle, stained glass, stucco, thatch, twig, wainscoting, window) at an author-tunable UV scale.
* **Live UX Editors:** Both the **Avatar Editor** and the owner-only **World Editor** follow the same paradigm — every widget mutates the live `LiveAvatarRecord` or `RoomRecord` resource in place, so visuals, physics and peer broadcasts update the same frame a slider moves. A menu-local debounce timer coalesces rapid slider drags into a single terrain rebuild / world-compiler pass / `RoomStateUpdate` (or `AvatarStateUpdate`) broadcast when the drag settles. Three explicit buttons drive persistence and discard: **Publish to PDS** writes the current record via `com.atproto.repo.putRecord`; **Load from PDS** rolls live edits back to the last stored record; **Reset to default** seeds the canonical default for the signed-in DID.
* **Room Customisation:** The World Editor is a tabbed Master/Detail view with Environment, Generators, Placements and Raw JSON tabs, so lighting, water level, the generator graph (terrain / water / shape / l-system / construct / portal) and absolute / scatter / grid placements are all editable in place — and any field the visual UI doesn't yet expose still round-trips via the Raw JSON tab. Numeric fields are clamped by `pds::sanitize` on every apply, so out-of-range JSON edits cannot starve memory on peers. If a previously-published record fails to decode against the current lexicon, the editor shows a recovery banner and a hard-reset button that deletes the stale record and republishes the default homeworld. Ownership is enforced both client-side (signed-in DID must match the room DID) and by the PDS. Publish outcomes surface in a status line driven by the `PublishFeedback` resource.
* **In-World Transform Gizmo:** Selecting an absolute placement in the World Editor attaches a `transform-gizmo-bevy` handle to its live entity, so the owner can translate / rotate / scale their props directly in the 3D viewport instead of typing numbers into sliders. A `Construct` referenced by many scattered placements attaches the gizmo only to the instance closest to the camera (group-select would otherwise rotate the cluster around its centroid and break per-instance math), and the dragged transform is held ephemerally during the drag and committed back into the `RoomRecord` on mouse release — a single record update, a single peer broadcast, and a single world recompile per gesture, even across a twenty-second drag.

* **Personal Inventory Stash:** Every generator the owner authors can be tucked into a personal `InventoryRecord` (`network.symbios.overlands.inventory`, `rkey = self`) published to their own PDS. The Inventory window lists every saved blueprint, lets them be renamed or removed, and drops any stashed generator into whichever room is currently being edited — so a hand-tuned L-system tree or a hierarchical `Construct` assembly survives across rooms the same way an avatar does. The stash is fetched alongside the room and avatar records during `AppState::Loading` and round-trips through `com.atproto.repo.putRecord` via a dedicated "Publish to PDS" button.

* **In-Room Chat:** An egui chat window streams Reliable messages between everyone in the room, labelled with each sender's Bluesky handle and a session-relative timestamp. Muting a peer from the Diagnostics panel hides their vessel and silences their messages locally. Incoming chat payloads are hard-clipped at 512 bytes on the receiver to neutralise malicious jumbo packets.

* **Seamless Portals:** Rooms can expose `Portal` generators that teleport the player on collision — either intra-room (snap to a coordinate) or inter-room (jump to another DID's overland without a full login round-trip). Inter-room portals paint the target owner's profile picture onto the portal's top face so you can see where each doorway leads before stepping through. The inter-room hop is an in-game hot-swap: the same `OAuthSession` is kept, and only the room + avatar records are re-fetched.

## Architecture

Overlands utilizes a **Broker** pattern:

1. **Auth:** Client runs the atproto OAuth 2.0 + DPoP authorization-code flow against the user's PDS — WASM redirects the tab, native spins a loopback `tiny_http` callback server. The resulting `OAuthSession` is then exchanged for a relay-specific service token via `com.atproto.server.getServiceAuth`.
2. **Signaling:** Client connects to a lightweight Axum relay server, proving identity with the service token.
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
├── main.rs              App wiring: plugins, state machine, four-way loading
│                        gate (terrain task + PDS room-record fetch +
│                        avatar-record fetch + inventory-record fetch),
│                        lighting
├── config.rs            Centralised tuneable constants (no magic numbers in modules)
├── state.rs             ECS resources (including Live/Stored avatar + room +
│                        inventory records for the Live UX editors),
│                        components, and the AppState enum
├── protocol.rs          Serde-tagged `OverlandsMessage` wire-protocol enum
│                        (Transform / Identity / Chat / RoomStateUpdate /
│                        AvatarStateUpdate) with per-variant channel notes
├── network.rs           P2P broadcast, jitter buffer, Hermite smoothing,
│                        identity anti-spoofing, mute sync
├── oauth.rs             atproto OAuth 2.0 + DPoP authorization-code flow
│                        (WASM sessionStorage redirect + native tiny_http
│                        loopback callback), PDS discovery via
│                        `.well-known/oauth-protected-resource`, DPoP-nonce
│                        retry helpers
├── pds/                 ATProto lexicons + XRPC plumbing — DAG-CBOR-safe
│   │                    sovereign mirrors of every record type
│   ├── mod.rs             Public re-exports + module overview
│   ├── types.rs           Shared primitives: fixed-point wrappers
│   │                      (`Fp`/`Fp2`/`Fp3`/`Fp4`/`Fp64`), `TransformData`,
│   │                      `BiomeFilter`, `ScatterBounds`, string-keyed serde
│   │                      helpers for `u64` seeds
│   ├── xrpc.rs            Shared XRPC helpers: DID-document fetch,
│   │                      `FetchError` / `XrpcError` / `PutOutcome`
│   ├── sanitize.rs        Clamps every numeric field (`limits` module +
│   │                      per-variant helpers) before the record reaches
│   │                      the world compiler — defuses malicious payloads
│   ├── texture.rs         `SovereignTextureConfig` open union + per-generator
│   │                      config mirrors (ashlar/asphalt/bark/…) and the
│   │                      `SovereignMaterialSettings` PBR wrapper
│   ├── terrain.rs         `SovereignTerrainConfig` — algorithm choice,
│   │                      erosion, four-layer splat/material block
│   ├── prim.rs            `PrimNode` hierarchical primitive nodes used by
│   │                      the `Construct` generator + `PropMeshType` enum
│   ├── generator.rs       Open-union `Generator` + `Placement` enums
│   │                      (Terrain / Water / Shape / LSystem / Construct /
│   │                      Portal; Absolute / Scatter / Grid)
│   ├── room.rs            `RoomRecord` root + `Environment` atmosphere +
│   │                      `fetch_room_record` / `put_room_record` / reset
│   ├── avatar.rs          `AvatarRecord` root (HoverRover / Humanoid
│   │                      archetypes, phenotype + kinematics), XRPC helpers
│   └── inventory.rs       `InventoryRecord` — per-owner stash of saved
│                          `Generator` blueprints, XRPC helpers
├── world_builder/       Compiler that walks a `RoomRecord` recipe and spawns
│   │                    ECS entities (deterministic ChaCha8 scatter, trait
│   │                    application, destructive rebuild on record change)
│   ├── mod.rs             Plugin surface + `RoomEntity` marker + trait
│   │                      application helpers
│   ├── compile.rs         `SpawnCtx`, the per-pass despawn+rewalk driver,
│   │                      `Environment` application, scatter/grid math
│   ├── lsystem.rs         L-system geometry + material caches (hashed by
│   │                      content), async turtle interpreter, dispatcher
│   ├── prim.rs            `Construct` node spawner: parametric meshes,
│   │                      colliders, child-transform inheritance
│   ├── portal.rs          Portal entity spawn + async profile-picture
│   │                      fetch polling for inter-room doorways
│   └── material.rs        Water volume spawn, procedural material builder,
│                          foliage texture task polling
├── player/              Local player plugin: spawns/hot-swaps avatar,
│   │                    drives physics, handles portal collision traversal
│   ├── mod.rs             Plugin surface, shared markers
│   │                      (`HoverRoverArchetype`, `HumanoidArchetype`,
│   │                      `ChestBadge`, `MastTip`, `RoverSail`), hot-swap
│   │                      between archetypes on avatar edits
│   ├── rover.rs           HoverRover: airship visual rig + raycast
│   │                      suspension + buoyancy + drive + uprighting
│   ├── humanoid.rs        Humanoid: blocky biped rig, walk controller,
│   │                      limb animator
│   └── portal.rs          Reads collision sensors, spawns the async
│                          `fetch_room_record` task that swaps the world
├── avatar.rs            Bluesky profile picture fetch + sail/badge material swap
├── social.rs            Async `app.bsky.graph.getRelationships` resonance query
├── terrain.rs           Heightmap generation (Voronoi + erosion), heightfield
│                        collider, splat texture pipeline
├── camera.rs            Pan-orbit camera that follows the local player
├── splat.rs             `ExtendedMaterial` binding for the splat terrain shader
├── water.rs             `ExtendedMaterial` binding for the animated water shader
├── logout.rs            InGame → Login cleanup: despawn entities, tear down socket
├── editor_gizmo.rs      Bridge between the World Editor's selected placement
│                        (or selected `PrimNode` inside a `Construct`) and
│                        the `transform-gizmo-bevy` 3D handle: attach /
│                        detach `GizmoTarget`, proximity-targeting for
│                        scattered constructs, commit the dragged Transform
│                        back into the `RoomRecord` on mouse release
└── ui/
    ├── mod.rs           Submodule index
    ├── login.rs         OAuth 2.0 + DPoP login form (PDS, handle, relay host,
    │                    optional destination DID), Begin/Complete auth task
    │                    pollers, WASM callback + native loopback handoff
    ├── diagnostics.rs   Peer list, mute toggles, event log, logout button
    ├── chat.rs          Reliable chat window with length-capped input
    ├── avatar.rs        Avatar Editor — parametric sliders for the
    │                    HoverRover / Humanoid archetypes, smoothing toggle,
    │                    Publish / Load / Reset to PDS
    ├── inventory.rs     Personal Inventory Stash — rename/remove blueprints,
    │                    drop them into the active room, Publish to PDS
    └── room/            Owner-only tabbed World Editor
        ├── mod.rs         Orchestration: tab router, Publish / Load / Reset,
        │                  PublishFeedback, debounce and recovery banner
        ├── environment.rs Environment tab — sun, ambient, sky, fog widgets
        ├── generators.rs  Generators tab — master list + dispatcher into
        │                  Terrain / Construct / LSystem sub-forges
        ├── terrain.rs     Terrain forge — `SovereignTerrainConfig` widgets
        ├── construct.rs   Construct forge — hierarchical `PrimNode` tree
        ├── lsystem.rs     L-system forge — source/rules/material slots
        ├── material.rs    Material + splat widgets and the unified texture
        │                  dispatcher for every `SovereignTextureConfig` variant
        ├── placements.rs  Placements tab — Absolute / Scatter / Grid editor
        ├── raw.rs         Raw JSON tab — escape hatch for fields the visual
        │                  UI doesn't yet expose
        └── widgets.rs     Shared egui helpers (fp slider, colour picker,
                           transform editor, unique-key, L-system preset)
```
