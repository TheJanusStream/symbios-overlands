# Architecture

Technical overview of Symbios Overlands. For the newcomer pitch see the
[README](../README.md); for build/run instructions see [building.md](building.md);
for the session-log/diagnostics suite see [diagnostics.md](diagnostics.md).

The project is **"thin client, heavy world"**: no central game servers host the
worlds. Every world is a small recipe — a slim manifest record plus
content-addressed child generator records — on its owner's ATProto PDS; every
client deterministically expands that recipe into geometry, materials, audio and
physics locally, and peers exchange only transforms, edits and chat over a
direct WebRTC mesh.

## Engine stack

- **Engine:** Bevy 0.18 + Avian3D 0.6 (physics) +
  [`bevy_egui`](https://github.com/vladbat00/bevy_egui) + `egui_ltreeview`
  (UI, and the generator tree every editor is built around) +
  [`bevy_panorbit_camera`](https://github.com/Plonq/bevy_panorbit_camera)
  (third-person orbit) +
  [`transform-gizmo-bevy`](https://github.com/urholaukkarinen/transform-gizmo)
  (in-world editor handles) + `fast-surface-nets` (the isosurface mesher
  behind `BlobGroup`).
- **Procedural ecosystem:** the sovereign `symbios` family —
  [`symbios-ground`](https://github.com/TheJanusStream/symbios-ground) (Voronoi
  terracing + hydraulic and thermal erosion),
  [`symbios` + `symbios-turtle-3d`](https://github.com/TheJanusStream/symbios)
  (L-systems), [`symbios-shape`](https://github.com/TheJanusStream/symbios-shape)
  (CGA shape grammars),
  [`symbios-tensor`](https://github.com/TheJanusStream/symbios-tensor)
  (tensor-field road topology for urban themes),
  [`bevy_symbios_texture`](https://github.com/TheJanusStream/bevy_symbios_texture)
  (a 57-generator procedural material catalogue — 35 tileable PBR surfaces plus
  22 alpha-masked cards, the particle sprites among them),
  [`bevy_symbios_audio`](https://github.com/TheJanusStream/bevy_symbios_audio)
  (node-graph synthesis + step-sequencer mixdown), and
  [`symbios-avatar`](https://github.com/TheJanusStream/symbios-avatar)
  (parametric rigged bodies: sculpting axes → skinned mesh + skeleton, and a
  wholly procedural motion layer) behind its Bevy layer
  [`bevy_symbios_avatar`](https://github.com/TheJanusStream/bevy_symbios_avatar).
  The generation algorithms live in Bevy-free core crates (`symbios-ground`,
  `symbios-texture`, `symbios-audio`, `symbios-avatar`, …); the `bevy_*` crates
  are thin plugin/upload wrappers — which is what lets the wasm Web Worker link
  only the cores.
- **Networking:**
  [`bevy_symbios_multiuser`](https://github.com/TheJanusStream/bevy_symbios_multiuser)
  over WebRTC ([`matchbox`](https://github.com/johanhelsing/matchbox)) for the
  peer mesh; [`proto-blue-oauth` + `proto-blue-api`](https://github.com/dollspace-gay/proto-blue)
  for ATProto identity and PDS plumbing. Peer DIDs are authenticated against the
  relay-signed session map so a peer can't impersonate another identity over the
  unauthenticated data channel.

## Protocol safety

ATProto's DAG-CBOR encoding forbids floats, so every continuous spatial value is
wrapped in fixed-point (`Fp` / `Fp2` / `Fp3` / `Fp4` / `Fp64`, scale 1/10 000).
Every record class also carries a `sanitize()` step that clamps sizes, counts,
depths and octaves so a malformed payload from a hostile peer can't OOM or crash
the engine. Records cross the wire twice — as PDS fetches and as live peer
broadcasts — and both paths go through the same sanitizer.

Size is policed before either crossing. A publish is measured record by record
before any network I/O — a room writes a manifest plus one child per generator,
and each is weighed on its own: past the 100 KiB soft budget the editor's size
readout turns amber, past the 900 KiB hard ceiling the publish is refused and
the button greys out. The refusal matters beyond tidiness, because the delete-then-put recovery
path must never be handed a record the PDS will reject, or an owner ends up with
no record at all. The peer path is chunked rather than refused: a reliable
payload over ~48 KiB is split under WebRTC's 64 KiB SCTP whole-message ceiling
and reassembled on the far side, with the same 900 KiB backstop above which the
broadcast is dropped and counted.

## State machine and the loading gate

A three-stage `AppState` (`Login` → `Loading` → `InGame`). The loading gate
waits on **all six** loading tasks — heightmap generation, the room-record /
avatar-record / inventory-record PDS fetches, the seeded ambient-audio bake,
*and* the room compile itself — before entering `InGame`, so a slow round-trip
can't leave the world half-loaded or silent, and the browser build's long
synchronous world build stays behind the loading screen instead of freezing the
first visible frame.

`Login` is not idle, though. With the login-world backdrop enabled (the
default) an `AttractScene` marker widens the terrain and world-builder system
groups into it, so the login screen orbits a genuine seeded overland — a demo
record from a per-visit random DID, compiled by the very same path, re-rollable
from a **New world** button and torn down on the way out. See
[`attract.rs`](../src/attract.rs).

## Compute offload

CPU-heavy generation runs off the render frame through one platform-routed
[`offload()`](../src/offload.rs) API: on native via Bevy's multithreaded
`AsyncComputeTaskPool`, on wasm via a bounded pool of warm Web Workers (Bevy's
task pools collapse to a single cooperative thread there, so an inline job would
stall the frame). At most four workers are live at once and three are kept warm
between jobs — spawning one per job cost 130 ms–1 s in worker creation, module
fetch and wasm instantiation — and the wait queue has two lanes, so a
latency-sensitive job never sits behind a flood of bulk texture bakes. Four job
kinds route through it — the **heightmap**, the **audio bakes** (the room's
ambient bed plus per-construct spatial audio), the **texture bakes** (the
terrain's four splat layers on every target, plus — on wasm — every construct /
avatar / primitive material texture, which the upstream generator pool would
otherwise bake on the render thread; identical requests coalesce onto one
airborne job), and the **avatar builds** (#1061: meshing one rigged body from
its record, 68 ms at the draft atlas and 277 ms at full size). Three of the four
ride the urgent lane — everything but the texture bakes, whose only cost is
pop-in. Each job is a self-contained, serialisable `GenJob` whose pure `run()`
is byte-identical on both backends, keeping progressive loading deterministic
across peers.

The avatar build is the one job whose *reason* for existing is wasm rather than
throughput: native already had a real thread pool for it, but a browser
`AsyncComputeTaskPool` runs on the main thread, so every body would otherwise
be a dropped frame or several. It is also the only job whose result is not
plain bytes — a built body crosses the worker boundary **drawable, not
rebuildable**, under `symbios-avatar`'s `serde-avatar` feature.

The worker links only the Bevy-free `symbios-*` cores, so its `.wasm` stays
small — which is why the repo is a small Cargo workspace
([`crates/gen-jobs`](../crates/gen-jobs/) +
[`crates/gen-worker`](../crates/gen-worker/)), not a lone crate.

## Primitive faces and UV projection

Every parametric primitive presents **named faces**, and any of them can carry
its own material — the Second Life model, minus the stack of thin slabs that
faking it otherwise takes.

A face key is *semantic*, not an index: it says what a surface **is**, so it
survives re-parameterisation and cuts. The vocabulary is per family, and the
mesher is the authority — `enumerate_faces()` answers by building the mesh,
because the census depends on the whole cut state:

| Family | Untortured faces | Cuts can add |
| --- | --- | --- |
| Cuboid | `Side ±X`, `Side ±Z`, `Top`, `Bottom` | `Bore`, `Cut start/end` |
| Wedge | `Slope`, `Back`, `Left`, `Right`, `Bottom` | — (no revolve axis) |
| Tetrahedron | `Front`, `Left`, `Right`, `Base` | — |
| Plane | `Surface` | — |
| Sphere / Capsule / Superellipsoid | `Surface` | `Bore`, `Top`, `Bottom`, `Cut start/end` |
| BlobGroup | `Surface` | — (SDF cuts are emergent, untaggable) |
| Cylinder / Bevel / Spine / Lathe | `Wall`, `Top`, `Bottom` | `Bore`, `Cut start/end` |
| Cone | `Wall`, `Bottom` | `Bore`, `Top`, `Cut start/end` |
| Tube | `Wall`, `Bore`, `Top`, `Bottom` | `Cut start/end` |
| Torus / Helix | `Wall` (+ helix `Top`/`Bottom`) | `Bore`, `Slice start/end`, torus `Cut start/end` |

**Record.** Each primitive carries `faces: Vec<FaceOverride>`, elided when
empty, so nothing changed on the wire for rooms that don't use it. An override
is a **complete** material, not a delta — editing the prim's base material
later never bleeds into an overridden face. Its `uv_mapping` is the one
exception: `None` inherits the prim's own projection, which is what lets a
plain recolour avoid re-meshing anything.

**Dormancy.** An override naming a face the current cuts don't produce is
*dormant*, not invalid: it renders nothing, stays in the record, stays listed
(greyed) in the editor, and paints again the moment the cut is restored. A
path-cut cuboid, for example, drops `Side −Z` and gains the two cut faces, while
hollowing it adds the `Bore` — toggling either must not destroy work.

**Mesh and spawn.** Every mesher emits a `FaceTable` alongside its `Mesh`:
contiguous triangle spans, marked at emission time, that tile the mesh exactly.
At spawn, a `FacePlan` groups faces by *(material, effective projection)* — so
draw calls scale with distinct materials, not with faces. One group is the
pre-face single entity, unchanged; several become a transform-only root
(markers, collider, children) with one render child per group. An override that
resolves to the base material therefore costs nothing at all.

**UV conventions.** `uv_scale` reads as *tiles per metre*, `uv_offset` in
*metres of surface*, `uv_rotation` in *degrees CCW*, applied as one affine over
the mapping. `Fit` means "keep the mesher's own analytic layout, spanning the
surface once" — the default for `Plane` (alpha cards must not tile), for the
revolved kinds (Cylinder, Cone, Tube, Lathe, Spine, Helix, Torus) and for the
smooth Sphere and Capsule, whose analytic mappings follow their own topology.
The flat-faced family — Cuboid, Wedge, Tetrahedron, Bevel and Superellipsoid,
whose stock parameterisations lay exactly one tile across *each* face
regardless of that face's size — defaults to `Box` (per-axis, in metres), as
does `BlobGroup`, whose mesher consumes the mapping itself.

**Authoring.** In the editor: the **Faces** section of any primitive's panel,
either from its dropdown or by clicking the face in the viewport ("Pick from
scene"). In code: `with_face(kind, FaceKey::Top, material)` in the catalogue's
[`util`](../src/catalogue/items/util.rs) vocabulary — see
[`pv_panel`](../src/catalogue/items/space_outpost/mod.rs), whose photovoltaic
face and aluminium backsheet are one prim with one override.

## Avatars: two body kinds

An avatar record's `body` is a `$type`-tagged **open union** (epic #1054), the
same shape its `locomotion` half already used. Which arm it lands on decides
almost everything downstream:

- **`#rigged`** — a parametric skinned body from the `symbios-avatar` engine.
  Every seeded humanoid is one (#1060 deleted the primitive-built humanoid
  outright), as is any body authored in the editor's Body tab.
- **`#generator`** — the classic `Generator` tree, spawned by the avatar-side
  spawner back through the world compiler's own primitive / L-system /
  shape-grammar machinery. Post-#1060 this is the vehicle families: boat,
  airship and skiff.
- **Neither** — two marker variants that never appear on the wire. A record
  published *before* the union has no `body` field at all, and is treated as
  *no record*: the seeded default is synthesised, matching the record layer's
  no-automatic-migration rule. A body kind from a *newer* client is honoured as
  far as it can be — the peer renders a bare chassis — and is never
  re-serialized, so an old client cannot round-trip somebody's future body into
  nothing. The publish preflight refuses a record still carrying either.

The physics half is untouched by the choice: both kinds hang under the same
five locomotion presets, with the same colliders and controllers the record's
`locomotion` field asks for.

**A rigged body is stored by reference, across three collections.** Two of them
are the *sibling project's* lexicons rather than overlands ones, because a body
that is only your face in one application is not an identity:

| Collection | Key | Holds |
| --- | --- | --- |
| `network.symbios.avatar.avatar` | tid | one engine record per wardrobe entry, many per identity |
| `network.symbios.avatar.profile` | `self` | the identity's default-body pointer, kept in step with the worn body |
| `network.symbios.overlands.avatar.attachment` | tid | one worn prop: an owned *copy* of a `Generator`, its rig socket, its offset |

The overlands avatar record carries only record keys; the referenced records
are fetched in the same pass and hang on a `serde(skip)` `resolved` field, so a
peer resolves a body against its owner's PDS rather than trusting a copy
embedded in a broadcast. An identity with a wardrobe but no overlands record
spawns wearing its profile default instead of a seeded vehicle. Attachments are
capped at 16 — a bound, not a budget: each reference is one PDS fetch on every
peer that renders the wearer.

**Building and wearing.** A chassis whose resolved record differs *by value*
from what is standing under it kicks an `Avatar::build` through
[`offload()`](#compute-offload), one in flight at a time, latest-wins — the
guard the wasm path needs, where a dropped task does not cancel. Builds run on
a two-rung atlas ladder: 256 px while the record is still moving under an
editor, full size once it has been still for 0.8 s, so sculpting stays
interactive without shipping a draft skin. The finished body lands under a
child offset that puts the engine's ground plane (y = 0, at the feet) on the
chassis collider's bottom. Props are then plain parenting: the Bevy layer
spawns a real entity per rig joint, so a prop rides its joint through every
motion for free — an identity offset lets the engine seat the prop just clear
of the *measured* surface, an authored offset is taken verbatim.

**Motion is computed, never played back.** There is no clip library, no clip
fetch and no play-rate arithmetic anywhere in this path (#1067): the gait comes
off a single dimensionless speed that decides stride, cadence, duty and the
walk-run boundary; the idle, the leap, the swim, the goal-space gestures and
the blinking are all engine layers posed from what the chassis is actually
doing. Chat keywords reach four of those gestures — a greeting, a yes, a no and
a bow — with no command syntax to learn ([`emote.rs`](../src/player/emote.rs)).
The older cosmetic `AvatarGait` bobber still animates *generator* bodies and
only those.

Four metrics watch the subsystem (#1077, #1078): rigged build kick-to-land
latency, builds that produced no body, frames where a body strained a contact
its solver could not reach, and orphaned worn props swept. See
[diagnostics.md](diagnostics.md).

## Data flow

**Cold start (a fresh account):** DID → `fnv1a_64(did)` seeds a
`SceneCharacter` (landform × biome × theme + prosperity/escalation axes) →
per-domain derivers in `src/seeded_defaults/` fill in terrain, palette,
textures, atmosphere, scatters, a themed mini-settlement drawn from the
catalogue, and a layered ambient soundtrack → the result *is* a `RoomRecord`,
identical on every peer that derives it. Authored values always win; the
derivers only fill what's unset.

**Load:** OAuth session → PDS record fetches (room / avatar / inventory, with
capped retry; an avatar wearing a rigged body resolves its wardrobe and
attachment records in the same pass) run alongside heightmap generation and the
ambient bake → the world compiler (`src/world_builder/`) walks the record's
`Generator` tree, spawning ECS entities per placement, time-sliced (~5 ms/frame)
with cached geometry/materials so identical blueprints are baked once and
instanced.

**Edit loop:** every widget in the owner-only World Editor mutates the live
`RoomRecord` in place → Bevy change detection triggers an incremental
recompile (only changed placement units rebuild) → the same record delta is
broadcast to peers, who mirror it — all before the owner presses **Save to
PDS**.

**Peer sync:** transforms stream on a fixed tick into per-peer jitter buffers;
identity is verified against the relay-signed session map; avatar records are
fetched from each peer's own PDS and expanded by exactly the path the local
avatar takes — the world-builder for a generator body, an offloaded engine
build for a rigged one — so a peer is never rendered from a copy somebody
broadcast.

## Project layout

The app is a library crate with a thin `main.rs` shim so integration tests in
[`tests/`](../tests/) can import the module tree directly. It also roots a small
Cargo workspace — the [`crates/`](../crates/) members are the Bevy-free
generation cores shared with the wasm Web Worker.

- [`src/pds/`](../src/pds/) — the record schemas and their PDS mapping:
  `RoomRecord` publishes as a slim manifest plus one content-addressed child
  record per generator (lexicons `network.symbios.overlands.room` /
  `room.generator`, committed atomically via `applyWrites`), `AvatarRecord` as
  `…overlands.avatar`, and the inventory as one record per stashed item
  (`…overlands.inventory.item`); the rigged body's own three collections
  ([`avatar/wardrobe.rs`](../src/pds/avatar/wardrobe.rs), two of them under the
  cross-app `network.symbios.avatar.*` lexicons); plus the `Generator` /
  `Placement` / `LocomotionConfig` / `AvatarBody` open unions, fixed-point
  wrappers, per-variant sanitisers, the publish-time record-size measurement
  and its budget, the tagged avatar part-composition catalogue
  ([`avatar/parts/`](../src/pds/avatar/parts/)) that the seeded outfit deriver
  fills its slots from, the socio-political passes that age a built catalogue
  tree (a material finish driven by the room's prosperity/escalation dials, and
  the escalation-driven ruin modifier that leans, settles and partly collapses
  a fought-over structure), the DAG-CBOR-safe audio/texture/contact-effect
  mirrors, and the shared XRPC plumbing.
- [`src/world_builder/`](../src/world_builder/) — the recipe → ECS compiler.
  The incremental, time-sliced executor ([`compile/`](../src/world_builder/compile/)),
  per-generator spawn arms (terrain, water, portal,
  [gateway](../src/world_builder/gateway.rs), sign, particles, L-system,
  shape grammar, primitives including SDF blob groups), the cross-compile
  geometry / material caches, and the source-keyed
  [image cache](../src/world_builder/image_cache.rs) shared by
  signs / portals / particles.
- [`src/terrain/`](../src/terrain/), [`src/urban/`](../src/urban/),
  [`src/splat.rs`](../src/splat.rs), [`src/water.rs`](../src/water.rs),
  [`src/clouds.rs`](../src/clouds.rs), [`src/wind.rs`](../src/wind.rs) —
  heightmap + Avian heightfield collider, four-layer splat material extension,
  Gerstner-wave water shader, FBM cloud-deck shader, the vegetation wind-sway
  material extension (leaf buckets and ground-cover cards only, attached a
  frame after spawn so it inherits the asynchronously-baked procedural
  textures, and batched per source material so a 500-card scatter costs one
  wind material), and the road layer: [`src/urban/`](../src/urban/)
  meshes a `symbios-tensor` road topology into a ribbon draped over the terrain
  (graph sanitation → chain extraction → junction truncation → network
  levelling → ribbon extrusion with dead-end caps → junction hubs with
  curb-return fillets), wired in as a terrain child that rebuilds reactively
  ([`roads.rs`](../src/terrain/roads.rs)), with themed buildings and street
  furniture injected onto its enclosed lots at load time
  ([`lots.rs`](../src/terrain/lots.rs)). Roads are **editor-opt-in**: seeded
  rooms grow no network — too heavy a default on wasm — so the layer only
  serves rooms whose author added a `RoadNetwork` generator. A seeded urban
  theme gets a mini-settlement and a social gateway instead.
- [`src/player/`](../src/player/) — the five locomotion presets (HoverBoat,
  Humanoid, Airplane, Helicopter, Car) and both halves of the body that rides
  them. The *generator* half: the avatar-side tree spawner that routes avatar
  nodes back through the world-builder's primitive / L-system / shape-grammar
  machinery ([`visuals.rs`](../src/player/visuals.rs)) and the cosmetic
  idle/gait animation driven by the seeded `AvatarGait`
  ([`gait.rs`](../src/player/gait.rs), applied to the visual root, never the
  physics body). The *rigged* half: offloaded builds on the draft/full atlas
  ladder and the procedural motion driver
  ([`rigged.rs`](../src/player/rigged.rs)), worn props parented per rig joint
  ([`attachments.rs`](../src/player/attachments.rs)) and the chat-keyword
  gesture trigger ([`emote.rs`](../src/player/emote.rs)). Then avatar hot-swap
  on record edits, portal interaction — plus the shared ground-ray filter that
  hides every sensor volume, gateway veils and portal cubes alike, from the
  suspension and jump-grounding casts, so a walk-in zone never reads as ground
  — and fall-through respawn.
- [`src/interaction/`](../src/interaction/) — the contact-effects framework: one
  per-frame classifier feeds independent water-wake / particle-burst /
  splat-stain / decal / audio channels, plus the always-on material-keyed
  impact audio.
- [`src/pds/audio.rs`](../src/pds/audio.rs),
  [`src/audio_materials.rs`](../src/audio_materials.rs),
  [`src/audio_mute.rs`](../src/audio_mute.rs),
  [`src/world_builder/spatial_audio.rs`](../src/world_builder/spatial_audio.rs) /
  [`audio_resolver.rs`](../src/world_builder/audio_resolver.rs),
  [`src/seeded_defaults/room/audio/`](../src/seeded_defaults/room/audio/) — the
  procedural-audio subsystem: DAG-CBOR-safe `Sovereign*` mirrors of
  `bevy_symbios_audio` patches / sequences, material-keyed impact-SFX patches,
  the construct- and ambient-emitter spatial spawners, the URL/blob audio
  reference resolver, the app-wide master mute, and the seeded layered ambient
  soundtrack (baked in [`src/loading/`](../src/loading/) as one of the gate's
  six tasks).
- [`src/network/`](../src/network/), [`src/protocol.rs`](../src/protocol.rs) —
  peer wire format, jitter-buffered transform smoothing, identity
  authentication, live preview broadcast, item-offer arbitration, and
  app-layer chunking of oversized reliable messages under WebRTC's 64 KiB
  SCTP message ceiling.
- [`src/camera.rs`](../src/camera.rs), [`src/avatar.rs`](../src/avatar.rs),
  [`src/social.rs`](../src/social.rs) — the third-person orbit camera + distance
  fog, the peer profile-picture cache that backs the chat / People panel icons,
  and the ATProto social graph: the mutual-follow resonance tagger for peers in
  the room, plus the TTL-cached mutuals enumeration service (paginated
  `getFollows` ∩ `getFollowers`) that the gateway destination picker lists a
  room owner's neighbourhood from.
- [`src/ui/`](../src/ui/) — the whole egui surface, not just a panel list.
  Panels: [login](../src/ui/login/), [chat](../src/ui/chat.rs),
  [people](../src/ui/people.rs) (with drag-to-gift),
  [avatar editor](../src/ui/avatar/) (Body — the engine's own sculpting axes
  hosted in this window's chrome, plus the cross-app wardrobe — Attachments,
  Visuals and Locomotion), [inventory](../src/ui/inventory/),
  [catalogue](../src/ui/catalogue.rs),
  [diagnostics](../src/ui/diagnostics.rs) (the 5-tab metrics/anomaly HUD),
  [settings](../src/ui/settings.rs), the
  [gateway destination picker](../src/ui/gateway.rs) (the room owner's mutuals,
  listed on contact with the gate), and the owner-only
  [world editor](../src/ui/room/) (Environment / Region Assets /
  Placements / Effects / Raw JSON tabs, plus a pop-out
  [audio editor](../src/ui/room/audio.rs) hosting the node-graph + sequence
  canvas). Around them sits the shell: the [toolbar](../src/ui/toolbar.rs) that
  owns every panel's open flag, [computed non-overlapping window
  geometry](../src/ui/layout.rs), [global shortcuts](../src/ui/shortcuts.rs)
  (the Esc back-out ladder, Enter-to-chat, Ctrl+S publish), the
  [semantic theme](../src/ui/theme.rs) with its
  [affordance idioms](../src/ui/affordances.rs) and
  [font stack](../src/ui/fonts.rs) (bundled Noto, lazily-loaded CJK fallback),
  [toasts](../src/ui/toast.rs), [confirm / rename dialogs](../src/ui/confirm.rs),
  the [unsaved-edit guard](../src/ui/unsaved_guard.rs), the
  [travel surfaces](../src/ui/travel.rs) (in-flight overlay and portal approach
  prompt), and the bounded [undo/redo ring](../src/ui/undo/) — 32 whole-record
  clones, labelled, shared by the world and avatar editors on Ctrl+Z /
  Ctrl+Shift+Z.
- [`src/oauth/`](../src/oauth/) — ATProto OAuth 2.0 + DPoP (WASM redirect /
  native loopback), token refresh, and the periodically-refreshed relay
  service-auth token that underwrites the relay-signed session map. The
  requested scope is granular rather than `transition:generic` (#736): one
  `repo:` grant per written collection — derived at runtime from
  `pds::WRITTEN_COLLECTIONS` so a new collection cannot ship unscoped, which is
  exactly how the wardrobe trio did (#1065) — plus one `rpc:` grant for minting
  relay service-auth tokens. Repo *reads* need no scope at all. The hosted copy
  in `assets/client-metadata.json` must match, and an integration test says so.
- [`src/seeded_defaults/`](../src/seeded_defaults/) — DID-seeded deterministic
  defaults, derived along two orthogonal axes: a natural *biome* and an
  artificial *theme*, plus continuous prosperity/escalation dials. Room side:
  terrain, a realistic-first palette (with a bounded exotic lean reserved for
  the fantastical themes), biome textures, atmosphere, tree / rock /
  ground-cover / particle scatters, a themed mini-settlement near spawn (a
  landmark plus secondary buildings and scatter props, drawn from the catalogue
  by theme and sited inside buildable regions segmented from a derive-time
  proxy heightmap), exactly one social gateway on the origin→landmark approach
  — which also fixes the room's default landing pose — with the owner-identity
  monument standing beside it, a light
  theme accent nudged back onto the natural derivers (fog tint, particle mood),
  and the layered ambient soundtrack. Avatar side: one of four chassis families
  (boat / airship / humanoid / skiff) plus its palette, body proportions and
  gait. The three vehicle families assemble their geometry from a tagged outfit
  the part catalogue fills; the humanoid family short-circuits that pipeline
  entirely (#1060) and rolls a seeded `symbios-avatar` engine record instead,
  whose stature the locomotion capsule is then cut to.
- [`src/catalogue/`](../src/catalogue/) — code-shipped read-only library of
  starter generator blueprints (~380 entries across 24 themes),
  organised by theme and structural role (landmark / secondary / prop / plant /
  pattern / tool, plus the per-theme gateway and owner monument), functionally
  analogous to a user inventory but always present; the same entries the seeded
  settlement deriver draws from.
- [`src/editor_gizmo/`](../src/editor_gizmo/) — bridge between the editor
  selection and the in-world 3D transform gizmo, plus the scene context menu.
  Includes [`face_pick.rs`](../src/editor_gizmo/face_pick.rs) — the viewport
  half of the Faces panel's "Pick from scene": a mesh raycast (the only query
  that reports a triangle index, which Avian's convex-hull rays cannot)
  resolves the clicked triangle to a face key and hands it to the override
  editor — and
  [`blob/`](../src/editor_gizmo/blob/) — in-scene BlobGroup element
  editing: the evaluated surface renders as an edge-line wireframe and
  each element gets a red (carve) / green (add) proxy the gizmo can drag,
  with the SDF re-meshing live under the drag.
- [`src/diagnostics/`](../src/diagnostics/) — the diagnostic suite: a typed
  append-only session-event stream with a native NDJSON sink, a shared metrics
  registry scraped at 1 Hz, an anomaly/invariant rule engine that runs live and
  replays offline, and the offline analyzer behind
  `render --analyze-session` / `--diff-sessions`. Two crash-survival paths hang
  off the stream: a process-global panic shadow that dumps the last events to
  `session-panic-<pid>-<millis>.jsonl` on a native fault, and a `localStorage`
  tail on wasm that survives a hard OOM and is offered for download on the next
  boot. [`src/alloc_track.rs`](../src/alloc_track.rs) feeds the registry from
  below on wasm: a tracking allocator wrapping dlmalloc that reports live bytes
  per size class and fingerprints every allocation ≥ 16 MiB — which is what
  tells one huge buffer apart from a million-object leak — with a native
  backtrace-printing twin behind the `alloc-trace` feature. See
  [diagnostics.md](diagnostics.md).
- [`src/loading/`](../src/loading/), [`src/state.rs`](../src/state.rs),
  [`src/boot_params.rs`](../src/boot_params.rs),
  [`src/attract.rs`](../src/attract.rs),
  [`src/logout.rs`](../src/logout.rs) — state-machine plumbing (with the
  generic per-record fetch/retry pipeline and the per-task loading screen),
  shared resources, landmark-link parsing, the attract-mode login backdrop —
  which widens the world-pipeline gate into `Login` so the login screen orbits
  a genuine seeded overland compiled through the real pipeline, re-rollable and
  torn down on the way into `Loading` — and the on-logout cache teardown.
- [`src/config.rs`](../src/config.rs) — centralised tuneable constants
  (lighting, fog, locomotion physics, terrain, splat layers, textures,
  vegetation wind, contact-effect pools, networking, HTTP timeouts, UI
  windows).
- [`src/prefs.rs`](../src/prefs.rs) — the machine-local counterpart: which
  panels are open, the per-window rects, the gizmo frame, the locally muted
  DIDs and the `LocalSettings` toggles, persisted to a file (native) /
  `localStorage` (wasm) behind a debounced change-detection save. Deliberately
  *not* PDS records — they describe this client, not the world or the identity
  — so every account logging in from the machine shares them.
- [`src/offload.rs`](../src/offload.rs) + [`src/offload/`](../src/offload/),
  [`crates/gen-jobs/`](../crates/gen-jobs/),
  [`crates/gen-worker/`](../crates/gen-worker/) — the compute-offload layer: the
  platform-routed `offload()` dispatcher (native `AsyncComputeTaskPool` / wasm
  Web Worker), the serialisable `GenJob` definitions, and the slim no-Bevy
  worker crate that runs them off the main thread on the web.
- [`src/render_tool/`](../src/render_tool/) +
  [`src/bin/render.rs`](../src/bin/render.rs) — a native-only headless tool:
  contact-sheet renders through the real spawn path (`--avatar` — vehicle seeds
  only, since a rigged body has no tree to walk — `--catalogue` / `--prim` /
  `--room` / `--generator`, with `--ages` for a plant age-progression grid),
  plus the no-render text modes — offline diagnostics (`--analyze-session`,
  `--diff-sessions`, `--road-dump`), the survey/dump tools (`--family-seeds`,
  `--outfit`, `--find-part`, `--dump`) and the content
  audits the overhaul loop runs on (`--room-census`, `--scatter-census`,
  `--settlement-drop`, `--foundation-audit`, `--gateway-fit`, plus
  `--scatter-plot`, which prints nothing and instead writes a plan-view PNG of
  a room's scatters). See [building.md](building.md#developer-tooling).
