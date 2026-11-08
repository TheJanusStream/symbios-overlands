# Your own body

The avatar is the one record the agent may edit in any world. Two kinds of
body: a **rigged** person, four-legged beast or winged body (a sculpt and worn
items, kept in records of their own) and a **generator** body - every vehicle:
a car, a hover-boat, an airship, a helicopter, an airplane - whose whole shape
is one generator tree at
`/record/body/visuals`, the same JSON as a building's
([building.md](building.md)). Everyone sees a generator body's edits at
once; other clients fetch a rigged body's sculpt and worn items from your
PDS, so they see a change to those only after `A save avatar --wait`.

## Choosing how it moves

`/record/locomotion` decides. For building and inspecting, a rotorcraft (the
`helicopter` preset every airship and every seeded helicopter flies, #1614)
is the best body found so far: a
`walk-to` climbs over whatever is in the way instead of ending `stuck`, and
it lands on its point - a roof or a hill to look down at a build from.
Keep what the seed gave unless there is a reason: the numbers are tuned
together, and `hover_thrust` must stay `mass x 9.81` or the craft sinks or
climbs at idle. A seed that rolled a person (or anything else) becomes a
rotorcraft by wearing one from the Catalogue: `A wear avatar_airship_hybrid`
brings that craft's tuned flight, gait and sound in one undo step, and the
body is then yours to reshape - `/record/body/visuals` and
`chassis_half_extents` - with its flight numbers kept (Kairos, session 1000,
the hybrid's 10.6 m/s). A movement begun before the wear keeps steering the
old body (#1851): `halt` and start it again. Landed, it rests its weight on its box's underside (#1612),
so a shape that hangs below `-half_y` stands in the ground. A rotorcraft
that should tip into its motion - nose down flying forward, banked into a
turn - says how far in `/record/gait/lean_degrees` (0 to 45; absent is 0,
level as an airship flies; a seeded utility helicopter leans 15, a
bubble-canopy trainer 18 (#1621), an autogyro 12, which levels her
tail-down stance flying forward (#1624), a coaxial 10, her pusher
shoving her along (#1628), and a tandem-rotor transport 6, flying flat
under her two rotors (#1630); and a seeded helicopter's gait zeroes
the airship's heave and nose wander, so she sits still on her skids or her
wheels). An autogyro is drawn standing tail-down in her level box, every part
turned about her main axle, so her box's floor is her tyres.
A seeded airship is one of seven types (#1657, #1658-#1663): a Zeppelin, a
blimp, a steam dirigible, a sky galleon, a hot-air balloon, a hybrid airship
or an organic floater, each flying her own feel on the same preset - flat
out from the floater's 7.2 m/s to the hybrid's 10.6, turning from the steam
dirigible's 58 degrees a second to the blimp's 126 (#1664). Landed, she
rests on her lowest drawn part: a balloon on her basket, a floater on her
tentacles' tips, a steam dirigible on her pusher's disc.
A seeded airplane flies the `airplane` preset (#1617): she banks and pitches
as she flies, so her gait does nothing, and her record carries where her
drawn wheels stand - `gear_track`, `gear_ahead`, `tail_wheel_aft`,
`tail_gear_share` and `center_of_mass_ahead`, shares of her box. A
`tail_gear_share` under 1 is what sits her tail-down on the ground and level
in the air; move her wheels in the tree and these with them, or she stands
on wheels nobody can see. A seeded high-wing flies the preset's handling, and
a seeded biplane (#1623) her own - `min_airspeed` 9 against 11,
`cruise_speed` 17 against 20, 1.4 times the drag, `roll_rate` 120 against 90
and `pitch_rate` 50 against 45 - a seeded twin-engine transport (#1625)
hers: `min_airspeed` 13, `cruise_speed` 24, 0.85 times the drag, `roll_rate`
70, `pitch_rate` 40 and `yaw_rate` 22 - and a seeded air racer (#1629) hers:
`min_airspeed` 13, `cruise_speed` 26, 0.6 times the drag, `roll_rate` 150,
`pitch_rate` 60 and `yaw_rate` 30, the fastest of the four, which a pilot
reads off her record as it reads any airplane's. Her `roll_rate` sets how
soon she reaches her bank, not only how fast she may roll (#1619): the racer
banks to 40 degrees in 0.41 s, the high-wing in 0.67, the twin in 0.88; past
the racer's 150, on the default `angular_damping`, a rate banks her no
sooner.

## Shaping a generator body

- **The collider is a box**, `locomotion.chassis_half_extents`, centred on
  the tree's origin. Nothing in the tree collides. Put the lowest point of
  the shape at `-half_y`, or the body floats above the ground it lands on
  or sinks into it; a thin part that trails (a tail, a thread) stays above
  `-half_y` too, or it cuts the ground when landed.
- **A body meant to hover** turns that rule round: draw its lowest point
  above `-half_y` on purpose, and it floats by the gap landed and flying.
  Kairos's factory stops 0.33 m over its collider's floor, with a glowing
  ring, lit vents and a falling shimmer under it to say why.
- **Front is local -Z**: what trails goes to +Z.
- **Make the root the node at the origin with no transform**, so every
  child's translation is a body-space position.
- **Caps** (the sanitiser's, for a body): any one dimension 16 m, the
  product of scales down any branch 4, 1024 nodes, 16 points on a `lathe`
  or `spine`.
- `lathe` stations run bottom to top **in list order**, so one profile can
  go out along an underside and back over a dome (a mushroom cap); a
  station of radius 0 closes it. `torus` needs `major_resolution` and
  `minor_resolution`. A refused JSON names where it failed: the generator
  node (`in the node at ...`, #1446), or any other place (`at ...`, #1457).
- **Colours are sRGB.** A glow reads when it is deep and saturated, with
  `base_color` and `emission_color` the same (the catalogue's mushroom glow
  is `[0.28, 0.86, 0.50]` at strength about 1.4); a pale colour driven
  emissive washes out to white.
- A `particles` node gives a body a trail: `inherit_velocity: 0` leaves what
  it emits behind as it moves (a spore drift in the body's wake).
- **Smoke that billows** is a tiled animation: `procedural_texture` a `Puff`
  with `variant_rows` and `variant_cols` 4 (16 cells, each its own puff) and
  `frame_mode` `{"$type": "network.symbios.particle.frame.over_lifetime",
  "fps": 50000}`, so each particle runs through the sheet as it rises,
  grows and fades (Kairos's stacks, session 1000). Particles do not turn,
  so the atlas is the motion. A sprite's colours are LINEAR; light grey at
  half alpha vanished against a pale sky, a mid-grey at two-thirds read.

## One body on every way of moving

Kairos keeps a fleet: the same deck over a car, a hover-boat, an airplane
and a rotorcraft (session 1000; `exports/kairos/b/fleet.py`,
[accounts/kairos/](accounts/kairos/README.md)). Wear the Catalogue craft of
each family for its tuned numbers, set your tree and grown collider, test,
then `stash-avatar` each one.

- **A car's ground is fixed by its springs**: at rest it lies `-half_y -
  (suspension_rest_length - m g / 4 k)` under the box's centre (Jink's
  formula); draw every wheel's bottom there. Roll terms take the tyre's
  radius and read the part's own velocity, so a wheel rolls the right way
  whichever way its axis points; a steer term reads forward along its
  parent's +Z, so a nose at -Z steers with a negative gain.
- **Grow the turning torque with the box**: a wider box turns slower on the
  same torque; scale it by `(hx^2 + hz^2)` new over old. A car's top speed
  is `drive_force / (mass x linear_damping)`: the Catalogue rover's is 2.8
  m/s, slower than a person runs.
- **An airplane's gear lives in its record** as shares of its box; the
  sanitiser keeps `tail_wheel_aft` at most 1 (the box's end) and
  `center_of_mass_ahead` behind `gear_ahead`. Read the kept values back
  (`adjusted_at`) and draw the wheels there, or she stands on wheels nobody
  can see.
- **Walking is a rigged body's alone.** A generator body that should look
  as if it walks can turn RHex-style legs with `roll` at the hip-to-foot
  radius; a leg hung from a crank cannot be held upright, since both `roll`
  and `speed` read the part's own motion.
- `render --driver` drives a car offline; start it away from the landing's
  gateway (`--drive-bearing`). A hover-boat or an airplane is tried on an
  offline daemon with its own `XDG_CONFIG_HOME`.
- **Wear the boat on water**: a car or a walker that follows its admin into
  a lake sinks to the bed (Kairos as the Roadfactory, live session 20; the
  owner: "in such situations it would be best to switch to your boat avatar
  variant"). [tools/companion.py](tools/README.md)'s `--water-level` does it.

## Seeing it before anyone else does

`look` puts the camera about 11 m behind the body - too far to judge detail.
Render the tree offline instead, from four sides, in about 20 seconds:

```bash
BEVY_ASSET_ROOT=<repo> <repo>/target/test-release/render \
    --generator visuals.json --body --out sheet.png
```

where `visuals.json` is the tree as `avatar set /record/body/visuals`
takes it. The four views are fixed in yaw - `--yaw` changes nothing there,
`--elev` raises or lowers them - and the body is small in each tile:
`--size 1024` draws each tile 1024 px, and Kairos's `exports/kairos/b/crop.py
SHEET OUT` cuts the tiles' middles and lays them two by two, close enough to
judge a 2 cm part. Then `A avatar set /record/body/visuals --file visuals.json` and
`A avatar set /record/locomotion/chassis_half_extents '[x, y, z]'` (two
steps of the avatar's undo), and `look` once to see it in the world.
Write the body as a script that prints the JSON from named dimensions, as a
building is.

**Find a body's loose parts with the floating report** ([building.md](building.md),
"Sizes, footprints, clearances"). It reads worlds only, so put the tree as the
one generator in a copy of any room record, placed with `snap_to_terrain` false
and lifted clear of the ground there (8 m does). Stood on the ground, the report
anchors on whatever touches it, and a loose wheel, leg or skirt passes as
grounded; lifted, nothing reaches the ground and it anchors on the tree's first
part, so each class-a row is a part that touches nothing of the body. The
Seafactory's stern fans were 18 such parts, 0.29 m off, that the owner saw
floating (live session 20); the other four fleet bodies had none.

A **rigged** person is seen the same way, from its whole avatar rather than a
tree (#1482). Pull it, edit the copy, and render the copy:

```bash
python3 <repo>/docs/agent/tools/rec.py pull avatar avatar.json
BEVY_ASSET_ROOT=<repo> <repo>/target/test-release/render \
    --rigged avatar.json --out person.png
```

The file is `{record, body, worn}` (the whole `agent avatar get ""` answer
reads too). The sheet is the body built, hung and dressed exactly as the game
does it, standing with its arms at its sides: four full-body views over three
head close-ups. The full-body views are at one fixed scale, so a taller or
shorter sculpt stands taller or shorter in its tile. It prints the body's size
and one line per worn item, where it hangs or why it is not drawn; a malformed
file is refused with the JSON pointer of what is wrong. Read the lines above
the size line too: anything in the file that is not drawn as written (a
misspelt key, a hair style name this build does not know, a value pulled into
range) is named there by its pointer, and the game would draw it the same way
without a word. Edit a sculpt axis in the copy (under `/body`, the same
pointers `avatar set /body/...` takes), render again, and bring the edit live
only once the sheet shows what was meant and names nothing. For scale, walk
it through its world beside its buildings, in the world's saved record
(`rec.py pull room room.json`):

```bash
BEVY_ASSET_ROOT=<repo> <repo>/target/test-release/render --world <did> \
    --world-record room.json --walker-avatar avatar.json --focus walker \
    --dist 9 --elev 12 --yaw 150 --out walk.png
```

`--focus walker` follows the file's body; `--yaw 150` looks at its face, and
`--walker <seed>` adds that seed's body beside it for comparison.
`--first-person` looks out of the body's own eyes instead, as a player who
zooms all the way in does (#1745), with the body out of the picture and its
shadow on the ground: `--elev` lowers the head that many degrees and `--yaw`
turns it left, so a doorway, a ceiling or a railing is judged at the height
a person meets it.

## Dressing a rigged person

Session 879 made Reeve, a rigged reeve of about 1300, and the admin
judged him three times: "look at it from various angles ... while walking
and running", then "both side views: the centre part looks completely
wrong and the offsets need tuning".

- **A seeded account may be a vehicle.** The Avatar window's Body tab has
  "Wear a rigged body" (`ui click "Avatar > Wear a rigged body"`);
  `status.locomotion` then reads `humanoid`, or `four_legged` on an account
  whose seed rolls a beast (see "A four-legged body"), or `winged` on one
  whose seed rolls a winged body (see "A winged body").
- **The sculpt** is `avatar set /body/...`. Wire scale: 1000 = 1.0 on the
  axes (-1..1, explorable to +-3), height in millimetres, age in whole
  years; read symbios-avatar's `plan/*.rs` for the ranges, not memory.
  Hair is per region (scalp, brows, chin, flanks, moustache): a style, a
  cut, root and tip colours, and a skin layer (stubble); a chin and flank
  skin density of 0.8/0.7 darkened the whole lower face, 0.52/0.26 read as
  a close shave.
- **The engine dresses a body in a top and trousers only.** Anything else
  is a worn item, and a worn item's own record is editable: stash a
  catalogue wearable (each stash is named anew - `Traveler's Satchel_1`,
  `_2`; `wear`, `take-off` and `unstash` given the slug name it back since
  #1484), `wear` it by that name,
  then `avatar set /worn/N/item/item` (your generator) and
  `/worn/N/item/socket`. Save the inventory too, so every worn item's
  source exists.
- **Measure the socket frames before authoring; do not assume them.** Hang
  a ruled marker (a bar with a coloured ball each 10 cm along X, Y and Z)
  on each socket and render once, against the bare body (hide the worn
  items with the account's `b/markers.py --hide`). Reeve's, measured so in
  sessions 879 and 883: `hips` and `waist` sit at the body's FRONT, a
  buckle's place, the body's middle about 9 cm behind them - and `waist`
  rides well above the girdle (a sword hung there had its hilt at his
  chest); the leg roots (`left-hip`, `right-hip`) sit OUTSIDE each hip,
  0.139 m out from the middle, 0.12 m below the hips socket and 0.10 m
  behind it, +Y up, +Z outward, +X backward on the left leg and forward on
  the right; `neck` is about 6 cm behind the nape at jaw height, the neck's
  middle 12 cm in front of it; each foot socket is about 5 cm past the toe
  tip, 1.7 cm over the sole and 3-4 cm OUTSIDE its foot's centre line, +Z
  forward and +X the body's left on BOTH feet (not mirrored); `right-hand`'s
  +Y and +X lie 45 degrees either side of the hanging forearm, +Z forward
  and 17 degrees up, so the body's vertical there is about (0.68, 0.68,
  0.29) - a staff aimed along +Y leaned 45 degrees. Guessed frames put the
  shoes in front of the feet and the hood on his back like a pack, and
  shoes centred on the foot socket left each foot's inner side and heel
  bare.
- **A worn item is rigid on its joint, and a leg root swings with the
  thigh.** Judge every worn thing walking AND running from four sides:
  `render --world DID --world-record R --walker-avatar FILE --walker-pace
  1.4` (5.0 runs) `--walk-from X,Z --walk-to X,Z --focus walker --dist 4.5
  --elev 8 --yaw 0|90|150|270`. One long rigid skirt on the pelvis was
  pierced by the legs at a walk and left behind at a run; a sword on a leg
  root swung out with the thigh. Hang what must not swing from the hips;
  give what follows a leg to that leg's root. A long skirt becomes a riding
  surcoat: a short yoke on the hips and a half-skirt on each leg root (a
  lathe with `path_cut` [0, 0.5], its axis set back to the body's middle).
  Keep the halves to the knee: at a run a panel turns with its thigh, and a
  mid-calf one jutted 0.6 m forward like a board.
- **Measure the body, not only its sockets.** Session 879 noted Reeve's hips
  as 0.30 m wide and cut his yoke to that; the bare body is 0.186 m wide at
  the hips socket (0.21 m over the buttocks) - the arms, hanging 0.17 m out,
  had been measured in. The owner saw the result at once: "the clothing
  piece underneath [the belt] ... is too large". Read widths from a `render
  --rigged --size 1024` sheet in pixels against the markers' 2 cm balls,
  and check the scale against the body's height.
- **A body is shallower than it is wide**: a round skirt, girdle or cape
  stands off it front and back - scale them 0.62-0.76 in Z. Not at the
  neck: there the body is deeper than it is wide (the upper chest and back
  reach 0.11 m from the neck's middle), so a shoulder cape shallow enough
  for the shoulder tips showed the chest through it at any depth scale up
  to 0.78, and a collar, a round yoke and a band fitted by eye still showed
  the shoulders' slope between them. A solid BlobGroup mantle (a flattened
  ellipsoid over the shoulders blended into one round the neck) cannot show
  what it contains. A single-walled lathe shows its lit outer faces from
  inside as a pale lampshade; give a skirt two walls (down the inside,
  across the hem, up the outside) so the inside is a dark lining.
- **A worn thing's underside**: an ellipsoid sole curves up at its sides and
  narrows toward the toes, so the foot's edge showed along it; a flat blob
  box under the foot's sole covers it.
- **A rigid thing in the hand follows the forearm**: at a run the engine
  lifts the forearm level, so a staff held upright at rest points forward
  like a lance for part of each stride. Walking, it moves as a staff does.
- **Judge motion at chosen moments**: `render --world ... --walker-avatar
  FILE --walker-lead S` starts the capture S seconds into the walk, so a
  handful of leads from both sides, walking and running, tiled into one
  sheet (the account's `b/gait_sheet.py`), shows every worn thing mid-stride
  in one picture.
- **An item's root shows where its socket is outside the body.** A carrier's
  1 cm root cube at the neck socket sat beside the jaw in every close-up:
  move the root into the body and shift its children back by as much.
- **Save the avatar after every accepted change** (`rec.py save avatar`).
  Unsaved avatar edits live only in the running daemon, and other players
  see a rigged body's changes only once saved. (The old clothes the admin
  saw after a restart in session 879 were a stale copy in his own client,
  #1485, since fixed: Reeve's change had been saved.)

## A four-legged body

A rigged body can be a dog, a cat or a horse instead of a person, or a cross
of them (#1799). In the Avatar window's Body tab, `quadruped` rolls one from
the body's seed and `humanoid` rolls a person back (`A ui click "Avatar > body
> quadruped"`), or send a whole body with `A avatar set /body ...`. Either way
its collider, walk, run, weight and jump follow the body in the same undo step,
and `status.locomotion` reads `four_legged`. Others see the new body once it is
saved (`A save avatar --wait`), like any rigged sculpt.

An account can also start as one: one seed in eight of those on foot is a
beast, about one avatar in 48 (#1808). Offline,
`A start --offline --stand-in did:plc:agentofflinebeast22222ia` is a dog, and
the render tool's `--family-seeds beast` lists more seeds.

- **It walks as a person does**: `walk-to`, `follow`, Shift to run, swimming.
  `look --view eyes` looks from its own eyes.
- **Its collider lies along its trunk** and cannot be set by hand: the body is
  what sizes it. The default dog is 0.88 m long and 0.36 m deep, the cat 0.69
  by 0.29, the horse 2.28 by 0.89.
- **It gestures at what it says**, as a person does. A greeting's words wag, a
  thanks plays a bow, a yes tilts the head, and a no plays nothing. `sit`, `lie
  down` and `get up` (or `up`) are a beast's alone and mean nothing to a person.
  It stays down until it moves, swims, leaves the ground or says `get up`. A
  player who arrives after it sat sees it standing: nothing sent says it sits. A
  gesture within 2 s of the last is dropped, so `hi` and then `sit` at once
  loses the sit.
- **It wears what is cut for it.** The catalogue's `beast_collar`, `saddle_pad`
  and `ear_bow` go on its neck, back and between its ears, each fitted to the
  body by whoever draws it. Beyond them it wears only what is seated at
  `crown`, so the circlet at its authored size. A person's banner, lantern or
  satchel is refused, and `wear` says why. A person is refused a beast's cut the
  same way. Either stays in the outfit, unworn, for a body that wears it.

## A winged body

A rigged body can be winged instead (#1875): a bird, a bat or a sugar glider,
or a cross of two of them. In the Avatar window's Body tab, `winged` rolls one
from the body's seed with every lock lifted - so it may land on a bird, a
bat, a glider or a cross, not only the owl - and `humanoid` or `quadruped` rolls
the other kinds back (`A ui click "Avatar > body > winged"`); the family, the
size and the shape are the viewer's own picks under it, with a bird's plumage
or a bat's or a glider's coat. Or send a whole body with `A avatar set /body
...`, its `archetype` a `network.symbios.avatar.defs#winged` with a `family`
(`bird`, `bat` or `glider`), and for a cross a `hybrid` family and its
`blend`. Either way the new body, its collider and its flight section are one
undo step, and `status.locomotion` reads `winged`, whichever family leads.
Others see it once it is saved (`A save avatar --wait`).

An account can also start as one: one seed in eight of the persons the beasts
leave has wings, about one avatar in 55 (#1882), a bird, a bat, a glider or a
cross as the engine's own roll mixes them. Offline,
`A start --offline --stand-in did:plc:agentofflinewinged2222cu` is an owl,
and the render tool's `--family-seeds winged` lists more seeds (`--outfit
<seed>` says which family each is). The Catalogue's Avatars offer an owl, a
fruit bat, a sugar glider and two crosses (`A catalogue winged`).

- **The leading family decides its posture**, and a cross takes its leading
  family's (the one with the larger share; at an even blend the `hybrid`). A
  bird stands upright on two legs, on the level floor a person stands on; a
  bat or a glider goes on four, lying along its trunk as a beast does, and
  plants its feet on the real terrain and on the roads, floors and decks under
  it as a beast does.
- **Its collider is its body's** and cannot be set by hand. A bird stands in
  an upright capsule as tall as it is to the top of its head and as wide as
  its trunk - the default owl 0.67 m tall and 0.26 m across; a bat or a
  glider lies in a capsule along its trunk, the default bat 0.46 m long and
  0.19 m deep, the glider 0.41 by 0.16. Its run (Shift), its mass and its
  jump follow the body as a beast's do - the owl runs 3.2 m/s and weighs
  6.5 kg, the bat 1.9 m/s and 2.2 kg, the glider 2.1 m/s and 1.4 kg - on
  every edit and across a change of family, a bird made a bat; a run you
  tuned keeps its distance from the body's own.
- **Its flight section is its body's too.** The numbers its body sizes -
  `flight_power`, `trim_speed`, `stall_speed`, `beat_speed`, `wingbeat_rate`
  and `takeoff_speed` - are refitted on every edit of the body, the wingbeat
  the one its wings are drawn at; the handling - `stamina_beats`,
  `stamina_recovery`, `glide_ratio`, `max_bank`, `roll_rate` and `pitch_rate`
  - is yours to tune and is kept through every edit (the Locomotion tab shows
  the first as read-outs and the second as sliders). `flight_power` is 1 for a
  bird and 0.7 for a bat, a cross's by the families' shares, and 0 for a
  glider or any cross a glider leads. The default owl trims at 6.2 m/s,
  stalls under 3.4 and beats 3.0 times a second; the bat 3.9, 2.1 and 2.6;
  the glider 4.1 and 2.2, and does not beat. A winged body made a
  person or a beast loses its section; one an older game saved without it
  gets it back from its body when its owner's game installs the record.
- **It flies as "A body that flies" says**, posed by the mode (#1877): it
  crouches and leaps, beats in time with the flap key, spreads its wings to
  glide, folds them to dive, flares and lands, and turns its tail and outer
  wing into a banked turn; everyone else sees the same, each mode as its body
  is drawn there. A bat or a glider, lying along its capsule, raises its nose
  over the take-off's first fifth of a second rather than at once. Down, it
  plays its landing out - its legs give and its wings fold, about half a
  second - unless it walks off, swims or is set down elsewhere first; Space
  in that time takes off again from a crouch.
  - **A glider cannot flap.** It leaps twice as hard as a flapper its size
    would (on top of the 1.6 times every non-flapper gets) and only glides:
    from level ground the default glider rises about 1.35 m and glides about
    3.7 m, so launch it from a height to go anywhere.
  - **A weak flapper** - a cross under a bat's 0.7 - beats with a bat's push,
    so it can climb, but spends its stamina sooner.
  - **Not from water.** In water it swims, and Space swims up: it cannot
    take off there.
  - An agent cannot fly it: `walk-to` and `follow` walk it, and
    `status.flight` reads what it is doing.
- **It gestures at what it says**, by its leading family's names. A message
  plays the first of these it holds:

  | Words | Bird | Bat | Glider |
  | --- | --- | --- | --- |
  | a greeting (`hi`, `hello`, `bye`, ...), `stretch` | Wing Stretch | Wing Stretch | Stretch |
  | a yes (`yes`, `ok`, `sure`, ...) | Head Tilt | Head Tilt | Head Tilt |
  | a thanks (`thanks`, `bow`, `gg`, ...), `preen`, `groom` | Preen | Groom | Groom |
  | `sit`, `perch`, `rest`, `curl` | Perch | Rest | Curl |
  | `lie down` (`lay down`), `roost` | Roost | Rest | Curl |
  | `shake` | Shake | Shake | Shake |
  | `get up`, `up` | gets up | gets up | gets up |
  | a no (`no`, `nope`, ...) | nothing | nothing | nothing |

  A rest (a perch, a roost, a bat's rest, a glider's curl) is held until it
  moves, swims, takes off or says `get up`, and a player who arrives after it
  sat down sees it standing. A gesture within 2 s of the last is dropped, as
  on any body. The chat window's hint names only the words that play on it.
- **It wears what is cut for an animal, at three places**: its neck (the
  catalogue's `beast_collar`, fitted over its feathers or fur), its crown
  (the `ear_bow`, standing on its head, which a bird has no ears to read; or
  the circlet at its authored size, as anything seated at the crown) and its
  legs (the `leg_band`, cut for it, on the left leg - the Wearables tab moves
  it to the right - at the middle of a bird's tarsus or a bat's or a glider's
  shin; a glider's folded membrane hides it for now). Not its back, where the
  folded wings lie, so the `saddle_pad` is refused, and not its hands, which
  are a wing's wrists. A person's banner, lantern or satchel is refused, and
  `wear` says why; whatever it cannot wear stays in the outfit, unworn, for a
  body that does.
- **It is framed by its size**, as a beast is: the orbit's focus at its eyes
  and its rest radius in proportion to how tall it stands, from 5 m (the
  default owl, and every smaller body, which so looks small in the frame) to
  a person's 12 m. In flight the orbit follows its heading, as a vehicle's
  does, and after a landing the focus eases down to it. First person looks
  out of its seated eyes on foot, and in flight rides its attitude - banking
  and pitching with it - once it has been off its feet a tenth of a second.
  `look --view eyes` looks from its seated eyes, level, from clear of its
  own head, tail and wings.
- **`render --rigged`** draws a winged body and what it wears from its file,
  as for a person ("Seeing it before anyone else does").

## A body that flies

An on-foot body can fly (#1861): its preset's optional `flight` section says
how, and Space takes it off. It is one body with a mode, never a change of
preset, so taking off edits nothing and saves nothing. A person or a beast
keeps its section through every edit of its body, a rename or `avatar set
/body` among them; a rigged winged body has its own, sized from its body ("A
winged body", above). Before the winged bodies there was a stand-in: a
generator bird made of primitives, which still flies.
[`tests/fixtures/winged/`](../../tests/fixtures/winged/)
holds its body (`standin_bird_visuals.json`), the whole avatar
(`standin_bird_avatar.json`) and the script that prints both
(`standin_bird.py`).

To wear it, from a body that is already a generator body (a seeded person is
rigged, and JSON cannot change a body's kind): `A wear avatar_airship_hybrid`
first if it is not, then

```
A stash-avatar "before the bird"
A avatar set /record --file tests/fixtures/winged/standin_bird_avatar.json
A save avatar --wait
```

`A wear "before the bird"` puts the old body back. `status.flight` reads the
mode (`ground`, `takeoff`, `flap`, `glide`, `dive`, `flare`, `land`),
`stamina` (0 to 1), `stalled`, `pitch_deg`, `bank_deg`, `beats`,
`flight_power` and `can_flap`; `null` for a body that does not fly.

- **The keys.** On the ground it walks as a person does, and Space takes it
  off: a leap up and forward and two beats. In the air a Space press beats
  once and holding it beats at `wingbeat_rate`; no key glides; W and S put the
  nose down and up, trading height for speed; A and D bank it, which turns
  it; Shift folds the wings and dives. Walked off a ledge, it opens its wings
  after a moment's fall. An agent cannot fly it yet: `walk-to` walks it, and
  `drive`, which holds keys segment by segment, takes only a body on wheels.
- **The numbers** (scaled ints, as every record number): `flight_power` 0 to 1
  (1 a bird, about 0.7 a bat, 0 a glider: under 0.25 it cannot flap at all and
  leaps 1.6 times as hard), `glide_ratio` and `trim_speed` (a hands-off glide
  settles at the trim and sinks `trim_speed / glide_ratio`, 1.3 m/s on the
  bird), `stall_speed` (slower, the nose drops and it dives until it flies
  again), `beat_speed` (what a full-power beat adds, m/s), `wingbeat_rate`,
  `stamina_beats` (beats a full stamina holds; a weaker flyer's beats cost
  more), `stamina_recovery` (seconds gliding from empty to full, three times
  faster standing), `takeoff_speed`, `max_bank`, `roll_rate` and
  `pitch_rate` (degrees). The bird's are the defaults for a body 0.75 m tall;
  speeds grow with a bigger body and the wingbeat slows.
- **It lands on the tops of things**: ground, a roof, a deck, a beam - any
  surface flatter than 30 degrees - when its path closes on one within its
  capture radius (0.6 of its height). The flare starts about 0.45 s before
  contact, whatever the speed, and draws it onto the spot; down, it settles a
  quarter of a second and is on its feet; a rigged winged body then plays
  its landing out. A wall stops it but is never landed on. Into water it
  swims, and it cannot take off from water.
- **A generator body on foot hangs from the capsule's middle**: draw its
  feet at `-(capsule_length / 2 + capsule_radius)`, the collider's bottom, or
  it stands in the ground or over it. Nothing refits a generator body's
  capsule, as a rigged body's is refitted.
- **Wings are a pair of parts swinging along the body**: each a joint node
  either side of the middle with a `.swing` spin about the fore-aft axis
  (`[0, 0, 10000]` on the right, `[0, 0, -10000]` on the left, so both rise
  together), the wing hung from it. What makes a body fly is its record's
  `flight` section, never its wings: an older game drops the key when it saves
  the record, and nothing gives a generator body's back - set it again. A
  section that leaves a number out takes the default for it. The wings flap
  all the time, on the ground too: only a rigged winged body is posed by the
  mode.

## A whole body in the inventory

The inventory keeps whole avatars (#1843): a body worth keeping before you
change it, or a ready-made one from the Catalogue.

- `A stash-avatar [name]` copies the avatar you wear - its body, every prop
  it wears, how it moves - unsaved edits included, as the Avatar editor's
  Save to Inventory does, in any world. Without a name the item takes the
  body's own name, or what it is (`person`, `winged`, `boat`, `aircraft`). It
  is an inventory edit: `A save inventory` keeps it.
- `A wear <item>` with an avatar item puts the whole avatar on in place of
  the one worn: one step of the avatar's undo history (`A undo avatar` takes
  it off), unsaved until `A save avatar`, which writes its body and props
  and retires the outfit it replaced. A rigged body is resolved at once,
  with no fetch. The one already on is refused: "You are already wearing
  ...". `A inventory` marks the avatar item you wear `worn: true`.
- The Catalogue's avatars - one per craft, a spread of people, beasts and
  winged bodies -
  are in `A catalogue <words>`'s `avatars`, by slug (`avatar_...`). `A wear
  <slug>` wears one as the Catalogue's Wear does; `A stash <slug>` copies it
  in under its name.
- A whole avatar is worn or given, never placed: `place` refuses one
  (`place` of a Catalogue avatar's slug says to `wear` it). `A gift give
  @admin "<avatar item>"` gives a whole body from the inventory (#1847), but
  only to a player whose game can take one - an older game is refused by
  name, and one still announcing its version says to try again in a moment.
  A Catalogue avatar is not given (they have the same Catalogue): tell them
  its name, or `stash` it and give the item. An avatar the admin offers
  arrives as `gift_offered` with `is: "avatar"` and its kind, and `gift
  accept` puts it in the inventory; the body you wear stays on. To give one
  build rather than a whole body, stash the part of the body it is ("A
  workbench on the body", below).
- Save a body you care about before trying another: `stash-avatar`, `save
  inventory`, then `wear` the new one. Wearing the item again puts it all
  back.

## A workbench on the body

Away from its own world the agent can edit nothing but its avatar. So give
a generator body a flat surface to build on - a node whose children are the
builds - and a build made there is stashed and gifted like anything else:

1. `A avatar set /record/body/visuals/children/<bench>/children/- '<json>'`
   - the build's root sits at the bench's top surface (children are placed
   from their parent's centre, so that is half the bench's thickness up).
   An empty bench has no `children` in the record at all (an empty list is
   left out); an append makes it (#1458 - before that, the first build on
   an empty bench was refused with "nothing is at"). The answer's `pointer`
   is where the build landed (`.../children/<n>`, #1470): the pointer step 3
   takes.
2. Look at it, fix it, `avatar set` again.
3. `A stash "<name>" --from-avatar /record/body/visuals/children/<bench>/children/<n>`:
   the item stands on the build's own origin, wherever it sat on the body.
4. `A gift give @admin "<name>" --wait`.
5. Take the build off the body again (`avatar set` of the bench's
   `children` without it), or it rides along everywhere.

Hypha's bench is `/record/body/visuals/children/0`, a disc of 0.55 m radius
sunk into a shallow depression at the centre of its cap and coloured like
the cap's dark centre (session 874: a proud, lighter 0.75 m disc read as a
lid); a build's root goes at `[0, 150, 0]` (1.5 cm, the disc's half
thickness) in the disc's frame. Keep the bench level when the cap is not:
a stashed build keeps the turn it had on the body.

Kairos's is a whole build plate (session 1000): `/record/body/visuals/children/0`,
a plain slate square 1.70 m a side and 4 cm thick, a build's root at
`[0, 200, 0]` in its frame. A plate meant for building is kept plain and the
deck's colours and lights stay off it and below its top, so a build reads on
it and never meets them ([accounts/kairos/](accounts/kairos/README.md)).

## Making a body look grown, not turned

Session 874 rebuilt Hypha's cap after the admin asked for "more detail" and
"a more natural cap". What made the difference, and what went wrong first:

- **Break the symmetry a little.** A cap tilted 1 degree and scaled to a
  0.95 oval stops reading as a turned disc. Anything that must fit the cap
  (a depression the bench sits in) needs room for the oval and the tilt.
- **Detail at a size that reads.** From the game's camera, 11 m back, a
  2-4 cm scale is invisible; 10-20 cm fibrous scales (flattened spheres,
  long axis radial, lying along the surface, crowding toward the centre)
  read as a honey fungus. Sink a decoration by LESS than its own half
  height - sunk by more, only slivers showed.
- **One lathe per continuous surface.** A stem built as three colour bands
  at equal radii showed a shading seam at every joint (it looked stacked
  like cushions); a colour sleeve 1 cm proud read as a flower pot. One
  lathe with a fibrous texture looked like a stem.
- **Lathe stations run counter-clockwise in (radius, height)** - out along
  the underside, back over the top - or the faces point inward. A hanging
  skirt (a ring on a stem) is listed underside first.
- **A lathe's texture runs round it**: a `Bark` texture's furrows came out
  as horizontal rings until `uv_rotation` 90 (degrees; wire 900000) turned
  them lengthwise.
- **Gills deepest at the stem, rising into the flesh at the margin** - hung
  at one depth, 88 plates showed as a picket fence under the rim.
- **Inspection renders**: a turntable frames the whole tree, so trailing
  threads made the mushroom tiny; render a copy without them. A negative
  `--elev` needs the `=` (`--elev=-12`, looking up at the gills).
- **Give the body something that is its own.** Reviewed from the game's
  camera, Hypha read as a plain mushroom set down on the ground. A honey
  fungus's rhizomorphs are the part that really glows (foxfire), so session
  878 gave it seven glowing root cords from the stem's flared base, each
  forking twice, lying a hair above the collider's bottom plane (-half_y):
  landed, they spread on the ground like the network under it; flying, they
  hang as its roots. 56 spines, +6.7k triangles, the record 42 -> 65 KB.
  Anything that lies at the base must stay above -half_y by its own radius,
  or it cuts the ground when landed.
- **Re-run safety**: a body script that keeps parts from the saved body
  must find them by something they alone have (the threads' glow), not by
  kind - the new young caps' stalks were spines too.
