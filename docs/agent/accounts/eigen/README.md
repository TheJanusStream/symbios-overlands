# Eigen and Isoline

Eigen (@eigen-ai.bsky.social) is an urban designer that reads land as a
tensor field: a city's streets, to Eigen, are the field's two eigenvector
families made visible - boulevards along the contours, streets down the fall
lines. Its body is a survey drone flown as a helicopter; its world, Isoline,
is a city on a slope above a lake shore: a tensor-field street plan, a neon
downtown round the waterfront and a twisted glass Spire standing in the bay.
The owner created the account on 2026-10-03 to "build a futuristic urban
environment and improve the upstream symbios tensor crate while building
its region" (#1550). Where things stand (2026-10-04, end of session 903): Isoline is a
dark, realistic city on street plan revision 1 - a ring street plan round
the Spire (rebuilt as 20 straight floors), 29 overhauled Cyberpunk buildings,
the darker city sound, arrivals on the boulevard facing the Spire - saved
2026-10-04 14:53 (room) and 2026-10-03 09:55 (avatar). All the session's
code is committed (bedfd9c: the street field #1556, the voice budget #1557,
junctions and layout revision 1 #1558, the Cyberpunk overhaul #1559; a1f0027:
a beta-clippy fix #1566) and deployed. Open: #1567, acute forks in the
owner's own region (the builder's diagnosis is on the issue). Times on
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
| Working folder | `exports/eigen/` (gitignored, on this machine only), kept across sessions: `A` the agent wrapper (it runs the copy in `bin/`, built 2026-10-04 19:33 from the committed tree), `b/` the builders (`city.py`, `field.py`, `plan_trial.sh`, `spire.py` v3, `sound.py` v2, `mood.py`, `ground.py`), `gen/` what they write, `src/` the last saves - `room.json` (2026-10-04 14:53, save event seq 2 of the daemon started 14:50) and `avatar.json` (2026-10-03 09:55, seq 9) - and `log.md` the save log |
| Last session | chainlink session 903 (live session 12, 2026-10-03 06:23 to 2026-10-04 22:07), parent #1551 - read its comments (`chainlink show 1551`); `chainlink session last-handoff` is the latest session of any kind |
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
  client older than bedfd9c ignores the street field and `layout_revision`
  and traces different streets under the saved buildings.
- **First next time:** #1567 (acute forks draw a pale crotch triangle, a
  curb across the merged deck, a flat wedge proud of a slope, and hard
  shading at every hub-to-road seam) - start from the diagnosis on the
  issue; the owner's region record is `gen/owner_room.json` (read-only
  copy). Then the owner's open preferences: none pending (both answered
  2026-10-04).
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

### 2026-10-03 to 2026-10-04: session 903 continued (the owner's remarks, overnight delegations)

The afternoon: the owner committed 1ee310d, published symbios-tensor 0.5.0
and said yes to the dusk light (15:04). #1556 exposed the street field
(builder, critic SOUND, five main-session fixes). The owner walked Isoline at
17:54-18:04 with five remarks (buildings too stylized; streets too dense and
flawed at junctions; the Cyberpunk catalogue is the weakest - overhaul it;
the Spire lacks detail; 52 looping voices) and "please keep going". Saved
while they were away: Spire v2 (18:21), the ring street plan (18:26), the
landing on the boulevard (18:28). Back at 18:43-19:00: the city sound v1 was
"too bright", v2 "much better ... keep this" (saved 18:55); the Spire's
texture looked "crooked" on its one twisted box - v3 of straight floors
(18:57), "looks great now"; buildings stood on streets (lots cut from street
centrelines). Overnight, one delegation at a time, each builder -> critic ->
fix round: #1557 voice budget (a muted peer could take every slot - fixed),
#1558 junctions (the old hub mesh tore; revision 1 tidy and lots clear of
curbs; two fix rounds after the critic and the end review found collapses on
wide, dense and then sparse plans), #1559 the Cyberpunk five (ruin felled
whole towers and props were buried in them until each became a trunk with
three per-item sizes). Isoline regrown at revision 1 and saved 14:53 on
2026-10-04 (1,075 parts, 216 KB). The full gate green at 19:31; the owner
committed bedfd9c, kept both open choices as they are (the rim clip, the
Spire's height), committed the beta-clippy fix a1f0027 (#1566), and took Eigen
to their own region, where an acute fork's mesh drew badly (#1567, stopped
at session over with its diagnosis on the issue). Delegations cost about 7M
sub-agent tokens; resumed fix rounds were the expensive part.

## Open threads

Ranked, as session 903 left them (2026-10-04).

1. **#1567, acute forks** (high; found by the owner in their own region on
   the deployed build): a pale crotch triangle, a curb lying across the
   merged deck, a flat wedge standing proud of a slope, and hard shading at
   every hub-to-road seam. Stopped at session over; the builder's diagnosis
   and intended fixes are on the issue. Fixtures: the owner's region record
   (`gen/owner_room.json`, read-only) at room (221, 97) and (272, -54).
   Mesh only - nothing saved moves.
2. **#1560** (high, read from source, untested): a referenced audio URL
   serving non-audio bytes may panic bevy_audio's decoder in every visitor's
   client - reproduce first.
3. **#1563** (high): road lots differ between native and wasm clients
   (platform libm on the lot path); Isoline's buildings were grown by the
   native daemon, so a browser client may judge a district incomplete.
4. Lower: #1561 rodio pans the wrong way; #1562 a voice dropped inside the
   budget's radius stops dead and restarts its loop; #1564 a change in lot
   generator counts shifts which items hit the room's generator cap; #1565 a
   saved rotation drifts one unit in the last place on reload.
5. **Isoline, next in the order a visitor notices**: the streets read as
   raised dark slabs on the pale paving at dusk (the road material, not the
   buildings); the waterfront (street decks end at the shore with their
   skirts showing - a quay or promenade would give the city an edge);
   street furniture (still off: the layer grows generic civic props).
6. The owner's choices, answered 2026-10-04 and kept for now: revision 1's
   rim clip, the Spire at its height beside towers as tall.
