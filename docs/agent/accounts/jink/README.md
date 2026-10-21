# Jink and Parabola Flats

Jink (@jink-ai.bsky.social) is a land-skiff stunt driver who treats every
jump as an experiment and measures the hang time rather than guessing it.
Its body is a three-wheeled stunt cyclecar that drives as a car; its world,
Parabola Flats, is a dry lake in red mesa country laid out as a stunt park:
a jump line with a ring of fire over its biggest gap, a crowd in the
stands, a drop off a mesa brow, a drift circle, a 945 m circuit round a rock
spire, and sandstone buttes on the skyline. The owner created the account on
2026-09-30 and asked, the same morning, for Jink's region, a custom body and
better land-skiff physics (#1523). Where things stand (2026-10-02, after
session 895): the park and the body are built and saved (room 09:32, avatar
09:21, the room once more at 15:34); session 893's air model was
committed by the owner (13d5515);
session 895's physics - fleet damping, a softer bump stop, terrain without
ghost edges, cars that brake onto a walk-to's point - is in the working
tree, not committed (that is the owner's), and Jink's daemon ran a build of
it from 14:25, on which the Jump Line and the Mesa Drop were driven again.
In session 955 (live session 15, 2026-10-21, #1734) the owner gave Jink
leave to change and save its region and body with the newer features in
mind: the park now turns through a desert day and night, its windsocks,
flags, windmill and ring of fire move, its crowd waves, fans line the
run-up, judges sit by the Big One, and the car's wheels roll and steer;
then, on the owner's follow-up, the Freestyle Pit, a stunt compound for
playing with the air model: the Moon Shot (2 s of air, room for flips and
rolls), the Step-Up, the hip and the Mound, with its own crowd and
floodlights ([History](#history)). Where things stand at the session's end
(10:00 on 2026-10-21): everything saved (room 09:50, seq 11; avatar 08:47,
seq 4) and the agent stopped with nothing discarded. The owner ended the
session from the terminal without visiting, so nobody has driven the Pit
but the offline runs.
Times on this page are local (CEST).

## Start here

What a session prompt no longer has to say. Read this section first, every
session.

| | |
|---|---|
| Account | @jink-ai.bsky.social, `did:plc:lr2ocunor73lfqpwtgvt274o` |
| Commands | every agent command takes `--account jink-ai.bsky.social`; every tool call starts `AGENT_ACCOUNT=jink-ai.bsky.social` ([session.md](../../session.md#starting)) |
| Admin | @codewright.bsky.social, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Parabola Flats, Jink's own world: its DID is Jink's, the `--world` of every offline render |
| Working folder | `exports/jink/` (gitignored, on this machine only), kept across sessions: `A` the agent wrapper (it runs the copy in `bin/`), `b/` the builders, `gen/` what they write, `src/` the last saves - `room.json` (2026-10-21 09:50, save event seq 11) and `avatar.json` (2026-10-21 08:47, seq 4) - and `log.md` the save log; numbered subfolders (`893/`, `895/`, `896/`, `955/`) are earlier scratchpads, read-only |
| Last session | chainlink session 955 (live session 15), parent #1734 - read its comments (`chainlink show 1734`); `chainlink session last-handoff` is the latest session of any kind |
| Default mode | self-guided ([session.md](../../session.md#self-guided)), with the owner's leave to save region and body (2026-10-21, [Standing decisions](#standing-decisions)) |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's world (Ashmere) or avatar ([reeve/](../reeve/README.md)), or Eigen's world (Isoline) or avatar ([eigen/](../eigen/README.md)) |

- **The wrapper runs a copy.** `exports/jink/A` runs `exports/jink/bin/agent`,
  copied from `target/test-release/` at the session's start (since
  2026-10-21 the build of HEAD d0565a9; session 895's are `*.s895-1425`: a
  build that old knows no spin, weather or turning day and would drop them
  on a save - never save from it), so a rebuild in
  the tree (a delegation's, say) never changes the running tool under you;
  put `AGENT_BIN` and `AGENT_RENDER` (the copies in `bin/`) in front of the
  tools while one runs. After a client change, copy the new binaries in and
  restart on purpose.
- **Plan on a copy without the day**: the room's day turns (120 minutes),
  so an offline render without `--hour` shows the wall clock's hour - often
  night. `views.py --hour 16` is the owner's approved afternoon, 21 a night
  under stars; `955/plan_room.json` is the 08:56 save, before the Freestyle
  Pit (`src/room.json` has it).
- **Skiffs sink, and only skiffs and hover-boats ride ramps.** Both drive on
  the raycast suspension; walkers and airships watch - hence the stands.
- **Slopes cost a skiff nothing**: its suspension pushes straight up, so a
  hill never slows it ([moving.md](../../moving.md)). A ramp launches it
  through its springs, well under the ramp's own angle (a 30 degree kicker
  about 22, 42 about 31-34: [region.md](../../region.md#ramps-and-jumps-for-wheels)),
  and a curve steeper than about 45 degrees throws it off over the top. Top
  speed on the flat is 14.4 m/s, reached in about 40 m; a U-turn at full
  lock at 13.5 m/s is about 9.3 m round. In the air (#1524) it levels itself
  when no key is held; W/S pitch it, A/D yaw it, Q/E roll it, strongly
  (0.25 s of D is 48 degrees; [moving.md](../../moving.md#driving-a-run-stunts-measured));
  a key already held as the wheels leave counts in the air only once
  pressed again (Q/E excepted), so W held off a lip does not nose the car
  in.
- **Fly a ramp offline before saving it**: `bin/render --driver
  src/avatar.json --world did:plc:lr2ocunor73lfqpwtgvt274o --world-record
  TRY.json --drive-from X,Z --drive-bearing B --drive-keys "W@6 none@4"
  --drive-lead 10 --drive-log RUN.jsonl` (with `BEVY_ASSET_ROOT` set) runs
  the game's own car physics in about 6 s, no daemon; `python3
  955/flight.py RUN.jsonl LIPX,LIPZ B` reads the log against a lip. A
  segment of 0 seconds aborts the render. Every Freestyle Pit feature was
  flown so before it was applied; `b/pit955.py --try` composes
  `955/try_pit.json` from `955/plan_room.json`.
- **Drive and measure the Jump Line** from `exports/jink`: `b/lineup.py`
  takes the car back to the start by open-pan waypoints and aims it at
  bearing 315 with `b/aim.py` (pulses of A/D, read against
  `status.heading_deg`) - run `b/aim.py 315` once more if it ends a few
  degrees off (session 895: 3.4 off missed the Big One); then
  `./A drive W@20 none@4 --wait`. The answer's `report.jumps` gives each
  jump's airtime, distance, rise, landing pitch, roll and tilt, and speeds
  ([moving.md](../../moving.md#driving-a-run-stunts-measured)).
- **Drive the Mesa Drop**: `python3 b/place.py -78.17 67.86 -54.61 78.85`
  puts the car at rest on the start arch and aims it at the lip (short
  throttle-and-brake moves; written while a car's walk-to rolled 7-11 m
  past its point, #1536, now fixed), then `./A drive W@5 none@4 --wait`:
  1.24 s and 15.6 m off the brow at 13 m/s. Never come in along the run's
  axis from behind: the Stadium Gate stands on it 40 m back.
- **After an `apply`, `look` until `world_building` is false before a
  run**: a run started seconds after one (15:34) drove the whole line
  without a single jump, most likely on ramps still being rebuilt.
- **Catalogue pieces face local -Z**, as `place --yaw` counts it: the
  bleachers' seats, the scoreboard's screen, the floodlights' lamps, the
  trailers' doors. Session 893 wrote the opposite here and turned seven
  pieces backwards; session 895 turned them round
  ([building.md](../../building.md#frames-and-yaw)). Read a piece from four
  sides after placing it.

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
- **Body v2** (`b/body.py`, saved 2026-10-02 09:21, seq 160): copper
  hubcaps on every wheel's outer face (both faces of the fat rear one) and
  white racing roundels on the hull's flanks - 33 parts, 10.7 KB. v1's
  copper hubs sat inside the tyres, and a lathe fills its own middle, so
  every wheel read plain black (`exports/jink/895/body_v4.png`).
- **Body v3** (`b/body.py`, saved 2026-10-21 08:47, seq 4): the parts that
  move now move (#1604's spin). Every wheel hangs from an axle stub at its
  centre that rolls with the car (`roll`, radius 0.346 m), a light
  knock-off spinner across each hubcap so the roll shows (a round tyre and
  dome look the same rolling or still); each front wheel's axle hangs from a
  steering knuckle (`steer`, gain -0.6, limit 28: a steer term counts
  forward along its parent's +Z, as seeded vehicles are drawn, and Jink's
  nose is its -Z, so a positive gain would turn the wheels out of a bend -
  checked from above mid-turn with `render --driver`); the driver's head
  leans against the car's speeding up and out of bends on two neck stubs
  (`lean`, negative: a head stands on its neck); an orange dune-buggy whip
  flag on the roll hoop's right shoulder lags the same way, its pennant
  fluttering. 49 parts, 20,224 triangles, 14.4 KB. The sanitiser raises a
  spin period under 0.5 s to 0.5 and a radius or thickness under 1 cm to 1
  cm.
- **Suspension damping 900** (15:49, seq 155; the seed's was 271.1). Per
  corner that is about 0.37 of critical damping where the seed's was 0.11:
  on the seed's, Jink landed a jump and porpoised on its springs for about
  2.5 s; on 900 it settles at once. Since #1534 (the owner's decision,
  2026-10-02) every seeded skiff gets 0.35 of critical too; a record saved
  before keeps what it saved.
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
- **Ground**, four layers with LINEAR colours: `CrackedEarth` on the pan, a
  playa crust since 2026-10-02 (`b/ground_playa.py`: the envelope's 20
  plates a tile, about 0.57 m, cracks about 5 cm, little curl, a paler
  plate - session 893's 1.4 m plates with 14 cm cracks read as giant crazy
  paving), red sandstone `Rock` on the walls (slope 0.035-0.2), warm `Sand`
  on the aprons (and wherever no rule matches), `Gravel` desert pavement on
  the tops (its `scale` capped at 64).
- **Water**: the plane at -5 m, under the whole map. A dry lake: skiffs have
  no buoyancy.
- **Light and day** (session 955, `b/atmos955.py`): a turning desert day of
  120 minutes (#1677). Noon's sun stands due south 50 degrees up, near-white
  at 14,000 lux (16,000 blew the pale playa white) under a deep desert blue;
  on that arc 16:00 stands at bearing 249, 22.5 degrees up - the
  first session's light (bearing 248, 24 degrees up, 11,000 lux), which the
  owner approved on 2026-10-02, is the day's four o'clock; the room before
  is `955/room_before_atmos.json`. By night the moon and the desert's
  stars, the ring of fire and the floodlight lamps glowing. The wind blows
  toward ENE (`cloud_wind_dir`), as the windsocks and dust devils always
  showed; fair-weather cumulus (cover 0.22) whose shade crosses the pan; a
  warm grade (+0.02, saturation 1.08); 1,500 m visibility, the fog's pale
  blue (or the whole sky turns its tint).
- **No weather**: storms at 0.45, 0.9 and 0.25 each turned the red desert
  into a grey murk (`955/review/storm_s.png`), and a weather that comes and
  goes falls about half the time whatever its intensity - #1735 asks for a
  share-of-time setting and a dust storm kind.
- **Grass** (#1700): the `dry_grass` tuft scatter made the playa and the
  walls grass country on Auto, and the cracked pan grew tufts; the grass
  group (session 955) grows none there and sparse dry bunchgrass on the
  aprons (0.30) and the tops (0.15), straw-pale, mostly dry, clumped, a few
  late desert flowers.

### Arrival

`default_landing` is (-72, 44), on the tongue mesa's east brow about 13 m
up on an 11.5 degree slope, `yaw_deg` 335 since 2026-10-02 (bearing 25,
NNE; it was 315.3, bearing 45): the arrival camera looks down over the Big
One with its ring of fire and wrecks on the left, the medium jump, the two
stands and their crowd, a floodlight, the circuit with its stands beyond
and the buttes and dust devils on the skyline, the sun behind. At bearing
45 the Big One sat just outside the frame (`exports/jink/895/cmp_landing.png`).
Three spots on the brow were compared in session 893
(`exports/jink/pics/landcams.png`).

### Places

| Place | x, z (metres; -Z is north, +X is east) | What it is | Session built |
|---|---|---|---|
| Arrival | -72, 44 | the tongue mesa's east brow, facing NE over the pan | 893 |
| Stadium Gate | -91.8, 63.8 | the world's gateway (catalogue `sports_rec_gateway`), 28 m behind the arrival so its afternoon shadow stays out of the arrival camera; proven by driving in (`picker: open`) | 893 |
| The Jump Line | 58.8, 91.2 to -125, -93 | on the pan, launching NW (bearing 315), side-on to the arrival, every ramp 9 m wide and solid, each kicker with a two-wedge curved lead-in: the start gantry at its start (a red banner, a drag-race light tree facing the run-up); a small 15 degree tabletop at 16.4, 48.8 (lip 1.0 m, a 6 m table 0.5 m below the lip, a 2.5 degree landing 6-17.5 m past the lip); a medium 20 degree tabletop at -36.7, -4.3 (lip 1.6 m, a 10 m table, a 6.5 degree landing 10-24 m past); the Big One, a 24 degree kicker (lip 2.4 m) at -86.2, -53.8 over a 16 m gap with four wrecked cars side by side and a landing hump (a 30 degree face to 1.4 m, a 14 degree run-out); flags at each lip (`b/jumps.py`, `b/start_drift.py`). Sized from measured runs (History) | 893 |
| Ring of fire | -92.8, -60.4 | a glowing 3.2 m hoop on two striped posts across the Big One's gap, 9.4 m past its lip and 4.6 m up where the car's apex is; twelve flame emitters round it and a smoke plume; nothing solid - the Big One flew through it 1.3 m off its middle (`b/fire_ring.py`). Since session 955 the hoop and its flames turn about the run at 36 degrees a second, a wheel of fire (`b/spin955.py`) | 895, 955 |
| Fans along the run-up | 14-54 m down the Jump Line from the start, on the stands' side | 16 sponsor boards in Jink's navy and yellow 7.5 m out from the run, and 24 standing fans in groups behind them facing it, a third waving, some holding up a flag (`b/fans955.py`) | 955 |
| Judges' booth | -110.2, -58.0 | by the Big One, 14 m out on the arrival's side where its jumps come down, facing the run: a scaffold booth, its deck 2.4 m up behind a yellow and navy panel, a red roof, a ladder; three judges at a table, two holding up white score cards that bob slowly; a black pennant turning downwind (`b/judges955.py`) | 955 |
| Distance posts | beside each landing | short white posts every 5 m past each lip and tall red ones at the tens, on the stands' side 6.5 m out: small 5-20 m, medium 10-25, the Big One 15-30 (`b/markers.py`) | 895 |
| Grandstand | -34.5, -38.9 and -50.1, -54.5 | two bleachers on the far side of the Jump Line, turned on 2026-10-02 to face SW across it at the jumps and the arrival (session 893's placing sat them backwards), with 14 and 12 seated spectators (`b/crowd.py`); floodlight masts along that side | 893, 895 |
| Owner's scoreboard | 63.0, 75.6 | the catalogue's owner monument (a scoreboard showing the room owner) by the Jump Line's start, its screen turned to bearing 250 (the start and the way down from the arrival) on 2026-10-02; it had shown the arrival its back | 893, 895 |
| The Mesa Drop | lip -54.6, 78.9 | a 20 degree kicker (lip 1.6 m, 9 m wide, curved lead-in, a back ramp) on the tongue mesa's crest, launching ESE (bearing 115) off the brow onto the natural 15-19 degree slope; its frame tilted to a plane fitted to the ground; tall pennants at the lip, a striped start arch 26 m back (not solid), a windsock beside it. Driven: 1.24 s, 15.6 m, up to 3.34 m over the slope at 13 m/s, landing level, 94% kept (`b/mesa_drop.py`, `b/lineup_md.py`, `b/place.py`) | 895 |
| Drift Circle | 96, 52 | a painted 18 m ring, a striped pylon, skid-mark arcs round it (each mark at its own height) | 893 |
| The Freestyle Pit | 60-240, -103 to -8 | a stunt compound on the open pan east of the Jump Line, between the circuit's start straight and the drift circle, for playing with the air model. Four features make a clockwise loop: east along its north lane (z -78), round the east end over or past the Mound, west along its south lane (z -32), round the west end. The ramps are in Jink's navy with red decks, yellow lips, and flags whose pennants turn downwind. Two bleachers stand back to back between the lanes at 150, -55, one facing each lane, 14 spectators on each behind yellow and navy boards. Floodlight masts at 100, -103 and 205, -8. Every feature was flown offline before it was saved (`b/pit955.py`) | 955 |
| The Moon Shot | lip 140, -78 | the Pit's big air, launching east: a 42 degree kicker drawn as a circular arc of 10 chord wedges (lip 2.8 m, 10 m wide); a landing hill 12 m wide whose 4.5 m crest stands 16 m out (a 22 degree face, an 18 degree landing to 30 m); distance posts 15-35 m on its south side. Flown at 14 m/s: about 2 s in the air, 8.2-8.6 m up, down 25-30 m out. A barrel roll (Q 1.1 s) comes round and a backflip (S 1.0 s) nearly does; a car at 10 m/s lands on the face and climbs it | 955 |
| The Step-Up | lip 165, -32 | launching west: a 36 degree arc kicker (lip 1.8 m) up onto a deck 3.0 m high and 22 m long (its 25 degree face from 5 m out), then off its far edge down a 12 degree landing. At 14 m/s the car lands on the deck 18.5 m out; at 13.6 m/s it comes down on the face's top and drives up | 955 |
| The hip | lip 92, -32 | at the south lane's west end, launching west: the Step-Up's kicker and a landing hill (crest 3.4 m, 15 m out, 20 m wide) turned 35 degrees right. Flown straight, the car lands across its slope and carries on west. With 0.25 s of D in the air it turns 48 degrees, and the landing sends it off NW toward the north lane | 955 |
| The Mound | 200, -55 | in the Pit's east turnaround: an octagonal frustum of earth (the whoops' material), 3.5 m high and 36 m across, with a 6 m top and 13 degree faces, to hop from any side: 0.6 s of air at 14 m/s and a soft landing on the far face. It is a convex lathe at resolution 8, so its collider is what is drawn | 955 |
| The camp | 104-152, 72-124 | the show's camp SE of the start: two crew trailers and a shade tarp (turned on 2026-10-02 to face the camp and the start), a water tower, a windmill (its head put back on its tower, `b/windmill_fix.py`), a windsock by the start gantry (`b/camp.py`, `b/windsock.py`) | 893, 895 |
| The Circuit | start/finish -20, -150 | a 945 m asphalt loop on the northern pan, 12 m wide, clockwise: the start straight east along z -150, round the Needle's west and north sides, the back straight west past the Castle, back down the west side; eight overlapping flat lanes (`b/circuit.py`) | 893 |
| Circuit furniture | bends at 205, -190; 230, -320; -48, -169 | a chequered start/finish gantry and line, moved 7.5 m on at 13:22 to cover the seam where the last lane piece ends; tyre walls round the outside of the three tightest bends, checkered kerbs on their insides (`b/circuit_kit.py`); white edge lines the whole way round, 4.5 m out, riding each lane piece (`b/circuit_lines.py`, 2026-10-02); a tabletop on the start straight at 84.5, -147.5, rebuilt on 2026-10-02 from the tuned small jump, 6 m wide (`b/track_jump.py`: 0.70 s, 9.8 m at 14 m/s); six earth whoops across the back straight near 116, -399 (driven 2026-10-02: hops of 0.07-0.23 s, 0.70 s off the last); a grandstand of two bleachers facing the start straight (32-45, -128, turned north on 2026-10-02) with 8 spectators each, a floodlight turned to light the straight, and a paddock inside the loop (containers, tyres, barrels) (`b/circuit_extras.py`) | 893, 895 |
| The buttes | Mitten 365, -170; Castle 90, -470; Thumb 450, -345; Needle 250, -300 | sandstone formations on the NE skyline, each one BlobGroup of blended boxes on a talus mound (`b/buttes.py`) | 893 |

### Life and sound

- **Plants** (`b/camp.py`): saguaros (the catalogue's `lsys_cactus` at 0.55,
  about 7.5 m) in four scatters kept off the pan (splat layers 2 and 3):
  behind and west of the arrival, on the south mesa, thin over the map;
  dry grass tufts on the tongue mesa's aprons. None stand in the arrival's
  view: the first try put a saguaro across half its frame.
- **Sound**: the seed's desert wind and gusts (session 893: its organ dirge
  and bass removed, the wind bed halved to 0.25), softened on the owner's
  word on 2026-10-02 (session 897, saved 20:21). They had found it "a
  little too harsh and thereby unpleasant on the ears": the bed was pink
  noise through a HIGH-pass at about 1.35 kHz (59% of the sound above 2
  kHz, none under 500 Hz - hiss), the gusts white noise band-passed at
  1.75 kHz (a whistle). Now the bed runs through a gentle low-pass at 700
  Hz (Q 0.70) that breathes 300 Hz either way, and the gusts through a band
  at 650 Hz twice as wide (Q 0.8); every instrument, level and event as
  before (`b/sound_soft.py soft`). They chose it by ear ("wind_soft sounds
  good") from `exports/jink/896/wind_soft.wav` over `wind_medium.wav`,
  which a slip in the builder (fixed) gave the same bed: the two differed
  only in their gusts.
- **What moves** (session 955, #1604's spin; `b/spin955.py`, `b/crowd.py`):
  the windsocks turn downwind and flutter, the flags on every jump and the
  Mesa Drop stream downwind and flutter round their poles, the windmill's
  head turns its tail downwind and its rotor spins, the ring of fire turns;
  about one spectator in three on the stands waves an arm, each at its own
  pace and phase (13 of 96; the figures as session 895 built them). A turning
  part hangs from a 2 cm stub at its pivot; the floating report counts those
  stubs, in the air by design. Proven in clips (`render --frames 7
  --keep-frames`): the rotor changed 11 grey levels a frame, its tower 0.1.
  The Freestyle Pit's pennants turn and flutter the same way, and 9 of its
  28 spectators wave (`b/pit955.py`, seating `b/crowd.py`'s figures).
- **The show's life** (2026-10-02): 42 seated spectators on the four
  stands in show-day shirts, a few in caps (a body lathe and a head each:
  `b/crowd.py`; the floating report counts them as free, since they sit on
  another generator's seats); two dust devils on the far pan, each its own
  particle generator and seed, puffs rising 15-20 m and leaning ENE
  (`b/dust.py`); windsocks at both launch points trailing ENE
  (`b/windsock.py`); the ring of fire's flames and smoke.

### Budget

After session 955 (the save of 2026-10-21 09:50, seq 11): 992,026 triangles
(the ground 522,242) and 2,667 parts - 67,224 triangles and 586 parts more
than it began with. Through 08:56 that was the fans' 24 figures and 16
boards, the booth's 50 parts, the waving arms and the turning stubs (39,464
triangles, 265 parts). The Freestyle Pit added 27,760 triangles and 321
parts, most of them its two bleachers (45 parts each), their 28 spectators
and the two floodlights; its four features are 104. The room record is
522,453 bytes of compact JSON, 57% of the 900 KiB live-update ceiling, so
live edits reach visitors. The largest single record is `fans_runup`
(44,676 of 102,400 bytes); the avatar is 14,441 bytes. The floating report
finds nothing of the Pit's ramps, flags or posts; its crowds read as free,
as every crowd does.

`render --triangle-report` on the save of 2026-10-02 15:34 (seq 90; the
same as at 09:32):
924,802 triangles (the ground 522,242) and 2,081 parts (1,832 at session
893's end: the crowd 96, the circuit's lines 80, the ring of fire, the
Mesa Drop, the windsocks and posts the rest); the saguaros are still the
largest share after the ground (180 copies, 185,760 triangles). The room
record is 391,450 bytes of compact JSON, 42% of the 900 KiB live-update
ceiling (#1499): live edits reach visitors. The largest single record is
the `shipping_containers` generator (43,056 of 102,400 bytes,
`status.editing.record_size`). The avatar is 10,729 bytes.

## Standing decisions

| Date | Issue | Decision |
|---|---|---|
| 2026-09-30 | #1522 | **The account**: @jink-ai.bsky.social, created by the owner; the concept in the owner's words: "focussed on playing with the physics of land-skiffs. So its region will contain props like ramps for jumping, maybe a race-track and themed around stunt-driving with land-skiffs." |
| 2026-09-30 | #1523 | **Build Jink's region and custom avatar**: "Now please add Jink properly and start building Jink's region and custom avatar" - full permission to edit Jink's region and avatar, and leave to save each improvement (a first session run self-guided) |
| 2026-09-30 | #1523, #1524 | **Change land-skiff physics in general**: "You are allowed to make changes to the way Overlands land-skiff locomotion and physics work in general, to enhance the driving (and jumping) experience." |
| 2026-10-02 | - | **The light is approved** (the owner, terminal): "The light looks good." The late-afternoon sun, sky and fog stay as session 893 set them |
| 2026-10-02 | - | **The softer wind** (the owner, terminal): the wind was "a little too harsh"; of two renders they chose the soft one ("wind_soft sounds good"), saved 20:21 (the Sound bullet) |
| 2026-10-02 | #1534 | **Fleet damping**: every seeded skiff's suspension damping raised to about 0.35 of critical (from 0.11), so a skiff settles after a landing instead of porpoising |
| 2026-10-02 | #1535 | **Soften the bump stop's rebound**: no landing may come back up harder with the stop than without it (kept, with #1538's terrain fix, after the critic measured terrain landings) |
| 2026-10-02 | #1523 | **No lap timing**: checkpoints and a lap timer for the Circuit are NOT wanted - it stays an untimed loop |
| 2026-10-21 | #1734 | **Improve and save freely, with the newer features in mind** (the owner, terminal): "Please let them improve their avatar and region with all the newer available features in mind (Spin, Weather, Athmosphere, Day-cycles, etc ...). Same as Hypha and Reeve, Jink is allowed to change and save their avatar and region as they see fit, without waiting for my approval." Session 955 read the mood (the day, the sky) as within it, the approved light kept as the day's 16:00 |
| 2026-10-21 | #1734 | **More to play with vehicle physics** (the owner, terminal, after the morning's saves): "Please continue to improve Jink's region by building it out with more opportunities to play with vehicle physics." Session 955 built the Freestyle Pit, flying every feature offline on the game's own physics before saving it |
| 2026-09-26 | #1481 | **Agents keep to their own**: never touch another account's world or avatar |
| 2026-09-26 | #1474 | **Build for the browser**: count parts as well as triangles |
| 2026-09-27 | #1467 | **Terrain reflectance 0.25 in every world** (in code) |

## Working material

`exports/jink/` holds everything, made at session 893's start.

| Path | What it holds |
|---|---|
| `A`, `bin/` | the agent wrapper and the copies of `agent` and `render` it and the tools use (since 2026-10-21 the build of HEAD d0565a9, with spin, weather and the turning day); beside them the earlier ones: `*.s895-1425` (session 895's build of 14:25, with #1528-#1538: never save from it, it would drop the newer fields), `*.s895-0817` (the committed 13d5515, session 895's morning), `*.s893-1739` (session 893's last), `agent.head-64e27ab` (HEAD before session 893), `agent.drive-oldphysics`, `*.drive-1530` |
| `src/room.json`, `src/avatar.json` | the last saves (room 2026-10-21 09:50, seq 11; avatar 08:47, seq 4); `src/room_955.json` and `src/avatar_955.json` are the pulls at session 955's start (equal to the saves before it) |
| `src/room_seeded.json`, `src/avatar_seeded.json` | the seed's world and body (the reference) |
| `b/land.py` | the land, ground, light, sound and arrival; also writes `edits_clear_ONCE.txt` (never again: it replaces every generator and placement with the bare terrain) and `edits_land.txt` |
| `b/jumps.py`, `b/start_drift.py`, `b/props.py` | the Jump Line, its start gantry and the drift circle, the catalogue props round it (`props.py` prints `place` commands) |
| `b/circuit.py`, `b/circuit_kit.py`, `b/circuit_extras.py` | the Circuit (its centre line in `gen/circuit_samples.json`), its furniture, its riding features and stands |
| `b/buttes.py`, `b/camp.py`, `b/body.py` | the buttes; the camp and plants; the body |
| `b/lineup.py`, `b/aim.py`, `b/trace.py` | back to the Jump Line's start and facing down it; turning to a bearing within 0.5 degrees; a drive with `status` polled every 0.1 s |
| `b/ground_playa.py`, `b/mesa_drop.py`, `b/lineup_md.py`, `b/place.py` | the playa crust (`key=value` overrides); the Mesa Drop (`angle=`, `lip=`, `lip_s=`; fits the kicker's plane to the ground, re-runs set its placements); a route round the kicker onto the run-up; the car at rest on a point by throttle-and-brake moves, then aimed |
| `b/windmill_fix.py`, `b/track_jump.py`, `b/crowd.py`, `b/dust.py`, `b/windsock.py`, `b/fire_ring.py`, `b/markers.py`, `b/circuit_lines.py` | the windmill's head, braces and blades; the circuit tabletop from the tuned jump; the spectators; the dust devils; the windsocks; the ring of fire; the distance posts; the circuit's edge lines (each on its own lane piece, by `thread.py --ride`) |
| `b/atmos955.py`, `b/spin955.py`, `b/fans955.py`, `b/judges955.py` | session 955's: the day, sky, wind, grass (and `none` or a weather to try); the moving parts (built from `955/room_before_atmos.json`, never over a copy that already turns: a re-run would nest a second stub); the fans and boards; the judges' booth. `b/body.py` (v3) and `b/crowd.py` (the waving arms, their own random stream; since the Pit its build runs under a main guard, so importing it rebuilds nothing) were changed in place |
| `b/pit955.py` | the Freestyle Pit: its four features (each a generator whose origin is its lip, launching along local -Z, or the Mound's centre), the stands' crowds and boards, and the placements of its bleachers and floodlights; `edits_pit955.txt` keeps each landed placement's index across re-runs (82-91); `--try` composes `955/try_pit.json` from `955/plan_room.json` for offline flights and views |
| `b/sound_soft.py` | the softer wind (`soft`, saved; `medium`, the other proposal); it stops at its first assert on the saved record, whose bed no longer has the high-pass it replaces |
| `b/land_try.py`, `b/viewcopy.py`, `b/butte_blob.py` | seed-scan copies; a viewing copy with the fog pushed out; the first blob butte trial |
| `log.md` | the save log |
| `893/` | session 893's notes (`notes.md`), scans, apply logs, command files, and the Jump Line's runs: `run_before_1.json` and `trace_before_*.txt` on the old physics, `run_after_*.json` and `trace_after_*.txt` on #1524's |
| `895/` | session 895's review pictures and comparisons, the Mesa Drop's ground grids (`md_ground*.json`) and runs (`run_md_*.json`, `run_md_4_newphys.json` on the new physics), the tabletop's and whoops' runs, the Jump Line through the ring (`run_jl_ring.json`) and on the new physics (`run_jl_newphys.json`, `trace_jl_newphys.txt`), the triangle and floating reports, and `room_sound_soft.json` (the saved room with the soft wind: what `896/wind_soft.wav` was rendered from, and what was saved at 20:21) |
| `955/` | session 955's: the room before it (`room_before_atmos.json`), trial compositions (`try_*.json`), `plan_room.json`, the sign-in logs, the v2 builders kept (`body_v2.py`, `crowd_v1.py`), and `review/` - the opening review (`start.png`), the hours (`atmos_hours_s.png`), the storms (`storm_s.png`, `storm25_s.png`), the clips proving the spins (`ringclip.gif`, `millclip.gif`, `crowdclip.gif`, `drive_v3.gif`, `roll_v3.gif` and their frames), the floating reports, the closing review (`end_s.png`). For the Freestyle Pit: `flight.py` (reads a `render --driver` log against a lip), the prototypes `qp_test.py` (the quarter pipe that threw the car over) and `moon_test.py`, the drive logs and stills (`review/pit_*`, `moon_*`, `vol_*`, `loop*`), the views (`pit_views1-4.png`, `pit_night.png`) and its triangle and floating reports (`tri_pit.txt`, `float_pit.txt`) |
| `896/` | session 896's wind renders for the owner's ear (`render --ambient-wav`): `wind_now.wav` (the harsh one), `wind_soft.wav` (chosen), `wind_medium.wav` and its record |

The pulled records are the truth, not the builders: diff a builder's output
against the record before applying it. `b/circuit.py` rewrites
`edits_circuit.txt` with `/placements/-` lines: re-apply only its generator
lines (the placements are 21-28).

## History

### 2026-10-21: session 955, live session 15, #1734, self-guided

- **Mode**: self-guided, on the owner's prompt: improve the avatar and the
  region with the newer features in mind (spin, weather, atmosphere, the
  day cycle), saving as Jink sees fit. Jink's saved sign-in was refused
  (`invalid_grant`: it predated the newer collections); the owner approved
  a new one on the third request (the first was cancelled on the
  authorization page, the second ran out of time). Its wrapper now runs a
  build of HEAD d0565a9. The pulled records matched the last saves.
- **Built** (11 logged saves, 08:40-09:50): the turning desert day, the wind,
  the clouds, the grade and the grass group; the park's moving parts; body
  v3; the waving crowd; the fans and boards along the run-up; the judges'
  booth. Then, on the owner's follow-up ("continue to improve Jink's region
  by building it out with more opportunities to play with vehicle
  physics"), the Freestyle Pit: its three ramps (09:39, seq 9), its stands,
  crowd and floodlights (09:42, seq 10), and the Mound (09:50, seq 11).
- **Measured** offline on the game's own car physics (`render --driver`,
  logs in `exports/jink/955/review/pit_*.jsonl`, `moon_*.jsonl`,
  `vol_*.jsonl`, `loop*.jsonl`):
  - Top speed on the flat is 14.4 m/s, reached in about 40 m. A full-lock
    U-turn at 13.5 m/s is 9.3 m round.
  - A kicker launches well under its own angle: 30 degrees at about 22, 36
    at about 27, 42 at 31-34.
  - The Moon Shot gives 1.9-2.1 s in the air, 8.2-8.6 m up.
  - In the air, D for 0.25 s yaws the car 48 degrees and 0.6 s yaws it 126.
    Q for 1.1 s rolls it right round. S for 1.0 s takes it about 290
    degrees round a backflip, and it lands nose-high on its rear wheels,
    upright.
  - The hip's landing swings a yawed car's path 55 degrees, at 30% of its
    speed.
  - The loop's natural line from the Moon Shot round the east end runs
    over the Mound, a 0.4 s hop. A scripted lap missed the Step-Up twice
    on mistimed turns: by hand, each leg is reachable.
- **Failed at first or not built**: the noon at 16,000 lux blew the playa
  white; every storm tried (0.25 to 0.9) greyed the desert, so no weather
  (#1735); round wheels and hubcaps showed no roll until a spinner went
  across each hub; the sanitiser raised three spin periods and two
  thicknesses (the builder now writes its values); the booth's legs ended
  flush with its deck (4 z-fights), saved before the CHECK line was read -
  `rec.py apply`'s output was filtered with grep, which hid its `zf` lines,
  and the save was chained with `;`; the show car at the arrival (no level
  ground in the arrival's view: 16-20 degrees of mesa wall).
- **Failed at first in the Pit**:
  - A quarter pipe (4 m radius, to 80 degrees) threw the car up to 74
    degrees and off over its top, 4.4 m clear.
  - The first Step-Up (30 degrees onto a 2.8 m deck) cased its face at
    full speed.
  - The hip's crest at 16.5 m caught the car only 2 m past it (moved to
    15).
  - A 19 degree, 4 m mound threw the car past its far face onto the flat
    (down at 9.4 m/s), and two domes never left the ground.
- **Code**: none; filed #1735. `b/crowd.py`'s build now runs under a main
  guard so `b/pit955.py` can seat its figures; it writes the same four
  crowds byte for byte.
- **Session over** (10:00, the owner in the terminal): Jink said goodbye in
  chat (nobody was in the world) and its agent stopped with nothing
  discarded. The owner never came into the world this session.

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

### 2026-10-02: session 895, live session 10 continued, #1523

- **Mode**: self-guided, on the owner's word in the terminal - continue
  #1523: build the region out, work the follow-ups, close what is done. The
  owner answered three questions at the start: raise the fleet's damping
  (#1534), soften the bump stop's rebound (#1535), no lap timing.
- **Built** (17 logged saves, 08:28-09:32; nobody visited): the pan as a
  playa; the arrival turned to bearing 25; the Mesa Drop; the windmill's
  head; the circuit tabletop from the tuned jump; all four stands, the
  scoreboard, the circuit floodlight, both trailers and the tarp turned to
  face what they serve (catalogue fronts are local -Z); 42 spectators; two
  dust devils; two windsocks; the ring of fire; white edge lines round the
  circuit; distance posts at every landing; body v2 (hubcaps, roundels).
- **Measured**: the Mesa Drop's first kicker (15 degrees, 0.6 m lip) flew
  0.6 s and never rose (the springs took the kick); at 20 degrees and 1.6 m
  it flies 1.23 s, 15.6 m, 3.35 m over the slope. The tabletop 0.70 s; the
  whoops hops of 0.07-0.23 s and 0.70 s off the last. On the new physics
  (14:25 build): the Jump Line 0.73 / 0.87 / 1.27 s, the Mesa Drop 1.24 s,
  all level, 94-97% kept.
- **Failed at first**: thin cracks (under two pixels of the 512 px bake)
  drew as zippers; the run-up arch's solid posts caught an off-axis car;
  the kicker's 1.8 m back face stopped a car climbing from the pan; walk-to
  lineups rolled 7-11 m past (#1536); the first dust devils rose as chimney
  plumes; the ring's first flames were too small to see; a line-up 3.4
  degrees off missed the Big One.
- **Code**: a builder (676k tokens, 3 h 46 min) fixed #1528-#1535; its
  critic (307k, about 67 min) found one high: the softer bump stop let a
  box reach the ground, and the terrain - a heightfield with no
  internal-edge fix - stopped such a car dead (65 of 176 bench landings for
  records published before #1534). Fixed at the root in the main session:
  #1538 (parry's FIX_INTERNAL_EDGES on the terrain and the bench floor:
  after it 0 dead stops of 176 on the new damping, 16 for records published
  before #1534, about HEAD's 14 on the fixed terrain); it also made the
  airplane slide 15.6 m after touching down,
  so its autopilot aims 14 m short (was 8). Also #1536 (walk-to braking),
  #1537 (the catalogue windmill), #1517 (test directories in /tmp). One end
  reviewer over that code (364k tokens, 46 min) found #1536's sibling - a
  car on `follow` drove through its player (fixed, with a test) - the
  windmill's vane through its new cap (raised 0.3 m, saved 15:34), an
  overstated figure and stale comments (fixed). Filed #1539 and #1540 (lows)
  and #1541 (a car sent to a point behind it ends `stuck`: its swing
  outlasts the 6 s progress rule; pre-existing).

### 2026-10-02: sessions 896 and 897, live session 11, #1523

- **Mode**: on the owner's word in the terminal. 896 (offline): they
  approved the light and found the wind too harsh; the cause read off the
  patch, two softer versions rendered for their ear, and parry's
  internal-edge fix put into bevy_symbios_ground's collider builder for
  their release (#1542). 897: they published bevy_symbios_ground 0.6.0 and
  chose the soft wind.
- **Built**: the soft wind, applied live and saved at 20:21 (Jink up two
  minutes to do it); the saved record is the one `wind_soft.wav` was
  rendered from, compared whole.
- **Code**: overlands builds its terrain and its physics benches' floor
  with the crate's builder again (#1542). The crate spans a map's samples
  times its scale, so a bench floor sized as before came out 8 m wider,
  and an airplane bench that flies out past the floor's edge came round
  differently and did not land in time; the floor keeps its old 512 m
  and asserts it.
- **Found**: the terrain collider is stretched against the drawn ground,
  in every room since it existed (#1543): 0.29 m off at 500 m out on a 0.3
  slope. On Parabola Flats' level pan it moves nothing; the Mesa Drop's
  landing, on a 15-19 degree slope about 110 m out, sits 6-7 cm off.

### 2026-10-02: session 900, the promo, #1545

- **Mode**: offline, on the owner's word: "make a promo-video for Jink and
  his region, similar to the ones you made for Reeve and Hypha, but this
  one more focussed on action and Jink driving stunts in his region."
- **Made**: `exports/promo/parabola-flats-promo.mp4` (69.3 s, with `-web`
  and `-ambience` copies): the car flying the Jump Line, the Big One
  through the ring of fire from three angles (two slowed), the Mesa Drop,
  the Circuit, the whoops and the Drift Circle, under eight cards and a
  synthesised score. Every stunt is flown on the game's own physics by the
  render tool's new `--driver` (#1546), from the saved room and avatar,
  with the keys this page's runs use: the tool flies the Jump Line in 0.75
  / 0.89 / 1.36 s against the 0.70 / 0.90 / 1.33 measured live.
- **Found**: a car's physics depends on the frame rate (#1548, fixed in
  session 912: every vehicle reads the physics' own pose now, and the
  render tool flies the Jump Line alike at any frame rate); a car
  threw no dust (#1549, fixed in session 902: the car throws dust from its
  wheels now, and thuds when it lands a jump); from the side the ring of
  fire is edge-on and vanishes - shoot it along the line.
- **The owner approved the promo** (2026-10-02) and committed the tool
  (f877a19).

## Open threads

- **Filed in session 895, open**: #1541 (medium: a car's walk-to of a
  point behind it ends `stuck` while it swings round - turn it first with
  `face` or `b/aim.py`), #1539 (a car on its side with its roof downhill on
  a side slope stays down), #1540 (the controls guard does not stop a key
  read beside `CAR_KEYS`).
- **#1735** (the owner's decision): a weather falls about half the time
  whatever its intensity; a share-of-time setting or a dust storm would give
  the desert the storms session 955 kept out.
- **Ideas not built**: a parked show car at the arrival - its foreground is
  16-20 degrees of mesa wall to the pan (session 955), so it would need a
  levelled pad; lights that light the ground at night (no node casts light:
  the floodlights only glow).
- **The Freestyle Pit, next** (ranked):
  1. Watch the owner drive it, and tune what they find: the Step-Up's deck
     edge and the hip's crest were placed from the offline runs alone.
  2. A rhythm section of rollers big enough to double (the circuit's
     whoops hop 0.07-0.23 s).
  3. A narrow raised beam to drive along, for precision.

  The Pit is not in the arrival's view (bearing about 70 from it, the
  camera looks 25), and it needs no berms: a full-lock U-turn is 9.3 m
  round at speed.
