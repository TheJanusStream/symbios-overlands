# Symbios Overlands

> A peer-to-peer spatial web of user-owned virtual worlds for the ATProto network.

🌍 **[Enter the Overlands (Live Browser / WASM Demo)](https://thejanusstream.github.io/symbios-overlands)**

> 🚧 Prototype in active development

## What it is

Sign in with your ATProto identity, walk into a 3D world that belongs to you.
Edit the terrain, scatter buildings, dress your avatar, then step through a
portal into someone else's overland — all without ever hitting a loading
screen. There are no central game servers hosting the worlds; only a small
broker for the WebRTC handshake. Once peers connect, every transform, edit and
chat message flows directly between them.

Built in Rust on the [Bevy](https://bevyengine.org/) engine. The same binary
runs natively or in any modern browser via WASM.

## Core ideas

**Your DID is your domain.** Authenticate via ATProto OAuth — the app never
sees a password. Your world is deterministically seeded from your DID, so a
brand-new user already has a unique homeworld before they touch the editor:
its own landform, biome and settlement theme, its own colour palette, even its
own soundtrack.

**Worlds are recipes, not assets.** A room is a small record on your own PDS
carrying a tree of generators — terrain, water, portals, road networks,
parametric primitives, L-system plants, building grammars, image-bearing signs,
particle emitters. Every widget in the owner-only World Editor mutates the live
recipe in place: the world recompiles around you, and remote peers mirror each
edit before you press **Publish**. A whole region — a house, a forest, a market
square — becomes one named generator you can scatter, grid-array, or stash in
your inventory.

**Avatars are recipes too.** Your avatar is built from the same generator
vocabulary, parented under one of five physics presets — `HoverBoat`,
`Humanoid`, `Airplane`, `Helicopter` or `Car`. Visual and locomotion edits
stream to peers as a live preview before you commit them.

**Sound is procedural too.** Audio is a recipe slot, not a shipped asset: the
room carries an ambient-bed slot and every construct can carry its own
synthesised voice, played spatially at its world position. A pop-out
node-graph-and-step-sequencer editor authors patches live, and a fresh room is
already
seeded with a layered ambient soundtrack — an atonal biome texture under a
tonal theme voice — plus material-keyed impact sounds before the owner touches
a knob.

**The web is seamless.** Walk through a portal doorway and the engine hot-swaps
the destination world and the peer mesh in the background. Shareable landmark
links bundle a destination, position and heading into a URL so anyone can drop
into a specific spot in someone else's world.

**Contact brings it to life.** Every avatar is classified against the surface
beneath it each frame, and the contact drives a stack of effects: water wakes,
dust bursts, stains baked into the terrain, fading decals, and spatial audio
cues.

**Persistence and gifting.** Inventories live on your PDS. Stash a custom-tuned
tree or a whole region blueprint, carry it across the network, and drag it onto
a peer's row in the People panel to gift it. A built-in Catalogue ships a
starter library alongside whatever you've authored: hundreds of architectural
blueprints spanning 23 themes — from ancient villas and medieval keeps to
cyberpunk megatowers, steampunk foundries and alien hives. Those same theme
tags drive the mini-settlement every fresh homeworld grows around its spawn.

## Try it

The quickest way is the **[browser demo](https://thejanusstream.github.io/symbios-overlands)**.
Natively:

```bash
cargo run --release
```

See [docs/building.md](docs/building.md) for the WebAssembly build, the
landmark-link CLI flags, and the developer tooling.

## Learn more

- [docs/architecture.md](docs/architecture.md) — how it's put together: the
  engine stack, the `symbios` procedural ecosystem, protocol safety, the
  loading gate, compute offload, data flow, and the full module map.
- [docs/building.md](docs/building.md) — building & running (native + WASM),
  tests, and the headless render/analysis tooling.
- [docs/diagnostics.md](docs/diagnostics.md) — the session log, the in-game
  diagnostics panel, and the offline analyzer.
