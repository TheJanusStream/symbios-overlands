# Atrium and Strata

Atrium (@atrium-ai.bsky.social) shepherds Strata, a huge arcology stepped
like a ziggurat on a frozen plain: four walkable levels from the poor
undercity at its foot to the gardens and villas of its crown, joined by
ramps round a great central atrium open to the sky. Its body is a rigged
person of a moderate build, Strata's steward, in a knee-length teal coat
over a twill jacket, a visor that hugs the face and a steward's kit (insignia, tool belt, tablet,
shoulder pieces, a back unit that docks its survey drone, boots), the drone
orbiting over its shoulders. As of 2026-11-18, after its first session
(live 25): the land made flat, the first mood set, and Strata built whole -
its walkable shell and every level's inhabited heart, the Sump, the Works,
the Terraces and the Crown with its 149 m spire - each walked live and
saved; then the owner's first visit and five fixes they asked for in chat,
the body and then its drone reworked among them, and the world given its
own name and banner. Its second session (live 26) filled every level's
outer hall with districts in that level's wealth - the Sump's Cistern
Quarter, Stacks and Rag Town; the Works' farms, warehouses and power
hall; the Terraces' Market Street, library, park and homes - each walked
live (the world last saved 2026-11-19 04:17, seq 293 of session 1034's
daemon; the avatar 00:37, seq 339 of session 1032's).

## Start here

| | |
|---|---|
| Account | `@atrium-ai.bsky.social`, `did:plc:v6ftr6n6g54bbxmqpaeik3dv` |
| Commands | every agent command takes `--account atrium-ai.bsky.social` (the wrapper `exports/atrium/A` carries it); every tool call starts `AGENT_ACCOUNT=atrium-ai.bsky.social` ([../../session.md](../../session.md)) |
| Admin | `@codewright.bsky.social`, the owner: `start --admin @codewright.bsky.social --allow-save` |
| Region | Strata, its own world (`did:plc:v6ftr6n6g54bbxmqpaeik3dv`) |
| Working folder | `exports/atrium/` (gitignored, on this machine only), kept across sessions: `A` the wrapper; `b/avatar.py` the body's dress and the drone (session 1032's first body), `b/avatar2.py` the body as reworked (the sculpt, the refitted visor, the kit; it takes the drone from `b/drone2.py`, the rebuilt drone and its flight, and the coat's four items from `b/coat.py` (live 27), reads `src/avatar.json` and writes `gen/avatar2/`: `sculpt.json`, `worn.json` (item, socket and generator file per worn slot), one generator file per item and `1032/avatar2/test.json` for `render --rigged`), `b/motion_sim.py` (a copy of Kairos's motion sampler; the drone's sweep is `1032/drone/tools/sweep.py`), `b/markers.py` socket markers (from Reeve's folder), `b/landscan.py` the flat-centre seed scan; the shell `b/strata_common.py` (every dimension and material), `b/shell_*.py`, `b/shell_build.py` writing `b/shell.edits`, its check `b/shell_check.py`; the Sump `b/sump_*.py`, `b/sump_build.py` writing `b/sump.edits`, its check `b/sump_check.py`; the Works `b/works_*.py` (`b/works.edits`, `b/works_check.py`); the Terraces `b/terr_*.py` (`b/terraces.edits`, `b/terr_check.py`; `b/terr_garden_fix.edits` the re-centred seedling rows); the Crown `b/crown_*.py` (`b/crown.edits`, `b/crown_check.py`); `b/arrival_fix.edits` the moved portal, monument and banner and the landing's pitch -4; the owner's fixes `b/bench_fix.edits`, `b/bench_fix2.edits`, `b/bench_fix3.edits` (the three bench models), `b/lounger_fix.edits`, `b/rails_fix.edits` (the sheared ramp rails: `strata_well1-3`) and `b/gate_uv_fix.edits` (the gate and door panels), each setting whole generators the fixed builders write (the applied edits files have their `/placements/-` lines rewritten to the indices they landed at: re-running one sets, never appends; re-running a `*_build.py` writes appends again, so set single placements by index instead); `b/banner.py` the world's name, banner and steel standard (`b/banner.edits`: `/identity`, `/generators/world_banner`, the standard's spot); the outer districts of session 1034 (live 26): the Sump's `b/sumpout_*.py` (`b/sumpout_build.py` writing `b/sumpout.edits` and `gen/sumpout/`, its check `b/sumpout_check.py` with the routes in `b/sumpout_routes.py`, a top-down plan tool `b/sumpout_plan.py`), the Works' `b/worksout_*.py` (the same shape: `b/worksout_build.py`, `b/worksout.edits`, `gen/worksout/`, `b/worksout_check.py`, `b/worksout_routes.py`, `b/worksout_plan.py`), the Terraces' `b/terrout_*.py` (the same, with `b/terrout_account.py`; `1034/terrout/run.sh` rebuilds, composes and checks; `b/terrout.edits.applied` the applied file with its indices, `b/terrout_deckfix.edits` the pavilion's lowered deck - a re-run of a `*_build.py` writes appends again, so keep the applied file as `b/<name>.edits.applied`, restore it after the rebuild and set the changed generator and placement by index as that file does), `b/sound.py` the outer districts' sounds (`b/sound.edits`: six generators, each from its builder's own output with a sound copied from one the world had), `b/trim.py` the torus trim (`b/trim.edits`: the fire barrels and the standard; run it again after any re-run of `b/sump_build.py`, which writes the Catalogue's barrel fire back); `b/water.py` (live 27) Strata's eight pools in one generator `strata_water` placed at identity, each pool a direct child of its identity root carrying its world transform - the engine answers a nested pool's physics at its LOCAL transform (#1949; the owner swam on the Sump's dry floor at the well's foot) - and stripped from their pieces: re-run it after any rebuild of the cistern (after `b/sound.py`); `b/water2.py` its second pass - a water plane has no bottom, so pools at their true places flooded the levels under them (the owner swam on the Sump's avenue under the Crown's axis pool): `strata_water` keeps only the ground-level cistern, and the seven upper pools are back in their pieces, each wrapped in a hidden near-black 1 cm cube 10 cm under its surface so it registers only at the origin at level 0.1, under every floor (water to no one); re-run it after rebuilding the park, the canal, the villa or the axis pool; the live-session patch scripts of live 27, each editing live generators by path (re-run after rebuilding what they patch, in this order: `b/trim.py`, `b/sound.py`, `b/water.py`, `b/water2.py`, `b/steam2.py`, `b/chimney.py`, `b/flame.py`): `b/steam2.py` steam born inside what it rises from (the cooling vent, the canteen pots, the soup line's and the kitchen's pots; the floor grates' sunk emitter is in `b/sump_lit.py` itself), `b/chimney.py` the well foot kitchen's hood, stovepipe and cap with the steam out of the chimney, `b/flame.py` the fire drums' Flame flipbooks (4 x 4 at 14 fps) and their lights at the strongest flicker allowed (0.29 at 2 Hz; #1950 asks for more); the moving life `b/motion_*.py` (`b/motion_build.py` writing `b/motion.edits` and `gen/motion/`, its check `b/motion_check.py RECORD [BASE] --samples N`, a clearance sweep of every moving part); the last saves: the world `src/room.json` (2026-11-19 11:47, seq 73 of session 1035's daemon: the moving life; seq 52 at 11:37 the kitchen's chimney; seq 49 at 11:32 the fire lights' flicker, 47 at 11:29 the flame flipbooks, 45 at 11:25 the steam born inside, 43 at 11:24 the grates' sunk steam, 39 at 11:18 the upper pools wrapped; seq 31 at 11:04 the pools moved into `strata_water`; seq 28 at 10:54 the turning day; seq 24 at 10:39 the cistern's sludge mats; seq 295 of session 1034's at 05:11 two more sounds and the trim; seq 294 at 04:23 the first sounds and the cistern's pipe caps; seq 293 at 04:17 the Terraces' outer districts; seq 189 at 03:17 the Works', seq 84 at 02:18 the Sump's; before them seq 336 of session 1032's at 00:14, the name and banner; seq 310 the gate seams, 296 the rails, 294 the sun-beds, 291 the benches, 288 the Crown and the arrival, 172 the Terraces, 108 the Works, 37 the Sump, 24 the shell, 2 the land), the avatar `src/avatar.json` (2026-11-19 10:22, seq 14 of session 1035's daemon: the drone's frame cube hidden; seq 11 at 10:18 the tablet's screen turned in; seq 8 at 10:14 the coat and the gripped tablet; seq 339 of session 1032's at 00:37 the rebuilt drone, seq 318 at 23:54 the rework, seq 4 at 17:33 the first body), the inventory 10:14 (seq 7: fourteen Traveler's Satchels, `_1` to `_13` and the unsuffixed first, the worn items' sources); the seeded records `src/room_seeded.json` and `src/avatar_seeded.json`; the save log `log.md`. `1032/` is the first session's scratchpad (briefs, renders, records before each step), read-only |
| Last session | chainlink session 1035 (live session 27), parent #1948 - read its comments (`chainlink show 1948`): the coat, the owner's hints and fixes, the water bugs (#1949), the flicker (#1950), the moving life; before it 1034 (live 26, #1947: the outer districts) and 1032 (live 25, the first, #1946: the owner's decisions and the first concept) |
| Default mode | [self-guided](../../session.md#self-guided), building Strata level by level in the order a visitor climbs |
| Never | touch Hypha's world (the Understory) or avatar ([hypha/](../hypha/README.md)), Reeve's (Ashmere, [reeve/](../reeve/README.md)), Jink's (Parabola Flats, [jink/](../jink/README.md)), Eigen's (Isoline, [eigen/](../eigen/README.md)), Catoid's (Purlieu, [catoid/](../catoid/README.md)) or Kairos's ([kairos/](../kairos/README.md)); close a live session's parent before the owner has committed its work |

First, every session:

- **Read the concept on #1946** before building: the levels' names,
  heights and footprints are fixed there, and every builder keys on them.
- **Budget by level**: the world's whole record holds about 900 KB; the
  shell takes its share once, then each level about 150 KB. Measure
  `status.editing.live_sync` and `--triangle-report` before and after each
  piece.
- **Walk what you build** with the person body (`A walk-to X Z`, ramp by
  ramp): "fully walkable" is the owner's word, and only a live walk proves a
  ramp or a sill. `walk-to` runs a straight line and stops about 1 m
  short: route legs round the columns (every enclosed level has one on
  each 30 m grid point at +-15, +-45, +-75 ... outside the well's 30 m;
  L0's 1.8-2.1 m square, L1's 1.7 m, L2's 1.6 m round) and keep a leg
  1 m off any solid edge; the builders' check routes (`walk(...)` lines in
  each `b/*_check.py`) are lines, not capsules.

## Who Atrium is

Named for the great atrium at Strata's heart, the light well every level
looks into. Atrium is Strata's steward: it keeps the arcology walkable from
the Sump to the Crown, knows what each level lacks, and builds where the
arcology needs it, from the cramped stalls of the undercity to the crown's
gardens. In chat: calm, precise, a little formal, with a builder's pride in
a well-made ramp; it calls each level by its name. The owner chose the
handle on 2026-11-18 from four offered (Insula, Atrium, Plenum, Domus).

## The body

A rigged person, the seed's own (age 58, 1.78 m), renamed Atrium, of a
moderate build since the owner's review (live 25: "the body shape is rather
extreme and visor has too large of a radius, flying in front of the face.
It could also make use of more attachments, preferrably detailed and
thoughtful").

- **Sculpt** (`b/avatar2.py` `SCULPT`): two composites changed, nothing
  else - `/body/composites/mass` 1801 to -150 (a little under the middle)
  and `/body/composites/bodyFat` 511 to 200 (0.20, under the 0.245 the
  crate centres a body of this mass, frame and age on). The seeded
  archetype's chest, projection and hip values are kept: at this body fat
  they read flat. The CLI takes a negative number for a flag: set these
  with `--file` (`gen/avatar2/mass.json`, `bodyfat.json`).
- **Dress** (`b/avatar.py`): the engine's close-fitting outfit only, a top
  and trousers: a teal twill jacket with full sleeves over charcoal twill
  trousers. A garment's colour is on the avatar's 1000 = 1.0 scale and
  multiplies its texture, so the colour is white and the weave carries the
  teal and the charcoal.
- **Worn items**, ten, in slots 0-9 (`gen/avatar2/worn.json`, built by
  `b/avatar2.py` against frames measured with ruled markers on this sculpt,
  its docstring lists every frame; each item's root is a 1 cm cube inside
  the body):

  | Slot | Socket | Item |
  |---|---|---|
  | 0 | `face` | the visor: two smoked lens arcs meeting in a shallow V over the nose bridge, 4 mm off the brow and 3-8 mm off the face to the eyes' outer corners, a cyan lower edge, copper hinges and temple arms back toward the ears |
  | 1 | `crown` | the survey drone (`b/drone2.py`, live 25 00:37): a 27 cm craft - a pale faceted lathe hull (224 triangles where a smooth one cost 1,920) with a teal canopy, a copper waist band, a lit cyan sensor bar, a swept fin and an antenna; four ducted fans on copper arms, each shroud's top rim lit cyan and its two-blade prop spinning 1,440 deg/s (the record's ceiling; diagonal pairs opposite); a camera gimbal under the nose; red (port) and green navigation beads and a white tail bead; a red beacon on the spine swelling every 1.2 s, carrying the drone's one light (range 0.8 m, lit at every hour); a copper docking ring on four legs that sits in the back unit's cradle. 14,852 bytes, 2,264 triangles, 50 nodes 9 deep |
  | 2 | `chest` | Strata's insignia: a stepped ziggurat in gold on deep teal enamel |
  | 3 | `hips` | a tool belt: buckle, measuring reel, pouches, a holstered survey scanner |
  | 4 | `left-hand` | a holo-tablet showing Strata's levels in section, gripped since live 27 (the owner: "stuck to the outside of your hand instead of being held by the hand"): the slate in the fingers' own plane, its top third between the fingers and the thumb (`b/avatar2.py` `TAB_C` (0.0, -0.095, -0.058) in body axes from the socket; it was (0.028, -0.075, -0.068), against the back of the fingers), its screen turned in toward the body (the owner: "the screen should be towards the avatar's body"; `face = cx - TAB_T / 2`) and its copper edge band centred on the case (set 1 mm back, its back face lay in the case's: a z-fight the owner saw) |
  | 5, 6 | `left-shoulder`, `right-shoulder` | copper-rimmed shoulder domes with rank lights |
  | 7 | `back` | a back unit with an antenna, status lights and the drone's charging cradle |
  | 8, 9 | `left-foot`, `right-foot` | boots |
  | 10 | `chest` | the coat's standing collar, open at the front, a copper band round its top, copper piping down each side of the jacket front to 2 cm above the belt (the jacket is the coat's body) |
  | 11 | `hips` | the coat's yoke, under the tool belt (its top inside the belt band's wall), flaring over the hips to 0.21 m below the socket; open front, piped edges, dark teal lining |
  | 12, 13 | `left-hip`, `right-hip` | the coat's half-skirts to the knee (hem 0.51 m up), their tops tucked 12 cm up inside the yoke; open front edges in copper, the back edges meeting as the vent, a copper hem band, dark lining |

  The coat (live 27, `b/coat.py`) is in the jacket's own twill (the warp
  and weft of `b/avatar.py`'s outfit), each panel ONE hollow lathe with
  per-face materials - the bore the lining, the cut and hem faces copper
  (two lathes a panel z-fought: the engine closes a lathe's cut with flat
  faces to its axis). 14 worn items of the engine's 16
  (`MAX_AVATAR_ATTACHMENTS`); the coat adds 4,664 triangles (the whole
  avatar 44,426) and 16.7 KB over four records.

  The sources are the inventory's fourteen Traveler's Satchels
  (`Traveler's Satchel`, `_1` to `_13`), stashed from the Catalogue, worn and given these
  generators and sockets (avatar.md, "Dressing a rigged person"): to rebuild
  one, `avatar set /worn/N/item/item --file gen/avatar2/<item>.json`, then
  `/worn/N/item/socket`. The first body's visor (a hollow cylinder kept to
  0.4 of a turn, about 2.6 cm off the brow) faced 18 degrees right of ahead:
  the `path_cut` yaw that faces a person's `face` forward is 90, not 72.
- **The drone's flight** (`b/drone2.py`, one node a motion, every frame a
  1 cm cube with no material hidden in the head or the hull): the hub at
  (0, -0.10, -0.095) from `crown` - over the head's middle, which is about
  9.5 cm behind the socket - turning +40 deg/s (+X toward -Z, a lap in
  about 9 s) plus a wobble of 14 degrees over 11 s, and since live 27
  carrying the rise and dip too (on a height node of its own under the
  radial one, the slide carried the drone past the radial node's cube,
  which the owner saw flying through the hull); the radial node 0.42 m
  out and 0.235 m up, sliding along +X 0.075 m over 7 s plus 0.012 m over
  4.6 s (0.33 to 0.51 m out, 7 s no divisor of the lap, so the path
  precesses); the hub sliding along +Y 0.045 m over 5.2 s plus 0.012 m
  over 3.7 s; the yaw node, nose to the hub's -Z, swinging 12.9
  degrees over 7 s at phase 270 so the nose follows the widening and
  closing path; the bank node rolled 16 degrees with port (the head's side)
  low, swinging 5 degrees over 7 s plus a 2 degree buffet over 2.9 s (21
  degrees as the orbit closes, 11 as it widens); the pitch node 8 degrees
  nose down with a 2.5 degree wobble; the gimbal panning 55 degrees over
  8.3 s plus 25 over 13 s and tilting from 25 degrees down by 18 over 5.9 s
  plus 12 over 9.7 s. Its rest pose is what Effects Off shows (Off stills
  turns as well as slides and swells): off the right shoulder, banked,
  facing ahead. Proved from renders, not reasoning: from overhead the nose
  leads at two places a quarter lap apart and red is on the head's side
  (`1032/drone/v_proof_top_z.png`); from behind, the port side is low
  (`v_proof_back_z.png`). A 6,016-moment sweep (`1032/drone/tools/sweep.py`
  over a copy of Kairos's sampler, `b/motion_sim.py`) against measured body
  boxes: least clearance 4.9 cm standing and 4.6 cm walking and running,
  a front duct to the head's box; never below the crown.
- **A body prim is held to 1 cm** on every dimension and `hollow` to 0.95:
  `render --rigged` names each place it raises.
- See it offline: `render --rigged 1032/avatar2/test.json --size 1024`
  (`b/avatar2.py` writes the file from `src/avatar.json`, sculpted and
  dressed).

## The region

### Concept

Strata, an arcology on a frozen plain, centred on the map (1057 m across): a
stepped pyramid, every level reached by ramps (at most 12 degrees) round THE
ATRIUM, a 60 m light well at the centre open to the sky down to the lowest
floor. L0 THE SUMP, the undercity, 480 m square, floor 0.3, roofed at 16 m:
dark, cramped, shacks, stalls, pipes, steam, a few flickering lights; the
poor look up the well at the rich. L1 THE WORKS, works and market, 360 m,
floor 18. L2 THE TERRACES, residences, 240 m, floor 36. L3 THE CROWN,
open-air on L2's roof at 54: gardens, water, villas and a spire in the sun.
Each enclosed level's outer terrace is the roof ring of the level below.
The owner's words: "a huge futuristic arcology, spanning most of the region,
with a building-focus in the indoor environments ... multiple levels, fully
walkable. The lowest level should be a bit dystopian and poor and the higher
the levels go, to more rich they are."

### Land, water and sky

Flat since live 25: the mesa recipe with ONE terrace
(`base_terrain` `voronoi_num_terraces` 1, `seed` "7", `height_scale` 30 m,
120 thermal iterations): ground 0.0 everywhere, the sea at -5 m, the seeded
icy ground layers kept (Ice, Ground, Rock, Snow). Seed scans at 6, 10, 14 and
50 cells with two terraces never gave a 400 m single-level centre
(`b/landscan.py`), so the mesas went. The sky: since live 27 (the owner's yes, 10:53) a day that turns once every two
real hours, a world hour every five minutes (`/environment/time_of_day`
`{"hour": 166878, "day_minutes": 1200000}`: the hour anchor kept 15:30 on
screen at the change - before it a standing 15:30, `{"hour": 155000,
"day_minutes": 0}`), with snow in 12-minute spells a quarter of the time at
0.45 (`/environment/weather`); the seeded noon fields and neutral icy fog
kept. At night the arcology shows its lit windows, neon and the reactor.

### Arrival

On the ice 50 m south of the Sump's south gate, `/default_landing` at
(0, 290) facing north (`yaw_deg` 0), its camera looking 4 degrees UP
(`pitch_deg` -40000), the account's own gateway (-11, 268) and monument
(10, 270) flanking the gate in the first frame, the world's standard
(14.5, 252, scaled 1.3) between the gate's right post and the monument,
shacks huddled against the wall either side of the gate, the neon avenue
glowing through it and the Crown's spire standing whole over the wall
(`1032/banner/arrival_c.png`, rendered with `render --world ... --focus
arrival --width 1920 --height 1080`, the game's own camera, which sits
about 1 m behind the landing; `views.py`'s `@landingcam` always looks 20
degrees down). At pitch 0 only the spire's saucer peeked over the wall;
looking up sets the orbit camera near the ice and closer to the body, so
the three pieces moved 20-28 m ahead to stay in frame - where the monument
hid the standard whole until it moved again (live 25, 00:14).

The world is named **Strata** (`/identity`, shown at the top of a
visitor's screen on arrival; the seed had named it "Lyra Outpost"), its
banner per chevron - the ziggurat's outline - in teal (0.08, 0.42, 0.45)
over copper-gold (0.92, 0.68, 0.30) with a tower for its mark. The
standard that flies it is the seeded catalogue piece restyled
(`b/banner.py`): dark steel plinth (buried in the flat ice) and crossbar,
a brushed-steel mast, copper bands and rings, a cyan finial and two cyan
beads; its sign is `network.symbios.sign.world_banner`, so it shows
whatever `/identity` holds.

### Places

| Place | x, z | What it is | Session built |
|---|---|---|---|
| The south main gate | 0, 240 | 24 x 12 m rusted portal into the Sump, an 8 m apron | 1032 |
| The market avenue | 0, 236 to 34 | the Sump's 16 m column-free street: stacked containers 2-4 high, stalls, kiosks, neon blade signs, cables of bulbs, pipes with steam and a drip, a STRATA advert at (0, 112) | 1032 |
| The well's foot | 0, 30 to -30 | the atrium's floor under the light column: soup kitchen (x 16-21.6, z 6.5-16), tap (3.5, 8), fire circle (-6, 2), sleepers, shacks round the east and north edges | 1032 |
| Alleys | E180 to x 70, W120 to x -76, E60 to x 48 | shanty lanes off the avenue, washing lines, bulbs | 1032 |
| The Cistern Quarter (L0) | x -75 to 75, z -30 to -239 | the north half: the cistern pool (x -28 to 28, z -126 to -56, the columns standing in the water) with a pump gantry at z -91 (flywheel, steam, two sodium lamps over the water), four pump houses at (+-60, -60) and (+-60, -120), risers and mains into the ceiling, container homes and a catwalk; floating mats of algae, sludge and grey foam over about a quarter of the cistern's water (live 27: they break the pale sky sheen the roofed pool showed); the scrap market on the north way (x -4 to 4 kept clear) with heaps and a derrick whose magnet swings; the scrap chapel at (-30, -210), door east, rose window, bell, neon cross | 1034 |
| The Stacks (L0) | x 75 to 239, the east gate street z -6 to 6 | container homes 3-4 high fronting the street, catwalks and ladders, laundry, bulb cables across at 5.7 m, a neon blade sign at (122, 7.3), container-wall blocks (a container-sized Brick texture) in the cells either side | 1034 |
| Rag Town (L0) | x -80 to -239, the west gate street | tarp and plank shanties, tents, four burning drums at the cross streets, the walk-in clinic tent at (-120, -10), the soup line at (-150, 8), shanty blocks either side | 1034 |
| Gate yards (L0) | inside the E, W and N gates | checkpoint booths at E (235.5, 9.2), W (-235.5, -9.2), N (9.2, -235.5) with lit amber signs and raised booms, shacks backed onto the wall | 1034 |
| The Atrium | 0, 0 | the 60 m light well; ramps R1 (west then north walls, L0 to L1), R2 (east then south, L1 to L2), R3 (west then north, L2 to the Crown), two 10-degree flights each; level landings NE1 (L1, 18), SW2 (L2, 36), NE3 (Crown, 54) | 1032 |
| The lift towers (the Atrium) | (-16, 5), (14, -13), (6.5, 17.5) | three steel lattice towers from the well's foot to a machine house over the Crown (58.6-61.8 m, a red beacon), each with a lit teal car riding up and down - slowing, dwelling about 5 s at each level (stops 0.45, 18.30, 36.15, 54.00), moving on - its yellow counterweight passing the other way, its cable following; visual only, no bridges to the levels (`motion_lift_frame` an instance set, solid; `motion_lift_cars`) | 1035 |
| The cargo drones (the Atrium) | over the well, 12-48 m up | five: two on a hub turning a lap a minute at 22 and 30 m, three on chords (x -4 at 12 m, z -5 at 40 m, the diagonal at 48 m) turning about at each end; pulsing beacons (`motion_drones`) | 1035 |
| The Freight Line (L1) | over the Freight Aisle, z 47 to 162, y 31-32 | a suspended monorail beam on hangers from the slab, two hanging terminal sheds with lit bands, a two-car teal train running out of one shed into the other at 5 m/s, a train every 20 s, 8.7 m over the aisle (`motion_tram`) | 1035 |
| The Rim Promenade (L1) | ring 30.4 to 36.5 m round the well, y 18 | lamps in planters, benches, coin telescopes | 1032 |
| The Exchange (L1) | x -13 to 43, z -48 to -73 | covered market of 32 lit stalls, a canteen (x -13 to 2.5) with menus and pendants, an info pylon at (23.2, -59.85); entrance at (23.2, -48.6) | 1032 |
| The forecourt (L1) | x 1 to 41, z -36 to -44 | a waiting shelter with a chevron ticker (x 1 to 11), kiosks at (20.5, -41.5) and (-8, -41.5), the Works' sign pylon at (41, -36) | 1032 |
| The gantry yard (L1) | x 48 to 72, z -17 to -40 | a crane traveling on rails, its trolley and a hoist on a swelling cable, containers | 1032 |
| The conveyor shop (L1) | x 48 to 72, z -50 to -73 | a belt of crates under a stamping press, picker arms, a cooling unit with fan and steam; its back half is reached round the belt's west end, a 2 m passage between the open west side (x 48) and the infeed box (x 50.0-51.6, z -62.1 to -60.9): walk it at x 48.8-49 (walked live, 00:15 on 2026-11-19); at x 49.5 the capsule meets the box | 1032 |
| The fabrication cell (L1) | x 78 to 102, z -50 to -73 | a drone chassis on a turntable, four welding arms with flickering tips, gate at (79.5, -61.5) | 1032 |
| The cargo depot (L1) | x 78 to 102, z -17 to -40 | a shed on a dock with roller doors, a lit drone pad, circling drones | 1032 |
| The Works' east terrace | out of the door at (180, 0) | benches and telescopes over the plain | 1032 |
| The Hydroponic Farms (L1) | x -38 to -179.5, the Farm Avenue at z -5 to 5 | the farm gate at x -38.5 (a sprout emblem in a magenta ring), grow racks in tiers under magenta strips either side of the avenue, six rack fields with walkways at z +-24.1, a tank yard and the pump booth at (-53.2, 29.5), the packing shed at (-58, -31) with a belt, grow lamps, misting lines, sealed grow halls further out | 1034 |
| The Warehouses (L1) | z 40 to 179.5, the Freight Aisle at x -5 to 5 | an amber board over the aisle, a loader lane (x 5.6 to 8.8) of five shuttling loaders, the dispatch office at (25.5, 60), the open store at (30, 120) with racked aisles, the freight lift at (-27, 90) carrying a container 0.1 to 13.1 m every 26 s, storage blocks with roller doors, container yards | 1034 |
| The Power Hall (L1) | north of the heart (z under -80) and east of it (x over 105) | the reactor at (0, -120): two counter-turning cyan rings round a banded core, a guard wall, the heart's hum; the control room at (28.5, -120) facing it; eight fenced transformer yards, switch houses, cable trays 8 m up, bolt boards | 1034 |
| The Rim Walk (L2) | ring 30.4 to 37 m round the well, y 36 | lanterns, trees in limestone planters, benches, flower troughs | 1032 |
| Fountain Square (L2) | x -43.5 to -16.5, z 37 to 73.5 | honey paving, the fountain at (-30, 57) with the Catalogue's jet and water sound, benches | 1032 |
| The Square Row (L2) | (0, 60) | shops and flats (grammar), a cafe terrace at x -10.5 with parasol lanterns and a swinging sign, a fruit stand | 1032 |
| Apartment blocks (L2) | A1 (-60, 30), A2 (-60, 0) and nine more west and south | 2 to 4 storeys, one grammar generator with variants: lit windows, planted balconies, doors with lamps | 1032 |
| School and clinic (L2) | school (-30, 94), clinic (0, 94) | the school's yard with a gate clock and a turning roundabout; the clinic's green cross at (6, 85) | 1032 |
| The terrace garden (L2's terrace) | outside the south door, z 120.6 to 176.6 | raised beds, fruit trees, a greenhouse with a swinging vane, a pinwheel, benches over the plain | 1032 |
| The Market Street (L2) | the north door's axis, x -7 to 7, z -37 to -117 | a lit gateway at z -38.6, 16 shopfronts with lit windows, fascias and awnings under cream upper storeys, the florist (east row) and the bakery (west row) with blade signs, lanterns, benches, cafe tables, a plaza at the north door | 1034 |
| The library (L2) | x -38 to -22, z -51.5 to -68.5, its square to z -37 | a portico of four columns with a lit name bar, warm tall windows, a walk-in reading room (doorway x -31.3 to -28.7): shelves on three walls, reading tables with green lamps, a librarian's desk | 1034 |
| The park (L2) | x 50.5 to 99.5, z -39.5 to 9.5, on the east street to the east door | lawns either side of a walk (x 72 to 78), a long pond (x 54 to 96, z -36 to -27) with lily pads, a timber pavilion at (88, -20) whose deck is 3 cm over the lawn, hedges, benches facing the water, lanterns | 1034 |
| The outer homes (L2) | 16 cells north-east, south-east and north-west | lighter house blocks (`terrout_house3`, `terrout_house4`: one solid cuboid, a grammar drawing panes, balconies, a door and lamp) with front gardens, and two `terr_block` variants facing the park at (60, 30) and (90, 30) | 1034 |
| The door belvederes (L2's terrace) | outside the north, east and west doors, 166 to 176 out | a paved way from the door, an 18 x 10 m belvedere, a lantern, four benches facing the plain, a tree | 1034 |
| The Spire (Crown) | (66, 0) | a marble podium, a pearl shaft to 140 m, leaning fins with glowing edges, three halos, two counter-turning rings, a saucer sky deck at 115.5-122.5 m, a needle and a red pulsing beacon at 148.9 m; uplights and lanterns round its plaza | 1032 |
| The Long Walk (Crown) | x 30.4 to 54, z -31.5 to -23.5 | glowing bollards from the NE landing to the spire's plaza | 1032 |
| The Canal Garden (Crown) | x 44 to 100, z -117 to -36 | a reflecting canal (water in a marble box) on the spire's axis, a fountain at its head with the Catalogue jet and water sound, lawns with yews, cypresses and urns, a pergola, pines | 1032 |
| The knot garden (Crown) | x -10 to 10, z -72 to -52 | a clipped hedge square and diamond, an urn, roses | 1032 |
| The Grove (Crown) | x -115 to -58, z -115 to -48 | pines, the sculpture "Ascent" on a lawn at (-88, -80) ringed by cypresses | 1032 |
| The landing pad (Crown) | (-80, 0) | a lit pad with a private skycraft, a windsock turning downwind | 1032 |
| The villas (Crown) | West (-70, 78), East (70, 78), North (-40, -86) | one generator three times: a walk-in marble and glass hall with a fire, a timber upper volume with lit windows, a garden with a plunge pool behind | 1032 |
| The Sky Lounge (Crown) | x -14.5 to 14.5, z 92.5 to 107.5 | a white roof on marble columns, sofas, a fire bowl, a bar, the view south | 1032 |
| The south axis (Crown) | x -7 to 7, z 62 to 86 | a reflecting pool with the turning "Orrery", marble sails, flower terraces either side (its curb stops a walk: go round by x 12.5) | 1032 |

### Life and sound

The Sump: 43 lights on glowing parts (flickering amber and pulsing cyan
neon, bare bulbs, fire drums), a turning fan, a ticker belt, steam from
grates and a pipe, a falling drip, the neon hum of the kiosks and signs, the
fire drums' crackle. Snow falls into the well's foot from the open sky.

### Budget

The world 2026-11-18 22:39 (seq 288): live sync 557,459 of 921,600 bytes
(60 %: the shell about 96 KB, the Sump 112 KB, the Works 135 KB, the
Terraces 91 KB, the Crown 117 KB; about 364 KB left), the largest record the
manifest at 59,510 of 102,400; after the owner's fixes (23:28, seq 310)
562,793 (61 %: the sheared rails and the gate panels' face overrides about
5 KB). Live 26, the Sump's outer districts (2026-11-19 02:18, seq 84):
647,862 (70 %), triangles on the placements 338,336, parts 7,986 (the
tightest budget: an instance set counts every prim of every copy), the
Sump's lights 63, the manifest 70,306. The Works' outer districts (03:17,
seq 189): 736,748 (80 %), triangles 358,308, parts 9,001, the Works'
lights 67, the manifest 80,167 of 102,400 - every placement lives in it
(about 214 bytes each), so placements, not bytes, are the next ceiling.
The Terraces' outer districts (04:17, seq 293): 806,560 (87.5 %; about
115 KB left), triangles on the placements 396,846 (with the ground about
919,000), parts 9,688, the Terraces' lights 62 (Strata about 239 in all,
the nearest 64 drawn), the manifest 82,592 of 102,400 (about 20 KB, some
90 placements, left) - its 16 placements held the reused heart pieces in
one shared set each across districts. Live 27's moving life (11:47, seq 73): 861,508
(93.5 %; about 60 KB left), the manifest 83,454, triangles about 397,600,
parts about 9,930 - the budget's last tenth. After the trim (05:11, seq 295,
`b/trim.py`: the fire barrels' and the standard's rings from 28 x 10 torus
segments to 16 x 6) the placements draw 389,118 triangles (911,360 with
the ground). Parts (`--triangle-report`, copies one by
one): the shell 2,479, with the Sump 4,081, the Works 5,129, the Terraces
6,034, the Crown 6,783; triangles on the placements about 310,000 (the
Terraces' blocks 82k and soft-blob trees and greens about 60k: the place to
trim if the web strains), the ground 522,000. Lights: the Sump 43, the Works
45, the Terraces 44, the Crown 47 (the nearest 64 drawn). The
avatar (rigged, ten worn items) 35,264 bytes as compact JSON of the pulled
`{record, body, worn}` (75,068 as `rec.py` writes it, indented; 16,822 before
the rework); each worn item is its own record, the largest the drone at
14,852 of 102,400 since 00:37 (the belt next, 5,286). Every prim, world or body, is held to at least 1 cm on
every dimension.

## Standing decisions

- 2026-11-18 (terminal, #1946): the theme - "a huge futuristic arcology,
  spanning most of the region, with a building-focus in the indoor
  environments ... multiple levels, fully walkable", the lowest level poor
  and a bit dystopian, the higher the richer; the handle @atrium-ai.
- 2026-11-18 (terminal, #1946, "I go with your choices and
  recommendations"): the avatar is a person, Strata's steward, in a long
  coat with a survey drone circling its shoulder; visitors arrive at the
  arcology's foot in the undercity and climb; four levels stepped back like
  a ziggurat round a central atrium open to the sky, ceilings tall enough
  for the play camera.
- 2026-11-18 (chat, live 25, the owner's first visit): the body is of a
  moderate build, its visor close to the face, and it wears "more
  attachments, preferrably detailed and thoughtful"; the drone circling it
  "could be much better" (23:58). Benches' backrests lean back, sun-beds'
  head-rests rise from the bed, ramp rails end vertical (shear), and a
  gate's or door's panels carry the wall's texture on without a seam.
- 2026-11-19 (chat, live 27, 10:24): "Work on your avatar is complete for
  now. It looks very good." - the body as it stands (the coat, the
  gripped tablet with its screen in, the drone) is accepted: no further
  body changes unasked.
- 2026-11-19 (chat, live 27, 10:53): a turning day of about two hours ("Yes.
  That sounds good.") - the mood is the owner's; Strata's day now runs
  through night, its lights the show. Also (10:37): the column fields may
  stay instance sets while the manifest has room; four grids a level round
  the well are the owner's suggested saving if it runs short.

## Working material

`exports/atrium/`: `A` the wrapper; `b/` the builders (`avatar.py`,
`markers.py`, `landscan.py`); `gen/` their output (`gen/avatar/` the outfit,
visor and drone); `src/` the pulled records - the truth, compared with a
fresh `rec.py pull` before building on them; `1032/` the first session's
scratchpad (briefs, renders, the seed scans).

## History

- **1032 - 2026-11-18, live 25, #1946, first session.** The owner made the
  account (the handle chosen from four offered) and gave the theme; signed
  in on the first link. The seed rolled a person in an icy, snowy world.
  Cleared the seeded world, made the land flat and set the first mood (saved
  17:23); dressed the body as the steward (saved 17:33). Strata's shell by
  one offline builder (363k tokens, 34 min), applied clean and walked live
  from the landing up all six flights to the Crown, every step arriving at
  its floor's height (saved 18:17); the Sump's heart by a second builder
  (376k, 34 min), walked live down the ramps and along the avenue (saved
  19:03); the Works (438k, 44 min, saved 20:14), the Terraces (393k, 37
  min, saved 21:20) and the Crown with its spire (373k, 38 min, saved 22:39),
  each walked live along its builder's tested routes; the arrival raised to
  look up at the spire (pitch -4) with the portal and monument moved ahead
  into the frame. No reviewer: world content with thorough numeric checks,
  and the live walk as the test. Failed at first: three spin phases of 450
  and 540 degrees clamped to 360 (set to 90 and 180); the greenhouse's
  seedling rows pulled to z 100 by the blob element clamp (re-centred); my
  own straight walk legs through a market's stalls and round a fountain in
  four points (walk-to does not path-find); the arrival's pitch 0 hid the
  spire, and looking up dropped the near portal and monument out of frame.
  Before that:
  the outfit colours written on the 10,000 scale were clamped to white
  (the avatar's scale is 1000), and the visor's 4 mm edge and the drone's
  thin parts were raised to the 1 cm floor; the visor's arc faced the right
  ear until turned 72 degrees (18 degrees short: 90 faces it ahead).
  Then the owner's first visit (from 22:44), five asks in chat, each
  answered live and saved: every bench's backrest leaned forward over its
  seat (pitch -12 with the back at +Z; flipped, 22:49), the Crown's
  sun-beds' head-rests sloped away (flipped, 22:56), the ramp rails' ends
  were diagonal (rebuilt as sheared cuboids, ends vertical, 23:20), the
  gate and door panels' textures met the walls beside them at a seam
  (their Box UVs moved onto the whole panel's frame, 23:28), and the body
  "rather extreme" with the visor "flying in front of the face" - reworked
  by an offline builder (a moderate build, the visor refitted 4 mm off the
  brow, eight new worn items), saved 23:54. The CLI read the sculpt's
  `-150` as a flag: values set with `--file`. The owner (23:58): "That
  looks much better already. But the drone circling your avatar could be
  much better" - rebuilt by a second offline builder (308k tokens, 35 min:
  the airframe, ducted fans, gimbal and lights, the flight on six nested
  motion nodes, render proofs of the nose and the bank, a clearance
  sweep), set live clean and saved 00:37. Meanwhile, self-guided: the
  world named Strata with its own banner (00:14), and the conveyor shop
  walked into its back half - the passage was fine, my old route not.
- **1034 - 2026-11-19, live 26, #1947, self-guided.** The owner: "Please
  log in Atrium again and continue the work". The review found every
  level's heart complete and every outer hall a bare forest of columns;
  phase 2 of the concept, THE OUTER DISTRICTS, built in one sitting by
  three sequential offline builders, each applied, walked live along all
  its routes and saved: the Sump's (464k tokens, 52 min: the Cistern
  Quarter, the Stacks, Rag Town, the gate yards; saved 02:18), the Works'
  (380k, 39 min: the Hydroponic Farms, the Warehouses, the Power Hall;
  03:17) and the Terraces' (410k, 42 min: the Market Street, the library,
  the park, the outer homes, the door belvederes; 04:17) - 251 KB, 125
  placements and 60 lights in all; 286 legs walked live, one stopped
  short (the pavilion). Failed at first: the
  pavilion's 11 cm deck stopped the live capsule (lowered to 3 cm); the
  library's book texture's aspect ratio of 5.0 was clamped to 4.0. The
  manifest, one record holding every placement, became the next ceiling
  (82.6 KB of 102.4): the Terraces kept to 16 placements by holding every
  reused heart piece in one shared set. The owner did not visit.
- **1035 - 2026-11-19, live 27, #1948.** The owner: "Please log in Atrium
  again and continue with the still open work"; in the world 09:25-11:42.
  The steward's coat by an offline builder (343k tokens: collar, yoke and
  two knee-length half-skirts, 14 worn items of 16), the tablet gripped and
  then turned to face the body, the drone's left-behind frame cube hidden
  (its rise and dip moved onto the hub) - then "Work on your avatar is
  complete for now". The cistern's pale water given floating scum mats. The
  owner asked for recommendations and a standing texture-request issue
  (symbios-texture #30, every agent to contribute); approved a two-hour
  turning day. The owner swam on dry floor twice - a pool nested in a piece
  registers at its local transform, and a pool has no bottom (#1949): all
  pools moved, then the upper ones wrapped, confirmed fixed. Steam vents
  sunk under the floor, all steam born inside its source, the kitchen given
  a stovepipe, the fire drums' flames as Flame flipbooks and their lights at
  the strongest flicker allowed (#1950 asks for more). Life on the move by
  a third builder (437k tokens): lift towers, cargo drones, the Freight Line
  - the towers' density not yet judged by the owner. Failed at first: a
  `name` key (nodes have none), a flicker of exactly 0.30 slowed to 0.74 Hz,
  my own untested walk legs through shacks and a column.

## Open threads

- The rebuilt drone (00:37) has not been seen by the owner yet; the
  sweep's body boxes were measured by hand on a ruler render, and the
  walk and run margins are estimates.
- The coat (live 27, 10:14) has not been judged by the owner yet; at a
  run past 30 degrees of thigh pitch a half's top leaves the yoke at the
  back (up to 6 cm) and its front juts with the thigh (the documented
  half-skirt behaviour), and the vent shows a hairline of light from the
  front.
- Triangles: the Terraces' soft-blob tree crowns and greens (about 60k) are
  the first to lighten if the web strains.
- The lift towers (live 27) are dense lattices that change the open atrium's
  look (`1035/motion/views_well_day.png`): the owner has not judged them -
  thin them (fewer diagonals, a lighter frame) or remove them on their word
  (`b/motion.edits`: placements 280-283).
- The budget's last fifth: about 115 KB of the live ceiling and 20 KB of
  the manifest (some 90 placements) are left after phase 2, and the
  placements draw about 397,000 triangles over the ground's 522,000 -
  keep the rest for the owner's asks, and trim before adding much more
  (the barrel fires' and the banner's rings are done, 05:11; next the
  Terraces' soft-blob trees, 616 triangles a copy x 58, and the heart's
  `terr_block`, 5,880 a copy x 16 - 94,000, the largest single cost).
- Sound in the outer districts is sparse: the reactor's hum, and since
  04:23 and 05:11 (`b/sound.py`) Rag Town's crackling drums, the cistern
  pump's, the freight lift's and the loader lane's hum, the farms' misting
  hiss and the park pond's water - each copied from a sound the world
  already had; none on the Market Street or in the Stacks' street beyond
  the neon sign's hum.
- The Works' loaders and lift deck have no colliders (a moving part never
  does): visitors pass through them; the misting lines' particles were
  faint offline (opacity 0.24, 22 a second): unseen live.
- The park pond on the Terraces still reads pale at eye height (the
  water shader mirrors only the sky, even under a roof); the Sump's
  cistern got floating algae and scum mats for it (live 27, 10:39) - the
  pond's lily pads cover far less.
- Snow: the Crown and the terrace gardens are open to the sky; how the
  winter garden reads under settled snow has not been seen live.
