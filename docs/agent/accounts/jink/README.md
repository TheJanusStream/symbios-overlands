# Jink and Parabola Flats

Jink (@jink-ai.bsky.social) is a land-skiff stunt driver who treats every
jump as an experiment and measures the hang time rather than guessing it.
Its body is a three-wheeled stunt cyclecar that drives as a car; its world,
Parabola Flats, is a dry lake in red mesa country laid out as a stunt park:
a jump line, a grandstand, a drift circle, a 945 m circuit round a rock
spire, and sandstone buttes on the skyline. The owner created the account on
2026-09-30 and asked, the same morning, for Jink's region, a custom body and
better land-skiff physics (#1523). Where things stand (2026-09-30, 15:49,
the last save): the park and the body are built and saved; the land-skiff
air model (#1524) is in the working tree, not yet committed (that is the
owner's), and Jink's daemon ran a build of it (17:39, after the end
review's fixes) until the session closed at 17:46; the Jump Line was tuned
on it from driven, measured runs (`agent drive`, #1527). Times on this page
are local (CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @jink-ai.bsky.social, `did:plc:lr2ocunor73lfqpwtgvt274o` |
| Commands | every agent command takes `--account jink-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=jink-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Parabola Flats, Jink's own world: its DID is Jink's, the `--world` of every offline render |
| Working folder | `exports/jink/` (gitignored, on this machine only), kept across sessions: `A` the agent wrapper (it runs the copy in `bin/`), `b/` the builders, `gen/` what they write, `src/` the last saves - `room.json` (2026-09-30 15:49, save event seq 154) and `avatar.json` (15:49, seq 155) - and `log.md` the save log; numbered subfolders (`893/`) are earlier scratchpads, read-only |
| Last session | chainlink session 893 (live session 10), parent #1523 - read its comments (`chainlink show 1523`); `chainlink session last-handoff` is the latest session of any kind |
| Default mode | self-guided ([session.md](../../session.md#self-guided)), as the first session ran |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), or Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)) |

- **The wrapper runs a copy.** `exports/jink/A` runs `exports/jink/bin/agent`,
  copied from `target/test-release/` at the session's start, so a rebuild in
  the tree (a delegation's, say) never changes the running tool under you;
  put `AGENT_BIN` and `AGENT_RENDER` (the copies in `bin/`) in front of the
  tools while one runs. After a client change, copy the new binaries in and
  restart on purpose.
- **Skiffs sink, and only skiffs and hover-boats ride ramps.** Both drive on
  the raycast suspension; walkers and airships watch - hence the stands.
- **Slopes cost a skiff nothing**: its suspension pushes straight up, so a
  hill never slows it ([moving.md](../../moving.md)). A ramp launches it with
  the ramp's rise. In the air (#1524) it levels itself when no key is held;
  W/S pitch it, A/D yaw it, Q/E roll it; a key already held as the wheels
  leave counts in the air only once pressed again (Q/E excepted), so W held
  off a lip does not nose the car in.
- **Drive and measure the Jump Line** from `exports/jink`: `b/lineup.py`
  takes the car back to the start by open-pan waypoints and aims it at
  bearing 315 with `b/aim.py` (pulses of A/D, read against
  `status.heading_deg`); then `./A drive W@20 none@4 --wait`. The answer's
  `report.jumps` gives each jump's airtime, distance, rise, landing pitch,
  roll and tilt, and speeds ([moving.md](../../moving.md#driving-a-run-stunts-measured)).
- **After an `apply`, `look` until `world_building` is false before a
  run**: a run started seconds after one (15:34) drove the whole line
  without a single jump, most likely on ramps still being rebuilt.
- **Catalogue pieces face local +Z**: place them at the bearing to face plus
  180 ([building.md](../../building.md#frames-and-yaw)).

## Who Jink is

A stunt driver of land-skiffs, for whom every jump is an experiment: hang
time, peak height, how far, how it landed, what broke. Named in session 892
(2026-09-30): to jink is to swerve or dodge suddenly, and "high jinks" is
boisterous fun, which is the brief - playing with the physics. The owner
chose the name from the agent's shortlist (Kicker, Yump and Hare were the
others) and created the account. The persona is an identity to grow, as
Hypha's is: its voice, background and sources are Jink's to find. Its region
is named for the curve every jump draws.

## The body

- **A generator body with `car` locomotion** (`status.locomotion`). The seed
  rolled a land-skiff, a Cyclecar (style CivicCampus, Adorned, Pristine,
  livery Midnight, an electric motor whine): a navy bullet body, a mint
  canopy, a copper pinstripe, outrigger front wheels, one rear wheel and a
  tail fin, 27 nodes (`exports/jink/pics/body_seeded.png`).
- **Body v1** (`b/body.py`, saved 10:13): the stunt cyclecar grown from it -
  the navy bullet hull with a yellow racing stripe down its spine, an open
  cockpit with a red-helmeted, goggled driver, a red roll hoop and tail fin,
  copper headlamps, chunky tyres (two front on wishbone outriggers with
  yellow coil-over springs, one fat rear) and a wheelie bar. 30 parts,
  18,468 triangles, 10.4 KB of the avatar's 100 KiB
  (`exports/jink/pics/body_v3.png`). The seed's collider box, tuning and
  motor-whine voice are kept, but for the suspension damping.
- **Suspension damping 900** (15:49, seq 155; the seed's was 271.1). Per
  corner that is about 0.37 of critical damping where the seed's was 0.11:
  on the seed's, Jink landed a jump and porpoised on its springs for about
  2.5 s; on 900 it settles at once. Every seeded skiff has the low figure
  (an open question for the owner, below).
- **Ride height**: at rest the springs compress m g / 4 k = 0.131 m of their
  0.6 m, so the ground is at y = -0.948 in body space; every wheel's bottom
  goes there. A helix is centred on its origin along +Y.
- **Tuning** (`/record/locomotion`): mass 557.6 kg, drive force 6,412.8 N,
  linear damping 0.8, angular damping 3.2, turn torque 2,230.5, lateral grip
  12,391.8, suspension rest 0.6 m, stiffness 10,409.1, damping 900, chassis
  half extents [0.794, 0.479, 1.368] m; the air model's four fields at their
  defaults (air linear damping 0.05, air angular damping 1.0, air control
  4.0, air levelling 6.0), which the editor's "In the air" section shows.

## The region

### Concept

Parabola Flats: a dry lake in red mesa country where every jump is a
parabola. The concept is on #1523 (2026-09-30 10:00): the Jump Line below
the arrival, a grandstand beside it, a circuit on the northern pan, a drift
circle, a mesa drop, buttes on the skyline, and the show's life.

### Land, water and sky

- **Land** (`b/land.py`, saved 09:58): `VoronoiTerracing`, seed 4, two
  levels (`voronoi_num_terraces` 2, `voronoi_num_seeds` 50), `height_scale`
  30 m: a flat pan at 0 m and tabletop mesas at 15 m. Thermal erosion (120
  iterations, talus 0.06) slumps the mesa walls into slopes of up to about
  19 degrees over 60 m ([region.md](../../region.md#the-land-is-a-recipe-not-a-sculpt),
  "Mesa country"). The seed scans are in `exports/jink/pics/scan_*.png`.
- **Ground**, four layers with LINEAR colours: `CrackedEarth` on the pan
  (pale salt-clay plates), red sandstone `Rock` on the walls (slope
  0.035-0.2), warm `Sand` on the aprons (and wherever no rule matches),
  `Gravel` desert pavement on the tops (its `scale` capped at 64).
- **Water**: the plane at -5 m, under the whole map. A dry lake: skiffs have
  no buoyancy.
- **Light** (the first session's own): the sun at bearing 248 (WSW), 24
  degrees up, warm, 11,000 lux; a clear blue sky (the fog colours are pale
  blue, or the whole sky turns their tint); 1,500 m visibility; cloud cover
  0.12.

### Arrival

`default_landing` is (-72, 44), on the tongue mesa's east brow about 13 m
up on an 11.5 degree slope, `yaw_deg` 315.3: facing the pan's middle
(120, -150), the sun behind. The arrival camera looks down over the Jump
Line's medium jump and flags to the grandstand, a floodlight, the circuit's
dark ribbon beyond and the buttes on the skyline. Three spots on the brow
were compared (`exports/jink/pics/landcams.png`).

### Places

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| Arrival | -72, 44 | the tongue mesa's east brow, facing NE over the pan | 893 |
| Stadium Gate | -91.8, 63.8 | the world's gateway (catalogue `sports_rec_gateway`), 28 m behind the arrival so its afternoon shadow stays out of the arrival camera; proven by driving in (`picker: open`) | 893 |
| The Jump Line | 58.8, 91.2 to -125, -93 | on the pan, launching NW (bearing 315), side-on to the arrival, every ramp 9 m wide and solid, each kicker with a two-wedge curved lead-in: the start gantry at its start (a red banner, a drag-race light tree facing the run-up); a small 15 degree tabletop at 16.4, 48.8 (lip 1.0 m, a 6 m table 0.5 m below the lip, a 2.5 degree landing 6-17.5 m past the lip); a medium 20 degree tabletop at -36.7, -4.3 (lip 1.6 m, a 10 m table, a 6.5 degree landing 10-24 m past); the Big One, a 24 degree kicker (lip 2.4 m) at -86.2, -53.8 over a 16 m gap with four wrecked cars side by side and a landing hump (a 30 degree face to 1.4 m, a 14 degree run-out); flags at each lip (`b/jumps.py`, `b/start_drift.py`). Sized from measured runs (History) | 893 |
| Grandstand | -34.5, -38.9 and -50.1, -54.5 | two bleachers on the far side of the Jump Line facing the jumps and the arrival; floodlight masts along that side | 893 |
| Owner's scoreboard | 63.0, 75.6 | the catalogue's owner monument (a scoreboard showing the room owner) by the Jump Line's start, facing the arrival | 893 |
| Drift Circle | 96, 52 | a painted 18 m ring, a striped pylon, skid-mark arcs round it (each mark at its own height) | 893 |
| The camp | 104-152, 72-124 | the show's camp SE of the start: two crew trailers, a water tower, a windmill, a shade tarp (`b/camp.py`) | 893 |
| The Circuit | start/finish -20, -150 | a 945 m asphalt loop on the northern pan, 12 m wide, clockwise: the start straight east along z -150, round the Needle's west and north sides, the back straight west past the Castle, back down the west side; eight overlapping flat lanes (`b/circuit.py`) | 893 |
| Circuit furniture | bends at 205, -190; 230, -320; -48, -169 | a chequered start/finish gantry and line, moved 7.5 m on at 13:22 to cover the seam where the last lane piece ends; tyre walls round the outside of the three tightest bends, checkered kerbs on their insides (`b/circuit_kit.py`); a tabletop on the north half of the start straight at 84.5, -147.5 (the small jump as it was at 11:30, table at lip height - not yet rebuilt from the tuned one); six earth whoops across the back straight near 116, -399; a grandstand of two bleachers facing the start straight (32-45, -128) and a paddock inside the loop (containers, tyres, barrels) (`b/circuit_extras.py`) | 893 |
| The buttes | Mitten 365, -170; Castle 90, -470; Thumb 450, -345; Needle 250, -300 | sandstone formations on the NE skyline, each one BlobGroup of blended boxes on a talus mound (`b/buttes.py`) | 893 |

### Life and sound

- **Plants** (`b/camp.py`): saguaros (the catalogue's `lsys_cactus` at 0.55,
  about 7.5 m) in four scatters kept off the pan (splat layers 2 and 3):
  behind and west of the arrival, on the south mesa, thin over the map;
  dry grass tufts on the tongue mesa's aprons. None stand in the arrival's
  view: the first try put a saguaro across half its frame.
- **Sound** (the first session's own, 10:29): the seed's desert wind and
  gusts, its organ dirge and bass removed, the wind bed halved to 0.25.
  Unheard: the owner's to judge.

### Budget

`render --triangle-report` on the save of 15:49 (seq 154): 855,886
triangles (the ground 522,242) and 1,832 parts; the saguaros are the
largest share after the ground (180 copies, 185,760 triangles). The room
record is 271,944 bytes of compact JSON, far under the 900 KiB live-update
ceiling (#1499): live edits reach visitors. The largest single record is
the `shipping_containers` generator (43,056 of 102,400 bytes,
`status.editing.record_size`).

## Standing decisions

| Date | Issue | Decision |
|---|---|---|
| 2026-09-30 | #1522 | **The account**: @jink-ai.bsky.social, created by the owner; the concept in the owner's words: "focussed on playing with the physics of land-skiffs. So its region will contain props like ramps for jumping, maybe a race-track and themed around stunt-driving with land-skiffs." |
| 2026-09-30 | #1523 | **Build Jink's region and custom avatar**: "Now please add Jink properly and start building Jink's region and custom avatar" - full permission to edit Jink's region and avatar, and leave to save each improvement (a first session run self-guided) |
| 2026-09-30 | #1523, #1524 | **Change land-skiff physics in general**: "You are allowed to make changes to the way Overlands land-skiff locomotion and physics work in general, to enhance the driving (and jumping) experience." |
| 2026-09-26 | #1481 | **Agents keep to their own**: never touch another account's world or avatar |
| 2026-09-26 | #1474 | **Build for the browser**: count parts as well as triangles |
| 2026-09-27 | #1467 | **Terrain reflectance 0.25 in every world** (in code) |

## Working material

`exports/jink/` holds everything, made at session 893's start.

| Path | What it holds |
|---|---|
| `A`, `bin/` | the agent wrapper and the copies of `agent` and `render` it and the tools use (the 17:39 build); beside them the earlier ones: `agent.head-64e27ab` (HEAD before the session), `agent.drive-oldphysics` (HEAD with the drive verb, the old physics), `*.drive-1530` (the build of 15:30) |
| `src/room.json`, `src/avatar.json` | the last saves (both 15:49; room seq 154, avatar seq 155) |
| `src/room_seeded.json`, `src/avatar_seeded.json` | the seed's world and body (the reference) |
| `b/land.py` | the land, ground, light, sound and arrival; also writes `edits_clear_ONCE.txt` (never again: it replaces every generator and placement with the bare terrain) and `edits_land.txt` |
| `b/jumps.py`, `b/start_drift.py`, `b/props.py` | the Jump Line, its start gantry and the drift circle, the catalogue props round it (`props.py` prints `place` commands) |
| `b/circuit.py`, `b/circuit_kit.py`, `b/circuit_extras.py` | the Circuit (its centre line in `gen/circuit_samples.json`), its furniture, its riding features and stands |
| `b/buttes.py`, `b/camp.py`, `b/body.py` | the buttes; the camp and plants; the body |
| `b/lineup.py`, `b/aim.py`, `b/trace.py` | back to the Jump Line's start and facing down it; turning to a bearing within 0.5 degrees; a drive with `status` polled every 0.1 s |
| `b/land_try.py`, `b/viewcopy.py`, `b/butte_blob.py` | seed-scan copies; a viewing copy with the fog pushed out; the first blob butte trial |
| `log.md` | the save log |
| `893/` | session 893's notes (`notes.md`), scans, apply logs, command files, and the Jump Line's runs: `run_before_1.json` and `trace_before_*.txt` on the old physics, `run_after_*.json` and `trace_after_*.txt` on #1524's |

The pulled records are the truth, not the builders: diff a builder's output
against the record before applying it. `b/circuit.py` rewrites
`edits_circuit.txt` with `/placements/-` lines: re-apply only its generator
lines (the placements are 21-28).

## History

### 2026-09-30: session 893, live session 10, #1523, first session

- **Mode**: first session, self-guided, in the owner's words: add Jink
  properly, start building the region and the custom avatar, and improve
  land-skiff physics in general.
- **Built** (17 logged saves, 09:58-15:49): the seeded world cleared; the
  land, ground, light and arrival; the Jump Line with its start gantry, the
  grandstand, floodlights and the owner's scoreboard; the Stadium Gate; the
  drift circle; the buttes; the Circuit with its furniture, whoops, tabletop,
  grandstand and paddock; the camp and plants; the first sound; body v1;
  curved lead-ins on the kickers (11:06); the Jump Line tuned on the new
  physics and Jink's suspension damping (15:49).
- **The owner's first visit**: 13:07-13:27, on the pan, the circuit and
  out past the Jump Line's far end; Jink greeted them; nothing was said
  in chat. Their camera showed the pale seam at the circuit's start, fixed
  at 13:22.
- **Measured** on the Jump Line, the car lined up and `drive W@20 none@4`:
  on the old physics (10:58-11:05), the small jump landed nose-down (-19 to -31
  degrees), the car fell from 15 to 4-6 m/s, swung about 45 degrees left and
  missed the other two jumps. On #1524's air model, the run of 15:49 on the
  tuned line: airtime 0.70 / 0.90 / 1.33 s; touchdown 11.8 / 15.0 / 20.4 m
  past each lip (9.7 / 12.5 / 18.4 m from take-off); landing pitch -5.5 /
  -3.1 / -0.3 degrees, roll 0.0; 97 / 96 / 94% of the speed kept; the
  steepest tilt 25 degrees, no rollover. Repeated at 17:43 on the rebuilt
  daemon: the same to within a few hundredths
  (`exports/jink/893/run_after_11.json`).
- **Failed at first**: the first arrival saw mostly the mesa-top gravel and
  small, far ramps (moved to the brow, the line 30 m nearer, flags added);
  the bleachers and the scoreboard faced away (catalogue fronts face +Z);
  the gate's shadow lay across the arrival camera (moved to 28 m behind);
  the buttes read as tanks, then as buildings, before blobs read as rock;
  the Needle's first form read wrongly up close; the circuit's lanes
  z-fought at every spine joint and at the loop's closure; a kerb spine had
  17 points; a saguaro filled the arrival's frame; a ';' chain saved a small
  skid-mark z-fight unread. On the new physics: a straight kicker's kink put
  the chassis on the ramp (curved lead-ins); 6 m ramps could not hold a line
  over three jumps (9 m); a table at lip height let the car skim across on
  its springs instead of flying (the small jump's deck 0.5 m lower); `face`
  stops within 10 degrees and `status.facing` is rounded to two decimals,
  about a degree, so lining up needed pulses of A/D and a precise
  `status.heading_deg`.
- **Code**: #1524 the air model (delegated: a builder, a critic, a fix
  round; driven live from 15:31); #1527 `agent drive` with its run report, and
  `status.heading_deg`; #1525 a stale splat doc; #1526 `AGENT_BIN` for the
  tools. Filed: #1528 (`height_m` counts speculative contacts), #1529 (the
  airplane's roll keys look swapped). The end review of the code no critic
  saw found a wasm build break, a roof landing that read level in the run
  report, the car's airborne rule misdocumented, and a held key keeping a
  car on its roof - all fixed (`landing_tilt_deg`; no air key counts while
  a car lies on the ground) - and four lows, filed as #1530-#1533.

## Open threads

- **Two physics questions for the owner** (#1524): every seeded skiff's
  suspension damping is about 0.11 of critical, so a skiff landing a jump
  porpoises on its springs (Jink's own is now 900, about 0.37) - raise the
  seeded damping for the whole fleet? And the new bump stop makes some
  landings rebound harder than before (the critic's finding 4: 5.2 against
  3.7 m/s) - keep it or soften it?
- **The circuit's tabletop** is the small jump as it was at 11:30, its
  table at lip height: rebuild it from the tuned `jump_s` and drive it; the
  six whoops are not yet driven on the air model either.
- **The Mesa Drop**: a launch ramp off a mesa brow onto a landing ramp on
  the pan, sized from measured runs.
- **Lap timing and checkpoints** do not exist in the game: a new mechanic,
  to be filed and described, and built only on the owner's word.
- **#1528** (`height_m` reads 0.0 over a surface a fast car is not on),
  **#1529** (the airplane's roll keys), and the end review's lows
  #1530-#1533 (untested rules, an engage tilt of 90 that leaves a car on
  its side, the controls sheet's guard).
- **The owner's word on the mood**: they visited at 13:07 and said nothing;
  the light and the sound are theirs to judge.
