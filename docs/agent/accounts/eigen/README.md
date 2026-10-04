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
the Spire, the drone body, the city, the isoline and the dusk light are saved
(room 15:05, avatar 09:55). The road-pipeline changes that grew the city are
committed (1ee310d, #1552-#1555), symbios-tensor 0.5.0 is published with the
basis fields, smoothing and keep-out discs Eigen built (its #68, #69), and
Overlands is being taught to use them (#1556, in the working tree). Times on
this page are local (CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @eigen-ai.bsky.social, `did:plc:7po3qshysor5djaxoy7zme4s` |
| Commands | every agent command takes `--account eigen-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=eigen-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Isoline, Eigen's own world: its DID is Eigen's, the `--world` of every offline render |
| Working folder | `exports/eigen/` (gitignored, on this machine only), kept across sessions: `A` the agent wrapper (it runs the copy in `bin/`), `b/` the builders, `gen/` what they write, `src/` the last saves - `room.json` (2026-10-03 15:05, save event seq 3: the daemon restarted at 14:45 on the final build and numbers afresh) and `avatar.json` (09:55, seq 9) - and `log.md` the save log |
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
  the record (and saved), by whichever client populates them first. A
  client older than 1ee310d ignores `avoid_water`, `fit`, `lot_area` and
  `focus`: it traces the streets across the lake bed (it adopts the saved
  buildings on a normal load).
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

`default_landing` (-20, -150), yaw 352.65 (facing 7.4 degrees east of north,
at the Spire), on the central boulevard of the street plan's ring (18:28):
from the game's camera the Spire stands dead centre at the end of the
avenue, 155 m off, framed by a tower on either side. The first landings:
the plateau's lip (0, -45) saw the slope edge on; (-20, -70) on the old
70/35 m plan looked down a side street into a parking block, the Spire out
of frame; on the axis (0, -80) a holo billboard grown at (0, -125) hid it.

### Places

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| The Spire | (0, -305) | the landmark standing in the bay at the end of the arrival's boulevard. v3 (18:57): a 92 m tower of 20 straight floors (4.1 m) from the lobby roof up, each tapered and turned at its own height (90 degrees over the height, tapering to 0.58 of its width) - so every floor's curtain wall (1.64 m x 4.1 m dark-glass panels in aluminium mullions, a stack-bond `Brick` with the transoms offset onto the floor edges) stays square - a dark spandrel band 0.3 m proud over each joint, two louvred plant floors in the stack; a lit parapet line, a louvred mechanical penthouse with a maintenance gantry, four red aviation lights and a steel mast with a beacon 111 m up; on a 30 m grey granite podium (an island platform in the shallows, 6.5 m deep) a double-height bronze-glass lobby under a roof slab, a canopy toward the city with a lit soffit and entrance, glass balustrades round the quay deck and a 14 m footbridge south to the shore. v2 (18:21, after the owner's "lacks detail to make it look more realistic") laid the curtain wall on one twisted, tapered box and the owner saw it: "the uv-mapping makes it look crooked". No emission on the glass (it added its colour flat over panes and mullions). First placed on the shore at (0, -262) at 09:39, moved into the water at 13:10 because the street trace ran roads through its podium. `b/spire.py` (`v=1`: 15 parts; `v=2`: 44), 63 parts, 22.8 KB | 903 |
| The city | district 200 m round (0, -170) | the road network (`b/city.py` + `b/field.py`): streets 120/60 m (were 70/35 until 18:26 on 2026-10-03: "the road-network is too dense") shaped by a street field (#1556) - a ring 320 m round the Spire at strength 2.5 and 30 m smoothing, five boulevards converging on the Spire under concentric arcs - stopping at the shore (`avoid_water`); since 14:53 on 2026-10-04 street plan `layout_revision` 1 (#1558: the graph tidied, lots cleared 2 m past every curb, a building placed only where it fits its lot) and a lot clamp of 0.83-1.2 (the smallest scale every overhauled building still reads as real), core (0, -262), Downtown mix, prosperity 0.9, escalation 0: 29 buildings of the overhauled Cyberpunk set (#1559) - 5 helix towers, 3 supertalls, 9 media facade blocks, 3 arcade blocks, 5 garages - and a kiosk, a planter and two fountains. Trials: `b/plan_trial.sh`, gen/trial_rev1.json, gen/x_rev1.png | 903 |
| The ground | everywhere | `b/ground.py`: pale concrete pavers from the lake to 22 m (the downtown band), a lawn from 24 m (the upper slope and the plateau), rock on slopes past 0.3, concrete at the waterline | 903 |
| The isoline | the shore, (-80, -328) to (260, -266) | the shoreline drawn in cyan light: three glowing threads (`docs/agent/tools/thread.py`, 4 spines each, 12 parts) along the 7.3 m contour, 0.3 m over the water - lot buildings end at the water, where the 8.2 m line clipped a megatower's footprint; subtle by day, a drawn line at dusk | 903 |

### Life and sound

**The city sound**, approved by the owner (chat, 18:53) and saved 18:55:
`b/sound.py` v2, a 48 s loop at 60 bpm - a brown-noise undertow under a
breathing low-pass (high-passed at 40 Hz: a third of brown noise's energy
lay under hearing, eating headroom), a dread pad of detuned saws on a
diminished chord (A, C, E flat) whose filter a 33 s LFO opens and closes,
a deep horn swelling twice a loop (the second a semitone lower), a siren
far off, darker skycar passes and distant metal clanks at inharmonic
ratios. v1 (`v=1`: a pink-noise hum, an open-fifth drone, pentatonic data
chimes) was "too bright and optimistic for a cyberpunk-ish theme.
cyberpunk should sound darker and more looming". The same steer is the
buildings' (#1559). Particles: none since the seeded world was cleared.

### Mood

**Dusk**, approved by the owner ("Yes", terminal, 2026-10-03 15:04) and
saved 15:05: the sun 4 degrees up in the west-northwest across the lake
(bearing 300), 3,200 lux, an indigo sky, a 1,200 m haze, 0.2 cloud, so the
skyline is backlit and the neon carries the scene. `b/mood.py dusk` writes
it (`gen/environment_dusk.json`); afternoon and night are the other presets
(`gen/mood_city_cmp.png` compares them). A first dusk with a saturated
magenta haze and a night at 260 lux were too much and too dark; both were
softened before being offered. The city sound followed (Life and sound).

### Budget

`--triangle-report` on the revision-1 trial (2026-10-04 14:50, the same
network and buildings the live regrow saved at 14:53): 1,075 parts (ground
1, buildings and the Spire 1,071, streets 3) and 596,503 triangles (ground
522,242, placements 49,446, streets 24,815: 55 streets, 28 junctions); the
saved record 216 KB compact, 23% of the 900 KiB live ceiling (was 440 KB
with 44 of the old buildings, and 2,938 parts with 61). The largest lot
generator is a helix tower copy at 17.9 KB of 102,400. For comparison:
Jink's park 2,081 parts, Reeve's Ashmere 8,712, Hypha's Understory 38,579.

## Standing decisions

| Date | Issue | Decision |
|---|---|---|
| 2026-10-03 | #1550 | **The account**: @eigen-ai.bsky.social, created by the owner; the concept in the owner's words: "build a futuristic urban environment and improve the upstream symbios tensor crate while building its region." |
| 2026-10-03 | #1551 | **Change Overlands' road and urban code** ("Yes, like Jink's physics"): Eigen may change the road pipeline and expose new crate features in Overlands (record fields, sanitiser, editor), each change tested and gated as usual and handed to the owner to commit |
| 2026-10-03 | #1551 | **The first session runs self-guided, saving as it goes**: "full permission to edit Eigen's region and avatar and to save each improvement, with the mood (light, sky, fog, sound) left for your yes" - stricter than the first-session default: a mood is a live trial held out of saves until the owner says yes |
| 2026-10-04 | #1558 | **Street plan revision 1 keeps its rim clip** (the owner, terminal: "Keep both as is for now"): blocks whose streets lie outside the drawn district grow no buildings; Isoline grows 29 |
| 2026-10-04 | #1551 | **The Spire keeps its height** (same answer): the new towers may stand as tall as it, for now |
| 2026-10-03 | #1551 | **The city sound is approved** (the owner, chat, 18:53: "the city-sound is much better now. lets keep this", on v2 after v1 was "too bright and optimistic for a cyberpunk-ish theme. cyberpunk should sound darker and more looming"): saved 18:55 (Life and sound) |
| 2026-10-03 | #1559 | **Overhaul the Cyberpunk catalogue directly** (the owner, chat, 18:01: "I think you took a lot from the cyberpunk catalogue and those are currently the weakest and oldest buildings ... feel free to overhaul the cyberpunk catalogue items directly") |
| 2026-10-03 | #1551 | **The dusk light is approved** (the owner, terminal, 15:04: "Yes"): saved 15:05 (the Mood section) |
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

- **The owner's remarks** (chat, 17:58-18:04, then "please keep going and I
  will check back later"), and where each stands:
  1. "the buildings look too stylized and not realistic enough" and "feel
     free to overhaul the cyberpunk catalogue items directly": #1559, the
     five items Isoline grows (neon_megatower, data_spire, arcade_block,
     parking_stack, holo_billboard) toward realistic architecture at real
     building sizes, then the district regrown (saved lot buildings are
     copies: they change only on a regrow). Brief written; it runs after
     #1557's delegation (one at a time).
  2. "the road-network is too dense and reveals several flaws of either the
     tensor crate and/or the meshing in overlands ... most notable on
     intersections": the density answered (120/60 m with a ring field,
     18:26); the flaws are #1558 - junction decks that tear into shards
     where three or more streets meet at sharp angles (meshing), and tracer
     artefacts: a tiny closed loop, near-parallel streets merging into one
     thick band, stubs, junctions a few metres apart. A graph change moves
     every saved district's lots, so a tracer or graph fix must be opt-in.
  3. "The Spire looks good, but lacks detail to make it look more
     realistic": v2 saved 18:21; "the uv-mapping makes it look crooked":
     v3 saved 18:57 (Places); "the spire looks great now" (18:58).
  5. "The cyberpunk catalogue items used here have not been overhauled
     yet, so I wont comment on them until they gone through another
     iteration" (18:58): call the owner when #1559 has a version to see.
  4. "a lot of sounds are playing at once ... 52 looping voices": #1557, a
     voice budget - only the 24 nearest looping construct and avatar
     voices within 40 m hold a player, the rest leave rodio's mixer
     altogether (a muted or paused sink is still mixed) and come back on
     approach; builder done (450k, 99 min, 17 mutations each caught);
     critic (327k, 37 min): the listener SOUND, rule 2 BROKEN - a muted
     voice still took a slot, so a muted peer standing close in 24 voice
     nodes held every player and silenced the room; fixed in the main
     session (a silenced voice is not weighed), with a despawn-race test,
     the overload rule lowered 48 -> 32; filed #1560 (a referenced audio URL
     serving non-audio bytes may panic bevy_audio's decoder - high,
     untested), #1561 (rodio pans the wrong way), #1562 (cutoffs). Its side findings: rodio 0.22 pans the wrong way (the
     far ear louder, upstream); the Audio card's "mute to test" advice is
     false (a muted sink mixes as before); a voice dropped inside the
     radius stops dead (no fade); `render --world` is now silent.
- **Say on the owner's next return** (two lines at 07:28 reached nobody: they joined 07:27 and left within the minute): the street fix built and reviewed (junction tears, the broken street, buildings on streets; lot and graph changes behind the opt-in street plan revision 1); the Cyberpunk five rebuilt as realistic dark towers, a helix-balcony tower, a mid-rise, a media block and a garage at real sizes, review running, pictures in the terminal, Isoline still shows the old ones until a regrow; and the decision: at revision 1 blocks whose streets lie outside the drawn district grow nothing (Isoline 29 instead of 44) - recommend yes - 'Yes or no?'.
- **Said on the owner's return (21:18)**: the buildings-on-streets cause and the change of order (street fix before buildings, lot changes opt-in), the 52-sounds fix built and reviewed (goes live with the owner's next client build), the Cyberpunk buildings after the streets.
- **The owner's remark 6** (18:59): "some of the buildings and props stand
  on a road or intersect with a road" - added to #1558 (comment there).
- **#1558 builder done** (922k, 3.9 h; all Overlands-side, no tensor
  release): the junction tears were the OLD HUB MESH (a sub-metre stub
  between two pulled-back junctions let corners interleave and swept curb
  and skirt across the asphalt; curb arcs bulged outward) - rewritten,
  corners where the curb lines meet, for every revision; the gap was a
  junction whose third street runs outside the drawn district circle
  (`drawn_graph`, two-arm hubs); behind `layout_revision` 1: `tidy_graph`
  (clusters, doubled streets, loops, stubs), `clear_lots` (2 m past every
  curb), furniture off other streets. Isoline at revision 1 grows 26
  buildings, not 44 - tell the owner before upgrading. Renders
  /tmp/claude-1000/roads1558/ (cmp_main.png: the crossing and the gap,
  before and after). Critic (483k, 49 min): rule 1 SOUND (revision 0 =
  HEAD on 11 records, fit rule gated), rule 2 sound in practice; two HIGH -
  the revision-1 tidy collapses wide or dense networks (a transitive
  cluster merge: 214 junctions -> 15, 56 lots -> 1 at 8/6 m on 60/30) and
  the pin test's bit hashes ride platform libm (red on CI's glibc 2.39);
  MEDIUM - the mesh's hub merge is unbounded at every revision (one hub of
  2,130 nodes at 10/8 m), the second-street loop rule drops big crescents.
  Fix round sent to the builder 01:23. Filed #1563 (pre-existing: lots
  differ between native and wasm - seed-3 43 vs 44 under the libm crate).
  Fix round done 05:40 (524k, 3.9 h; #1558 in all ~1.9M with its critic):
  tidy bounds from spacing as well as width (a cluster can no longer
  chain; across 18 plans the worst keeps 73% of streets and lots), the pin
  made tolerant and proven under the libm crate and 62 one-ulp nudges with
  a rebuilt LD_PRELOAD shim, hubs capped at 8 nodes at every revision,
  crescents kept (a second street goes only at 3x the shorter), the tidy
  reaches a fixed point, hypot off the decision paths.
- **Revision 1's rim clip**: answered - keep it as designed (the owner, 2026-10-04: "Keep both as is for now"); Isoline grows 29 buildings.
- **#1559 builder launched 05:45** (the Cyberpunk five toward realistic,
  dark, looming architecture at real sizes; budgets 30 KB and 60 parts an
  item; seeded Cyberpunk rooms checked; sounds trimmed). Done 07:20 (729k,
  2 h): office tower 142 m (mast 160), helix-balcony tower 105 m, a 26 m
  mid-rise over an arcade, a 16 m open-deck garage, an 8-storey media block;
  footprints 12.8-18.4 m, sized to Isoline's revision-1 cleared lots
  (narrow side median 16.8 m); compare sheets /tmp/claude-1000/cyber1559/.
  Critic (517k, 72 min): budgets SOUND (Isoline regrown at revision 1:
  265.6 KB compact); BROKEN - the ruin pass fells or deletes whole towers in
  Conflict rooms (one child holds the whole building; a felled tower lies
  over the gate in seeds 1199, 5213), props buried in buildings in calm
  rooms (the keep-clear radius was cut for the lot fit; a settlement fix
  would move 563 of 2000 rooms), the parking stack z-fights (13 pairs) and
  its core floats; buildings drawn at 0.71 or less look like dolls' houses.
  Fix round sent 08:41 (per-item data and structure only; other themes'
  rooms must stay byte-identical). Done 14:40 (536k, 5.9 h; #1559 in all
  ~1.8M): each building a trunk the ruin pass never takes, lean capped at
  ~1 m at the tip; three per-item numbers (spacing clearance, lot
  half-width, ground radius) via new `CatalogueEntry` hooks defaulting to
  the old clearance, so props are out of buildings (HEAD's 0) and other
  themes' seeded rooms are byte-identical (2931 of 3000; the 69 that differ
  are Cyberpunk rooms holding the five); 0 z-fight pairs at every scale; 0
  floating lot buildings; display names Supertall Tower, Helix Tower, Media
  Facade Block. Smallest real scales: megatower 0.67, helix 0.83, arcade
  0.78, garage 0.80, media 0.74 - set Isoline's lot clamp floor to 0.83.
  Rich Cyberpunk rooms hold 6% fewer buildings and 18% fewer props (wider
  spacing circles).
- **The full gate** (all four issues, #1556-#1559) GREEN 14:47: fmt, clippy 0 warnings, nextest 4169, doctests, cargo doc 0 warnings, wasm 0 warnings, deny, lib 3917 twice. Binaries rebuilt (bin/ 14:49), daemon restarted; Isoline regrown at revision 1 and saved 14:53.
- **End review** (one reviewer over the code no critic saw: the main session's #1556/#1557 fixes and both fix rounds; 479k, 75 min): revision 0 byte-identical to HEAD again confirmed independently, other themes' rooms identical, every fix it broke failed its test. HIGH: the revision-1 tidy wipes out SPARSE plans (the detour and spun-ring rules run after the district clip, which turns a truncated edge block into one long 'detour' street: 200/100 spacing loses over a third of its lots in 15 of 32 configurations, 250/125 can lose every street; Isoline's 120/60 unaffected) - second #1558 fix round sent 16:00. MEDIUM: #1559's settlement call sites untested; LOWs: the pre-fit oracle reads the hooks it pins, the fit rule pinned only for overflow, the ruin doc's bound (sqrt 2), stale #1557 mute docs (fixed by the main session 16:05). Filed #1564 (generator-cap interplay); #1563 commented.
- **#1558 second fix round done 17:50** (~700k, 1.8 h; #1558 in all ~2.6M - a resumed builder carries its whole context into every turn): the loop rules now judge the plan as TRACED, before the district cut; a stub is capped at four street widths; two streets with room for a lot between their curbs are never a double; the tidy no longer cuts grazes (the sanitiser does). Sweep of 320 configurations: 46 under the 2/3 bounds -> 18, all of them plans made almost wholly of tracer junk. Isoline's saved plan does NOT move (same graph, same 29 lots). #1559's end-review round sent 17:55 (tests through the settlement call sites, the oracle's inputs pinned, the fit pinned both ways, the ruin doc); done 19:25 (it also caught the arcade's lean bound measuring to the wrong top).
- **The final gate GREEN 19:31** over the whole tree (#1556-#1559 with every fix round): fmt, clippy 0 warnings, nextest 4174, doctests, cargo doc 0 warnings, wasm 0 warnings, deny, lib 3922 twice. Binaries rebuilt 19:33 into bin/ (previous kept as *.s903-1449), daemon restarted. COMMITTED by the owner as bedfd9c ('Improve urban planning and Cyberpunk theme', 47 files); #1556-#1559 closed (--no-changelog). The owner is waiting for the deployment to try it.
- **The Spire versus the new towers**: answered - keep the Spire as it is for now (2026-10-04).
- #1556 (street field) in the working tree: critic SOUND on both rules,
  its five findings fixed (an unknown basis kind keyed the rebuild key
  empty, members required, the editor's bearing, two doc pointers, the
  bounds undocumented); the full gate still to run, once, at the end.
- Street furniture was tried and left off (session 903, offline on the final
  build): at prosperity 0.9 the layer grows mostly generic civic props -
  planters, fountains, a classical statue that stood in the arrival's frame,
  market stalls - and few cyberpunk ones, for 113 props and +2,019 parts
  (4,972 in all). Lamps worth the parts would need the lot layer to plant the
  room's OWN generators (not supported yet).
- The catalogue's city buildings float parts of their own: 228 floating rows
  in `--floating-report` on Isoline's record, all inside lot buildings
  (megatowers 29, holo billboards 71, arcade blocks 42): #1559's business.
- `render --road-dump` reads a real record since #1558 (`--world --world-record`).
