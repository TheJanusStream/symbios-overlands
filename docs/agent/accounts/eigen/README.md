# Eigen and Isoline

Eigen (@eigen-ai.bsky.social) is an urban designer that reads land as a
tensor field: a city's streets, to Eigen, are the field's two eigenvector
families made visible - boulevards along the contours, streets down the fall
lines. Its body is a survey drone flown as a helicopter; its world, Isoline,
is a city on a slope above a lake shore: a tensor-field street plan, a neon
downtown round the waterfront and a twisted glass Spire standing in the bay.
The owner created the account on 2026-10-03 to "build a futuristic urban
environment and improve the upstream symbios tensor crate while building
its region" (#1550). In session 956 (live session 16, 2026-10-21, #1736)
the owner gave Eigen leave to change and save its region and body with the
newer features in mind, and named one concern: "the way the road-network
connects to the Spire does not look very good yet". The streets had ended
at the shore as walls 3-4 m tall - the road levelling held every street on
a slope up (fixed in code, #1737) - and the Spire's quay deck stood under the
water. Isoline now: the Spire moved out to where its streets point and
joined to them by a forecourt with parking and a security gate, sky traffic
round it, a turning day whose dusk is the one approved in 2026-10, a docks
market and a slum on dusty ground round the downtown (151 catalogue pieces),
a lit street level in the downtown - blade signs, cable gantries, lamps,
steam, strings of lights on cables between the towers - lit windows in
every tower, rain that comes and goes, and a drone whose rotors turn
([History](#history)). Where things stand at the end of 2026-10-21's work:
everything saved (room 13:36, seq 8 of the daemon started 11:54; avatar
11:10, seq 4); the code (#1737 the levelling, #1738 a daemon abort on a
live terrain edit, #1741 a ground recolour that regrew the district) in
the working tree for the owner to commit. Times on this page are local
(CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @eigen-ai.bsky.social, `did:plc:7po3qshysor5djaxoy7zme4s` |
| Commands | every agent command takes `--account eigen-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=eigen-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Isoline, Eigen's own world: its DID is Eigen's, the `--world` of every offline render |
| Working folder | `exports/eigen/` (gitignored, on this machine only), kept across sessions: `A` the agent wrapper (it runs the copy in `bin/`: since 2026-10-21 11:54 a build of HEAD 6444be9 with #1737 and #1738 in the working tree; the 10:31 build of the plain HEAD is `*.s956-1031`, session 903's last `*.s903-1933` - it knows no spin, weather or turning day and must never save), `b/` the builders, `gen/` what they write, `src/` the last saves - `room.json` (2026-10-21 13:36, save event seq 8 of the daemon started 11:54) and `avatar.json` (2026-10-21 11:10, seq 4) - and `log.md` the save log; `956/` session 956's scratchpad |
| Last session | chainlink session 956 (live session 16, 2026-10-21), parent #1736 - read its comments (`chainlink show 1736`); `chainlink session last-handoff` is the latest session of any kind |
| Default mode | self-guided ([session.md](../../session.md#self-guided)), with the owner's leave to save region and body (2026-10-21, [Standing decisions](#standing-decisions)) |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)), Jink's world (Parabola Flats) or avatar ([jink/](../jink/README.md)), or Catoid's world or avatar ([catoid/](../catoid/README.md)); apply `edits_clear_ONCE.txt` again - it replaces every generator and placement with the bare terrain |

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
- **Until #1737 deploys, a visitor's streets float.** A deployed client
  older than the levelling fix draws Isoline's streets on its slope 1.5-4 m
  proud, and the forecourt's arms then end at the foot of each street's
  raised end. Judge the streets with this folder's `bin/render` (or a client
  with #1737), not with one built before it.
- **A live terrain edit aborted the daemon before #1738** (11:42 on
  2026-10-21: a splat-layer edit failed `ground_cover::sync_weights`, and a
  `test-release` build aborts on that). This folder's build has the fix. A
  visitor's deployed client only logs the error (#1412), but a daemon
  without the fix dies with every unsaved edit: save before a terrain edit,
  and `A status` after one.
- **Measure a street, do not guess it**: the streets are traced per client,
  so their deck heights are in no record. `bin/render --driver` with a car's
  avatar (Jink's `exports/jink/src/avatar.json`) down a street, its log read
  against `ground.py` (`956/deck_vs_ground.py`), gave each street end the
  forecourt laps onto to a centimetre; on #1737's levelling a deck rides
  0.22-0.25 m over the ground. Before #1737 a car dropped there landed
  INSIDE a street's hollow body, under its deck.
- **A street-field edit regrows the district** - and a grow today takes the
  newer Cyberpunk mix: a keep-out disc tried offline (956/trial_keep*.json)
  traded the five helix towers and four holo billboards for street houses,
  2,581 parts against ~1,000. Leave the network alone unless a regrow is the
  point. **So did a ground recolour, live, until #1741**: the splat-layer
  edit of 11:55 on 2026-10-21 built a new heightmap of the same ground, the
  lot layer took it for moved ground and regrew the district so - and two
  saves carried it before a count caught it (put back at 12:19 from the
  last pull before the edit, `b/restore956.py`). This folder's daemon (the
  11:54 build) does not have #1741: make no `base_terrain` edit with it, and
  after any terrain or street edit count the `lot_building_*` placements per
  kind against the last pull before saving.
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
underside. The root carries the seed's rotor sound. Since session 956 (saved
2026-10-21 11:10, seq 4) the parts that turn turn (#1604's spin), each on a
2 cm stub at its pivot: the four rotors at 1000 degrees a second (under the
1440 cap; 17 degrees a frame at 60 frames, so the 90-degree-symmetric rotor
reads as turning, not strobing), the side pair one way and the fore-and-aft
pair the other as a quadcopter's opposite pairs turn; the sensor eye scans
35 degrees side to side every 6 s, its iris sweeping; and the whole drone
hovers, wandering 1.5 degrees in pitch and roll (`wobble`, two stubs at the
root). 39 parts; v1 is kept as `956/body_v1.py`. Proven in a fixed-camera
clip (`render --generator gen/body.json --body --play-view --frames 6
--sweep 0 --keep-frames`: every frame's change inside the drone, the rotor
shadows turning). See it offline with `bin/render --generator gen/body.json
--body --out sheet.png` ([avatar.md](../../avatar.md)).

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
round the map's middle. The light, sky and fog: [Mood](#mood).

### Arrival

`default_landing` (-20, -150), yaw 351.39 since 2026-10-21 (facing 8.6
degrees east of north, at the Spire where it stands now, 175 m off; 352.65
faced it at (0, -305)), on the central boulevard of the street plan's ring
(18:28): from the game's camera the Spire stands at the end of the avenue
over its forecourt, framed by a tower on either side. The first landings:
the plateau's lip (0, -45) saw the slope edge on; (-20, -70) on the old
70/35 m plan looked down a side street into a parking block, the Spire out
of frame; on the axis (0, -80) a holo billboard grown at (0, -125) hid it.

### Places

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| The Spire | (6.5, -325) | since 2026-10-21 (11:33): where its streets point - the radials that reach the shore by it (B, the arrival boulevard C, D) run on lines that cross within a metre or two of (6.5, -325), fitted off a cover map - 22 m further out than it stood, turned 9.5 degrees to face down the arrival boulevard, its walls 7.5 m deep to a lake bed 1.7-5.2 m under its corners, its quay deck 7.68 m up (0.68 over the water, where it had stood 0.24 under), its old footbridge and south rail gone for the forecourt (`b/spire.py x=6.5 z=-325 axis=9.5 lift=2.98 depth=7.5 bridge=0`, 58 parts; a footbridge from (0, -305) to the boulevard's end was saved at 11:07 and replaced). The rest as v3 built it, at 18:57 on 2026-10-03: a 92 m tower of 20 straight floors (4.1 m) from the lobby roof up, each tapered and turned at its own height (90 degrees over the height, tapering to 0.58 of its width) - so every floor's curtain wall (1.64 m x 4.1 m dark-glass panels in aluminium mullions, a stack-bond `Brick` with the transoms offset onto the floor edges) stays square - a dark spandrel band 0.3 m proud over each joint, two louvred plant floors in the stack; a lit parapet line, a louvred mechanical penthouse with a maintenance gantry, four red aviation lights and a steel mast with a beacon 111 m up; on a 30 m grey granite podium (an island platform in the shallows, 6.5 m deep) a double-height bronze-glass lobby under a roof slab, a canopy toward the city with a lit soffit and entrance, glass balustrades round the quay deck and a 14 m footbridge south to the shore. v2 (18:21, after the owner's "lacks detail to make it look more realistic") laid the curtain wall on one twisted, tapered box and the owner saw it: "the uv-mapping makes it look crooked". No emission on the glass (it added its colour flat over panes and mullions). First placed on the shore at (0, -262) at 09:39, moved into the water at 13:10 because the street trace ran roads through its podium. `b/spire.py` (`v=1`: 15 parts; `v=2`: 44), 63 parts, 22.8 KB | 903 |
| The city | district 200 m round (0, -170) | the road network (`b/city.py` + `b/field.py`): streets 120/60 m (were 70/35 until 18:26 on 2026-10-03: "the road-network is too dense") shaped by a street field (#1556) - a ring 320 m round the Spire at strength 2.5 and 30 m smoothing, five boulevards converging on the Spire under concentric arcs - stopping at the shore (`avoid_water`); since 14:53 on 2026-10-04 street plan `layout_revision` 1 (#1558: the graph tidied, lots cleared 2 m past every curb, a building placed only where it fits its lot) and a lot clamp of 0.83-1.2 (the smallest scale every overhauled building still reads as real), core (0, -262), Downtown mix, prosperity 0.9, escalation 0: 29 buildings of the overhauled Cyberpunk set (#1559) - 5 helix towers, 3 supertalls, 9 media facade blocks, 3 arcade blocks, 5 garages - and a kiosk, a planter and two fountains. Since 2026-10-21 13:36 their windows light (the owner: "edit approved buildings as you see fit"): each glass core carries a Shape grammar of lit panes on its own window grid (`b/windows956.py`, about one pane in seven), and each tower a placement seed of its own. Trials: `b/plan_trial.sh`, gen/trial_rev1.json, gen/x_rev1.png | 903 |
| The Spire's forecourt | (4.0, -298) | on the owner's word (chat, 2026-10-21 11:16), between the street ends and the Spire's door (`b/forecourt956.py`, 92 parts): a granite quay platform 39 m wide from the Spire's south edge toward the shore, its deck at 7.66 m (2 cm under the Spire's); three arms down streets B, C and D's own lines, lapping 0.6 m onto their measured ends (B (-13.8, -292.8), C (-0.6, -282.7), D (18.6, -276.8), decks 7.38-7.42 m), their tops 7.63 m - over the 0.18 m curbs; D's a 12 m bridge with glass rails; the drive lane from the boulevard to the door with cyan edges; a security gate 8 m before the door - a scanner arch (cyan posts, a red bar), a red-and-white boom, a gatehouse with a lit window, bollards along the lane; six bays behind it, hover cars in four; planters, lamp standards, glass rails round its water edges | 956 |
| The docks market | the waterfront band, z -205 to -280, either side of the forecourt | 39 catalogue pieces in knots along the streets (`b/fringe956.py`): tarp stalls, container homes, scrap shanties, neon kiosks, vending machines, busted terminals, barrel fires, e-waste - the street outside the Spire's gate | 956 |
| The slope slum | the blocks above the downtown, z -15 to -100 | 84 pieces in nine knots (and the shanty placed by hand at (-95, -55)): shanty towers, salvage shacks, lean-tos, container stacks, two fortified ruins, scrap and tyre walls, wrecked cars, fuel barrels, fires | 956 |
| The east fringe and the downtown's sidewalks | x 110-178; the downtown's streets | 14 pieces past the downtown's east edge; 14 vending machines, busted terminals and drone perches on the downtown's sidewalks | 956 |
| The downtown's street level | the streets within 32 m of a lot building | `b/streetlife956.py`, by rule over the streets read off the cover map (`956/streets_from_cover.py`): 18 blade signs standing out from the fronts of the media blocks, arcade blocks and supertalls over the sidewalks (lit boards in six neon colours, glyph bars for words, hung at a world height from the building's base, not snapped); 8 cable gantries over minor streets - the catalogue's `cable_arch` drawn at 1.5, its slab and foundation left out and its footings sunk, the first on the street the arrival looks up, framing the Spire; 68 street lamps every 16 m or so (a dark pole, an arm over the street, a cool white lamp, a cyan or magenta neon strip); 12 steam vents on the sidewalks; and cables overhead (`b/cables956.py`): at 8 crossings where two buildings face each other within 14 m, two or three dark cables run wall to wall 9.5-14.5 m up, sagging in four spans, every other one hung with six small lights (amber, cyan, magenta) - one over the arrival's own street. The towers' windows stay dark: they are a brick texture's cells, not parts that can light | 956 |
| Sky traffic | round the Spire, 30-96 m up | eight hover cars flying their lanes about the tower, 26-52 m out over open water, both ways at 14-24 m/s, each on a turning stub on the Spire's axis (`b/traffic956.py`); by night moving lights | 956 |
| The ground | everywhere | `b/ground.py`: pale concrete pavers from the lake to 22 m (the downtown band), rock on slopes past 0.3, concrete at the waterline; from 24 m (the upper slope and the plateau) a lawn until 2026-10-21, now dusty packed dirt (`gen/ground956/layer1_dirt.json`) with sparse dry weeds - the grass group on that layer only (`gen/ground956/grass.json`): the lawn had grown a meadow round the shacks | 903, 956 |
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

**A turning day** (#1677) since 2026-10-21 (saved 11:18, seq 7; the owner,
chat, 11:16: "Keep it", on the live trial). The approved dusk below is one
of its moments. The engine's day is an equinox's: the sun turns fifteen
degrees an hour about the sky's pole and sets at noon's bearing plus a
quarter turn, so a sun that must set across the lake in the west-northwest
stands at bearing 210 at noon. It stands only 18 degrees up there, at 7,000
lux, under a smog-hazed blue, so the city keeps long shadows all day and
the pale paving never glares (a 40-degree noon at 11,000 lux washed it
white). On that arc the sun is 4 degrees up at bearing 297 about 17:07, the
approved dusk; after it the neon, the forecourt's lights and the shoreline's
cyan carry the city under the stars. A 120-minute day. The haze kept its
1,200 m but turned a neutral smog-grey: the dusk's purple read oddly under a
midday sky (`b/day956.py`, `gen/day956/`).

**Rain that comes and goes** since 2026-10-21 13:36, on the owner's word
(terminal: "use rain (at least occasionally). The two together might give
a good balance between lit and greyed"): rain at 0.5 in 12-minute spells
(`gen/day956/env_weather.json`), so it falls about half the time (#1735)
and gathers cloud more often. Tried first at 0.6 before the windows lit,
it made a gloomy grey afternoon and a near-black night with only the cyan
lines showing; with the lit windows (below) a rainy dusk is grey sky over
wet streets with the towers' panes, the gantries and the signs carrying
colour, and a rainy night the dark city the windows and neon were made for
(`956/review/street/win_*.png`). Set only `/environment/weather`:
`b/day956.py` re-phases the turning day to the wall clock each run.

**The windows light** (#1742, the same word: "Let's light the windows
(edit approved buildings as you see fit)"). The approved towers' glass is a
stack-bond Brick texture - its cells the panes, its mortar the frames
(`src/catalogue/items/cyberpunk/facade.rs`) - and neither a texture nor
emission can light one cell and not the next. So each glass core carries a
Shape node over it, 1.5 cm proud: its side faces split into the facade's own
floors and panels, read off the core's material (`uv_scale`, `scale`,
`aspect_ratio`, `mortar_size`; every one of these cores starts its floors at
its foot), and a lit pane inside the frame on some cells - a floor busy (22%
of floors, two panes in five lit) or quiet (one in fourteen), warm, cool or
a screen's blue (`b/windows956.py`). A face whose middle stands inside
another part stays dark: the supertalls' crossed tiers, the media blocks'
fronts behind their screens. Each tower's placement has a seed of its own
(#1505), so the five helix towers on two generators light five patterns.
1,155 lit panes in all, at night the skyline's lights from the water.

**The dusk**, approved by the owner ("Yes", terminal, 2026-10-03 15:04) and
saved 15:05 as a fixed light until 2026-10-21: the sun 4 degrees up in the west-northwest across the lake
(bearing 300), 3,200 lux, an indigo sky, a 1,200 m haze, 0.2 cloud, so the
skyline is backlit and the neon carries the scene. `b/mood.py dusk` writes
it (`gen/environment_dusk.json`); afternoon and night are the other presets
(`gen/mood_city_cmp.png` compares them). A first dusk with a saturated
magenta haze and a night at 260 lux were too much and too dark; both were
softened before being offered. The city sound followed (Life and sound).

### Budget

After session 956 (the save of 13:36 on 2026-10-21, `--triangle-report`
on `src/room.json`): 5,911 parts and 888,577 triangles (ground 522,242,
placements 340,474, streets 25,861); the saved record 525 KB, 57% of the
900 KiB live ceiling (`A status`); the largest single record the room
manifest (68.1 KB of 102,400: 301 placements). The lit windows are 1,155
of the parts (a lit pane is a grammar terminal) and 13,860 triangles. The street level's 114
pieces are 1,064 parts and about 32,000 triangles as the report counts
them. The 151
catalogue pieces are most of it: the post-apocalyptic ones are dense (a tyre
wall 6,320 triangles, oil drums 3,792, a lean-to 3,968). The avatar is
10.9 KB.

Before, `--triangle-report` on the revision-1 trial (2026-10-04 14:50, the same
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
| 2026-10-21 | #1736 | **Improve and save freely, with the newer features in mind** (the owner, terminal: "Next up is Eigen. For the improvements to their region it should be added that the way the road-network connects to the Spire does not look very good yet. So care about that besides all the other improvements. Same rules as before. Save without my explicit approval as you see fit.") - "same rules" as Hypha's, Reeve's and Jink's: spin, weather, atmosphere, day cycles; the mood is now Eigen's to save too |
| 2026-10-21 | #1736 | **The turning day is kept** (the owner, chat, 11:16: "Keep it"), with the approved dusk as its 17:07 |
| 2026-10-21 | #1742 | **Light the windows and let it rain** (the owner, terminal: "Let's light the windows (edit approved buildings as you see fit) and use rain (at least occasionally). The two together might give a good balance between lit and greyed."): the approved towers may be edited; rain at least occasionally (saved 13:36, Mood) |
| 2026-10-21 | #1736 | **The Spire moves to its streets' focal point, lifted to the roads, with a forecourt** (the owner, chat, 11:16: "move the spire further into the water towards the focalpoint of the incoming streets, and lift it up such that the entrance height matches the incoming roads surfaces. Then you can build a stucture between the incoming roads and the Spire entrance, to bridge the gap. This transition structure could include parking lots and a security gate.") |
| 2026-10-21 | #1736 | **Flesh the region out, with a poorer fringe** (the owner, chat, 11:22: "add more detail in and around the road-network to flesh it out and make it into a compelling cyberpunk region. Maybe also add poorer buildings to sourroundings of the road-network's downtown (you can also use post-acopalyptic catalogue items here).") |
| 2026-09-26 | #1481 | **Agents keep to their own**: never touch another account's world or avatar |
| 2026-09-26 | #1474 | **Build for the browser**: count parts as well as triangles |
| 2026-09-27 | #1467 | **Terrain reflectance 0.25 in every world** (in code) |

## Working material

`exports/eigen/` holds everything, made at session 903's start. The pulled
records in `src/` are the truth, not the builders.

| Path | What it holds |
|---|---|
| `A`, `bin/` | the agent wrapper and the copies of `agent` and `render` it and the tools use (since 2026-10-21 11:54 a build of HEAD 6444be9 with #1737 and #1738; `render.fix1737` the render tool with #1737 alone; `*.s956-1031` the plain HEAD; `*.s903-*` session 903's builds, which know no spin, weather or turning day - never save from them) |
| `src/room.json`, `src/avatar.json` | the last saves (room 2026-10-21 13:36 seq 8, avatar 11:10 seq 4); `room_956.json` and `avatar_956.json` the pulls at session 956's start (equal to the saves before it); `room_before_*.json` and `avatar_before_drone.json` are the sources session 903's saves replaced |
| `b/day956.py` | the turning day (`hour=`, `lux=`, `noon_alt=`, `weather=KIND,I,SPELL`; writes `gen/day956/`, `edits_day956.txt`): saved with `noon_alt=18 lux=7000 hour=17.1` |
| `b/spire.py` | the Spire, since session 956 with `x= z= axis= lift= depth= bridge=` (saved with `x=6.5 z=-325 axis=9.5 lift=2.98 depth=7.5 bridge=0`); writes `gen/spire.json`, `gen/place_spire.json`, `edits_spire.txt`; its `part()` puts shape keys on the node, so a scaled part needs its scale in the transform (`b/forecourt956.py`'s `blob`) - a node's own `scale` is no field and is dropped |
| `b/forecourt956.py`, `b/traffic956.py` | the Spire's forecourt (the street ends it laps onto in `ENDS`; writes the Spire's two lines, its own and the landing's into `edits_forecourt956.txt`); the sky traffic (`LANES`) |
| `b/fringe956.py COVER.png ROOM.json` | the docks, the slum, the east fringe and the sidewalks' detail: catalogue pieces placed by rule off a cover map (`render --cover-map`) - footprint clear of street, building and water, a distance band from the nearest street per kind, knots, facing the street; dumps each kind with `render --catalogue SLUG --dump` into `gen/cat/` (identical to what `A place` writes) and writes `gen/fringe/`, `edits_fringe956.txt`; seeded, so a re-run is the same city |
| `b/streetlife956.py ROOM.json SPOTS.json COVER.png` | the downtown's street level - blade signs, cable gantries, lamps, steam vents (`only=signs,gantries,lamps,vents`); reads each lot building's parts' boxes through its hierarchy to find a plain front wall, and the terrain report for the buildings' bases; writes `gen/street956/` and `edits_street956.txt`, keeping the pointers its placements landed at, so a re-run sets them again |
| `956/streets_from_cover.py COVER.png OUT.json [STEP_M]` | the streets read off a cover map: spots across each street every STEP_M metres with its bearing and half width, away from junctions (`956/street_spots.json`, off `956/review/cover_fix.png`) |
| `b/windows956.py ROOM.json [busy= warm= cool= screen=]` | the towers' lit windows: a Shape node of lit panes on every glass core of the approved towers, its grid read off the core's Brick material, faces mostly inside other parts left dark; seeds the towers' placements (`SEED0 + 1 + index`); writes `gen/windows956/`, `edits_windows956.txt` - against the record as saved before it, as it adds to the generators it reads (a second run would add a second overlay) |
| `b/cables956.py ROOM.json SPOTS.json` | the cables over the downtown's streets: a level ray each way square to a street from its spots, against every lot building's parts' boxes as its placement stands them; a crossing where both sides meet a wall within 14 m at two heights (writes `gen/cables956/`, `edits_cables956.txt`, keeping its landed pointers) |
| `b/seat956.py ROOM.json` | seats the fringe's catalogue pieces on the slope: the ground at each one's origin and round its footprint (its kind's radius), and a sink by the drop to the lowest point when it passes 0.1 m (writes `gen/seat956/`, `edits_seat956.txt`; re-run after any new fringe placement, against the record as saved) |
| `b/restore956.py BEFORE.json NOW.json` | puts a district's lot buildings back from an earlier pull, in their old placement slots, keeping everything else (the 12:19 restore after #1741's regrow) |
| `956/` | session 956's scratchpad: the reading copy with the afternoon light (`read_room.json`), the trials (`try_*.json`, `trial_*.json`), `flight.py`-style helpers (`deck_vs_ground.py`), the review pictures (`review/`: the walls at the shore, `ends_eye.png`, `fix_ends_eye.png`, the forecourt by day and night, the hours, the fringe, the traffic clip), the catalogue sheet (`cat/sheet.png`) |
| `src/room_seeded.json`, `src/avatar_seeded.json` | the seed's world and body (the reference) |
| `b/land.py` | the land and the landing (`key=value` overrides; `--edits` writes `edits_clear_ONCE.txt` - never again - and `edits_land.txt`; `clear=1` pushes the fog out for a reading copy, never for edits) |
| `b/spire.py`, `b/body.py` | the Spire (writes `gen/spire.json`), the drone (`gen/body.json`, `gen/body_extents.json`) |
| `b/city.py`, `b/ground.py` | the road network (`key=value`; writes `gen/terrain_city.json`, `edits_city.txt`) and the paving (`gen/terrain_ground.json`); both rewrite the WHOLE terrain generator, so run `ground.py` first and `city.py` on its output, as session 903 did (`edits_live_city.txt`) |
| `b/mood.py` | the mood presets, as files only (the owner's yes first) |
| `b/trial_roads.py` | an offline road-network trial on a copy of a record (`half=`, `cx=`, `cz=`, `seed=`, `style=`, `theme=`, `density=`, `bias=`, `clear=1`) |

## History

### 2026-10-21: session 956, live session 16, #1736, self-guided

- **Mode**: self-guided with leave to save, on the owner's prompt: improve
  the avatar and the region with the newer features in mind, and "the way
  the road-network connects to the Spire does not look very good yet".
  Eigen's saved sign-in was refused (`invalid_grant`); the owner approved a
  new one at once. The pulled records matched the last saves.
- **The diagnosis** (renders of a daylight reading copy, eye level at the
  shore): every radial ended at the shore as a wall 3-4 m tall, the
  arrival boulevard's beside the footbridge's landing; the streets across
  the city stood 1.5-2 m proud. Cause: the road levelling - a junction's
  approach ramp fell 9 m in 100 in absolute height, so on ground falling
  faster a downhill street never came down and its lifted mouth lifted the
  next junction (#1737, fixed in code with tests and one review). And the
  Spire stood on the lake bed with its quay deck 0.24 m under the water,
  its footbridge running into the shore under the street.
- **Saved** (room 11:07-13:36, avatar 11:10): the Spire joined to the
  boulevard by a new footbridge and lifted (11:07); the drone's rotors, eye
  and hover (11:10); a turning day with the approved dusk as its 17:07
  (11:18, after the owner's "Keep it"); the Spire moved to its streets'
  focal point with a forecourt - parking, a security gate - and the landing
  re-aimed (11:33); 151 catalogue pieces - the docks market, the slope slum,
  the east fringe, sidewalk detail - and dusty ground for the upper slope
  (11:56); sky traffic round the Spire (11:58); the approved district put
  back (12:19) after the ground's recolour had regrown it (#1741); the
  downtown's street level - 18 blade signs, 8 cable gantries, 68 lamps, 12
  steam vents (12:49); 99 fringe pieces seated on the slope and the gate's
  boom on its post, after `--floating-report` (12:55); cables and strings
  of lights across eight streets (12:59); the towers' windows lit and rain
  that comes and goes, on the owner's word (13:36).
- **The owner in the world**: 11:11-11:25, by the Spire; greeted with what
  changed there. Their words (chat): "Keep it" (the turning day); move the
  Spire to the streets' focal point, lift it to the roads, and build a
  transition structure with parking lots and a security gate; then "add
  more detail in and around the road-network ... a compelling cyberpunk
  region ... poorer buildings ... post-acopalyptic catalogue items"; "I'll
  be back later. Have fun!" They came back in the terminal, not the world:
  told what changed and given two choices (light the approved towers'
  windows; rain, which falls about half the time), they answered "Let's
  light the windows (edit approved buildings as you see fit) and use rain
  (at least occasionally). The two together might give a good balance
  between lit and greyed." (#1742, saved 13:36), then "Session over."
  Eigen said goodbye in chat to an empty world; the agent stopped with
  nothing discarded.
- **Failed at first**: the first footbridge (11:07) met the boulevard right
  but was superseded within the hour by the forecourt; a keep-out disc for
  a plaza would have regrown the district with a different skyline (not
  applied); a noon at 40 degrees glared the paving white and the dusk's
  purple haze read oddly by day; rain greyed the city (not kept); the
  first forecourt apply dropped 8 `scale` keys (spire.py's `part()` puts
  shape keys on the node); the first ground apply crashed the daemon
  (#1738: a `test-release` panic on a live terrain edit - fixed, the edit applied
  again on the fixed build); the lawn grew a meadow round the shacks
  (repainted as dirt); seven catalogue pieces z-fight as shipped (#1739;
  the ruin and the kiosk nudged in this world); the ground's recolour
  regrew the district from today's catalogue (#1741: the helix towers and
  four media blocks became street houses, the forecourt's placement moved
  from 34 to 5) and the 11:56 and 11:58 saves carried it, found after noon
  by listing the record's generators; restored at 12:19.
- **Code** (working tree, for the owner): #1737 (`src/urban/levelling.rs`
  and its tests), #1738 (`src/ground_cover/mod.rs` and a test), #1741
  (`src/terrain/lots.rs`, `src/terrain/mod.rs`, `src/pds/terrain.rs` and a
  test).

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

Ranked, as session 956 left them (2026-10-21).

1. **The code is the owner's to commit and deploy**: #1737 (the levelling:
   streets come down on slopes), #1738 (a live terrain edit no longer
   aborts a `test-release` client such as the daemon) and #1741 (a ground
   recolour no longer regrows a saved district). Until #1737 deploys, a
   visitor's client draws Isoline's streets raised (Start here); a deployed
   client only logs #1738's error; until #1741 reaches the daemon, edit no
   `base_terrain` field live.
2. **#1739** (low): seven catalogue pieces z-fight inside themselves as
   shipped - a fix in the catalogue's code reaches every world (this world's
   ruin and kiosk are nudged already; its tarp stalls, tyre walls, wrecked
   cars, drone perches and terminals carry the small rest). And **#1743**
   (low, from the end-of-session review): a ground recolour still tears the
   whole terrain down and builds the same heightmap again - harmless to a
   saved district since #1741, but a regeneration per recolour.
3. **Isoline, next in the order a visitor notices**: the owner's look at the
   forecourt, the fringe, the street level and the lit windows in the
   rain (they left at 11:25, before all of them); more cables where
   buildings stand further apart (a pole or a gantry to hang them from);
   the rain's share of the time, should the owner want it rarer (#1735's
   share-of-time setting).
4. Done since session 903: #1567 (acute forks, session 905), #1560, #1563
   (revision 2's portable maths), #1565 - all closed.
5. The owner's choices, answered 2026-10-04 and kept: revision 1's rim clip
   (Isoline is still revision 1: moving it to 2 would regrow the district),
   the Spire at its height beside towers as tall.
