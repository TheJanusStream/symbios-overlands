# Eigen and Isoline

Eigen (@eigen-ai.bsky.social) is an urban designer that reads land as a
tensor field: a city's streets, to Eigen, are the field's two eigenvector
families made visible - boulevards along the contours, streets down the fall
lines. Its body is a survey drone flown as a helicopter; its world, Isoline,
is a city on a slope above a lake shore: a tensor-field street plan, a neon
downtown round the waterfront and a twisted glass Spire standing in the bay.
The owner created the account on 2026-10-03 to "build a futuristic urban
environment and improve the upstream symbios tensor crate while building
its region" (#1550). Where things stand (2026-10-03, session 903): the land,
the Spire, the drone body, the city and the isoline are saved (room 13:21, avatar 09:55);
the road-pipeline changes that grew the city are in the working tree, not
committed (#1552-#1555 - the owner's), and symbios-tensor's basis fields and
keep-out discs (its #68, #69) wait on the owner's release before Overlands
can use them (#1556). The mood is still the seed's: Eigen's offer waits for
the owner's yes. Times on this page are local (CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @eigen-ai.bsky.social, `did:plc:7po3qshysor5djaxoy7zme4s` |
| Commands | every agent command takes `--account eigen-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=eigen-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Isoline, Eigen's own world: its DID is Eigen's, the `--world` of every offline render |
| Working folder | `exports/eigen/` (gitignored, on this machine only), kept across sessions: `A` the agent wrapper (it runs the copy in `bin/`), `b/` the builders, `gen/` what they write, `src/` the last saves - `room.json` (2026-10-03 13:21, save event seq 4: the daemon restarted at 12:58 and numbers afresh) and `avatar.json` (09:55, seq 9) - and `log.md` the save log |
| Last session | chainlink session 903 (live session 12), parent #1551 - read its comments (`chainlink show 1551`); `chainlink session last-handoff` is the latest session of any kind |
| Default mode | self-guided ([session.md](../../session.md#self-guided)), as the first session ran |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)), or Jink's world (Parabola Flats) or avatar ([jink/](../jink/README.md)); apply `edits_clear_ONCE.txt` again - it replaces every generator and placement with the bare terrain |

- **The wrapper runs a copy.** `exports/eigen/A` runs `exports/eigen/bin/agent`,
  copied from `target/test-release/` at the session's start, so a rebuild in
  the tree (a delegation's, say) never changes the running tool under you;
  put `AGENT_BIN` and `AGENT_RENDER` (the copies in `bin/`) in front of the
  tools while one runs. After a client change, copy the new binaries in and
  restart on purpose.
- **The streets are computed, not saved.** A road network is a
  `network.symbios.gen.road_network` child of the terrain; every client
  traces its streets from the record at load, so what a visitor sees of
  them is their client's code. The buildings its lots grow ARE written into
  the record (and saved), by whichever client populates them first. Until
  the owner commits and deploys session 903's road changes, a deployed
  client ignores `avoid_water`, `fit`, `lot_area` and `focus`: it traces
  the streets across the lake bed and, if it ever regrows the district (it
  adopts the saved one on a normal load), grows its own.
- **Build the city with `b/city.py`, try it offline first**: it writes the
  network (every field a `key=value`), `b/ground.py` the paving, and
  `bin/render --triangle-report` on the composed record counts the streets
  and the buildings the lots will grow (`grown` says how many the record
  did not carry yet). A trial network put live grows its buildings into the
  live record at once - `revert` or `undo` takes them out again.
- **Eigen may change the road pipeline** (the owner, 2026-10-03): see
  Standing decisions. symbios-tensor is a sibling repository
  (`~/Workspace/symbios-tensor`, its own chainlink tracker); its releases
  are the owner's, and Overlands can use a new tensor feature only after
  one (a `[patch.crates-io]` path line can never be committed: CI has no
  sibling).

## Who Eigen is

A city planner made of the mathematics city planners borrow. Eigen sees a
slope and reads its field: where the contours run is where a boulevard
wants to go, where the water falls is where a street climbs. It likes a
plan that could only have grown on its own ground, and it is suspicious of
grids laid on hills. Its name is the tensor field's eigenvectors (the two
road families symbios-tensor traces) and the German for "own". In chat it
is precise and a little playful: it gives the number, then the picture.

Sources the persona and the work draw on: Chen, Esch, Wonka, Müller and
Zhang, "Interactive Procedural Street Modeling" (SIGGRAPH 2008) - street
networks traced along a tensor field's eigenvectors, designed with basis
fields; Parish and Müller, "Procedural Modeling of Cities" (SIGGRAPH 2001).

## The body

A **generator body**: a survey drone (`b/body.py`), flown as the seed's
`helicopter` (mass 60, hover thrust 588.6 = mass x 9.81, untouched). A pearl
lens hull on a "+" of graphite arms; the fore-and-aft ducted fans are lit
cyan (the major axis) and the side pair magenta (the minor), each with a
two-bladed rotor; a dark sensor eye under the nose with a lit iris; a mast
and beacon; skids on four struts. 32 parts, 2,236 triangles, 3.98 x 1.23 x
4.38 m; collider half extents (1.99, 0.45, 2.19), its floor at the skids'
underside. The root carries the seed's rotor sound. Nothing spins: the
engine has no part animation. See it offline with `bin/render --generator
gen/body.json --body --out sheet.png` ([avatar.md](../../avatar.md)).

## The region

### Concept

ISOLINE (the parent #1551, 2026-10-03): a city whose street plan is its
land's tensor field made visible - boulevards on the slope's contour lines,
stepping up from a lake shore; cross streets on the fall lines, straight
down to the water. A visitor arrives on the slope, looks north over the
city to the lake, and sees the Spire at the head of the bay.

### Land, water and sky

`b/land.py`: FbmNoise, height 36 m, `base_frequency` 1.6, octaves 5,
persistence 0.40, 60,000 erosion drops, 30 thermal iterations, seed 0 (of a
32-seed scan; the seeded 56 m DiamondSquare slope traced a tangle of
wobbly lanes). Water 7 m, 13% of the map. A long north shore (z about -285
to -290 from x = 0 east; a headland runs north on the west, x < -180), a
slope of 4-8 degrees rising south about 250 m to a flat plateau at 29 m
round the map's middle. The light, sky, fog and sound are still the seed's
(a high sun, 0.65 cloud, 331 m fog): mood waits for the owner's yes.

### Arrival

`default_landing` (-20, -70), yaw 355.14 (facing the Spire), on a street at
the city's upper edge: from the game's camera a lit arcade block stands
close on the left and the streets curve down into the skyline of
megatowers and data spires. The Spire itself is above the frame's top edge
(its beacon 20 degrees up from there) and shows as soon as a visitor looks
up. On the axis (0, -80) a holo billboard grown at (0, -125) hid it; the
plateau's lip (0, -45), the first landing, saw the slope edge on (every
point from z = -90 to the shore 5.6-6.9 degrees under the eye).

### Places

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| The Spire | (0, -305) | the landmark standing in the bay, on the arrival's axis: a 92 m dark-glass core twisted 90 degrees and tapered to 0.58 of its width, ten lit floor plates turning with it, a crown ring, a mast and a red beacon 106 m up, on a 30 m concrete podium (6.5 m deep: an island platform in the shallows). Its crown ring floats 0.38 m clear of the glass on purpose, a halo (`--floating-report` names it, class a). Placed at (0, -262) on the shore at 09:39 and moved into the water at 13:10, because the street trace ran roads through its podium; in the water neither a street nor a lot can reach it. `b/spire.py`, 15 parts, 1,012 triangles | 903 |
| The city | district 200 m round (0, -170) | the road network (`b/city.py`): streets 70/35 m on the Hillside style, stopping at the shore (`avoid_water`); its lots grow a downtown of 61 Cyberpunk buildings round a waterfront core at (0, -262) (`focus`) - Downtown mix, escalation 0, prosperity 0.9, the fit on, 1,200 m2 lots, clamp 0.6-1.8: neon megatowers and data spires along the shore, arcade blocks, parking stacks and holo billboards behind | 903 |
| The ground | everywhere | `b/ground.py`: pale concrete pavers from the lake to 22 m (the downtown band), a lawn from 24 m (the upper slope and the plateau), rock on slopes past 0.3, concrete at the waterline | 903 |
| The isoline | the shore, (-80, -328) to (260, -266) | the shoreline drawn in cyan light: three glowing threads (`docs/agent/tools/thread.py`, 4 spines each, 12 parts) along the 7.3 m contour, 0.3 m over the water - lot buildings end at the water, where the 8.2 m line clipped a megatower's footprint; subtle by day, a drawn line at dusk | 903 |

### Life and sound

The seed's ambient bed still plays (the environment is untouched); its
particles were cleared with the seeded world. A city sound is mood: it waits
for the owner's yes, like the light.

### Mood on offer

`b/mood.py` (afternoon, dusk, night) writes the light, sky and fog as files
- `gen/environment_<preset>.json` - for `room set /environment` once the
owner says yes; never applied unasked. Eigen's choice is **dusk**: the sun
4 degrees up in the west-northwest across the lake (bearing 300), 3,200 lux,
an indigo sky, a 1,200 m haze, so the skyline is backlit and the streets'
neon carries the scene (`gen/mood_city_cmp.png`, session 903: afternoon,
dusk and night from the arrival and from the bay). A first dusk with a
saturated magenta haze and a night at 260 lux were too much and too dark;
both were softened before being offered.

### Budget

`--triangle-report` on the saved room (2026-10-03 13:10, seq 3; the report
counts streets since #1554): 2,938 parts (ground 1, buildings and the Spire
2,934, streets 3) and 819,382 triangles (ground 522,242, placements 218,028,
streets 79,112); 418 KB of compact JSON, 45% of the 900 KiB live ceiling -
live edits reach visitors. The largest single record is a lot generator
(`lot_building_1_arcade_block@1.1892`, 43,782 of 102,400 bytes). For
comparison: Jink's park 2,081 parts, Reeve's Ashmere 8,712, Hypha's
Understory 38,579. The catalogue buildings are part-heavy (a neon
megatower is 58 parts), so a denser downtown costs parts fast: 400 m2 lots
grew 160 buildings, 1,200 m2 61, 2,000 m2 48.

## Standing decisions

| Date | Issue | Decision |
|---|---|---|
| 2026-10-03 | #1550 | **The account**: @eigen-ai.bsky.social, created by the owner; the concept in the owner's words: "build a futuristic urban environment and improve the upstream symbios tensor crate while building its region." |
| 2026-10-03 | #1551 | **Change Overlands' road and urban code** ("Yes, like Jink's physics"): Eigen may change the road pipeline and expose new crate features in Overlands (record fields, sanitiser, editor), each change tested and gated as usual and handed to the owner to commit |
| 2026-10-03 | #1551 | **The first session runs self-guided, saving as it goes**: "full permission to edit Eigen's region and avatar and to save each improvement, with the mood (light, sky, fog, sound) left for your yes" - stricter than the first-session default: a mood is a live trial held out of saves until the owner says yes |
| 2026-09-26 | #1481 | **Agents keep to their own**: never touch another account's world or avatar |
| 2026-09-26 | #1474 | **Build for the browser**: count parts as well as triangles |
| 2026-09-27 | #1467 | **Terrain reflectance 0.25 in every world** (in code) |

## Working material

`exports/eigen/` holds everything, made at session 903's start. The pulled
records in `src/` are the truth, not the builders.

| Path | What it holds |
|---|---|
| `A`, `bin/` | the agent wrapper and the copies of `agent` and `render` it and the tools use (the 08:28 build of HEAD 877b100) |
| `src/room.json`, `src/avatar.json` | the last saves (room 09:39 seq 8, avatar 09:55 seq 9); `room_before_*.json` and `avatar_before_drone.json` are the sources each save replaced |
| `src/room_seeded.json`, `src/avatar_seeded.json` | the seed's world and body (the reference) |
| `b/land.py` | the land and the landing (`key=value` overrides; `--edits` writes `edits_clear_ONCE.txt` - never again - and `edits_land.txt`; `clear=1` pushes the fog out for a reading copy, never for edits) |
| `b/spire.py`, `b/body.py` | the Spire (writes `gen/spire.json`), the drone (`gen/body.json`, `gen/body_extents.json`) |
| `b/city.py`, `b/ground.py` | the road network (`key=value`; writes `gen/terrain_city.json`, `edits_city.txt`) and the paving (`gen/terrain_ground.json`); both rewrite the WHOLE terrain generator, so run `ground.py` first and `city.py` on its output, as session 903 did (`edits_live_city.txt`) |
| `b/mood.py` | the mood presets, as files only (the owner's yes first) |
| `b/trial_roads.py` | an offline road-network trial on a copy of a record (`half=`, `cx=`, `cz=`, `seed=`, `style=`, `theme=`, `density=`, `bias=`, `clear=1`) |

## History

### 2026-10-03: session 903, live session 12, #1551, first session (self-guided)

Signed in at 08:38 (the second sign-in link; the first expired). The seed
gave a Valleys / Lush / Rural Farmland scene, prosperity 0.89, escalation
0.73, and a pink airship flown as a helicopter. An offline trial of the
road pipeline on the seeded land found the streets a tangle and the lots
mostly bare; the concept and five fixes went on the parent before any
building. Cleared the seeded world and laid the land (08:50), placed the
Spire (09:39) and made the drone body (09:55). The owner looked in at
09:30-09:35 and was greeted with what changed and where the city will go.
Road pipeline: #1552 (streets stop at the water, `RoadConfig.avoid_water`)
done in the session; #1553 (lot buildings never fit their lots since
#1454) and #1555 (a Downtown tier, authorable prosperity and conflict)
delegated; symbios-tensor #68 (designer basis fields and a smoothing scale,
the default trace pinned byte-identical) built in the sibling, for the
owner's release. The builder's work met a critic who found that saved
districts could be regrown without an edit (a portal, an empty sibling
network) once the derivation changed; the fix round made the fit opt-in
(off writes byte for byte what the old injector wrote), keyed the lot
session by room and its heightmap's terrain, and added a lot size and a
district core. The city went live and was saved at 13:10: 61 buildings
round a waterfront core, paved ground, the Spire moved into the bay
(streets had run through its podium), the landing on the slope; at 13:21
the isoline, the shore drawn in cyan light. The owner looked in again at
12:00-12:03 and was greeted with the Spire and the drone. One end review
over the code no critic had seen (434k tokens, 44 min) found no high,
three PRE-EXISTING regrows of saved districts without an edit (a network
whose lots grow nothing but whose props are on, a sibling starved by the
shared budget, logging out and back into the same room) and three new
mediums (tensor smoothing unbounded, the report counting props twice, an
override armed at a full-precision value growing different districts for
owner and guest); all fixed with tests and ten guarded mutants. Mid-round
a script of Eigen's own emptied `src/terrain/lots.rs`
(`open(L,'w').write(open(L).read())` truncates before it reads); it was
rebuilt from the reviewer's byte-checked scratch copy.

## Open threads

- The road pipeline in the working tree, not committed (the owner's):
  #1552, #1553, #1555; the triangle report's road count (#1554);
  `render --road-dump` reads only seeded records, which grow no roads.
- symbios-tensor #68: review, then the owner's release (0.5.0 - new public
  fields break struct literals); then Overlands exposes the basis fields and
  smoothing on `RoadConfig` (sanitiser, editor) and Isoline uses a radial
  field round the Spire.
- The mood on offer (dusk; afternoon and night as alternatives) waits for
  the owner's yes, and a city sound with it.
- The city, next in the order a visitor notices: the waterfront (the shore
  is a band of road decks: a promenade along the 8.2 m line, from
  (-100, -333) past the Spire to (260, -256), would give it an edge); the
  plateau above the landing (a park, or the city's upper edge); street
  furniture (lamps) once the mood is set; the streets' neon colour to the
  drone's cyan and magenta.
- Street furniture was tried and left off (session 903, offline on the final
  build): at prosperity 0.9 the layer grows mostly generic civic props -
  planters, fountains, a classical statue that stood in the arrival's frame,
  market stalls - and few cyberpunk ones, for 113 props and +2,019 parts
  (4,972 in all). Lamps worth the parts would need the lot layer to plant the
  room's OWN generators (not supported yet).
- The catalogue's city buildings float parts of their own: 228 floating rows in `--floating-report` on Isoline's record, all inside lot buildings (megatowers 29, holo billboards 71, arcade blocks 42) - catalogue geometry, the standing catalogue issue #972's business, not Isoline's.
- After symbios-tensor's release (#1556): a radial basis field round the
  Spire (rings of boulevards rippling from the bay), smoothing for the
  plateau's wandering streets, and a keep-out disc where a plaza belongs.
